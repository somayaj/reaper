use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::SystemTime;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use super::gradle::{find_gradle_root, resolve_gradle_command, run_gradle_with_command};
use super::symbols::{ClassSearchHit, SymbolLocation, class_name_match_score};

const INDEX_PATH: &str = "java-index.json";
/// Bump when index shape/rules change so stale caches rebuild once.
const INDEX_VERSION: u32 = 10;

/// JDK module directories inside extracted `src.zip` (Java 9+ layout).
const JDK_SOURCE_MODULES: &[&str] = &[
    "java.base/",
    "java.sql/",
    "java.logging/",
    "java.net.http/",
    "java.xml/",
    "java.naming/",
    "java.management/",
    "java.instrument/",
    "java.compiler/",
    "java.desktop/",
    "java.datatransfer/",
    "java.prefs/",
    "java.rmi/",
    "java.scripting/",
    "java.security.jgss/",
    "java.transaction.xa/",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
struct JavaIndex {
    project_root: String,
    symbols: Vec<IndexedSymbol>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct IndexedSymbol {
    name: String,
    qualified: String,
    kind: String,
    path: String,
    line: u32,
    column: u32,
}

/// In-memory index with O(1) symbol lookup (avoids reparsing 100MB+ JSON on every F12).
struct IndexLookup {
    project_root: String,
    symbols: Vec<IndexedSymbol>,
    by_qualified: HashMap<String, usize>,
    by_name: HashMap<String, Vec<usize>>,
}

impl IndexLookup {
    fn from_index(index: JavaIndex) -> Self {
        let mut by_qualified = HashMap::new();
        let mut by_name: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, sym) in index.symbols.iter().enumerate() {
            by_qualified.entry(sym.qualified.clone()).or_insert(i);
            by_name.entry(sym.name.clone()).or_default().push(i);
        }
        Self {
            project_root: index.project_root,
            symbols: index.symbols,
            by_qualified,
            by_name,
        }
    }

    fn empty(project_root: String) -> Self {
        Self {
            project_root,
            symbols: Vec::new(),
            by_qualified: HashMap::new(),
            by_name: HashMap::new(),
        }
    }

    fn type_by_qualified(&self, fqcn: &str) -> Option<&IndexedSymbol> {
        self.by_qualified.get(fqcn).and_then(|&i| {
            let sym = &self.symbols[i];
            if sym.kind != "method" {
                Some(sym)
            } else {
                None
            }
        })
    }

    fn method_by_qualified(&self, fqcn: &str) -> Option<&IndexedSymbol> {
        self.by_qualified.get(fqcn).and_then(|&i| {
            let sym = &self.symbols[i];
            if sym.kind == "method" {
                Some(sym)
            } else {
                None
            }
        })
    }

    fn types_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a IndexedSymbol> {
        self.by_name
            .get(name)
            .into_iter()
            .flat_map(|indices| indices.iter())
            .filter_map(|&i| {
                let sym = &self.symbols[i];
                if sym.kind != "method" {
                    Some(sym)
                } else {
                    None
                }
            })
    }

    fn methods_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a IndexedSymbol> {
        self.by_name
            .get(name)
            .into_iter()
            .flat_map(|indices| indices.iter())
            .filter_map(|&i| {
                let sym = &self.symbols[i];
                if sym.kind == "method" {
                    Some(sym)
                } else {
                    None
                }
            })
    }

    fn members_for_type<'a>(
        &'a self,
        type_fqcn: &str,
        member_prefix: &str,
        limit: usize,
    ) -> Vec<&'a IndexedSymbol> {
        let qual_prefix = format!("{type_fqcn}.");
        let member_prefix_lower = member_prefix.to_lowercase();
        let mut items: Vec<&IndexedSymbol> = self
            .symbols
            .iter()
            .filter(|sym| {
                sym.qualified.starts_with(&qual_prefix)
                    && sym.kind != "class"
                    && (member_prefix.is_empty()
                        || sym.name.to_lowercase().starts_with(&member_prefix_lower))
            })
            .collect();
        items.sort_by(|a, b| {
            let rank = |k: &str| if k == "method" { 0 } else { 1 };
            rank(&a.kind)
                .cmp(&rank(&b.kind))
                .then_with(|| a.name.len().cmp(&b.name.len()))
                .then_with(|| a.name.cmp(&b.name))
        });
        items.truncate(limit);
        items
    }

    fn methods_for_type<'a>(
        &'a self,
        type_fqcn: &str,
        member_prefix: &str,
        limit: usize,
    ) -> Vec<&'a IndexedSymbol> {
        let qual_prefix = format!("{type_fqcn}.");
        let member_prefix_lower = member_prefix.to_lowercase();
        let mut items: Vec<&IndexedSymbol> = self
            .symbols
            .iter()
            .filter(|sym| {
                sym.kind == "method"
                    && sym.qualified.starts_with(&qual_prefix)
                    && (member_prefix.is_empty()
                        || sym.name.to_lowercase().starts_with(&member_prefix_lower))
            })
            .collect();
        items.sort_by(|a, b| a.name.len().cmp(&b.name.len()).then_with(|| a.name.cmp(&b.name)));
        items.truncate(limit);
        items
    }

    fn types_matching_name_prefix<'a>(
        &'a self,
        prefix: &str,
        limit: usize,
    ) -> Vec<&'a IndexedSymbol> {
        let prefix_lower = prefix.to_lowercase();
        let mut items: Vec<&'a IndexedSymbol> = self
            .symbols
            .iter()
            .filter(|sym| {
                sym.kind != "method" && sym.name.to_lowercase().starts_with(&prefix_lower)
            })
            .collect();
        items.sort_by(|a, b| {
            let pa = spring_priority(&a.qualified);
            let pb = spring_priority(&b.qualified);
            pa.cmp(&pb)
                .then_with(|| a.name.len().cmp(&b.name.len()))
                .then_with(|| a.name.cmp(&b.name))
        });
        items.truncate(limit);
        items
    }

    fn types_matching_fqcn_prefix<'a>(
        &'a self,
        prefix: &str,
        limit: usize,
    ) -> Vec<&'a IndexedSymbol> {
        let prefix_lower = prefix.to_lowercase();
        let mut items: Vec<&'a IndexedSymbol> = self
            .symbols
            .iter()
            .filter(|sym| {
                sym.kind != "method" && sym.qualified.to_lowercase().starts_with(&prefix_lower)
            })
            .collect();
        items.sort_by(|a, b| {
            a.qualified
                .len()
                .cmp(&b.qualified.len())
                .then_with(|| a.qualified.cmp(&b.qualified))
        });
        items.truncate(limit);
        items
    }
}

struct CachedLookup {
    mtime: SystemTime,
    stamp: String,
    lookup: Arc<IndexLookup>,
}

static LOOKUP_CACHE: LazyLock<Mutex<HashMap<String, CachedLookup>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

static GRADLE_ROOT_CACHE: LazyLock<Mutex<HashMap<String, PathBuf>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

static IMPORTS_CACHE: LazyLock<Mutex<HashMap<String, ImportMap>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

static DEFINITION_CACHE: LazyLock<Mutex<HashMap<String, Option<SymbolLocation>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

static JDK_LOCATION_CACHE: LazyLock<Mutex<HashMap<String, SymbolLocation>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

static LIBRARY_SOURCE_DIRS_CACHE: LazyLock<Mutex<HashMap<String, Vec<PathBuf>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

const DEFINITION_CACHE_MAX: usize = 4096;

fn invalidate_lookup_cache(gradle_root: &Path) {
    if let Ok(key) = gradle_root.canonicalize() {
        let key_str = key.display().to_string();
        if let Ok(mut guard) = LOOKUP_CACHE.lock() {
            guard.remove(&key_str);
        }
        if let Ok(mut guard) = GRADLE_ROOT_CACHE.lock() {
            guard.retain(|k, _| !k.starts_with(&format!("{key_str}:")));
        }
        if let Ok(mut guard) = IMPORTS_CACHE.lock() {
            guard.retain(|k, _| !k.starts_with(&format!("{key_str}:")));
        }
        if let Ok(mut guard) = DEFINITION_CACHE.lock() {
            guard.retain(|k, _| !k.starts_with(&format!("{key_str}:")));
        }
        if let Ok(mut guard) = JDK_LOCATION_CACHE.lock() {
            guard.retain(|k, _| !k.starts_with(&format!("{key_str}:")));
        }
        if let Ok(mut guard) = LIBRARY_SOURCE_DIRS_CACHE.lock() {
            guard.remove(&key_str);
        }
    }
}

fn cached_gradle_root(ws: &Path, from_path: &str) -> Result<Option<PathBuf>> {
    let ws_key = ws
        .canonicalize()
        .unwrap_or_else(|_| ws.to_path_buf())
        .display()
        .to_string();
    let cache_key = format!("{ws_key}:{from_path}");
    if let Ok(guard) = GRADLE_ROOT_CACHE.lock() {
        if let Some(root) = guard.get(&cache_key) {
            return Ok(Some(root.clone()));
        }
    }
    let root = find_gradle_root(ws, from_path)?;
    let root = match root {
        Some(r) => Some(r),
        None => super::maven::find_maven_root(ws, from_path)?,
    };
    let root = match root {
        Some(r) => Some(r),
        None => find_plain_java_root(ws, from_path),
    };
    if let Some(ref root) = root {
        if let Ok(mut guard) = GRADLE_ROOT_CACHE.lock() {
            guard.insert(cache_key, root.clone());
        }
    }
    Ok(root)
}

fn definition_cache_key(
    gradle_root: &Path,
    from_path: &str,
    line: u32,
    column: u32,
    content: &str,
) -> String {
    let index_mtime = reaper_dir(gradle_root)
        .join("java-index.json")
        .metadata()
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!(
        "{}:{}:{}:{}:{}:{index_mtime}",
        gradle_root.display(),
        from_path,
        line,
        column,
        content_fingerprint(content)
    )
}

fn content_fingerprint(content: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    content.hash(&mut hasher);
    hasher.finish()
}

fn cache_definition(key: String, hit: Option<SymbolLocation>) {
    if hit.is_none() {
        return;
    }
    let Ok(mut guard) = DEFINITION_CACHE.lock() else {
        return;
    };
    if guard.len() >= DEFINITION_CACHE_MAX {
        guard.clear();
    }
    guard.insert(key, hit);
}

fn cached_definition(key: &str) -> Option<Option<SymbolLocation>> {
    DEFINITION_CACHE
        .lock()
        .ok()
        .and_then(|guard| guard.get(key).cloned())
}

fn parse_imports_cached(gradle_root: &Path, from_path: &str, content: &str) -> ImportMap {
    let fp = content_fingerprint(content);
    let key = format!("{}:{}:{fp}", gradle_root.display(), from_path);
    if let Ok(guard) = IMPORTS_CACHE.lock() {
        if let Some(imports) = guard.get(&key) {
            return imports.clone();
        }
    }
    let imports = parse_imports(content);
    if let Ok(mut guard) = IMPORTS_CACHE.lock() {
        if guard.len() >= DEFINITION_CACHE_MAX {
            guard.clear();
        }
        guard.insert(key, imports.clone());
    }
    imports
}

fn get_lookup(ws: &Path, gradle_root: &Path) -> Result<Arc<IndexLookup>> {
    let key = gradle_root
        .canonicalize()
        .unwrap_or_else(|_| gradle_root.to_path_buf())
        .display()
        .to_string();

    let cache_path = reaper_dir(gradle_root).join("java-index.json");
    let index_mtime = if cache_path.is_file() {
        std::fs::metadata(&cache_path)?.modified()?
    } else {
        SystemTime::UNIX_EPOCH
    };

    if let Ok(guard) = LOOKUP_CACHE.lock() {
        if let Some(entry) = guard.get(&key) {
            if entry.mtime == index_mtime {
                return Ok(Arc::clone(&entry.lookup));
            }
        }
    }

    let stamp = if cache_path.is_file() {
        std::fs::read_to_string(reaper_dir(gradle_root).join("classpath.stamp")).unwrap_or_default()
    } else {
        String::new()
    };
    let mtime = index_mtime;

    let index = try_load_index(ws, gradle_root)?.unwrap_or_else(|| {
        empty_index(ws, gradle_root)
    });
    let lookup = Arc::new(IndexLookup::from_index(index));

    if let Ok(mut guard) = LOOKUP_CACHE.lock() {
        guard.insert(
            key,
            CachedLookup {
                mtime,
                stamp,
                lookup: Arc::clone(&lookup),
            },
        );
    }

    Ok(lookup)
}

#[derive(Debug, Clone, Serialize)]
pub struct CompletionItem {
    pub label: String,
    pub kind: String,
    pub detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub insert: Option<String>,
    pub path: Option<String>,
    pub line: Option<u32>,
    pub column: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub documentation: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WarmIndexStatus {
    pub indexed: bool,
    pub project_root: Option<String>,
    pub symbol_count: usize,
    pub cached: bool,
    pub dependency_jars: usize,
    pub source_jars: usize,
    pub jdk_sources: bool,
    pub spring_symbols: usize,
    pub jdk_symbols: usize,
}

pub fn is_gradle_workspace(ws: &Path) -> bool {
    super::gradle::find_all_gradle_roots(ws)
        .map(|roots| !roots.is_empty())
        .unwrap_or(false)
}

pub fn is_java_indexable_workspace(ws: &Path) -> bool {
    find_all_index_roots(ws)
        .map(|roots| !roots.is_empty())
        .unwrap_or(false)
}

fn find_all_index_roots(ws: &Path) -> Result<Vec<PathBuf>> {
    let mut roots = super::gradle::find_all_gradle_roots(ws)?;
    for maven_root in super::maven::find_all_maven_roots(ws)? {
        if !roots.iter().any(|r| r == &maven_root) {
            roots.push(maven_root);
        }
    }
    if roots.is_empty() && workspace_has_plain_java_sources(ws) {
        roots.push(ws.canonicalize().unwrap_or_else(|_| ws.to_path_buf()));
    }
    roots.sort_by(|a, b| a.display().to_string().cmp(&b.display().to_string()));
    roots.dedup();
    Ok(roots)
}

pub fn workspace_has_plain_java_sources(ws: &Path) -> bool {
    plain_java_scan(&ws.join("src"), 0)
}

fn plain_java_scan(dir: &Path, depth: u32) -> bool {
    if depth > 8 || !dir.is_dir() {
        return false;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if plain_java_scan(&path, depth + 1) {
                return true;
            }
        } else if path.extension().and_then(|e| e.to_str()) == Some("java") {
            return true;
        }
    }
    false
}

/// Single-module Java tree (no Gradle/Maven) under `src/**/*.java`.
fn find_plain_java_root(ws: &Path, rel_path: &str) -> Option<PathBuf> {
    let ws_canon = ws.canonicalize().unwrap_or_else(|_| ws.to_path_buf());
    if is_java_like(rel_path) {
        let file = ws.join(rel_path);
        if file.is_file() {
            if let Some(parent) = file.parent().and_then(|p| p.canonicalize().ok()) {
                if parent.starts_with(&ws_canon) {
                    return Some(ws_canon);
                }
            }
        }
    }
    if workspace_has_plain_java_sources(ws) {
        return Some(ws_canon);
    }
    None
}

const TOOLING_CLASSPATH_DONE: &str = "tooling-classpath.done";
const CLASSPATH_JARS_CACHE: &str = "classpath-jars.json";
const CLASSPATH_OUTPUTS_CACHE: &str = "classpath-outputs.json";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ProjectClasspathOutputs {
    #[serde(default)]
    classes_dirs: Vec<String>,
    #[serde(default)]
    source_dirs: Vec<String>,
}

type IndexProgress<'a> = Option<&'a Box<dyn Fn(&str, usize) + Send>>;

fn report_index_progress(progress: IndexProgress, phase: &str, count: usize) {
    if let Some(cb) = progress {
        cb(phase, count);
    }
}

/// Drop cached Java indexes so the next warm_index rebuilds (e.g. after branch checkout).
pub fn invalidate_caches(ws: &Path) -> Result<()> {
    for root in find_all_index_roots(ws)? {
        invalidate_lookup_cache(&root);
        let reaper = reaper_dir(&root);
        let _ = std::fs::remove_file(reaper.join("classpath.stamp"));
        let _ = std::fs::remove_file(reaper.join(TOOLING_CLASSPATH_DONE));
        let _ = std::fs::remove_file(reaper.join(CLASSPATH_JARS_CACHE));
        let _ = std::fs::remove_file(reaper.join(CLASSPATH_OUTPUTS_CACHE));
        let _ = std::fs::remove_file(reaper.join(INDEX_PATH));
        let _ = std::fs::remove_file(reaper.join(META_PATH));
    }
    Ok(())
}

fn reaper_dir(gradle_root: &Path) -> PathBuf {
    gradle_root.join(".reaper")
}

/// Resolved compile/runtime JAR paths for javac (Spring, JDK libs, etc.).
pub fn cached_classpath_jars(gradle_root: &Path) -> Vec<PathBuf> {
    let path = reaper_dir(gradle_root).join(CLASSPATH_JARS_CACHE);
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    serde_json::from_str::<Vec<String>>(&text)
        .unwrap_or_default()
        .into_iter()
        .map(PathBuf::from)
        .filter(|p| p.is_file())
        .collect()
}

fn save_classpath_jars_cache(gradle_root: &Path, jars: &[PathBuf]) -> Result<()> {
    std::fs::create_dir_all(reaper_dir(gradle_root))?;
    let paths: Vec<String> = jars.iter().map(|p| p.display().to_string()).collect();
    std::fs::write(
        reaper_dir(gradle_root).join(CLASSPATH_JARS_CACHE),
        serde_json::to_string(&paths)?,
    )?;
    Ok(())
}

pub fn save_classpath_jars_cache_pub(project_root: &Path, jars: &[PathBuf]) -> Result<()> {
    save_classpath_jars_cache(project_root, jars)
}

fn load_project_classpath_outputs(project_root: &Path) -> ProjectClasspathOutputs {
    let path = reaper_dir(project_root).join(CLASSPATH_OUTPUTS_CACHE);
    let Ok(text) = std::fs::read_to_string(path) else {
        return ProjectClasspathOutputs::default();
    };
    serde_json::from_str(&text).unwrap_or_default()
}

fn save_project_classpath_outputs(project_root: &Path, outputs: &ProjectClasspathOutputs) -> Result<()> {
    std::fs::create_dir_all(reaper_dir(project_root))?;
    std::fs::write(
        reaper_dir(project_root).join(CLASSPATH_OUTPUTS_CACHE),
        serde_json::to_string_pretty(outputs)?,
    )?;
    Ok(())
}

fn outputs_to_paths(outputs: &ProjectClasspathOutputs) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let classes = outputs
        .classes_dirs
        .iter()
        .map(PathBuf::from)
        .filter(|p| p.is_dir())
        .collect();
    let sources = outputs
        .source_dirs
        .iter()
        .map(PathBuf::from)
        .filter(|p| p.is_dir())
        .collect();
    (classes, sources)
}

fn paths_to_outputs(classes_dirs: &[PathBuf], source_dirs: &[PathBuf]) -> ProjectClasspathOutputs {
    ProjectClasspathOutputs {
        classes_dirs: classes_dirs
            .iter()
            .map(|p| p.display().to_string())
            .collect(),
        source_dirs: source_dirs
            .iter()
            .map(|p| p.display().to_string())
            .collect(),
    }
}

/// Compiled project output dirs (e.g. build/classes/java/main) from last Gradle/Maven resolve.
pub fn cached_project_classes_dirs(project_root: &Path) -> Vec<PathBuf> {
    let (mut classes, _) = outputs_to_paths(&load_project_classpath_outputs(project_root));
    for dir in discover_project_output_dirs(project_root).0 {
        if !classes.iter().any(|p| p == &dir) {
            classes.push(dir);
        }
    }
    classes
}

/// Project Java source roots including generated sources (annotation processors, etc.).
pub fn cached_project_source_dirs(project_root: &Path) -> Vec<PathBuf> {
    let (_, mut sources) = outputs_to_paths(&load_project_classpath_outputs(project_root));
    for dir in discover_project_output_dirs(project_root).1 {
        if !sources.iter().any(|p| p == &dir) {
            sources.push(dir);
        }
    }
    sources
}

fn discover_project_output_dirs(project_root: &Path) -> (Vec<PathBuf>, Vec<PathBuf>) {
    discover_gradle_output_dirs(project_root)
}

fn discover_gradle_output_dirs(project_root: &Path) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let mut classes = Vec::new();
    let mut sources = Vec::new();
    let mut seen_classes = HashSet::new();
    let mut seen_sources = HashSet::new();
    discover_gradle_output_dirs_inner(
        project_root,
        project_root,
        0,
        &mut classes,
        &mut sources,
        &mut seen_classes,
        &mut seen_sources,
    );
    (classes, sources)
}

fn discover_gradle_output_dirs_inner(
    project_root: &Path,
    dir: &Path,
    depth: usize,
    classes: &mut Vec<PathBuf>,
    sources: &mut Vec<PathBuf>,
    seen_classes: &mut HashSet<PathBuf>,
    seen_sources: &mut HashSet<PathBuf>,
) {
    if depth > 12 || !dir.is_dir() {
        return;
    }
    for suffix in [
        "build/classes/java/main",
        "build/classes/java/test",
        "build/classes/kotlin/main",
        "build/classes/kotlin/test",
        "target/classes",
        "target/test-classes",
    ] {
        let p = dir.join(suffix);
        if p.is_dir() && seen_classes.insert(p.clone()) {
            classes.push(p);
        }
    }
    for generated_rel in [
        "build/generated/sources",
        "build/generated/source",
        "build/generated/sources/headers",
        "target/generated-sources",
        "target/generated-test-sources",
    ] {
        let generated = dir.join(generated_rel);
        if !generated.is_dir() {
            continue;
        }
        if generated_rel.starts_with("target/") {
            let annotations = generated.join("annotations");
            if annotations.is_dir() && seen_sources.insert(annotations.clone()) {
                sources.push(annotations);
            }
        }
        collect_java_dirs_under(&generated, sources, seen_sources);
        collect_java_source_roots_with_files(&generated, sources, seen_sources);
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = entry.file_name();
        if name == "build"
            || name == "target"
            || name == ".gradle"
            || name == "node_modules"
            || name == ".git"
            || name == ".reaper"
        {
            continue;
        }
        discover_gradle_output_dirs_inner(
            project_root,
            &path,
            depth + 1,
            classes,
            sources,
            seen_classes,
            seen_sources,
        );
    }
}

fn collect_java_dirs_under(dir: &Path, out: &mut Vec<PathBuf>, seen: &mut HashSet<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name == "main" || name == "test" {
            if seen.insert(path.clone()) {
                out.push(path);
            }
            continue;
        }
        if name == "java" {
            let main = path.join("main");
            let test = path.join("test");
            if main.is_dir() {
                if seen.insert(main.clone()) {
                    out.push(main);
                }
            } else if seen.insert(path.clone()) {
                out.push(path);
            }
            if test.is_dir() && seen.insert(test.clone()) {
                out.push(test);
            }
            continue;
        }
        collect_java_dirs_under(&path, out, seen);
    }
}

/// Any directory under generated output that directly contains `.java` files (MapStruct, etc.).
fn collect_java_source_roots_with_files(dir: &Path, out: &mut Vec<PathBuf>, seen: &mut HashSet<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut has_java = false;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() {
            if path.extension().and_then(|e| e.to_str()) == Some("java") {
                has_java = true;
                break;
            }
        }
    }
    if has_java && seen.insert(dir.to_path_buf()) {
        out.push(dir.to_path_buf());
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_java_source_roots_with_files(&path, out, seen);
        }
    }
}

pub fn index_build_tooling_enabled() -> bool {
    std::env::var("REAPER_INDEX_SKIP_BUILD").as_deref() != Ok("1")
}

pub fn project_roots(ws: &Path) -> Result<Vec<PathBuf>> {
    find_all_index_roots(ws)
}

pub fn is_build_tool_project_root(project_root: &Path) -> bool {
    super::maven::is_maven_project_root(project_root)
        || super::gradle::is_gradle_project_dir(project_root)
}

pub fn needs_tooling_classpath_resolve(project_root: &Path) -> bool {
    if !index_build_tooling_enabled() || !is_build_tool_project_root(project_root) {
        return false;
    }
    !tooling_classpath_resolved(project_root)
}

pub fn mark_tooling_classpath_done(project_root: &Path) -> Result<()> {
    std::fs::create_dir_all(reaper_dir(project_root))?;
    std::fs::write(
        reaper_dir(project_root).join(TOOLING_CLASSPATH_DONE),
        project_root.display().to_string(),
    )?;
    if let Ok(stamp) = classpath_stamp(project_root) {
        std::fs::write(reaper_dir(project_root).join("classpath.stamp"), stamp)?;
    }
    Ok(())
}

fn tooling_classpath_stamp_valid(project_root: &Path) -> bool {
    let reaper = reaper_dir(project_root);
    let Ok(current) = classpath_stamp(project_root) else {
        return false;
    };
    let Ok(stored) = std::fs::read_to_string(reaper.join("classpath.stamp")) else {
        // Tooling finished; index rebuild may not have written a stamp yet.
        return true;
    };
    stored == current
}

/// True when Gradle/Maven compile classpath was saved via tooling (not tree-walk alone).
pub fn tooling_classpath_resolved(project_root: &Path) -> bool {
    reaper_dir(project_root)
        .join(TOOLING_CLASSPATH_DONE)
        .is_file()
        && tooling_classpath_stamp_valid(project_root)
}

pub fn invalidate_root_index_cache(project_root: &Path) {
    invalidate_lookup_cache(project_root);
    let reaper = reaper_dir(project_root);
    let _ = std::fs::remove_file(reaper.join("java-index.json"));
    // Keep classpath.stamp when tooling cache is still valid (build files unchanged).
}

pub fn needs_any_tooling_classpath_resolve(ws: &Path) -> bool {
    project_roots(ws)
        .ok()
        .map(|roots| {
            roots
                .iter()
                .any(|root| needs_tooling_classpath_resolve(root))
        })
        .unwrap_or(false)
}

/// Resolve Maven/Gradle classpaths (offline cache first, then tooling) before indexing.
pub fn resolve_classpaths_for_index(
    ws: &Path,
    progress: Option<&Box<dyn Fn(&str, usize) + Send>>,
) -> Result<()> {
    let roots = project_roots(ws)?;
    for root in roots {
        if !is_build_tool_project_root(&root) {
            continue;
        }
        if let Some(cb) = progress {
            cb("classpath-resolve", 0);
        }
        resolve_root_classpath(&root, progress)?;
    }
    let _ = ws;
    Ok(())
}

fn resolve_root_classpath(root: &Path, progress: IndexProgress) -> Result<Vec<PathBuf>> {
    if gradle_classpath_from_tooling_cache(root).is_none() {
        let _ = try_resolve_classpath_via_tooling(root, progress);
    }

    let jars: Vec<PathBuf> = resolve_full_project_classpath(root)
        .into_iter()
        .filter(|p| p.is_file())
        .collect();

    if !jars.is_empty() {
        let _ = ensure_dependency_sources(root, &jars);
        let _ = save_classpath_jars_cache_pub(root, &jars);
        tracing::info!(
            "Resolved {} classpath JARs for {} before index",
            jars.len(),
            root.display()
        );
    }

    Ok(jars)
}

/// Run Maven/Gradle to download dependency JARs and sources (blocking).
pub fn resolve_classpath_via_tooling(project_root: &Path) -> Result<Vec<PathBuf>> {
    Ok(resolve_classpath_via_tooling_full(project_root, None)?.jars)
}

fn resolve_classpath_via_tooling_full(
    project_root: &Path,
    progress: IndexProgress,
) -> Result<GradleClasspath> {
    if super::maven::is_maven_project_root(project_root) {
        resolve_maven_classpath_via_tooling(project_root, progress)
    } else {
        resolve_gradle_classpath(project_root, progress)
    }
}

fn sources_coverage_low(jars: &[PathBuf], source_jars: &[PathBuf]) -> bool {
    if jars.is_empty() {
        return false;
    }
    source_jars.len() * 2 < jars.len()
}

fn ensure_dependency_sources(project_root: &Path, jars: &[PathBuf]) -> Result<()> {
    if !index_build_tooling_enabled() || jars.is_empty() {
        return Ok(());
    }
    let source_jars = discover_source_jars_for_jars(jars);
    if !sources_coverage_low(jars, &source_jars) {
        return Ok(());
    }
    tracing::info!(
        "Fetching dependency sources for {} ({}/{} JARs had *-sources.jar locally)",
        project_root.display(),
        source_jars.len(),
        jars.len()
    );
    if super::maven::is_maven_project_root(project_root) {
        ensure_maven_dependencies(project_root, None)?;
    } else if super::gradle::is_gradle_project_dir(project_root) {
        let _ = resolve_gradle_classpath(project_root, None);
    }
    Ok(())
}

fn resolve_maven_classpath_via_tooling(
    maven_root: &Path,
    progress: IndexProgress,
) -> Result<GradleClasspath> {
    ensure_maven_dependencies(maven_root, progress)?;
    let resolved = resolve_classpath_from_m2(maven_root);
    if !resolved.jars.is_empty() {
        return Ok(resolved);
    }
    resolve_maven_classpath_via_mvn(maven_root, progress)
}

pub fn compile_classpath_jars(gradle_root: &Path) -> Result<Vec<PathBuf>> {
    Ok(resolve_dependency_jars(gradle_root))
}

/// JARs from pom.xml / Gradle build files + transitive POM walk (offline M2/Gradle cache).
pub fn resolve_dependency_tree_jars(project_root: &Path, include_test_scope: bool) -> Vec<PathBuf> {
    if super::maven::is_maven_project_root(project_root) {
        return resolve_classpath_from_m2_scoped(project_root, include_test_scope).jars;
    }
    if super::gradle::is_gradle_project_dir(project_root) {
        return resolve_classpath_from_gradle_cache_scoped(project_root, include_test_scope).jars;
    }
    Vec::new()
}

fn build_tree_classpath_sufficient(_project_root: &Path, jars: &[PathBuf]) -> bool {
    !jars.is_empty()
}

fn cached_classpath_trustworthy(project_root: &Path, cached: &[PathBuf]) -> bool {
    if cached.is_empty() {
        return false;
    }
    if tooling_classpath_resolved(project_root) {
        return true;
    }
    !resolve_dependency_tree_jars(project_root, true).is_empty()
}

/// Full project classpath: tooling + declared build-file transitive tree + compiled/generated outputs.
pub fn resolve_full_project_classpath(project_root: &Path) -> Vec<PathBuf> {
    let mut entries = resolve_dependency_tree_jars(project_root, true);

    let tooling_jars = if tooling_classpath_resolved(project_root) {
        cached_classpath_jars(project_root)
    } else if let Some(cp) = gradle_classpath_from_tooling_cache(project_root) {
        cp.jars
    } else {
        let cached = cached_classpath_jars(project_root);
        if cached_classpath_trustworthy(project_root, &cached) {
            cached
        } else {
            Vec::new()
        }
    };
    entries = merge_classpath_jars(&tooling_jars, &entries);
    entries.extend(cached_project_classes_dirs(project_root));
    dedupe_classpath_entries(filter_existing_classpath_entries(entries))
}

/// Dependency JAR files only (transitive tree + tooling cache).
pub fn resolve_dependency_jars_for_project(project_root: &Path) -> Vec<PathBuf> {
    resolve_full_project_classpath(project_root)
        .into_iter()
        .filter(|p| p.is_file())
        .collect()
}

fn resolve_classpath_jars_preferring_build_tree(
    project_root: &Path,
    _include_test_scope: bool,
) -> Vec<PathBuf> {
    resolve_full_project_classpath(project_root)
        .into_iter()
        .filter(|p| p.is_file())
        .collect()
}

/// Cached dependency JARs for javac — never triggers Maven/Gradle during diagnostics.
pub fn resolve_dependency_jars_cached(project_root: &Path) -> Vec<PathBuf> {
    resolve_classpath_jars_preferring_build_tree(project_root, true)
}

/// True when javac needs test-scoped dependency JARs for this file.
pub fn file_needs_test_classpath(rel_path: &str, content: &str) -> bool {
    super::java_ecosystem::is_test_file_path(rel_path)
        || content.contains("@Test")
        || content.contains("@ParameterizedTest")
        || content.contains("@SpringBootTest")
        || content.contains("org.junit.jupiter")
        || content.contains("org.junit.")
        || content.contains("org.springframework.boot.test")
        || content.contains("org.mockito")
        || content.contains("@Mock")
        || content.contains("@InjectMocks")
        || content.contains("@Spy")
        || content.contains("MockitoExtension")
}

/// Whether resolved JAR paths include Spring Data API types (commons/jpa/etc.), not just Boot starters.
pub fn classpath_includes_spring_data_deps(jars: &[PathBuf]) -> bool {
    jars.iter().any(|p| jar_path_is_spring_data_api(&p.to_string_lossy()))
}

fn jar_path_is_spring_data_api(path: &str) -> bool {
    let s = path.to_ascii_lowercase();
    // Boot starters declare spring-data but do not contain org.springframework.data.domain.* classes.
    if s.contains("spring-boot-starter-data") {
        return false;
    }
    s.contains("spring-data-commons")
        || s.contains("spring-data-jpa")
        || s.contains("spring-data-mongodb")
        || s.contains("spring-data-rest")
        || s.contains("spring-data-redis")
        || s.contains("spring-data-elasticsearch")
        || s.contains("spring-data-cassandra")
        || s.contains("spring-data-neo4j")
        || s.contains("spring-data-r2dbc")
}

/// Whether resolved JAR paths include common Spring libraries.
pub fn classpath_includes_spring_deps(jars: &[PathBuf]) -> bool {
    jars.iter().any(|p| {
        let s = p.to_string_lossy().to_ascii_lowercase();
        s.contains("spring-data")
            || s.contains("spring-core")
            || s.contains("spring-context")
            || s.contains("spring-beans")
            || s.contains("spring-boot")
            || s.contains("/spring-")
    })
}

/// Whether resolved JAR paths include common test-scoped libraries.
pub fn classpath_includes_test_deps(jars: &[PathBuf]) -> bool {
    classpath_includes_junit(jars)
        || classpath_includes_mockito(jars)
        || jars.iter().any(|p| {
            let s = p.to_string_lossy().to_ascii_lowercase();
            s.contains("spring-boot-test")
                || s.contains("spring-test")
                || s.contains("assertj")
        })
}

pub fn classpath_includes_junit(jars: &[PathBuf]) -> bool {
    jars.iter().any(|p| {
        let s = p.to_string_lossy().to_ascii_lowercase();
        s.contains("junit-jupiter") || s.contains("/junit/")
    })
}

pub fn classpath_includes_mockito(jars: &[PathBuf]) -> bool {
    jars.iter().any(|p| {
        p.to_string_lossy()
            .to_ascii_lowercase()
            .contains("mockito")
    })
}

pub fn classpath_includes_slf4j(jars: &[PathBuf]) -> bool {
    jars.iter().any(|p| {
        let s = p.to_string_lossy().to_ascii_lowercase();
        s.contains("slf4j") || s.contains("logback") || s.contains("log4j")
    })
}

pub fn classpath_includes_lombok(jars: &[PathBuf]) -> bool {
    jars.iter().any(|p| {
        p.to_string_lossy()
            .to_ascii_lowercase()
            .contains("lombok")
    })
}

fn merge_classpath_jars(primary: &[PathBuf], extra: &[PathBuf]) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for jar in primary.iter().chain(extra.iter()) {
        if seen.insert(jar.clone()) {
            out.push(jar.clone());
        }
    }
    out
}

/// Union tooling/offline JARs with Maven/Gradle build-file transitive tree (all declared scopes).
fn merge_with_build_file_dependency_tree(
    project_root: &Path,
    jars: &[PathBuf],
    include_test_scope: bool,
) -> Vec<PathBuf> {
    let tree = resolve_dependency_tree_jars(project_root, include_test_scope);
    merge_classpath_jars(jars, &tree)
}

/// Classpath for live javac diagnostics — transitive libraries + compiled/generated output dirs.
pub fn resolve_dependency_jars_for_java_file(
    project_root: &Path,
    rel_path: &str,
    content: &str,
) -> Vec<PathBuf> {
    let include_test = file_needs_test_classpath(rel_path, content);
    let mut entries = resolve_full_project_classpath(project_root);
    if include_test {
        let test_tree = resolve_dependency_tree_jars(project_root, true);
        entries = merge_classpath_jars(&entries, &test_tree);
        entries.extend(cached_project_classes_dirs(project_root));
        entries = dedupe_classpath_entries(filter_existing_classpath_entries(entries));
    }
    entries
}

/// Cached or resolved dependency JARs for javac (offline only — tooling runs in background).
pub fn resolve_dependency_jars(project_root: &Path) -> Vec<PathBuf> {
    resolve_dependency_jars_cached(project_root)
}

/// Gradle, Maven, or plain-Java project root for a workspace file.
pub fn project_build_root(ws: &Path, from_path: &str) -> Result<Option<PathBuf>> {
    cached_gradle_root(ws, from_path)
}

/// Best FQCN to import for a simple type name (e.g. RestController).
pub fn import_fqcn_for_symbol(
    ws: &Path,
    from_path: &str,
    content: &str,
    symbol: &str,
) -> Result<Option<String>> {
    if symbol.is_empty() || !symbol.chars().next().is_some_and(|c| c.is_uppercase()) {
        return Ok(None);
    }

    if let Some(fqcn) = well_known_import(symbol) {
        if !content.contains(&format!("import {fqcn};")) {
            return Ok(Some(fqcn.to_string()));
        }
        return Ok(None);
    }

    let Some(root) = cached_gradle_root(ws, from_path)? else {
        return Ok(well_known_import(symbol).map(str::to_string));
    };
    let lookup = get_lookup(ws, &root)?;
    let imports = parse_imports_cached(&root, from_path, content);
    if imports.explicit.contains_key(symbol) {
        return Ok(None);
    }
    if resolve_type_fqcn(&lookup, symbol, &imports, &root).is_some() {
        return Ok(None);
    }

    let mut candidates: Vec<&IndexedSymbol> = lookup
        .types_named(symbol)
        .filter(|s| is_library_fqcn(&s.qualified))
        .collect();
    if candidates.is_empty() {
        return Ok(well_known_import(symbol).map(str::to_string));
    }
    if candidates.len() == 1 {
        return Ok(Some(candidates[0].qualified.clone()));
    }
    candidates.sort_by(|a, b| {
        import_match_priority(&a.qualified, &imports)
            .cmp(&import_match_priority(&b.qualified, &imports))
            .then_with(|| spring_priority(&a.qualified).cmp(&spring_priority(&b.qualified)))
    });
    Ok(Some(candidates[0].qualified.clone()))
}

fn well_known_import(symbol: &str) -> Option<&'static str> {
    WELL_KNOWN_JAVA_IMPORTS
        .iter()
        .find_map(|(name, fqcn)| (*name == symbol).then_some(*fqcn))
}

const WELL_KNOWN_JAVA_IMPORTS: &[(&str, &str)] = &[
    ("RestController", "org.springframework.web.bind.annotation.RestController"),
    ("Controller", "org.springframework.stereotype.Controller"),
    ("Service", "org.springframework.stereotype.Service"),
    ("Component", "org.springframework.stereotype.Component"),
    ("Repository", "org.springframework.stereotype.Repository"),
    ("Autowired", "org.springframework.beans.factory.annotation.Autowired"),
    ("Value", "org.springframework.beans.factory.annotation.Value"),
    ("RequestMapping", "org.springframework.web.bind.annotation.RequestMapping"),
    ("GetMapping", "org.springframework.web.bind.annotation.GetMapping"),
    ("PostMapping", "org.springframework.web.bind.annotation.PostMapping"),
    ("PutMapping", "org.springframework.web.bind.annotation.PutMapping"),
    ("DeleteMapping", "org.springframework.web.bind.annotation.DeleteMapping"),
    ("PatchMapping", "org.springframework.web.bind.annotation.PatchMapping"),
    ("PathVariable", "org.springframework.web.bind.annotation.PathVariable"),
    ("RequestParam", "org.springframework.web.bind.annotation.RequestParam"),
    ("RequestBody", "org.springframework.web.bind.annotation.RequestBody"),
    (
        "SpringBootApplication",
        "org.springframework.boot.autoconfigure.SpringBootApplication",
    ),
    ("ResponseEntity", "org.springframework.http.ResponseEntity"),
    ("HttpStatus", "org.springframework.http.HttpStatus"),
    ("Page", "org.springframework.data.domain.Page"),
    ("Pageable", "org.springframework.data.domain.Pageable"),
    ("PageRequest", "org.springframework.data.domain.PageRequest"),
    ("Sort", "org.springframework.data.domain.Sort"),
];

pub fn well_known_spring_data_simple_names() -> impl Iterator<Item = &'static str> {
    WELL_KNOWN_JAVA_IMPORTS
        .iter()
        .filter(|(_, fqcn)| fqcn.contains("org.springframework.data"))
        .map(|(name, _)| *name)
}

pub fn is_java_test_source_path(rel_path: &str) -> bool {
    let path = rel_path.replace('\\', "/");
    path.contains("/src/test/java/") || path.contains("/test/java/")
}

fn filter_existing_jars(jars: Vec<PathBuf>) -> Vec<PathBuf> {
    jars.into_iter().filter(|p| p.is_file()).collect()
}

fn filter_existing_classpath_entries(entries: Vec<PathBuf>) -> Vec<PathBuf> {
    entries
        .into_iter()
        .filter(|p| p.is_file() || p.is_dir())
        .collect()
}

fn dedupe_classpath_entries(entries: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for entry in entries {
        let key = entry
            .canonicalize()
            .unwrap_or_else(|_| entry.clone())
            .display()
            .to_string();
        if seen.insert(key) {
            out.push(entry);
        }
    }
    out
}

pub fn warm_index(ws: &Path) -> Result<WarmIndexStatus> {
    warm_index_with_progress(ws, None)
}

pub fn warm_index_with_progress(
    ws: &Path,
    progress: Option<Box<dyn Fn(&str, usize) + Send>>,
) -> Result<WarmIndexStatus> {
    let roots = find_all_index_roots(ws)?;
    if roots.is_empty() {
        return Ok(empty_warm_status());
    }

    let mut combined = WarmIndexStatus {
        indexed: false,
        project_root: None,
        symbol_count: 0,
        cached: true,
        dependency_jars: 0,
        source_jars: 0,
        jdk_sources: false,
        spring_symbols: 0,
        jdk_symbols: 0,
    };

    for root in roots {
        let cached = is_index_cached(ws, &root)?;
        let index = if cached {
            load_index(ws, &root)?
        } else {
            build_index(ws, &root, progress.as_ref())?
        };
        let meta = index_meta(&root);
        combined.indexed = true;
        combined.project_root = Some(index.project_root.clone());
        combined.symbol_count += index.symbols.len();
        combined.cached = combined.cached && cached;
        combined.dependency_jars += meta.dependency_jars;
        combined.source_jars += meta.source_jars;
        combined.jdk_sources = combined.jdk_sources || meta.jdk_sources;
        combined.spring_symbols += meta.spring_symbols;
        combined.jdk_symbols += meta.jdk_symbols;
    }

    Ok(combined)
}

/// Read index status from disk without building (for UI polling).
pub fn peek_index_status(ws: &Path) -> Result<WarmIndexStatus> {
    let roots = find_all_index_roots(ws)?;
    if roots.is_empty() {
        return Ok(empty_warm_status());
    }

    let mut combined = WarmIndexStatus {
        indexed: false,
        project_root: None,
        symbol_count: 0,
        cached: true,
        dependency_jars: 0,
        source_jars: 0,
        jdk_sources: false,
        spring_symbols: 0,
        jdk_symbols: 0,
    };

    for root in roots {
        let cached = is_index_cached(ws, &root)?;
        if let Some(index) = try_load_index(ws, &root)? {
            let meta = index_meta(&root);
            combined.indexed = true;
            combined.project_root = Some(index.project_root);
            combined.symbol_count += index.symbols.len();
            combined.cached = combined.cached && cached;
            combined.dependency_jars += meta.dependency_jars;
            combined.source_jars += meta.source_jars;
            combined.jdk_sources = combined.jdk_sources || meta.jdk_sources;
            combined.spring_symbols += meta.spring_symbols;
            combined.jdk_symbols += meta.jdk_symbols;
        }
    }

    Ok(combined)
}

/// True when a cached Java index should be fully rebuilt (missing types, empty deps, etc.).
/// Pending Gradle/Maven tooling alone does not require a rebuild — see `tooling_classpath_pending`.
pub fn java_index_needs_refresh(ws: &Path) -> bool {
    let Ok(roots) = find_all_index_roots(ws) else {
        return false;
    };
    for root in roots {
        if !is_build_tool_project_root(&root) {
            continue;
        }
        if super::java_ecosystem::project_declares_spring_data(&root)
            && index_missing_spring_data_domain(ws, &root)
        {
            return true;
        }
        if let Ok(peek) = peek_index_status(ws) {
            let markers = super::java_ecosystem::project_build_markers(&root);
            if markers.spring && peek.indexed && peek.dependency_jars == 0 {
                return true;
            }
        }
    }
    false
}

/// True when compile classpath still needs Gradle/Maven resolve (diagnostics may be incomplete).
pub fn tooling_classpath_pending(ws: &Path) -> bool {
    needs_any_tooling_classpath_resolve(ws)
}

fn index_missing_spring_data_domain(ws: &Path, root: &Path) -> bool {
    let Some(index) = try_load_index(ws, root).ok().flatten() else {
        return true;
    };
    !index
        .symbols
        .iter()
        .any(|s| s.qualified.starts_with("org.springframework.data.domain."))
}

pub fn search_indexed_classes(ws: &Path, query: &str, limit: usize) -> Result<Vec<ClassSearchHit>> {
    let roots = find_all_index_roots(ws)?;
    if roots.is_empty() {
        return Ok(Vec::new());
    }

    let mut scored: Vec<(u32, ClassSearchHit)> = Vec::new();
    for root in roots {
        let lookup = get_lookup(ws, &root)?;
        for sym in lookup.symbols.iter() {
            if sym.kind == "method" {
                continue;
            }
            let loc = to_location(ws, &root, sym);
            let path_norm = loc.path.replace('\\', "/");
            if query.trim().is_empty() && skip_indexed_class_on_empty_browse(&path_norm, &sym.qualified) {
                continue;
            }
            let Some(base) = class_name_match_score(query, &sym.name, &sym.qualified) else {
                continue;
            };
            let bonus = indexed_class_priority(&loc.path, &sym.qualified);
            scored.push((
                base + bonus,
                ClassSearchHit {
                    name: sym.name.clone(),
                    qualified: sym.qualified.clone(),
                    kind: sym.kind.clone(),
                    path: loc.path,
                    line: loc.line,
                    column: loc.column,
                },
            ));
        }
    }

    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.name.cmp(&b.1.name)));
    Ok(dedupe_indexed_classes(scored, limit))
}

fn skip_indexed_class_on_empty_browse(path_norm: &str, qualified: &str) -> bool {
    if path_norm.contains(".reaper/java-sources/jdk/") {
        return true;
    }
    if path_norm.contains("/org/springframework/") {
        return false;
    }
    if path_norm.contains(".reaper/") {
        return true;
    }
    qualified.starts_with("java.") || qualified.starts_with("jdk.")
}

fn indexed_class_priority(path: &str, qualified: &str) -> u32 {
    let path = path.replace('\\', "/");
    if path.contains(".reaper/java-sources/jdk/") || qualified.starts_with("java.") || qualified.starts_with("jdk.") {
        0
    } else if path.contains("/org/springframework/") || qualified.starts_with("org.springframework.") {
        120
    } else if path.contains("/src/") || !qualified.contains('.') {
        300
    } else if path.contains(".reaper/") {
        30
    } else {
        50
    }
}

fn dedupe_indexed_classes(scored: Vec<(u32, ClassSearchHit)>, limit: usize) -> Vec<ClassSearchHit> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for (_, hit) in scored {
        let key = format!("{}:{}:{}", hit.qualified, hit.path, hit.line);
        if seen.insert(key) {
            out.push(hit);
            if out.len() >= limit {
                break;
            }
        }
    }
    out
}

fn empty_warm_status() -> WarmIndexStatus {
    WarmIndexStatus {
        indexed: false,
        project_root: None,
        symbol_count: 0,
        cached: false,
        dependency_jars: 0,
        source_jars: 0,
        jdk_sources: false,
        spring_symbols: 0,
        jdk_symbols: 0,
    }
}

pub fn find_external_definition(
    ws: &Path,
    from_path: &str,
    line: u32,
    column: u32,
    content: &str,
) -> Result<Option<SymbolLocation>> {
    if !is_java_like(from_path) {
        return Ok(None);
    }

    let Some(root) = cached_gradle_root(ws, from_path)? else {
        return Ok(None);
    };

    let _ = ensure_navigation_sources(ws, &root);

    let cache_key = definition_cache_key(&root, from_path, line, column, content);
    if let Some(cached) = cached_definition(&cache_key) {
        return Ok(cached);
    }

    let hit = find_external_definition_inner(ws, &root, from_path, line, column, content)?;
    cache_definition(cache_key, hit.clone());
    Ok(hit)
}

fn find_external_definition_inner(
    ws: &Path,
    root: &Path,
    from_path: &str,
    line: u32,
    column: u32,
    content: &str,
) -> Result<Option<SymbolLocation>> {
    let lookup = get_lookup(ws, root)?;
    let symbol = match super::symbols::word_at(content, line, column) {
        Some(s) if !s.is_empty() && !super::symbols::is_keyword(&s) => s,
        _ => return Ok(None),
    };

    let imports = parse_imports_cached(root, from_path, content);

    if let Some(type_name) =
        super::symbols::java_member_qualifier(content, line, column, &symbol)
    {
        if let Some(fqcn) = resolve_type_fqcn(&lookup, &type_name, &imports, root) {
            if let Some(hit) = find_method_in_index(&lookup, &fqcn, &symbol) {
                return Ok(Some(to_location(ws, root, hit)));
            }
        }
    }

    if let Some(fqcn) = super::symbols::java_class_from_source_path(from_path) {
        if let Some(hit) = find_method_in_index(&lookup, &fqcn, &symbol) {
            return Ok(Some(to_location(ws, root, hit)));
        }
    }

    if let Some(loc) = resolve_imported_type(ws, root, &lookup, &symbol, &imports) {
        return Ok(Some(loc));
    }

    if let Some(fqcn) = resolve_type_fqcn(&lookup, &symbol, &imports, root) {
        if is_library_fqcn(&fqcn) {
            if let Some(loc) = resolve_type_by_fqcn(ws, root, &lookup, &fqcn, &symbol) {
                return Ok(Some(loc));
            }
        }
    }

    if symbol.chars().next().is_some_and(|c| c.is_uppercase()) {
        if let Some(loc) = fast_java_lang_location(ws, root, &symbol, &imports)? {
            return Ok(Some(loc));
        }
        if let Some(loc) = resolve_jdk_type_location(ws, root, &symbol, &imports)? {
            return Ok(Some(loc));
        }
    }

    let mut candidates: Vec<&IndexedSymbol> = lookup.types_named(&symbol).collect();

    if candidates.is_empty() {
        if let Some(fqcn) = import_fqcn_for_symbol(ws, from_path, content, &symbol)? {
            if let Some(loc) = resolve_type_by_fqcn(ws, root, &lookup, &fqcn, &symbol) {
                return Ok(Some(loc));
            }
        }
        let mut methods: Vec<&IndexedSymbol> = lookup.methods_named(&symbol).collect();
        if methods.is_empty() {
            return Ok(None);
        }
        methods.sort_by_key(|s| spring_priority(&s.qualified));
        return Ok(Some(to_location(ws, root, methods[0])));
    }

    if candidates.len() == 1 {
        return Ok(Some(to_location(ws, root, candidates[0])));
    }

    candidates.sort_by(|a, b| {
        import_match_priority(&a.qualified, &imports)
            .cmp(&import_match_priority(&b.qualified, &imports))
            .then_with(|| spring_priority(&a.qualified).cmp(&spring_priority(&b.qualified)))
    });
    Ok(Some(to_location(ws, root, candidates[0])))
}

pub fn java_completions(
    ws: &Path,
    from_path: &str,
    line: u32,
    column: u32,
    content: &str,
    prefix: &str,
    overlays: &[(String, String)],
) -> Result<Vec<CompletionItem>> {
    if !is_java_like(from_path) {
        return Ok(Vec::new());
    }

    let Some(root) = cached_gradle_root(ws, from_path)? else {
        return Ok(Vec::new());
    };

    let lookup = get_lookup(ws, &root)?;
    let at_annotation = is_annotation_context(content, line, column);
    let imports = parse_imports_cached(&root, from_path, content);
    let prefix = if prefix.is_empty() {
        super::symbols::word_at(content, line, column).unwrap_or_default()
    } else {
        prefix.to_string()
    };

    if super::symbols::is_java_import_line(content, line) {
        if let Some(import_prefix) = super::symbols::java_import_fqcn_prefix(content, line, column) {
            return Ok(import_fqcn_completions(ws, &root, &lookup, &import_prefix));
        }
    }

    if let Some((qualifier, member_prefix)) = super::symbols::java_dot_qualifier(content, line, column) {
        let member_items = member_completions_for_qualifier(
            ws,
            &root,
            from_path,
            &lookup,
            &imports,
            content,
            &qualifier,
            &member_prefix,
            overlays,
        )?;
        if !member_items.is_empty() {
            return Ok(member_items);
        }
    }

    if prefix.is_empty() && !at_annotation {
        if super::symbols::is_java_for_type_start(content, line, column) {
            let mut seen = HashSet::new();
            let mut items = Vec::new();
            for prim in [
                "int", "long", "boolean", "char", "byte", "short", "float", "double", "var",
                "String",
            ] {
                if seen.insert(prim.to_string()) {
                    items.push(CompletionItem {
                        label: prim.to_string(),
                        kind: "keyword".to_string(),
                        detail: Some("type".into()),
                        insert: None,
                        path: None,
                        line: None,
                        column: None,
                        documentation: None,
                    });
                }
            }
            for sym in lookup.types_matching_name_prefix("", 60) {
                if !seen.insert(sym.name.clone()) {
                    continue;
                }
                items.push(symbol_to_completion_item(ws, &root, sym, None));
                if items.len() >= 80 {
                    break;
                }
            }
            return Ok(items);
        }
        if super::symbols::is_java_for_iterable_context(content, line, column) {
            let vars = super::symbols::collect_java_scope_variables(content, line);
            let mut items = Vec::new();
            let mut seen = HashSet::new();
            for (name, ty) in &vars {
                if !super::symbols::is_java_iterable_type_hint(ty) {
                    continue;
                }
                if seen.insert(name.clone()) {
                    items.push(CompletionItem {
                        label: name.clone(),
                        kind: "variable".to_string(),
                        detail: Some(ty.clone()),
                        insert: None,
                        path: Some(from_path.to_string()),
                        line: None,
                        column: None,
                        documentation: None,
                    });
                }
            }
            if items.is_empty() {
                for (name, ty) in vars {
                    if seen.insert(name.clone()) {
                        items.push(CompletionItem {
                            label: name,
                            kind: "variable".to_string(),
                            detail: Some(ty),
                            insert: None,
                            path: Some(from_path.to_string()),
                            line: None,
                            column: None,
                            documentation: None,
                        });
                    }
                }
            }
            return Ok(items);
        }
        return Ok(Vec::new());
    }

    let type_preferred = prefix.chars().next().is_some_and(|c| c.is_uppercase())
        || super::symbols::is_java_type_reference_context(content, line, column);

    if type_preferred && !prefix.is_empty() {
        let types = lookup.types_matching_name_prefix(&prefix, 80);
        if !types.is_empty() {
            return Ok(types
                .into_iter()
                .map(|sym| symbol_to_completion_item(ws, &root, sym, None))
                .collect());
        }
    }

    let prefix_lower = prefix.to_lowercase();

    let mut seen = HashSet::new();
    let mut items = Vec::new();

    for sym in lookup.symbols.iter() {
        if at_annotation && !is_annotation_index_symbol(sym) {
            continue;
        }
        if type_preferred && sym.kind == "method" {
            continue;
        }
        if !prefix.is_empty()
            && !sym.name.to_lowercase().starts_with(&prefix_lower)
            && !sym.qualified.to_lowercase().starts_with(&prefix_lower)
        {
            continue;
        }
        if !seen.insert(sym.qualified.clone()) {
            continue;
        }
        items.push(symbol_to_completion_item(ws, &root, sym, None));
        if items.len() >= 80 {
            break;
        }
    }

    items.sort_by(|a, b| {
        let type_rank = |k: &str| if k == "method" { 1 } else { 0 };
        type_rank(&a.kind)
            .cmp(&type_rank(&b.kind))
            .then_with(|| {
                let pa = spring_priority(a.detail.as_deref().unwrap_or(""));
                let pb = spring_priority(b.detail.as_deref().unwrap_or(""));
                pa.cmp(&pb)
            })
            .then_with(|| a.label.len().cmp(&b.label.len()))
            .then_with(|| a.label.cmp(&b.label))
    });
    Ok(items)
}

fn symbol_to_completion_item(
    ws: &Path,
    root: &Path,
    sym: &IndexedSymbol,
    insert: Option<String>,
) -> CompletionItem {
    CompletionItem {
        label: sym.name.clone(),
        kind: sym.kind.clone(),
        detail: Some(sym.qualified.clone()),
        insert,
        path: Some(normalize_index_path(ws, root, &sym.path)),
        line: Some(sym.line),
        column: Some(sym.column),
        documentation: None,
    }
}

fn enrich_java_completion_from_source(item: &mut CompletionItem, content: &str) {
    let Some(line) = item.line else {
        return;
    };
    let line_idx = line.saturating_sub(1) as usize;
    let Some(source_line) = content.lines().nth(line_idx) else {
        return;
    };

    if let Some(sig) = super::symbols::java_member_signature_on_line(source_line, &item.label) {
        item.detail = Some(sig);
    } else if item.kind == "field" {
        let trimmed = source_line.split("//").next().unwrap_or(source_line).trim();
        if !trimmed.is_empty() {
            item.detail = Some(trimmed.to_string());
        }
    }

    if item.kind == "method" || item.kind == "field" {
        item.documentation = super::symbols::java_javadoc_before_line(content, line);
    }
}

fn read_source_for_completion(
    ws: &Path,
    root: &Path,
    rel_path: &str,
    cache: &mut HashMap<String, String>,
) -> Option<String> {
    if let Some(content) = cache.get(rel_path) {
        return Some(content.clone());
    }
    let mut candidates = vec![ws.join(rel_path)];
    if let Ok(stripped) = Path::new(rel_path).strip_prefix("./") {
        candidates.push(ws.join(stripped));
    }
    candidates.push(root.join(rel_path));
    for abs in candidates {
        if abs.is_file() {
            if let Ok(content) = std::fs::read_to_string(&abs) {
                cache.insert(rel_path.to_string(), content.clone());
                return Some(content);
            }
        }
    }
    None
}

fn enrich_completion_items_from_sources(ws: &Path, root: &Path, items: &mut [CompletionItem]) {
    let mut cache = HashMap::new();
    for item in items.iter_mut() {
        let Some(path) = item.path.clone() else {
            continue;
        };
        let Some(content) = read_source_for_completion(ws, root, &path, &mut cache) else {
            continue;
        };
        enrich_java_completion_from_source(item, &content);
    }
}

fn import_fqcn_completions(
    ws: &Path,
    root: &Path,
    lookup: &IndexLookup,
    fqcn_prefix: &str,
) -> Vec<CompletionItem> {
    let types = lookup.types_matching_fqcn_prefix(fqcn_prefix, 80);
    let mut seen = HashSet::new();
    let mut items = Vec::new();
    for sym in types {
        if !seen.insert(sym.qualified.clone()) {
            continue;
        }
        let label = if fqcn_prefix.contains('.') {
            sym.qualified.clone()
        } else {
            sym.name.clone()
        };
        items.push(CompletionItem {
            label,
            kind: sym.kind.clone(),
            detail: Some(sym.qualified.clone()),
            insert: Some(sym.qualified.clone()),
            path: Some(normalize_index_path(ws, root, &sym.path)),
            line: Some(sym.line),
            column: Some(sym.column),
            documentation: None,
        });
    }

    // Suggest next package segment(s) while typing an import, e.g. org.springframework.data. → domain
    if fqcn_prefix.contains('.') {
        let base = if fqcn_prefix.ends_with('.') {
            fqcn_prefix.to_string()
        } else if fqcn_prefix
            .rsplit('.')
            .next()
            .is_some_and(|seg| seg.chars().next().is_some_and(|c| c.is_ascii_uppercase()))
        {
            // Cursor on a type name — no package segments to add.
            String::new()
        } else {
            format!("{fqcn_prefix}.")
        };
        if !base.is_empty() {
            let mut packages = HashSet::new();
            for sym in lookup.types_matching_fqcn_prefix(&base, 400) {
                let Some(rest) = sym.qualified.strip_prefix(&base) else {
                    continue;
                };
                let Some(seg) = rest.split('.').next() else {
                    continue;
                };
                if seg.is_empty() || seg.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
                    continue;
                }
                packages.insert(seg.to_string());
            }
            for pkg in packages {
                let insert = format!("{base}{pkg}.");
                if seen.insert(insert.clone()) {
                    items.push(CompletionItem {
                        label: pkg.clone(),
                        kind: "package".into(),
                        detail: Some(insert.trim_end_matches('.').to_string()),
                        insert: Some(insert),
                        path: None,
                        line: None,
                        column: None,
                        documentation: None,
                    });
                }
            }
        }
    }

    items.sort_by(|a, b| a.label.cmp(&b.label));
    items
}

fn member_completions_for_qualifier(
    ws: &Path,
    root: &Path,
    from_path: &str,
    lookup: &IndexLookup,
    imports: &ImportMap,
    content: &str,
    qualifier: &str,
    member_prefix: &str,
    overlays: &[(String, String)],
) -> Result<Vec<CompletionItem>> {
    let inferred_type = if qualifier == "this" || qualifier == "super" {
        None
    } else {
        super::symbols::infer_java_receiver_type_from_expr(content, qualifier)
    };
    let is_array_type = inferred_type
        .as_ref()
        .is_some_and(|t| t.contains('['));

    let fqcn = if qualifier == "this" || qualifier == "super" {
        super::symbols::java_class_from_source_path(from_path)
    } else {
        resolve_receiver_type_fqcn(ws, root, lookup, content, qualifier, imports)
    };

    let mut seen = HashSet::new();
    let mut items = Vec::new();

    if let Some(fqcn) = &fqcn {
        for sym in lookup.members_for_type(fqcn, member_prefix, 80) {
            if !seen.insert(sym.name.clone()) {
                continue;
            }
            let mut item = symbol_to_completion_item(ws, root, sym, None);
            if item.detail.is_none() {
                item.detail = Some(fqcn.clone());
            }
            items.push(item);
        }

        if items.len() < 8 || !is_library_fqcn(fqcn) {
            for item in members_from_type_source(ws, root, fqcn, member_prefix, overlays)? {
                if seen.insert(item.label.clone()) {
                    items.push(item);
                }
                if items.len() >= 80 {
                    break;
                }
            }
        }
    }

    if is_array_type {
        push_builtin_array_members(&mut items, &mut seen, member_prefix);
        if let Some(obj_fqcn) = resolve_type_fqcn(lookup, "Object", imports, root) {
            for sym in lookup.members_for_type(&obj_fqcn, member_prefix, 40) {
                if !seen.insert(sym.name.clone()) {
                    continue;
                }
                let mut item = symbol_to_completion_item(ws, root, sym, None);
                if item.detail.is_none() {
                    item.detail = Some(obj_fqcn.clone());
                }
                items.push(item);
                if items.len() >= 80 {
                    break;
                }
            }
            if items.len() < 12 {
                for item in members_from_type_source(ws, root, &obj_fqcn, member_prefix, overlays)? {
                    if seen.insert(item.label.clone()) {
                        items.push(item);
                    }
                }
            }
        }
    }

    for local in super::symbols::member_completions_from_content(content, qualifier, member_prefix, from_path) {
        if seen.insert(local.label.clone()) {
            items.push(local);
        }
        if items.len() >= 80 {
            break;
        }
    }

    push_known_jdk_static_members(&mut items, &mut seen, qualifier, member_prefix);

    enrich_completion_items_from_sources(ws, root, &mut items);

    Ok(items)
}

fn member_source_dirs(gradle_root: &Path) -> Vec<PathBuf> {
    let mut dirs = java_project_source_dirs(gradle_root);
    if dirs.is_empty() {
        for rel in super::java_sources::discovery_suffixes() {
            let p = gradle_root.join(rel);
            if p.is_dir() {
                dirs.push(p);
            }
        }
        let plain = gradle_root.join("src");
        if plain.is_dir() {
            dirs.push(plain);
        }
    }
    dirs.extend(library_source_dirs(gradle_root));
    dirs.extend(cached_project_source_dirs(gradle_root));
    let jdk = reaper_dir(gradle_root).join("java-sources/jdk");
    if jdk.is_dir() {
        dirs.push(jdk);
    }
    dirs
}

/// Every Maven/Gradle source root under a project (multi-module safe).
fn java_project_source_dirs(project_root: &Path) -> Vec<PathBuf> {
    super::java_sources::discover_source_prefixes(project_root)
        .into_iter()
        .map(|prefix| project_root.join(prefix))
        .collect()
}

fn read_project_java_source(
    ws: &Path,
    rel_path: &str,
    overlays: &[(String, String)],
) -> Option<String> {
    for (path, content) in overlays {
        if path == rel_path {
            return Some(content.clone());
        }
    }
    std::fs::read_to_string(ws.join(rel_path)).ok()
}

/// Re-index symbols for one saved `.java` file so completions/diagnostics see new methods immediately.
pub fn patch_java_index_file(ws: &Path, rel_path: &str, content: &str) -> Result<()> {
    if !rel_path.ends_with(".java") || rel_path.starts_with(".reaper/") {
        return Ok(());
    }
    let _ = super::safe_join(ws, rel_path)?;
    let Some(root) = cached_gradle_root(ws, rel_path)? else {
        return Ok(());
    };
    let index_path = reaper_dir(&root).join("java-index.json");
    if !index_path.is_file() {
        return Ok(());
    }
    let text = std::fs::read_to_string(&index_path)?;
    let mut index: JavaIndex = serde_json::from_str(&text)?;
    index
        .symbols
        .retain(|sym| sym.path != rel_path && sym.path != format!(".reaper/classpath-jar/{rel_path}"));
    index_java_content(
        content,
        rel_path,
        should_index_methods(rel_path),
        &mut index.symbols,
    );
    std::fs::write(&index_path, serde_json::to_string(&index)?)?;
    invalidate_lookup_cache(&root);
    Ok(())
}

fn members_from_type_source(
    ws: &Path,
    gradle_root: &Path,
    fqcn: &str,
    member_prefix: &str,
    overlays: &[(String, String)],
) -> Result<Vec<CompletionItem>> {
    let dirs = member_source_dirs(gradle_root);
    let Some(source_path) = find_java_source_for_fqcn(ws, gradle_root, &dirs, fqcn) else {
        return Ok(Vec::new());
    };
    let rel = rel_path_for(ws, &source_path).unwrap_or_else(|_| {
        source_path
            .strip_prefix(ws)
            .unwrap_or(&source_path)
            .to_string_lossy()
            .replace('\\', "/")
    });
    let content = match read_project_java_source(ws, &rel, overlays) {
        Some(text) => text,
        None => std::fs::read_to_string(&source_path)
            .with_context(|| format!("read source for {fqcn}"))?,
    };
    let mut symbols = Vec::new();
    index_java_content(&content, &rel, true, &mut symbols);

    let member_prefix_lower = member_prefix.to_lowercase();
    let mut items = Vec::new();
    let qual_prefix = format!("{fqcn}.");
    for sym in &symbols {
        if !sym.qualified.starts_with(&qual_prefix) || sym.kind == "class" {
            continue;
        }
        if !member_prefix.is_empty() && !sym.name.to_lowercase().starts_with(&member_prefix_lower) {
            continue;
        }
        let mut item = CompletionItem {
            label: sym.name.clone(),
            kind: sym.kind.clone(),
            detail: Some(fqcn.to_string()),
            insert: None,
            path: Some(normalize_index_path(ws, gradle_root, &sym.path)),
            line: Some(sym.line),
            column: Some(sym.column),
            documentation: None,
        };
        enrich_java_completion_from_source(&mut item, &content);
        items.push(item);
        if items.len() >= 80 {
            break;
        }
    }
    Ok(items)
}

fn resolve_receiver_type_fqcn(
    ws: &Path,
    root: &Path,
    lookup: &IndexLookup,
    content: &str,
    qualifier: &str,
    imports: &ImportMap,
) -> Option<String> {
    let qualifier = qualifier.trim();
    if qualifier.is_empty() {
        return None;
    }

    if let Some((parent, member)) = qualifier.rsplit_once('.') {
        let parent = parent.trim();
        let member = member.trim();
        if !parent.is_empty() && !member.is_empty() {
            let parent_fqcn = resolve_receiver_type_fqcn(ws, root, lookup, content, parent, imports)?;
            let field_type = field_type_name_from_class_source(ws, root, &parent_fqcn, member)?;
            return resolve_type_fqcn(lookup, &field_type, imports, root);
        }
    }

    if let Some(type_name) = super::symbols::infer_java_receiver_type_from_expr(content, qualifier) {
        let resolved = resolve_type_fqcn(lookup, &type_name, imports, root);
        if resolved.is_some() {
            return resolved;
        }
        let base = type_name.trim_end_matches("[]").trim();
        if base != type_name.as_str() {
            return resolve_type_fqcn(lookup, base, imports, root);
        }
    }

    resolve_type_fqcn(lookup, qualifier, imports, root)
}

fn field_type_name_from_class_source(
    ws: &Path,
    gradle_root: &Path,
    fqcn: &str,
    field: &str,
) -> Option<String> {
    let dirs = member_source_dirs(gradle_root);
    let Some(source_path) = find_java_source_for_fqcn(ws, gradle_root, &dirs, fqcn) else {
        return None;
    };
    let content = std::fs::read_to_string(&source_path).ok()?;
    for line in content.lines() {
        if let Some(type_name) = java_field_type_on_line(line, field) {
            return Some(type_name);
        }
    }
    None
}

fn java_field_type_on_line(line: &str, field: &str) -> Option<String> {
    let trimmed = line.split("//").next()?.trim();
    if trimmed.is_empty()
        || trimmed.contains('(')
        || trimmed.ends_with('{')
        || trimmed.ends_with('}')
        || !trimmed.ends_with(';')
    {
        return None;
    }
    if trimmed.starts_with("import ")
        || trimmed.starts_with("package ")
        || trimmed.starts_with('@')
    {
        return None;
    }
    let before_assign = trimmed.trim_end_matches(';').split('=').next()?.trim();
    let parts: Vec<&str> = before_assign.split_whitespace().collect();
    if parts.len() < 2 || *parts.last()? != field {
        return None;
    }
    const MODIFIERS: &[&str] = &[
        "public", "private", "protected", "static", "final", "volatile", "transient", "native",
        "synchronized", "abstract", "strictfp",
    ];
    let type_tokens: Vec<&str> = parts[..parts.len() - 1]
        .iter()
        .filter(|t| !MODIFIERS.contains(&t.to_lowercase().as_str()))
        .copied()
        .collect();
    if type_tokens.is_empty() {
        return None;
    }
    let type_name = type_tokens.join("");
    if type_name.is_empty() || super::symbols::is_keyword(&type_name) {
        return None;
    }
    Some(type_name)
}

fn push_builtin_array_members(
    items: &mut Vec<CompletionItem>,
    seen: &mut HashSet<String>,
    member_prefix: &str,
) {
    let prefix_lower = member_prefix.to_lowercase();
    for (name, kind) in [("length", "field"), ("clone", "method")] {
        if !member_prefix.is_empty() && !name.to_lowercase().starts_with(&prefix_lower) {
            continue;
        }
        if seen.insert(name.to_string()) {
            items.push(CompletionItem {
                label: name.to_string(),
                kind: kind.to_string(),
                detail: Some("array".into()),
                insert: None,
                path: None,
                line: None,
                column: None,
                documentation: None,
            });
        }
    }
}

fn push_known_jdk_static_members(
    items: &mut Vec<CompletionItem>,
    seen: &mut HashSet<String>,
    qualifier: &str,
    member_prefix: &str,
) {
    static KNOWN: &[(&str, &[(&str, &str)])] = &[
        ("System", &[("out", "field"), ("in", "field"), ("err", "field")]),
        ("Math", &[("PI", "field"), ("E", "field")]),
    ];
    let base = qualifier.rsplit('.').next().unwrap_or(qualifier).trim();
    let prefix_lower = member_prefix.to_lowercase();
    for (class, members) in KNOWN {
        if base != *class {
            continue;
        }
        for (name, kind) in *members {
            if !member_prefix.is_empty() && !name.to_lowercase().starts_with(&prefix_lower) {
                continue;
            }
            if seen.insert(name.to_string()) {
                items.push(CompletionItem {
                    label: name.to_string(),
                    kind: kind.to_string(),
                    detail: Some(format!("{qualifier}.{name}")),
                    insert: None,
                    path: None,
                    line: None,
                    column: None,
                    documentation: None,
                });
            }
        }
        break;
    }
}

fn is_index_cached(ws: &Path, gradle_root: &Path) -> Result<bool> {
    let cache = reaper_dir(gradle_root).join("java-index.json");
    if !cache.is_file() {
        return Ok(false);
    }
    let text = std::fs::read_to_string(&cache)?;
    let index: JavaIndex = serde_json::from_str(&text)?;
    index_fresh(ws, gradle_root, &index)
}

fn load_index(ws: &Path, gradle_root: &Path) -> Result<JavaIndex> {
    let lookup = get_lookup(ws, gradle_root)?;
    Ok(JavaIndex {
        project_root: lookup.project_root.clone(),
        symbols: lookup.symbols.clone(),
    })
}

fn try_load_index(ws: &Path, gradle_root: &Path) -> Result<Option<JavaIndex>> {
    let cache = reaper_dir(gradle_root).join("java-index.json");
    if !cache.is_file() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(&cache)?;
    let index: JavaIndex = serde_json::from_str(&text)?;
    if index_fresh(ws, gradle_root, &index)? {
        return Ok(Some(index));
    }
    // Use a stale index while a background rebuild runs so navigation keeps working.
    if !index.symbols.is_empty() {
        return Ok(Some(index));
    }
    Ok(None)
}

fn empty_index(ws: &Path, gradle_root: &Path) -> JavaIndex {
    JavaIndex {
        project_root: rel_path_for(ws, gradle_root).unwrap_or_default(),
        symbols: Vec::new(),
    }
}

fn index_fresh(ws: &Path, gradle_root: &Path, index: &JavaIndex) -> Result<bool> {
    let project_root = rel_path_for(ws, gradle_root)?;
    if index.project_root != project_root && !index.project_root.is_empty() {
        return Ok(false);
    }

    let stamp_path = reaper_dir(gradle_root).join("classpath.stamp");
    if !stamp_path.is_file() {
        return Ok(false);
    }
    let stamp = std::fs::read_to_string(&stamp_path).unwrap_or_default();
    let current = classpath_stamp(gradle_root)?;
    if stamp != current {
        return Ok(false);
    }

    if super::gradle::is_spring_boot_project(gradle_root)
        || super::maven::is_spring_boot_project(gradle_root)
    {
        let meta = index_meta(gradle_root);
        if meta.dependency_jars == 0 {
            return Ok(false);
        }
    }

    if index_meta(gradle_root).index_version < INDEX_VERSION {
        return Ok(false);
    }

    if needs_tooling_classpath_resolve(gradle_root) {
        return Ok(false);
    }

    let meta = index_meta(gradle_root);
    if meta.dependency_jars > 0 && meta.source_jars * 2 < meta.dependency_jars {
        return Ok(false);
    }

    Ok(true)
}

fn classpath_stamp(gradle_root: &Path) -> Result<String> {
    let mut parts = Vec::new();
    if super::maven::is_maven_project_root(gradle_root) {
        parts.extend(super::maven::classpath_stamp_parts(gradle_root)?);
    } else {
        for name in [
            "build.gradle",
            "build.gradle.kts",
            "settings.gradle",
            "settings.gradle.kts",
            "gradle.properties",
        ] {
            let path = gradle_root.join(name);
            if path.is_file() {
                let meta = std::fs::metadata(&path)?;
                parts.push(format!(
                    "{name}:{}:{}",
                    meta.len(),
                    meta
                        .modified()
                        .ok()
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| d.as_nanos())
                        .unwrap_or(0)
                ));
            }
        }
    }
    if let Ok(jdk) = toolchain_java_home() {
        parts.push(format!("jdk:{}", jdk.display()));
    }
    Ok(parts.join("|"))
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct IndexMeta {
    dependency_jars: usize,
    source_jars: usize,
    jdk_sources: bool,
    spring_symbols: usize,
    jdk_symbols: usize,
    #[serde(default)]
    index_version: u32,
}

const META_PATH: &str = "java-index-meta.json";

fn index_meta(gradle_root: &Path) -> IndexMeta {
    reaper_dir(gradle_root)
        .join("java-index-meta.json")
        .is_file()
        .then(|| std::fs::read_to_string(reaper_dir(gradle_root).join("java-index-meta.json")).ok())
        .flatten()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn write_index_meta(gradle_root: &Path, meta: &IndexMeta) -> Result<()> {
    std::fs::create_dir_all(reaper_dir(gradle_root))?;
    std::fs::write(
        reaper_dir(gradle_root).join("java-index-meta.json"),
        serde_json::to_string_pretty(meta)?,
    )?;
    Ok(())
}

fn build_index(
    ws: &Path,
    gradle_root: &Path,
    progress: Option<&Box<dyn Fn(&str, usize) + Send>>,
) -> Result<JavaIndex> {
    fn report(progress: Option<&Box<dyn Fn(&str, usize) + Send>>, phase: &str, count: usize) {
        if let Some(cb) = progress {
            cb(phase, count);
        }
    }

    invalidate_lookup_cache(gradle_root);
    let project_root = rel_path_for(ws, gradle_root)?;
    report(progress, "classpath", 0);
    let classpath = resolve_classpath_for_index(gradle_root);
    if classpath.jars.is_empty() {
        tracing::warn!(
            "No dependency JARs resolved for {} — indexing project sources and JDK only",
            gradle_root.display()
        );
    } else if !classpath.log.is_empty() {
        tracing::info!("Classpath for {}: {}", gradle_root.display(), classpath.log);
    }
    if classpath.source_jars.is_empty() && !classpath.jars.is_empty() {
        tracing::warn!(
            "No source JARs for {} — dependency member completion may be limited",
            gradle_root.display()
        );
    }

    report(progress, "sources", 0);
    let (source_dirs, jdk_sources) =
        materialize_sources(ws, gradle_root, &classpath.jars, &classpath.source_jars, progress)?;
    if !classpath.jars.is_empty() {
        if let Err(e) = super::spring_props::build_index(ws, gradle_root, &classpath.jars) {
            tracing::warn!("Spring properties index failed for {}: {e:#}", gradle_root.display());
        }
    }

    let mut symbols = Vec::new();
    report(progress, "indexing", 0);
    // Only index JDK sources from materialized dirs — dependency *-sources.jar stay on disk
    // for go-to-definition; library types are indexed quickly from .class files below.
    for dir in &source_dirs {
        if is_dependency_sources_dir(dir) {
            continue;
        }
        index_java_dir(ws, dir, &mut symbols, progress)?;
        report(progress, "indexing", symbols.len());
    }

    index_all_java_source_trees(ws, gradle_root, &mut symbols, progress)?;

    for dir in &classpath.project_source_dirs {
        if dir.is_dir() {
            index_java_dir(ws, dir, &mut symbols, progress)?;
            report(progress, "indexing", symbols.len());
        }
    }

    let spring_from_sources = symbols
        .iter()
        .filter(|s| {
            s.qualified.starts_with("org.springframework.")
                || s.path.contains("/org/springframework/")
        })
        .count();
    if !classpath.jars.is_empty() {
        report(progress, "jar-index", symbols.len());
        index_jar_classpath_fallback(
            ws,
            gradle_root,
            &classpath.jars,
            &source_dirs,
            &mut symbols,
            progress,
        )?;
    } else {
        tracing::info!(
            "Skipping JAR classpath fallback for {} ({} symbols, {} Spring from sources, no JARs)",
            gradle_root.display(),
            symbols.len(),
            spring_from_sources
        );
    }
    if !classpath.classes_dirs.is_empty() {
        report(progress, "jar-index", symbols.len());
        index_classes_dirs_fallback(
            ws,
            gradle_root,
            &classpath.classes_dirs,
            &mut symbols,
            progress,
        )?;
    }
    report(progress, "indexing", symbols.len());

    symbols.sort_by(|a, b| a.qualified.cmp(&b.qualified));

    let spring_symbols = symbols
        .iter()
        .filter(|s| {
            s.qualified.starts_with("org.springframework.")
                || s.path.contains("/org/springframework/")
        })
        .count();
    let jdk_symbols = symbols
        .iter()
        .filter(|s| {
            s.qualified.starts_with("java.") || s.path.contains(".reaper/java-sources/jdk/")
        })
        .count();

    let symbol_count = symbols.len();
    report(progress, "writing", symbol_count);

    let index = JavaIndex {
        project_root,
        symbols,
    };

    std::fs::create_dir_all(reaper_dir(gradle_root))?;
    if !classpath.jars.is_empty() {
        save_classpath_jars_cache(gradle_root, &classpath.jars)?;
    }
    if !classpath.classes_dirs.is_empty() || !classpath.project_source_dirs.is_empty() {
        let _ = save_project_classpath_outputs(
            gradle_root,
            &paths_to_outputs(&classpath.classes_dirs, &classpath.project_source_dirs),
        );
    }
    std::fs::write(
        reaper_dir(gradle_root).join("java-index.json"),
        serde_json::to_string(&index)?,
    )?;
    std::fs::write(
        reaper_dir(gradle_root).join("classpath.stamp"),
        classpath_stamp(gradle_root)?,
    )?;
    write_index_meta(
        gradle_root,
        &IndexMeta {
            dependency_jars: classpath.jars.len(),
            source_jars: classpath.source_jars.len(),
            jdk_sources,
            spring_symbols,
            jdk_symbols,
            index_version: INDEX_VERSION,
        },
    )?;

    if spring_symbols == 0 && super::gradle::is_spring_boot_project(gradle_root) {
        tracing::warn!(
            "Spring Boot project at {} indexed with 0 Spring symbols — run ./gradlew compileJava or check Gradle/network",
            gradle_root.display()
        );
    }
    if !jdk_sources {
        tracing::warn!(
            "JDK sources not found for {} — install a full JDK (not JRE) for java.* navigation",
            gradle_root.display()
        );
    }

    invalidate_lookup_cache(gradle_root);

    if index.symbols.is_empty() {
        bail!(
            "Java index is empty for {} — install a full JDK in Settings → Java",
            gradle_root.display()
        );
    }

    Ok(index)
}

fn materialize_sources(
    ws: &Path,
    gradle_root: &Path,
    jars: &[PathBuf],
    source_jars: &[PathBuf],
    progress: Option<&Box<dyn Fn(&str, usize) + Send>>,
) -> Result<(Vec<PathBuf>, bool)> {
    let dest_root = reaper_dir(gradle_root).join("java-sources");
    std::fs::create_dir_all(&dest_root)?;

    let mut dirs = Vec::new();
    let mut extracted = HashSet::new();
    let mut to_extract: Vec<PathBuf> = source_jars.to_vec();
    for jar in jars {
        let key = jar.to_string_lossy().to_string();
        if extracted.contains(&key) {
            continue;
        }
        if let Some(sources) = find_sources_jar(jar) {
            to_extract.push(sources);
        }
    }
    let source_total = to_extract.len().max(1);

    for (idx, sources) in to_extract.iter().enumerate() {
        if let Some(dir) = extract_sources_jar(ws, &dest_root.join("deps"), sources, &mut extracted)? {
            dirs.push(dir);
        }
        if let Some(cb) = progress {
            cb("extracting-sources", (idx + 1) * 1000 / source_total);
        }
    }

    let mut jdk_sources = false;
    if let Some(jdk_dir) = materialize_jdk_sources(&dest_root.join("jdk"))? {
        jdk_sources = true;
        dirs.push(jdk_dir);
    }

    Ok((dirs, jdk_sources))
}

/// Extract JDK + dependency *-sources.jar on demand for go-to-definition (no Maven/Gradle run).
fn ensure_navigation_sources(ws: &Path, project_root: &Path) -> Result<()> {
    let jdk_dest = reaper_dir(project_root).join("java-sources/jdk");
    if !jdk_dest.join(".extracted").is_file() {
        let _ = materialize_jdk_sources(&jdk_dest);
    }

    let deps_root = reaper_dir(project_root).join("java-sources/deps");
    let has_deps = deps_root.is_dir()
        && std::fs::read_dir(&deps_root)
            .ok()
            .and_then(|mut it| it.next())
            .is_some();
    if has_deps {
        return Ok(());
    }

    let mut jars = resolve_dependency_tree_jars(project_root, true);
    if jars.is_empty() {
        jars = cached_classpath_jars(project_root);
    }
    if jars.is_empty() {
        return Ok(());
    }

    let source_jars = discover_source_jars_for_jars(&jars);
    let _ = materialize_sources(ws, project_root, &jars, &source_jars, None);
    invalidate_library_source_dirs_cache(project_root);
    Ok(())
}

fn invalidate_library_source_dirs_cache(project_root: &Path) {
    let key = project_root
        .canonicalize()
        .unwrap_or_else(|_| project_root.to_path_buf())
        .display()
        .to_string();
    if let Ok(mut guard) = LIBRARY_SOURCE_DIRS_CACHE.lock() {
        guard.remove(&key);
    }
}

fn extract_sources_jar(
    ws: &Path,
    dest_root: &Path,
    sources: &Path,
    extracted: &mut HashSet<String>,
) -> Result<Option<PathBuf>> {
    if !sources.is_file() {
        return Ok(None);
    }
    let key = sources.to_string_lossy().to_string();
    if !extracted.insert(key) {
        return Ok(None);
    }

    let name = sources
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("lib")
        .trim_end_matches("-sources")
        .replace('.', "_");
    let out = dest_root.join(&name);
    if !out.join(".extracted").is_file() {
        let _ = std::fs::remove_dir_all(&out);
        extract_zip(sources, &out)?;
        std::fs::write(out.join(".extracted"), sources.to_string_lossy().as_ref())?;
    }
    let _ = ws;
    Ok(Some(out))
}

#[derive(Debug, Default)]
struct GradleClasspath {
    jars: Vec<PathBuf>,
    source_jars: Vec<PathBuf>,
    classes_dirs: Vec<PathBuf>,
    project_source_dirs: Vec<PathBuf>,
    log: String,
}

fn gradle_classpath_from_tooling_cache(project_root: &Path) -> Option<GradleClasspath> {
    if !tooling_classpath_resolved(project_root) {
        return None;
    }
    let jars = cached_classpath_jars(project_root);
    if jars.is_empty() {
        return None;
    }
    let source_jars = discover_source_jars_for_jars(&jars);
    let (classes_dirs, project_source_dirs) =
        outputs_to_paths(&load_project_classpath_outputs(project_root));
    Some(GradleClasspath {
        jars,
        source_jars,
        classes_dirs,
        project_source_dirs,
        log: "from Gradle/Maven compile classpath cache".into(),
    })
}

fn persist_tooling_classpath(project_root: &Path, cp: &GradleClasspath, preserve_index: bool) -> Result<()> {
    if cp.jars.is_empty() && cp.classes_dirs.is_empty() && cp.project_source_dirs.is_empty() {
        return Ok(());
    }
    if !cp.jars.is_empty() {
        let jars = merge_with_build_file_dependency_tree(project_root, &cp.jars, true);
        save_classpath_jars_cache_pub(project_root, &jars)?;
    }
    if !cp.classes_dirs.is_empty() || !cp.project_source_dirs.is_empty() {
        save_project_classpath_outputs(
            project_root,
            &paths_to_outputs(&cp.classes_dirs, &cp.project_source_dirs),
        )?;
    }
    mark_tooling_classpath_done(project_root)?;
    if preserve_index {
        invalidate_lookup_cache(project_root);
    } else {
        invalidate_root_index_cache(project_root);
    }
    Ok(())
}

fn has_materialized_java_index(project_root: &Path) -> bool {
    let path = reaper_dir(project_root).join(INDEX_PATH);
    path.is_file()
        && std::fs::metadata(&path)
            .map(|m| m.len() > 2)
            .unwrap_or(false)
}

fn try_resolve_classpath_via_tooling(
    project_root: &Path,
    progress: IndexProgress,
) -> Option<GradleClasspath> {
    try_resolve_classpath_via_tooling_inner(
        project_root,
        has_materialized_java_index(project_root),
        progress,
    )
}

fn try_resolve_classpath_via_tooling_inner(
    project_root: &Path,
    preserve_index: bool,
    progress: IndexProgress,
) -> Option<GradleClasspath> {
    if !index_build_tooling_enabled() || !is_build_tool_project_root(project_root) {
        return None;
    }
    tracing::info!(
        "Resolving compile classpath via {} for {}",
        if super::maven::is_maven_project_root(project_root) {
            "Maven"
        } else {
            "Gradle"
        },
        project_root.display()
    );
    match resolve_classpath_via_tooling_full(project_root, progress) {
        Ok(cp) if !cp.jars.is_empty() || !cp.classes_dirs.is_empty() || !cp.project_source_dirs.is_empty() => {
            if let Err(e) = persist_tooling_classpath(project_root, &cp, preserve_index) {
                tracing::warn!(
                    "Failed to persist tooling classpath for {}: {e:#}",
                    project_root.display()
                );
            }
            tracing::info!(
                "Resolved {} JARs via tooling for {} ({})",
                cp.jars.len(),
                project_root.display(),
                cp.log
            );
            Some(cp)
        }
        Ok(_) => {
            tracing::warn!(
                "Classpath tooling returned no JARs for {}",
                project_root.display()
            );
            None
        }
        Err(e) => {
            tracing::warn!(
                "Classpath tooling failed for {}: {e:#}",
                project_root.display()
            );
            None
        }
    }
}

const MAX_OFFLINE_CLASSPATH_JARS: usize = 800;

const SPRING_CACHE_GROUPS: &[&str] = &[
    "org.springframework",
    "org.springframework.boot",
    "org.springframework.data",
    "org.springframework.security",
    "org.springframework.integration",
    "org.springframework.kafka",
    "org.springframework.batch",
    "org.springframework.ws",
    "org.springframework.hateoas",
    "org.springframework.session",
    "jakarta.annotation",
    "jakarta.servlet",
    "jakarta.validation",
    "javax.annotation",
];

/// Resolve dependency JARs for indexing — tooling cache or offline fallback (tooling runs separately).
fn resolve_classpath_for_index(project_root: &Path) -> GradleClasspath {
    let base = if let Some(cp) = gradle_classpath_from_tooling_cache(project_root) {
        cp
    } else {
        let mut cp = resolve_classpath_offline_fallback(project_root);
        if !cp.jars.is_empty() {
            let _ = ensure_dependency_sources(project_root, &cp.jars);
            cp.source_jars = discover_source_jars_for_jars(&cp.jars);
        }
        cp
    };

    let jars: Vec<PathBuf> = resolve_full_project_classpath(project_root)
        .into_iter()
        .filter(|p| p.is_file())
        .collect();
    let source_jars = if jars.is_empty() {
        Vec::new()
    } else {
        discover_source_jars_for_jars(&jars)
    };

    let mut classes_dirs = base.classes_dirs;
    let mut project_source_dirs = base.project_source_dirs;
    let (disc_classes, disc_sources) = discover_project_output_dirs(project_root);
    for dir in disc_classes {
        if !classes_dirs.iter().any(|p| p == &dir) {
            classes_dirs.push(dir);
        }
    }
    for dir in disc_sources {
        if !project_source_dirs.iter().any(|p| p == &dir) {
            project_source_dirs.push(dir);
        }
    }

    GradleClasspath {
        jars,
        source_jars,
        classes_dirs,
        project_source_dirs,
        log: if base.log.is_empty() {
            "from build-file dependency tree".into()
        } else {
            format!("{} + build-file tree", base.log)
        },
    }
}

fn resolve_classpath_offline_fallback(project_root: &Path) -> GradleClasspath {
    if super::maven::is_maven_project_root(project_root) {
        resolve_classpath_for_maven_offline(project_root)
    } else {
        resolve_classpath_for_gradle_offline(project_root)
    }
}

fn resolve_classpath_for_gradle_offline(gradle_root: &Path) -> GradleClasspath {
    let cached = cached_classpath_jars(gradle_root);
    if cached_classpath_trustworthy(gradle_root, &cached) {
        tracing::info!(
            "Using {} cached classpath JARs for {}",
            cached.len(),
            gradle_root.display()
        );
        let source_jars = discover_source_jars_for_jars(&cached);
        let (classes_dirs, project_source_dirs) =
            outputs_to_paths(&load_project_classpath_outputs(gradle_root));
        return GradleClasspath {
            jars: cached,
            source_jars,
            classes_dirs,
            project_source_dirs,
            log: "from classpath-jars.json cache".into(),
        };
    }

    let offline = resolve_classpath_from_gradle_cache_scoped(gradle_root, true);
    if !offline.jars.is_empty() {
        tracing::info!(
            "Resolved {} JARs from local Gradle cache (tooling pending) for {}",
            offline.jars.len(),
            gradle_root.display()
        );
        return offline;
    }

    if index_build_tooling_enabled() {
        tracing::info!(
            "Gradle classpath empty offline for {} — awaiting tooling resolve",
            gradle_root.display()
        );
    }

    GradleClasspath::default()
}

fn resolve_classpath_for_maven_offline(maven_root: &Path) -> GradleClasspath {
    let cached = cached_classpath_jars(maven_root);
    if cached_classpath_trustworthy(maven_root, &cached) {
        tracing::info!(
            "Using {} cached classpath JARs for {}",
            cached.len(),
            maven_root.display()
        );
        let source_jars = discover_source_jars_for_jars(&cached);
        return GradleClasspath {
            jars: cached,
            source_jars,
            log: "from classpath-jars.json cache".into(),
            ..Default::default()
        };
    }

    let offline = resolve_classpath_from_m2_scoped(maven_root, true);
    if !offline.jars.is_empty() {
        tracing::info!(
            "Resolved {} JARs from local Maven repository (tooling pending) for {}",
            offline.jars.len(),
            maven_root.display()
        );
        return offline;
    }

    if index_build_tooling_enabled() {
        tracing::info!(
            "Maven classpath empty offline for {} — awaiting tooling resolve",
            maven_root.display()
        );
    }

    GradleClasspath::default()
}

fn ensure_maven_dependencies(maven_root: &Path, progress: IndexProgress) -> Result<()> {
    report_index_progress(progress, "running-maven-sources", 0);
    let output = super::maven::run_maven(
        maven_root,
        &[
            "-q",
            "dependency:resolve",
            "-DincludeScope=test",
            "dependency:resolve-sources",
            "dependency:sources",
        ],
    )?;
    if !output.status.success() {
        bail!(
            "mvn dependency:resolve failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

fn resolve_classpath_from_m2(maven_root: &Path) -> GradleClasspath {
    resolve_classpath_from_m2_scoped(maven_root, true)
}

fn resolve_classpath_from_m2_scoped(maven_root: &Path, include_test_scope: bool) -> GradleClasspath {
    let jar_list: Vec<PathBuf> = super::maven::collect_transitive_jar_paths(maven_root, include_test_scope)
        .into_iter()
        .take(MAX_OFFLINE_CLASSPATH_JARS)
        .collect();

    if !jar_list.is_empty() {
        return GradleClasspath {
            jars: jar_list.clone(),
            source_jars: discover_source_jars_for_jars(&jar_list),
            log: format!(
                "from Maven repository transitive ({} JARs, test={include_test_scope})",
                jar_list.len()
            ),
            ..Default::default()
        };
    }

    let mut jars = HashSet::new();
    for (group, artifact, version) in super::maven::collect_dependency_coordinates(maven_root) {
        if let Some(jar) = super::maven::find_m2_jar(&group, &artifact, &version) {
            jars.insert(jar);
        }
        if jars.len() >= MAX_OFFLINE_CLASSPATH_JARS {
            break;
        }
    }

    if super::maven::is_spring_boot_project(maven_root) {
        let files_root = super::maven::m2_home();
        for group in SPRING_CACHE_GROUPS {
            let remaining = MAX_OFFLINE_CLASSPATH_JARS.saturating_sub(jars.len());
            if remaining == 0 {
                break;
            }
            for jar in list_jars_in_m2_group(&files_root, group, remaining) {
                jars.insert(jar);
                if jars.len() >= MAX_OFFLINE_CLASSPATH_JARS {
                    break;
                }
            }
        }
    }

    let jar_list: Vec<PathBuf> = jars.into_iter().take(MAX_OFFLINE_CLASSPATH_JARS).collect();
    GradleClasspath {
        jars: jar_list.clone(),
        source_jars: discover_source_jars_for_jars(&jar_list),
        log: format!("from Maven repository ({} JARs)", jar_list.len()),
        ..Default::default()
    }
}

fn list_jars_in_m2_group(m2_root: &Path, group: &str, limit: usize) -> Vec<PathBuf> {
    let group_dir = m2_root.join(group.replace('.', "/"));
    if !group_dir.is_dir() {
        return Vec::new();
    }
    let mut jars = Vec::new();
    collect_m2_jars_under(&group_dir, limit, &mut jars);
    jars
}

fn collect_m2_jars_under(dir: &Path, limit: usize, out: &mut Vec<PathBuf>) {
    if out.len() >= limit || !dir.is_dir() {
        return;
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() {
            if path.extension().and_then(|e| e.to_str()) == Some("jar") {
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if !name.contains("-sources") && !name.contains("-javadoc") {
                    out.push(path);
                    if out.len() >= limit {
                        return;
                    }
                }
            }
        } else if path.is_dir() {
            collect_m2_jars_under(&path, limit, out);
            if out.len() >= limit {
                return;
            }
        }
    }
}

fn resolve_maven_classpath_via_mvn(maven_root: &Path, progress: IndexProgress) -> Result<GradleClasspath> {
    report_index_progress(progress, "running-maven-classpath", 0);
    let out_file = reaper_dir(maven_root).join("maven-classpath.txt");
    std::fs::create_dir_all(reaper_dir(maven_root))?;
    let out_path = out_file
        .to_str()
        .context("classpath output path")?;
    let output = super::maven::run_maven(
        maven_root,
        &[
            "-q",
            "dependency:build-classpath",
            "-Dmdep.includeScope=test",
            "-Dmdep.outputFile",
            out_path,
        ],
    )?;
    if !output.status.success() {
        bail!(
            "mvn dependency:build-classpath failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let text = std::fs::read_to_string(&out_file).unwrap_or_default();
    let jars: Vec<PathBuf> = text
        .split(':')
        .map(PathBuf::from)
        .filter(|p| p.is_file())
        .collect();
    let source_jars = discover_source_jars_for_jars(&jars);
    Ok(GradleClasspath {
        jars,
        source_jars,
        log: "from mvn dependency:build-classpath".into(),
        ..Default::default()
    })
}

fn gradle_user_home() -> PathBuf {
    std::env::var("GRADLE_USER_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            std::env::var("HOME")
                .map(|h| PathBuf::from(h).join(".gradle"))
                .unwrap_or_else(|_| PathBuf::from(".gradle"))
        })
}

fn resolve_classpath_from_gradle_cache(gradle_root: &Path) -> GradleClasspath {
    resolve_classpath_from_gradle_cache_scoped(gradle_root, true)
}

fn resolve_classpath_from_gradle_cache_scoped(
    gradle_root: &Path,
    include_test_scope: bool,
) -> GradleClasspath {
    let files_root = gradle_user_home().join("caches/modules-2/files-2.1");
    if !files_root.is_dir() {
        return GradleClasspath::default();
    }

    let roots = collect_gradle_dependency_coordinates(gradle_root);
    let mut jar_list =
        collect_transitive_gradle_jar_paths(&files_root, &roots, include_test_scope);
    if jar_list.is_empty() {
        let mut jars = HashSet::new();
        for (group, artifact, version) in &roots {
            if let Some(jar) = find_cached_jar(&files_root, group, artifact, version)
                .or_else(|| super::maven::find_m2_jar(group, artifact, version))
            {
                jars.insert(jar);
            }
            if jars.len() >= MAX_OFFLINE_CLASSPATH_JARS {
                break;
            }
        }
        jar_list = jars.into_iter().take(MAX_OFFLINE_CLASSPATH_JARS).collect();
    } else {
        jar_list.truncate(MAX_OFFLINE_CLASSPATH_JARS);
    }

    if jar_list.is_empty()
        && (super::gradle::is_spring_boot_project(gradle_root)
            || super::maven::is_spring_boot_project(gradle_root))
    {
        let mut jars = HashSet::new();
        for group in SPRING_CACHE_GROUPS {
            let remaining = MAX_OFFLINE_CLASSPATH_JARS.saturating_sub(jars.len());
            if remaining == 0 {
                break;
            }
            for jar in list_jars_in_cache_group(&files_root, group, remaining) {
                jars.insert(jar);
                if jars.len() >= MAX_OFFLINE_CLASSPATH_JARS {
                    break;
                }
            }
        }
        jar_list = jars.into_iter().take(MAX_OFFLINE_CLASSPATH_JARS).collect();
    }

    if jar_list.is_empty() {
        return GradleClasspath::default();
    }

    GradleClasspath {
        jars: jar_list.clone(),
        source_jars: discover_source_jars_for_jars(&jar_list),
        log: format!(
            "from Gradle cache transitive ({} JARs, {} roots)",
            jar_list.len(),
            roots.len()
        ),
        ..Default::default()
    }
}

fn collect_transitive_gradle_jar_paths(
    files_root: &Path,
    roots: &[(String, String, String)],
    include_test_scope: bool,
) -> Vec<PathBuf> {
    super::maven::collect_transitive_jars(
        roots,
        include_test_scope,
        |group, artifact, version| {
            find_cached_jar(files_root, group, artifact, version)
                .or_else(|| super::maven::find_m2_jar(group, artifact, version))
        },
        |group, artifact, version| {
            read_gradle_cached_pom(files_root, group, artifact, version)
                .or_else(|| super::maven::read_m2_pom_text(group, artifact, version))
        },
    )
}

fn read_gradle_cached_pom(
    files_root: &Path,
    group: &str,
    artifact: &str,
    version: &str,
) -> Option<String> {
    let version_dir = files_root.join(group).join(artifact).join(version);
    if !version_dir.is_dir() {
        return None;
    }
    let expected = format!("{artifact}-{version}.pom");
    for hash_entry in std::fs::read_dir(&version_dir).into_iter().flatten().flatten() {
        let hash_dir = hash_entry.path();
        if !hash_dir.is_dir() {
            continue;
        }
        let pom = hash_dir.join(&expected);
        if pom.is_file() {
            return std::fs::read_to_string(pom).ok();
        }
        for file_entry in std::fs::read_dir(&hash_dir).into_iter().flatten().flatten() {
            let path = file_entry.path();
            if path.is_file()
                && path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.ends_with(".pom"))
            {
                return std::fs::read_to_string(path).ok();
            }
        }
    }
    None
}

fn discover_source_jars_for_jars(jars: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for jar in jars {
        if let Some(sources) = find_sources_jar(jar) {
            let key = sources.to_string_lossy().to_string();
            if seen.insert(key) {
                out.push(sources);
            }
        }
    }
    out
}

fn find_cached_jar(files_root: &Path, group: &str, artifact: &str, version: &str) -> Option<PathBuf> {
    let version_dir = files_root.join(group).join(artifact).join(version);
    if !version_dir.is_dir() {
        return None;
    }
    for hash_entry in std::fs::read_dir(&version_dir).into_iter().flatten().flatten() {
        let hash_dir = hash_entry.path();
        if !hash_dir.is_dir() {
            continue;
        }
        for file_entry in std::fs::read_dir(&hash_dir).into_iter().flatten().flatten() {
            let path = file_entry.path();
            if !path.is_file() {
                continue;
            }
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name.ends_with(".jar") && !name.ends_with("-sources.jar") {
                return Some(path);
            }
        }
    }
    None
}

fn list_jars_in_cache_group(files_root: &Path, group: &str, max: usize) -> Vec<PathBuf> {
    let group_dir = files_root.join(group);
    if !group_dir.is_dir() || max == 0 {
        return Vec::new();
    }
    let mut jars = Vec::new();
    collect_jars_under(&group_dir, &mut jars, max);
    jars
}

fn collect_jars_under(dir: &Path, out: &mut Vec<PathBuf>, max: usize) {
    if out.len() >= max {
        return;
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() {
            if path.extension().and_then(|e| e.to_str()) == Some("jar") {
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if !name.ends_with("-sources.jar") {
                    out.push(path);
                    if out.len() >= max {
                        return;
                    }
                }
            }
        } else if path.is_dir() {
            collect_jars_under(&path, out, max);
            if out.len() >= max {
                return;
            }
        }
    }
}

fn collect_dependency_coordinates(gradle_root: &Path) -> Vec<(String, String, String)> {
    collect_gradle_dependency_coordinates(gradle_root)
}

fn collect_gradle_dependency_coordinates(gradle_root: &Path) -> Vec<(String, String, String)> {
    if super::maven::is_maven_project_root(gradle_root) {
        return super::maven::collect_dependency_coordinates(gradle_root);
    }
    let catalog = load_gradle_version_catalog(gradle_root);
    let management = load_gradle_effective_versions(gradle_root, &catalog);
    let files_root = gradle_user_home().join("caches/modules-2/files-2.1");
    let mut coords = Vec::new();
    let mut seen = HashSet::new();
    collect_resolved_coordinates_from_dir(
        gradle_root,
        0,
        5,
        &mut coords,
        &mut seen,
        &catalog,
        &management,
        &files_root,
    );
    coords
}

fn collect_resolved_coordinates_from_dir(
    dir: &Path,
    depth: usize,
    max_depth: usize,
    out: &mut Vec<(String, String, String)>,
    seen: &mut HashSet<String>,
    catalog: &GradleVersionCatalog,
    management: &HashMap<String, String>,
    files_root: &Path,
) {
    if depth > max_depth || !dir.is_dir() {
        return;
    }
    for name in [
        "build.gradle",
        "build.gradle.kts",
        "settings.gradle",
        "settings.gradle.kts",
    ] {
        let path = dir.join(name);
        if path.is_file() {
            if let Ok(text) = std::fs::read_to_string(&path) {
                for coord in parse_gradle_coordinates(&text, catalog, management, files_root) {
                    let key = format!("{}:{}:{}", coord.0, coord.1, coord.2);
                    if seen.insert(key) {
                        out.push(coord);
                    }
                }
            }
        }
    }
    if depth >= max_depth {
        return;
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = entry.file_name();
        if name == "build"
            || name == "out"
            || name == "bin"
            || name == ".gradle"
            || name == "node_modules"
            || name == ".git"
        {
            continue;
        }
        collect_resolved_coordinates_from_dir(
            &path,
            depth + 1,
            max_depth,
            out,
            seen,
            catalog,
            management,
            files_root,
        );
    }
}

fn load_gradle_effective_versions(
    gradle_root: &Path,
    catalog: &GradleVersionCatalog,
) -> HashMap<String, String> {
    let mut texts = Vec::new();
    collect_gradle_build_texts(gradle_root, 0, 5, &mut texts);
    let files_root = gradle_user_home().join("caches/modules-2/files-2.1");
    let mut management = HashMap::new();
    for text in &texts {
        for (group, artifact, version) in collect_declared_bom_coordinates(text, catalog) {
            merge_bom_managed_versions(&files_root, &group, &artifact, &version, &mut management);
        }
        for decl in parse_gradle_plugin_declarations(text, catalog) {
            for (group, artifact, version) in plugin_import_boms(&decl.id, &decl.version) {
                merge_bom_managed_versions(&files_root, &group, &artifact, &version, &mut management);
            }
        }
        for (group, artifact, version) in collect_dependency_management_boms(text, catalog) {
            merge_bom_managed_versions(&files_root, &group, &artifact, &version, &mut management);
        }
    }
    management
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct GradlePluginDecl {
    id: String,
    version: String,
}

/// Known Gradle plugins that import a Maven BOM for dependency versions.
fn plugin_import_boms(plugin_id: &str, version: &str) -> Vec<(String, String, String)> {
    let v = version.to_string();
    match plugin_id {
        "org.springframework.boot" => vec![(
            "org.springframework.boot".into(),
            "spring-boot-dependencies".into(),
            v,
        )],
        "org.jetbrains.kotlin.jvm"
        | "org.jetbrains.kotlin.android"
        | "org.jetbrains.kotlin.multiplatform"
        | "org.jetbrains.kotlin.plugin.spring"
        | "org.jetbrains.kotlin.plugin.jpa" => vec![(
            "org.jetbrains.kotlin".into(),
            "kotlin-bom".into(),
            v,
        )],
        "io.quarkus" | "io.quarkus.gradle" => vec![(
            "io.quarkus.platform".into(),
            "quarkus-bom".into(),
            v,
        )],
        "io.micronaut.application" | "io.micronaut.library" | "io.micronaut.minimal.application" => {
            vec![("io.micronaut.platform".into(), "micronaut-platform".into(), v)]
        }
        "org.springframework.dependency-management" | "io.spring.dependency-management" => Vec::new(),
        _ => Vec::new(),
    }
}

fn parse_gradle_plugin_declarations(
    text: &str,
    catalog: &GradleVersionCatalog,
) -> Vec<GradlePluginDecl> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for block in extract_named_blocks(text, "plugins") {
        ingest_plugin_block(&block, catalog, &mut out, &mut seen);
    }
    for block in extract_named_blocks(text, "pluginManagement") {
        for plugins in extract_named_blocks(&block, "plugins") {
            ingest_plugin_block(&plugins, catalog, &mut out, &mut seen);
        }
    }
    out
}

fn ingest_plugin_block(
    block: &str,
    catalog: &GradleVersionCatalog,
    out: &mut Vec<GradlePluginDecl>,
    seen: &mut HashSet<String>,
) {
    for line in block.lines() {
        let line = line.split("//").next().unwrap_or(line).trim();
        if line.is_empty() {
            continue;
        }
        if let Some(decl) = parse_plugin_line(line, catalog) {
            let key = format!("{}:{}", decl.id, decl.version);
            if seen.insert(key) {
                out.push(decl);
            }
        }
    }
}

fn parse_plugin_line(line: &str, catalog: &GradleVersionCatalog) -> Option<GradlePluginDecl> {
    if line.contains("alias(") {
        return parse_plugin_catalog_alias(line, catalog);
    }
    if !line.contains("id") {
        return None;
    }
    let tokens = extract_quoted_tokens(line);
    let id = tokens.first()?.clone();
    if id == "java" || id == "application" || id == "idea" || id == "eclipse" {
        return None;
    }
    let version = line
        .split("version")
        .nth(1)
        .and_then(|rest| {
            parse_toml_string_value(rest.split([' ', ',', ')', ';']).next()?.trim())
        })
        .or_else(|| tokens.get(1).cloned())?;
    if version.is_empty() || version.contains('$') {
        return None;
    }
    Some(GradlePluginDecl { id, version })
}

fn parse_plugin_catalog_alias(line: &str, catalog: &GradleVersionCatalog) -> Option<GradlePluginDecl> {
    let idx = line.find("libs.plugins.")?;
    let rest = &line[idx + 13..];
    let alias: String = rest
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '.' || *c == '-' || *c == '_')
        .collect();
    catalog.resolve_plugin(&alias).map(|(id, version)| GradlePluginDecl { id, version })
}

/// `dependencyManagement { imports { mavenBom '...' } }` blocks (io.spring.dependency-management).
fn collect_dependency_management_boms(
    text: &str,
    catalog: &GradleVersionCatalog,
) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for block in extract_named_blocks(text, "dependencyManagement") {
        for imports in extract_named_blocks(&block, "imports") {
            for line in imports.lines() {
                let line = line.split("//").next().unwrap_or(line);
                if !line.to_ascii_lowercase().contains("mavenbom") {
                    continue;
                }
                for token in extract_quoted_tokens(line) {
                    if let Some(coord) = parse_coordinate_token(&token) {
                        let key = format!("{}:{}:{}", coord.0, coord.1, coord.2);
                        if seen.insert(key) {
                            out.push(coord);
                        }
                    }
                }
                for alias in extract_version_catalog_aliases(line) {
                    if let Some(coord) = catalog.resolve(&alias) {
                        let key = format!("{}:{}:{}", coord.0, coord.1, coord.2);
                        if seen.insert(key) {
                            out.push(coord);
                        }
                    }
                }
            }
        }
    }
    out
}

fn extract_named_blocks(text: &str, name: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut search = 0usize;
    while search < text.len() {
        let Some(rel) = text[search..].find(name) else {
            break;
        };
        let idx = search + rel;
        let after_name = idx + name.len();
        let rest = text[after_name..].trim_start();
        let Some(brace_start) = rest.find('{') else {
            search = after_name + 1;
            continue;
        };
        if let Some(block) = extract_balanced_block(&rest[brace_start..]) {
            out.push(block);
        }
        search = after_name + brace_start + 1;
    }
    out
}

fn extract_balanced_block(s: &str) -> Option<String> {
    let s = s.strip_prefix('{')?;
    let mut depth = 1usize;
    let mut end = 0usize;
    for (i, ch) in s.char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    end = i;
                    break;
                }
            }
            _ => {}
        }
    }
    if depth != 0 {
        return None;
    }
    Some(s[..end].to_string())
}

fn extract_spring_boot_plugin_version(text: &str) -> Option<String> {
    parse_gradle_plugin_declarations(text, &GradleVersionCatalog::default())
        .into_iter()
        .find(|p| p.id == "org.springframework.boot")
        .map(|p| p.version)
}

fn collect_gradle_build_texts(dir: &Path, depth: usize, max_depth: usize, out: &mut Vec<String>) {
    if depth > max_depth || !dir.is_dir() {
        return;
    }
    for name in [
        "build.gradle",
        "build.gradle.kts",
        "settings.gradle",
        "settings.gradle.kts",
    ] {
        let path = dir.join(name);
        if path.is_file() {
            if let Ok(text) = std::fs::read_to_string(&path) {
                out.push(text);
            }
        }
    }
    if depth >= max_depth {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = entry.file_name();
        if name == "build"
            || name == "out"
            || name == "bin"
            || name == ".gradle"
            || name == "node_modules"
            || name == ".git"
        {
            continue;
        }
        collect_gradle_build_texts(&path, depth + 1, max_depth, out);
    }
}

fn collect_declared_bom_coordinates(
    text: &str,
    catalog: &GradleVersionCatalog,
) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for line in text.lines() {
        let line = line.split("//").next().unwrap_or(line);
        let lower = line.to_ascii_lowercase();
        if !(lower.contains("platform(")
            || lower.contains("enforcedplatform(")
            || lower.contains("mavenbom")
            || lower.contains("bom("))
        {
            continue;
        }
        for token in extract_quoted_tokens(line) {
            if let Some(coord) = parse_coordinate_token(&token) {
                let key = format!("{}:{}:{}", coord.0, coord.1, coord.2);
                if seen.insert(key) {
                    out.push(coord);
                }
            }
        }
        for alias in extract_version_catalog_aliases(line) {
            if let Some(coord) = catalog.resolve(&alias) {
                let key = format!("{}:{}:{}", coord.0, coord.1, coord.2);
                if seen.insert(key) {
                    out.push(coord);
                }
            }
        }
    }
    out
}

fn merge_bom_managed_versions(
    files_root: &Path,
    group: &str,
    artifact: &str,
    version: &str,
    out: &mut HashMap<String, String>,
) {
    let mut merged = super::maven::bom_managed_versions(group, artifact, version);
    if merged.is_empty() {
        if let Some(raw) = read_gradle_cached_pom(files_root, group, artifact, version) {
            merged = super::maven::bom_managed_versions_from_pom(&raw);
        }
    }
    for (ga, ver) in merged {
        out.entry(ga).or_insert(ver);
    }
}

fn resolve_gradle_coordinate(
    group: String,
    artifact: String,
    version: Option<String>,
    management: &HashMap<String, String>,
    files_root: &Path,
) -> Option<(String, String, String)> {
    let key = format!("{group}:{artifact}");
    let version = version
        .or_else(|| management.get(&key).cloned())
        .or_else(|| find_latest_cached_version(files_root, &group, &artifact))
        .or_else(|| find_latest_m2_version(&group, &artifact))?;
    Some((group, artifact, version))
}

fn find_latest_m2_version(group: &str, artifact: &str) -> Option<String> {
    let dir = super::maven::m2_home()
        .join(group.replace('.', "/"))
        .join(artifact);
    latest_version_in_dir(&dir)
}

fn find_latest_cached_version(files_root: &Path, group: &str, artifact: &str) -> Option<String> {
    latest_version_in_dir(&files_root.join(group).join(artifact))
}

fn latest_version_in_dir(dir: &Path) -> Option<String> {
    let entries = std::fs::read_dir(dir).ok()?;
    let mut versions = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if !name.starts_with('.') {
                    versions.push(name.to_string());
                }
            }
        }
    }
    versions.sort_by(|a, b| compare_version_tokens(b, a));
    versions.into_iter().next()
}

fn compare_version_tokens(a: &str, b: &str) -> std::cmp::Ordering {
    let a_parts = version_tokens(a);
    let b_parts = version_tokens(b);
    for (av, bv) in a_parts.iter().zip(b_parts.iter()) {
        let ord = av.cmp(bv);
        if ord != std::cmp::Ordering::Equal {
            return ord;
        }
    }
    a_parts.len().cmp(&b_parts.len())
}

fn version_tokens(version: &str) -> Vec<u64> {
    version
        .split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse().ok())
        .collect()
}

#[derive(Debug, Clone, Default)]
struct GradleVersionCatalog {
    versions: HashMap<String, String>,
    libraries: HashMap<String, (String, String, String)>,
    plugins: HashMap<String, (String, String)>,
}

fn load_gradle_version_catalog(gradle_root: &Path) -> GradleVersionCatalog {
    let path = gradle_root.join("gradle/libs.versions.toml");
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return GradleVersionCatalog::default();
    };
    parse_gradle_version_catalog(&raw)
}

fn parse_gradle_version_catalog(raw: &str) -> GradleVersionCatalog {
    let mut catalog = GradleVersionCatalog::default();
    let mut section = "";

    for line in raw.lines() {
        let line = line.split('#').next().unwrap_or(line).trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            section = &line[1..line.len() - 1];
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim().trim_matches('"').to_string();
        let value = value.trim();

        match section {
            "versions" => {
                if let Some(v) = parse_toml_string_value(value) {
                    catalog.versions.insert(key, v);
                }
            }
            "libraries" => {
                if let Some(coord) = parse_catalog_library_value(value, &catalog.versions) {
                    catalog.libraries.insert(key, coord);
                }
            }
            "plugins" => {
                if let Some((id, version)) = parse_catalog_plugin_value(value, &catalog.versions) {
                    catalog.plugins.insert(key, (id, version));
                }
            }
            _ => {}
        }
    }

    catalog
}

fn parse_toml_string_value(raw: &str) -> Option<String> {
    let raw = raw.trim().trim_end_matches('}').trim().trim_end_matches(',');
    if raw.starts_with('"') {
        let end = raw[1..].find('"')? + 1;
        return Some(raw[1..end].to_string());
    }
    if raw.starts_with('\'') {
        let end = raw[1..].find('\'')? + 1;
        return Some(raw[1..end].to_string());
    }
    if !raw.is_empty() && !raw.contains('{') {
        return Some(raw.to_string());
    }
    None
}

fn parse_catalog_plugin_value(
    raw: &str,
    versions: &HashMap<String, String>,
) -> Option<(String, String)> {
    let raw = raw.trim();
    if raw.starts_with('"') || raw.starts_with('\'') {
        let id = parse_toml_string_value(raw)?;
        return Some((id, String::new()));
    }
    if !raw.starts_with('{') {
        return None;
    }
    let id = extract_catalog_field(raw, "id")?;
    let version = extract_catalog_field(raw, "version").or_else(|| {
        let alias = extract_catalog_field(raw, "version.ref")?;
        versions.get(&alias).cloned()
    })?;
    Some((id, version))
}

fn parse_catalog_library_value(
    raw: &str,
    versions: &HashMap<String, String>,
) -> Option<(String, String, String)> {
    let raw = raw.trim();
    if raw.starts_with('"') || raw.starts_with('\'') {
        return parse_coordinate_token(parse_toml_string_value(raw)?.as_str());
    }
    if !raw.starts_with('{') {
        return None;
    }
    let module = extract_catalog_field(raw, "module")?;
    let version = extract_catalog_field(raw, "version")
        .or_else(|| {
            let alias = extract_catalog_field(raw, "version.ref")?;
            versions.get(&alias).cloned()
        })?;
    let parts: Vec<&str> = module.split(':').collect();
    if parts.len() != 2 {
        return None;
    }
    Some((parts[0].to_string(), parts[1].to_string(), version))
}

fn extract_catalog_field(raw: &str, field: &str) -> Option<String> {
    let needle = format!("{field} =");
    let idx = raw.find(&needle)?;
    let rest = raw[idx + needle.len()..].trim_start();
    parse_toml_string_value(rest.split(',').next()?.trim())
}

impl GradleVersionCatalog {
    fn resolve(&self, accessor: &str) -> Option<(String, String, String)> {
        let key = accessor.replace('.', "-");
        if let Some(coord) = self.libraries.get(&key) {
            return Some(coord.clone());
        }
        self.libraries.get(accessor).cloned()
    }

    fn resolve_plugin(&self, accessor: &str) -> Option<(String, String)> {
        let key = accessor.replace('.', "-");
        if let Some(entry) = self.plugins.get(&key) {
            return Some(entry.clone());
        }
        self.plugins.get(accessor).cloned()
    }
}

fn parse_gradle_coordinates(
    content: &str,
    catalog: &GradleVersionCatalog,
    management: &HashMap<String, String>,
    files_root: &Path,
) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for line in content.lines() {
        let line = line.split("//").next().unwrap_or(line);
        let lower = line.to_ascii_lowercase();
        if lower.contains("platform(")
            || lower.contains("enforcedplatform(")
            || lower.contains("mavenbom")
        {
            continue;
        }
        for token in extract_quoted_tokens(line) {
            if let Some(coord) = resolve_coordinate_token(&token, management, files_root) {
                let key = format!("{}:{}:{}", coord.0, coord.1, coord.2);
                if seen.insert(key) {
                    out.push(coord);
                }
            }
        }
        for alias in extract_version_catalog_aliases(line) {
            if let Some(coord) = catalog.resolve(&alias) {
                let key = format!("{}:{}:{}", coord.0, coord.1, coord.2);
                if seen.insert(key) {
                    out.push(coord);
                }
            }
        }
    }
    out
}

fn resolve_coordinate_token(
    token: &str,
    management: &HashMap<String, String>,
    files_root: &Path,
) -> Option<(String, String, String)> {
    if let Some(coord) = parse_coordinate_token(token) {
        return Some(coord);
    }
    let (group, artifact, version) = parse_coordinate_parts(token)?;
    resolve_gradle_coordinate(group, artifact, version, management, files_root)
}

fn parse_coordinate_parts(token: &str) -> Option<(String, String, Option<String>)> {
    let token = token.trim();
    let parts: Vec<&str> = token.split(':').collect();
    match parts.len() {
        2 => {
            let group = parts[0].trim();
            let artifact = parts[1].trim();
            if group.is_empty() || artifact.is_empty() {
                None
            } else {
                Some((group.to_string(), artifact.to_string(), None))
            }
        }
        3 => {
            let group = parts[0].trim();
            let artifact = parts[1].trim();
            let version = parts[2].trim();
            if group.is_empty() || artifact.is_empty() || version.is_empty() {
                None
            } else if version.contains('$') || version.contains('{') {
                Some((group.to_string(), artifact.to_string(), None))
            } else {
                Some((
                    group.to_string(),
                    artifact.to_string(),
                    Some(version.to_string()),
                ))
            }
        }
        _ => None,
    }
}

fn extract_version_catalog_aliases(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut search = line;
    while let Some(idx) = search.find("libs.") {
        let rest = &search[idx + 5..];
        let alias: String = rest
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '.' || *c == '-' || *c == '_')
            .collect();
        let alias_len = alias.len();
        if !alias.is_empty() {
            out.push(alias);
        }
        search = if rest.len() > alias_len {
            &rest[alias_len..]
        } else {
            break;
        };
    }
    out
}

fn extract_quoted_tokens(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut search = line;
    while let Some(start) = search.find('"') {
        let rest = &search[start + 1..];
        if let Some(end) = rest.find('"') {
            let token = rest[..end].to_string();
            if !token.is_empty() {
                out.push(token);
            }
            search = &rest[end + 1..];
        } else {
            break;
        }
    }
    while let Some(start) = search.find('\'') {
        let rest = &search[start + 1..];
        if let Some(end) = rest.find('\'') {
            let token = rest[..end].to_string();
            if !token.is_empty() {
                out.push(token);
            }
            search = &rest[end + 1..];
        } else {
            break;
        }
    }
    out
}

fn parse_coordinate_token(token: &str) -> Option<(String, String, String)> {
    let token = token.trim();
    let parts: Vec<&str> = token.split(':').collect();
    if parts.len() != 3 {
        return None;
    }
    let group = parts[0].trim();
    let artifact = parts[1].trim();
    let version = parts[2].trim();
    if group.is_empty() || artifact.is_empty() || version.is_empty() {
        return None;
    }
    if version.contains('$') || version.contains('{') {
        return None;
    }
    Some((group.to_string(), artifact.to_string(), version.to_string()))
}

fn index_all_java_source_trees(
    ws: &Path,
    scope: &Path,
    symbols: &mut Vec<IndexedSymbol>,
    progress: Option<&Box<dyn Fn(&str, usize) + Send>>,
) -> Result<()> {
    collect_and_index_java_dirs(ws, scope, symbols, progress)?;
    Ok(())
}

fn collect_and_index_java_dirs(
    ws: &Path,
    dir: &Path,
    symbols: &mut Vec<IndexedSymbol>,
    progress: Option<&Box<dyn Fn(&str, usize) + Send>>,
) -> Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    let entries = std::fs::read_dir(dir)?;
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = entry.file_name();
        if name == "build"
            || name == "out"
            || name == "bin"
            || name == ".gradle"
            || name == "target"
            || name == "node_modules"
        {
            continue;
        }
        if name == "src" {
            index_java_dir(ws, &path, symbols, progress)?;
            continue;
        }
        if name == "java" {
            if let Some(src_dir) = path.parent() {
                if let Some(src_kind) = src_dir.file_name() {
                    if src_kind == "main" || src_kind == "test" {
                        index_java_dir(ws, &path, symbols, progress)?;
                        continue;
                    }
                }
            }
        }
        collect_and_index_java_dirs(ws, &path, symbols, progress)?;
    }
    Ok(())
}

fn resolve_gradle_classpath(gradle_root: &Path, progress: IndexProgress) -> Result<GradleClasspath> {
    let cmd = resolve_gradle_command(gradle_root)?;
    let init = init_script_path();
    if !init.is_file() {
        bail!("classpath init script missing at {}", init.display());
    }

    let init_str = init
        .to_str()
        .context("classpath init script path is not valid UTF-8")?;

    let mut args = cmd.project_args.clone();
    args.extend([
        "--no-daemon".into(),
        "--no-configuration-cache".into(),
        "-I".into(),
        init_str.to_string(),
    ]);

    let mut gradle_log = String::new();

    // Resolve/download dependency JARs before printing classpath.
    report_index_progress(progress, "running-gradle-compile", 0);
    let warm_args: Vec<&str> = args
        .iter()
        .map(String::as_str)
        .chain(
            ["compileJava", "compileTestJava", "-q", "--console=plain"]
                .iter()
                .copied(),
        )
        .collect();
    if let Ok(out) = run_gradle_with_command(&cmd, &warm_args) {
        if !out.success() {
            gradle_log = format!(
                "compileJava failed (exit {}): {}",
                out.exit_code,
                out.stderr.trim()
            );
        }
    }

    args.extend([
        "reaperPrintClasspath".into(),
        "-q".into(),
        "--console=plain".into(),
    ]);
    report_index_progress(progress, "running-gradle-classpath", 0);
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();

    let out = run_gradle_with_command(&cmd, &arg_refs)?;
    if !out.success() {
        let msg = format!(
            "reaperPrintClasspath failed (exit {}): {}",
            out.exit_code,
            out.stderr.trim()
        );
        if gradle_log.is_empty() {
            gradle_log = msg;
        } else {
            gradle_log = format!("{gradle_log}\n{msg}");
        }
    }

    let mut jars = HashSet::new();
    let mut source_jars = HashSet::new();
    let mut classes_dirs = HashSet::new();
    let mut project_source_dirs = HashSet::new();
    for line in out.stdout.lines().chain(out.stderr.lines()) {
        if let Some(path) = line.strip_prefix("JAR:") {
            let path = PathBuf::from(path.trim());
            if path.is_file() {
                jars.insert(path);
            }
        } else if let Some(path) = line.strip_prefix("SOURCES:") {
            let path = PathBuf::from(path.trim());
            if path.is_file() {
                source_jars.insert(path);
            }
        } else if let Some(path) = line.strip_prefix("CLASSES:") {
            let path = PathBuf::from(path.trim());
            if path.is_dir() {
                classes_dirs.insert(path);
            }
        } else if let Some(path) = line.strip_prefix("SRCROOT:") {
            let path = PathBuf::from(path.trim());
            if path.is_dir() {
                project_source_dirs.insert(path);
            }
        }
    }
    Ok(GradleClasspath {
        jars: jars.into_iter().collect(),
        source_jars: source_jars.into_iter().collect(),
        classes_dirs: classes_dirs.into_iter().collect(),
        project_source_dirs: project_source_dirs.into_iter().collect(),
        log: gradle_log,
    })
}

fn init_script_path() -> PathBuf {
    if let Ok(dir) = std::env::var("REAPER_GRADLE_INIT") {
        return PathBuf::from(dir);
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(mac_os) = exe.parent() {
            let bundled = mac_os.join("../Resources/gradle/reaper-classpath.init.gradle");
            if bundled.is_file() {
                return bundled.canonicalize().unwrap_or(bundled);
            }
        }
    }

    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("gradle/reaper-classpath.init.gradle")
}

fn find_sources_jar(jar: &Path) -> Option<PathBuf> {
    let file_name = jar.file_name()?.to_string_lossy();
    if !file_name.ends_with(".jar") {
        return None;
    }
    let base = &file_name[..file_name.len() - 4];

    if let Some(parent) = jar.parent() {
        let sibling = parent.join(format!("{base}-sources.jar"));
        if sibling.is_file() {
            return Some(sibling);
        }
    }

    if let Some(hash_dir) = jar.parent() {
        if let Some(version_dir) = hash_dir.parent() {
            if let Ok(entries) = std::fs::read_dir(version_dir) {
                for entry in entries.flatten() {
                    let sub = entry.path();
                    if !sub.is_dir() {
                        continue;
                    }
                    if let Ok(files) = std::fs::read_dir(&sub) {
                        for file in files.flatten() {
                            let path = file.path();
                            if path.extension().and_then(|e| e.to_str()) != Some("jar") {
                                continue;
                            }
                            let name = path.file_name()?.to_string_lossy();
                            if name == format!("{base}-sources.jar") {
                                return Some(path);
                            }
                        }
                    }
                }
            }
        }
    }

    None
}

fn materialize_jdk_sources(dest: &Path) -> Result<Option<PathBuf>> {
    let Some(src_zip) = find_jdk_src_zip()? else {
        tracing::warn!("JDK sources (src.zip) not found — install a JDK (not just JRE) for java.* navigation");
        return Ok(None);
    };

    let marker = dest.join(".extracted");
    let marker_text = src_zip.to_string_lossy().to_string();
    if marker.is_file() {
        if std::fs::read_to_string(&marker).ok().as_deref() == Some(marker_text.as_str()) {
            return Ok(Some(dest.to_path_buf()));
        }
    }

    let _ = std::fs::remove_dir_all(dest);
    extract_zip(&src_zip, dest)?;
    std::fs::write(marker, marker_text)?;
    Ok(Some(dest.to_path_buf()))
}

fn find_jdk_src_zip() -> Result<Option<PathBuf>> {
    for home in jdk_home_candidates()? {
        for candidate in jdk_src_zip_candidates(&home) {
            if candidate.is_file() {
                return Ok(Some(candidate));
            }
        }
    }
    Ok(None)
}

fn jdk_src_zip_candidates(home: &Path) -> Vec<PathBuf> {
    let mut candidates = vec![home.join("lib/src.zip"), home.join("src.zip")];
    if home.file_name().and_then(|n| n.to_str()) == Some("jre") {
        if let Some(parent) = home.parent() {
            candidates.push(parent.join("lib/src.zip"));
            candidates.push(parent.join("src.zip"));
        }
    }
    candidates
}

fn jdk_home_candidates() -> Result<Vec<PathBuf>> {
    let mut homes = Vec::new();

    if let Ok(home) = crate::jdk::toolchain_java_home() {
        homes.push(home);
    }
    if let Ok(home) = crate::jdk::effective_java_home() {
        homes.push(home);
    }

    if let Ok(home) = std::env::var("JAVA_HOME") {
        let path = PathBuf::from(&home);
        if path.is_dir() {
            homes.push(path);
        }
    }

    #[cfg(target_os = "macos")]
    {
        if let Ok(out) = Command::new("/usr/libexec/java_home").output() {
            if out.status.success() {
                let path = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim());
                if path.is_dir() {
                    homes.push(path);
                }
            }
        }
    }

    if let Ok(home) = java_home_from_java_cmd() {
        homes.push(home);
    }

    let mut seen = HashSet::new();
    homes.retain(|h| seen.insert(h.display().to_string()));

    if homes.is_empty() {
        bail!("could not determine JAVA_HOME");
    }
    Ok(homes)
}

fn java_home_from_java_cmd() -> Result<PathBuf> {
    let out = Command::new("java")
        .args(["-XshowSettings:properties", "-version"])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .context("failed to run java")?;

    let text = String::from_utf8_lossy(&out.stderr);
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("java.home = ") {
            let home = PathBuf::from(rest.trim());
            if home.is_dir() {
                return Ok(home);
            }
        }
    }
    bail!("java.home not found in java settings output")
}

fn toolchain_java_home() -> Result<PathBuf> {
    crate::jdk::toolchain_java_home()
}

fn extract_zip(zip_path: &Path, dest: &Path) -> Result<()> {
    std::fs::create_dir_all(dest)?;
    let status = Command::new("unzip")
        .args([
            "-qo",
            zip_path
                .to_str()
                .context("zip path is not valid UTF-8")?,
            "-d",
            dest.to_str().context("dest path is not valid UTF-8")?,
        ])
        .status()
        .with_context(|| format!("failed to run unzip on {}", zip_path.display()))?;

    if !status.success() {
        bail!("unzip failed for {}", zip_path.display());
    }
    Ok(())
}

fn is_dependency_sources_dir(dir: &Path) -> bool {
    dir.to_string_lossy().contains("java-sources/deps")
}

fn index_jar_classpath_fallback(
    ws: &Path,
    gradle_root: &Path,
    jars: &[PathBuf],
    source_dirs: &[PathBuf],
    symbols: &mut Vec<IndexedSymbol>,
    progress: Option<&Box<dyn Fn(&str, usize) + Send>>,
) -> Result<()> {
    let _ = (ws, gradle_root, source_dirs);
    let base_symbols = symbols.len();
    let mut known: HashSet<String> = symbols.iter().map(|s| s.qualified.clone()).collect();
    let jar_refs = prioritize_jars_for_fallback(jars);
    let jar_total = jar_refs.len().max(1);
    let mut added = 0usize;

    for (jar_idx, jar) in jar_refs.iter().enumerate() {
        if let Some(cb) = progress {
            report_jar_index_progress(cb, base_symbols, jar_idx, jar_total);
        }
        if !jar.is_file() {
            continue;
        }
        let entries = list_jar_class_entries(jar)?;
        let entry_count = entries.len();
        let jar_label = format!(
            ".reaper/classpath-jar/{}",
            jar
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("classpath.jar")
        );
        for (entry_idx, (fqcn, kind)) in entries.into_iter().enumerate() {
            if entry_idx >= JAR_INDEX_MAX_ENTRIES_PER_JAR {
                break;
            }
            if known.contains(&fqcn) || !should_index_jar_entry(&fqcn, &kind) {
                continue;
            }
            let name = fqcn.rsplit('.').next().unwrap_or(&fqcn).to_string();
            symbols.push(IndexedSymbol {
                name,
                qualified: fqcn.clone(),
                kind,
                path: jar_label.clone(),
                line: 1,
                column: 1,
            });
            known.insert(fqcn);
            added += 1;

            if let Some(cb) = progress {
                if added % JAR_INDEX_PROGRESS_INTERVAL == 0 || entry_idx + 1 == entry_count {
                    report_jar_index_progress(cb, base_symbols, jar_idx, jar_total);
                }
            }
        }

        if let Some(cb) = progress {
            report_jar_index_progress(cb, base_symbols, jar_idx + 1, jar_total);
        }
    }

    if added > 0 {
        tracing::info!("Indexed {added} additional Java symbols from classpath JAR fallback");
    }
    Ok(())
}

fn index_classes_dirs_fallback(
    ws: &Path,
    gradle_root: &Path,
    classes_dirs: &[PathBuf],
    symbols: &mut Vec<IndexedSymbol>,
    progress: Option<&Box<dyn Fn(&str, usize) + Send>>,
) -> Result<()> {
    let _ = ws;
    let base_symbols = symbols.len();
    let mut known: HashSet<String> = symbols.iter().map(|s| s.qualified.clone()).collect();
    let mut added = 0usize;

    for dir in classes_dirs {
        if !dir.is_dir() {
            continue;
        }
        let rel_label = dir
            .strip_prefix(gradle_root)
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_else(|_| dir.display().to_string());
        let path_label = if rel_label.is_empty() {
            dir.display().to_string()
        } else {
            rel_label
        };
        for (fqcn, kind) in list_dir_class_entries(dir)? {
            if known.contains(&fqcn) || !should_index_jar_entry(&fqcn, &kind) {
                continue;
            }
            let name = fqcn.rsplit('.').next().unwrap_or(&fqcn).to_string();
            symbols.push(IndexedSymbol {
                name,
                qualified: fqcn.clone(),
                kind,
                path: path_label.clone(),
                line: 1,
                column: 1,
            });
            known.insert(fqcn);
            added += 1;
            if let Some(cb) = progress {
                if added % JAR_INDEX_PROGRESS_INTERVAL == 0 {
                    cb("jar-index", base_symbols + added);
                }
            }
        }
    }

    if added > 0 {
        tracing::info!("Indexed {added} additional Java symbols from project output dirs");
    }
    Ok(())
}

fn list_dir_class_entries(dir: &Path) -> Result<Vec<(String, String)>> {
    let mut entries = Vec::new();
    collect_dir_class_entries(dir, dir, &mut entries, 0)?;
    Ok(entries)
}

fn collect_dir_class_entries(
    root: &Path,
    dir: &Path,
    out: &mut Vec<(String, String)>,
    depth: usize,
) -> Result<()> {
    if depth > 24 || out.len() >= JAR_INDEX_MAX_ENTRIES_PER_JAR {
        return Ok(());
    }
    let Ok(read) = std::fs::read_dir(dir) else {
        return Ok(());
    };
    for entry in read.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_dir_class_entries(root, &path, out, depth + 1)?;
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) != Some("class") {
            continue;
        }
        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if file_name.contains('$') {
            continue;
        }
        let rel = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .with_extension("");
        let fqcn = rel
            .to_string_lossy()
            .replace('\\', "/")
            .replace('/', ".");
        if fqcn.is_empty() {
            continue;
        }
        let kind = read_path_class_bytes(&path)
            .map(|b| class_kind_from_bytes(&b, &fqcn))
            .unwrap_or_else(|| "class".into());
        out.push((fqcn, kind));
    }
    Ok(())
}

/// Java source roots for javac diagnostics (hand-written + Gradle-generated).
pub fn project_java_sourcepath(project_root: &Path, overlay_root: &Path) -> Vec<PathBuf> {
    let mut sourcepath = Vec::new();
    for prefix in super::java_sources::discover_source_prefixes(project_root) {
        sourcepath.push(overlay_root.join(&prefix));
        let dir = project_root.join(&prefix);
        if dir.is_dir() {
            sourcepath.push(dir);
        }
        if prefix.contains("/src/test/") {
            let main_prefix = prefix.replacen("/src/test/", "/src/main/", 1);
            if main_prefix != prefix {
                sourcepath.push(overlay_root.join(&main_prefix));
                let main_dir = project_root.join(&main_prefix);
                if main_dir.is_dir() {
                    sourcepath.push(main_dir);
                }
            }
        }
    }
    sourcepath.extend(cached_project_source_dirs(project_root));
    sourcepath.sort();
    sourcepath.dedup();
    sourcepath
}

const JAR_INDEX_MAX_ENTRIES_PER_JAR: usize = 4000;
const JAR_INDEX_PROGRESS_INTERVAL: usize = 128;

fn report_jar_index_progress(
    cb: &dyn Fn(&str, usize),
    base_symbols: usize,
    jar_idx: usize,
    jar_total: usize,
) {
    // Encode fractional jar progress in the low 3 digits (0–999) so the UI bar keeps moving.
    let jar_pct = ((jar_idx + 1) * 999 / jar_total.max(1)).min(999);
    cb("jar-index", base_symbols + jar_pct);
}

/// Spring/JUnit/Jakarta jars first when the classpath is large.
fn prioritize_jars_for_fallback<'a>(jars: &'a [PathBuf]) -> Vec<&'a PathBuf> {
    if jars.len() <= 120 {
        return jars.iter().collect();
    }
    let mut priority = Vec::new();
    let mut rest = Vec::new();
    for jar in jars {
        let name = jar.to_string_lossy().to_ascii_lowercase();
        if jar_is_index_priority(&name) {
            priority.push(jar);
        } else {
            rest.push(jar);
        }
    }
    if priority.is_empty() {
        jars.iter().collect()
    } else {
        priority.extend(rest);
        priority
    }
}

fn jar_is_index_priority(name: &str) -> bool {
    name.contains("spring-data")
        || name.contains("spring-core")
        || name.contains("spring-beans")
        || name.contains("spring-context")
        || name.contains("spring-web")
        || name.contains("spring-boot")
        || name.contains("junit")
        || name.contains("jakarta.")
        || name.contains("javax.persistence")
        || name.contains("hibernate")
        || name.contains("lombok")
}

fn is_annotation_index_symbol(sym: &IndexedSymbol) -> bool {
    if sym.kind == "annotation" {
        return true;
    }
    if sym.kind != "class" && sym.kind != "interface" {
        return false;
    }
    if sym.qualified.contains(".annotation.") {
        return true;
    }
    well_known_import(&sym.name).is_some_and(|fqcn| fqcn.contains(".annotation."))
}

const ACC_INTERFACE: u16 = 0x0200;
const ACC_ANNOTATION: u16 = 0x2000;

fn class_access_flags(class_bytes: &[u8]) -> Option<u16> {
    const MAGIC: [u8; 4] = [0xCA, 0xFE, 0xBA, 0xBE];
    if class_bytes.len() < 8 || class_bytes[..4] != MAGIC {
        return None;
    }
    let mut pos = 8usize; // skip magic + version
    pos = skip_constant_pool(class_bytes, pos)?;
    if pos + 2 > class_bytes.len() {
        return None;
    }
    Some(u16::from_be_bytes([class_bytes[pos], class_bytes[pos + 1]]))
}

fn skip_constant_pool(data: &[u8], mut pos: usize) -> Option<usize> {
    if pos + 2 > data.len() {
        return None;
    }
    let count = u16::from_be_bytes([data[pos], data[pos + 1]]) as usize;
    pos += 2;
    for _ in 1..count {
        if pos >= data.len() {
            return None;
        }
        let tag = data[pos];
        pos += 1;
        pos = match tag {
            1 => {
                if pos + 2 > data.len() {
                    return None;
                }
                let len = u16::from_be_bytes([data[pos], data[pos + 1]]) as usize;
                pos + 2 + len
            }
            3 | 4 => pos + 4,
            5 | 6 => pos + 8,
            7 | 8 => pos + 2,
            9 | 10 | 11 | 12 | 18 => pos + 4,
            15 => pos + 3,
            16 => pos + 2,
            _ => return None,
        };
        if pos > data.len() {
            return None;
        }
    }
    Some(pos)
}

fn class_kind_from_bytes(class_bytes: &[u8], fqcn: &str) -> String {
    if fqcn.contains(".annotation.") {
        return "annotation".into();
    }
    let Some(flags) = class_access_flags(class_bytes) else {
        return "class".into();
    };
    if flags & ACC_ANNOTATION != 0 {
        "annotation".into()
    } else if flags & ACC_INTERFACE != 0 {
        "interface".into()
    } else {
        "class".into()
    }
}

fn read_path_class_bytes(path: &Path) -> Option<Vec<u8>> {
    std::fs::read(path).ok().filter(|b| b.len() >= 8)
}

fn read_jar_entry_bytes(jar: &Path, entry: &str) -> Option<Vec<u8>> {
    let out = Command::new("unzip")
        .arg("-p")
        .arg(jar)
        .arg(entry)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if out.status.success() && out.stdout.len() >= 8 {
        Some(out.stdout)
    } else {
        None
    }
}

fn should_index_jar_class(fqcn: &str) -> bool {
    if fqcn.starts_with("java.") {
        return false;
    }
    fqcn.starts_with("org.springframework.")
        || fqcn.starts_with("jakarta.")
        || fqcn.starts_with("javax.")
        || fqcn.starts_with("kotlin.")
        || fqcn.starts_with("org.junit.")
        || fqcn.starts_with("org.mockito.")
        || fqcn.starts_with("org.assertj.")
        || fqcn.starts_with("org.slf4j.")
        || fqcn.starts_with("org.hamcrest.")
        || fqcn.starts_with("lombok.")
}

fn should_index_jar_entry(fqcn: &str, kind: &str) -> bool {
    if fqcn.starts_with("java.") || fqcn.starts_with("jdk.") {
        return false;
    }
    kind == "annotation" || should_index_jar_class(fqcn)
}

fn list_jar_class_entries(jar: &Path) -> Result<Vec<(String, String)>> {
    let out = Command::new("jar")
        .arg("tf")
        .arg(jar)
        .output()
        .with_context(|| format!("failed to run jar tf on {}", jar.display()))?;
    if !out.status.success() {
        return Ok(Vec::new());
    }
    let mut entries = Vec::new();
    let mut annotation_probe_reads = 0usize;
    const MAX_ANNOTATION_PROBE_READS: usize = 512;
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        let line = line.trim();
        if !line.ends_with(".class") || line.contains('$') {
            continue;
        }
        let fqcn = line.trim_end_matches(".class").replace('/', ".");
        if !fqcn.contains('.') {
            continue;
        }
        let kind = if fqcn.contains(".annotation.") {
            "annotation".to_string()
        } else if should_index_jar_class(&fqcn) {
            read_jar_entry_bytes(jar, line)
                .map(|b| class_kind_from_bytes(&b, &fqcn))
                .unwrap_or_else(|| "class".into())
        } else if annotation_probe_reads < MAX_ANNOTATION_PROBE_READS {
            annotation_probe_reads += 1;
            let Some(bytes) = read_jar_entry_bytes(jar, line) else {
                continue;
            };
            let detected = class_kind_from_bytes(&bytes, &fqcn);
            if detected != "annotation" {
                continue;
            }
            detected
        } else {
            continue;
        };
        if should_index_jar_entry(&fqcn, &kind) {
            entries.push((fqcn, kind));
        }
    }
    Ok(entries)
}

fn find_java_source_for_fqcn(
    ws: &Path,
    gradle_root: &Path,
    source_dirs: &[PathBuf],
    fqcn: &str,
) -> Option<PathBuf> {
    let rel = format!("{}.java", fqcn.replace('.', "/"));
    for dir in source_dirs {
        let direct = dir.join(&rel);
        if direct.is_file() {
            return Some(direct);
        }
        for module in JDK_SOURCE_MODULES {
            let module_dir = module.trim_end_matches('/');
            let modular = dir.join(module_dir).join(&rel);
            if modular.is_file() {
                return Some(modular);
            }
        }
    }
    let file_name = rel.rsplit('/').next()?;
    for dir in source_dirs {
        if let Some(found) = find_file_by_name(dir, file_name) {
            if let Ok(content) = std::fs::read_to_string(&found) {
                if source_matches_fqcn(&content, fqcn, &found) {
                    return Some(found);
                }
            }
        }
    }
    let reaper_sources = reaper_dir(gradle_root).join("java-sources");
    if reaper_sources.is_dir() {
        if let Some(found) = find_file_by_name(&reaper_sources, file_name) {
            if let Ok(content) = std::fs::read_to_string(&found) {
                if source_matches_fqcn(&content, fqcn, &found) {
                    return Some(found);
                }
            }
        }
    }
    let _ = ws;
    None
}

fn jdk_source_exists(jdk_dir: &Path, fqcn: &str) -> bool {
    let rel = format!("{}.java", fqcn.replace('.', "/"));
    if jdk_dir.join(&rel).is_file() {
        return true;
    }
    for module in JDK_SOURCE_MODULES {
        let module_dir = module.trim_end_matches('/');
        if jdk_dir.join(module_dir).join(&rel).is_file() {
            return true;
        }
    }
    false
}

fn ensure_jdk_sources_materialized(gradle_root: &Path) -> Result<bool> {
    let jdk_dest = reaper_dir(gradle_root).join("java-sources/jdk");
    Ok(jdk_dest.join(".extracted").is_file())
}

fn resolve_fqcn_from_jdk_files(
    gradle_root: &Path,
    symbol: &str,
    imports: &ImportMap,
) -> Option<String> {
    ensure_jdk_sources_materialized(gradle_root).ok()?;
    let jdk = reaper_dir(gradle_root).join("java-sources/jdk");
    let mut candidates = Vec::new();
    if let Some(fqcn) = imports.explicit.get(symbol) {
        candidates.push(fqcn.clone());
    }
    for prefix in &imports.wildcards {
        candidates.push(format!("{prefix}.{symbol}"));
    }
    candidates.push(format!("java.lang.{symbol}"));
    candidates.into_iter().find(|fqcn| jdk_source_exists(&jdk, fqcn))
}

fn resolve_jdk_type_location(
    ws: &Path,
    gradle_root: &Path,
    symbol: &str,
    imports: &ImportMap,
) -> Result<Option<SymbolLocation>> {
    let Some(fqcn) = resolve_fqcn_from_jdk_files(gradle_root, symbol, imports) else {
        return Ok(None);
    };
    let cache_key = format!("{}:{}", gradle_root.display(), fqcn);
    if let Ok(guard) = JDK_LOCATION_CACHE.lock() {
        if let Some(loc) = guard.get(&cache_key) {
            return Ok(Some(loc.clone()));
        }
    }

    let jdk = reaper_dir(gradle_root).join("java-sources/jdk");
    let Some(source_path) = find_java_source_for_fqcn(ws, gradle_root, &[jdk], &fqcn) else {
        return Ok(None);
    };
    let rel = rel_path_for(ws, &source_path)
        .map(|p| normalize_index_path(ws, gradle_root, &p))
        .unwrap_or_else(|_| normalize_index_path(ws, gradle_root, &source_path.to_string_lossy()));
    let simple = fqcn.rsplit('.').next().unwrap_or(symbol);
    let (line, column) = read_type_line_in_java_source(&source_path, simple);
    let loc = SymbolLocation {
        name: symbol.to_string(),
        kind: "class".into(),
        path: rel,
        line,
        column,
    };
    if let Ok(mut guard) = JDK_LOCATION_CACHE.lock() {
        if guard.len() >= DEFINITION_CACHE_MAX {
            guard.clear();
        }
        guard.insert(cache_key, loc.clone());
    }
    Ok(Some(loc))
}

fn fast_java_lang_location(
    ws: &Path,
    gradle_root: &Path,
    symbol: &str,
    imports: &ImportMap,
) -> Result<Option<SymbolLocation>> {
    if imports.explicit.contains_key(symbol) {
        return Ok(None);
    }
    for prefix in &imports.wildcards {
        let fqcn = format!("{prefix}.{symbol}");
        if fqcn.starts_with("java.lang.") {
            continue;
        }
        let jdk = reaper_dir(gradle_root).join("java-sources/jdk");
        if jdk_source_exists(&jdk, &fqcn) {
            return Ok(None);
        }
    }

    let fqcn = format!("java.lang.{symbol}");
    let cache_key = format!("{}:{}", gradle_root.display(), fqcn);
    if let Ok(guard) = JDK_LOCATION_CACHE.lock() {
        if let Some(loc) = guard.get(&cache_key) {
            return Ok(Some(loc.clone()));
        }
    }

    let jdk = reaper_dir(gradle_root).join("java-sources/jdk");
    if !jdk.join(".extracted").is_file() {
        return Ok(None);
    }
    let rel = format!("java/lang/{symbol}.java");
    let source_path = ["java.base", ""]
        .into_iter()
        .filter_map(|module| {
            let path = if module.is_empty() {
                jdk.join(&rel)
            } else {
                jdk.join(module).join(&rel)
            };
            path.is_file().then_some(path)
        })
        .next();
    let Some(source_path) = source_path else {
        return Ok(None);
    };

    let rel_path = rel_path_for(ws, &source_path)
        .map(|p| normalize_index_path(ws, gradle_root, &p))
        .unwrap_or_else(|_| normalize_index_path(ws, gradle_root, &source_path.to_string_lossy()));
    let (line, column) = read_type_line_in_java_source(&source_path, symbol);
    let loc = SymbolLocation {
        name: symbol.to_string(),
        kind: "class".into(),
        path: rel_path,
        line,
        column,
    };
    if let Ok(mut guard) = JDK_LOCATION_CACHE.lock() {
        guard.insert(cache_key, loc.clone());
    }
    Ok(Some(loc))
}

fn read_type_line_in_java_source(path: &Path, simple: &str) -> (u32, u32) {
    let Ok(file) = std::fs::File::open(path) else {
        return (1, 1);
    };
    let reader = BufReader::new(file);
    for (idx, line) in reader.lines().map_while(Result::ok).take(160).enumerate() {
        for keyword in ["class", "interface", "enum", "@interface"] {
            if java_type_on_line(&line, keyword).as_deref() == Some(simple) {
                let col = line.find(simple).map(|i| i as u32 + 1).unwrap_or(1);
                return (idx as u32 + 1, col);
            }
        }
    }
    (1, 1)
}

fn source_matches_fqcn(content: &str, fqcn: &str, source_path: &Path) -> bool {
    let Some((pkg, class_name)) = fqcn.rsplit_once('.') else {
        return true;
    };
    let rel = source_path.to_string_lossy().replace('\\', "/");
    package_from_source_or_path(content, &rel)
        .is_some_and(|p| p == pkg)
        && content.lines().any(|line| {
            line.contains("class ")
                && line.contains(class_name)
                || line.contains("interface ")
                    && line.contains(class_name)
                || line.contains("@interface ")
                    && line.contains(class_name)
                || line.contains("enum ")
                    && line.contains(class_name)
        })
}

fn find_file_by_name(dir: &Path, file_name: &str) -> Option<PathBuf> {
    let mut stack = vec![dir.to_path_buf()];
    let mut seen = 0usize;
    while let Some(current) = stack.pop() {
        if seen > 50_000 {
            break;
        }
        let Ok(entries) = std::fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            seen += 1;
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if entry.file_name().to_string_lossy() == file_name {
                return Some(path);
            }
        }
    }
    None
}

fn index_java_dir(
    ws: &Path,
    dir: &Path,
    symbols: &mut Vec<IndexedSymbol>,
    progress: Option<&Box<dyn Fn(&str, usize) + Send>>,
) -> Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    index_java_dir_inner(ws, dir, dir, symbols, progress)
}

fn index_java_dir_inner(
    ws: &Path,
    _root: &Path,
    dir: &Path,
    symbols: &mut Vec<IndexedSymbol>,
    progress: Option<&Box<dyn Fn(&str, usize) + Send>>,
) -> Result<()> {
    let entries = std::fs::read_dir(dir)?;
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            index_java_dir_inner(ws, _root, &path, symbols, progress)?;
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) != Some("java") {
            continue;
        }
        let rel = rel_path_for(ws, &path).unwrap_or_else(|_| {
            path.strip_prefix(ws)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/")
        });
        if !should_index_file(&rel) {
            continue;
        }
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        index_java_content(&content, &rel, should_index_methods(&rel), symbols);
        if let Some(cb) = progress {
            if symbols.len() % 32 == 0 {
                cb("indexing", symbols.len());
            }
        }
    }
    Ok(())
}

/// Skip heavy JDK modules (desktop, swing, etc.) — keep common API surface for navigation.
fn should_index_file(rel_path: &str) -> bool {
    let rel = rel_path.replace('\\', "/");
    if !rel.contains(".reaper/java-sources/jdk/") {
        return true;
    }
    if JDK_SOURCE_MODULES
        .iter()
        .any(|module| rel.contains(&format!(".reaper/java-sources/jdk/{module}")))
    {
        return true;
    }
    // JDK 8 flat layout: .../jdk/java/lang/String.java
    for prefix in ["/java/", "/javax/"] {
        if rel.contains(prefix) {
            return true;
        }
    }
    false
}

/// Methods are indexed for project code, Spring, and core JDK packages used in completions.
fn should_index_methods(rel_path: &str) -> bool {
    let rel = rel_path.replace('\\', "/");
    if rel.contains("src/main/java/") || rel.contains("src/test/java/") {
        return true;
    }
    if rel.contains("/build/generated/") && rel.ends_with(".java") {
        return true;
    }
    if rel.contains("/generated-sources/") && rel.ends_with(".java") {
        return true;
    }
    if rel.contains("/src/") && rel.ends_with(".java") && !rel.contains("/build/classes/") {
        return true;
    }
    if rel.contains("/org/springframework/") {
        return true;
    }
    if rel.contains(".reaper/java-sources/jdk/") {
        return rel.contains("/java/lang/")
            || rel.contains("/java/util/")
            || rel.contains("/java/io/")
            || rel.contains("java.base/java/lang/")
            || rel.contains("java.base/java/util/")
            || rel.contains("java.base/java/io/");
    }
    false
}

fn index_java_content(content: &str, rel_path: &str, index_methods: bool, symbols: &mut Vec<IndexedSymbol>) {
    let package = package_from_source_or_path(content, rel_path);
    let pkg_prefix = package.as_deref().map(|p| format!("{p}."));

    let mut current_class: Option<String> = None;

    for (idx, line) in content.lines().enumerate() {
        let line_no = idx as u32 + 1;
        for (kind, keyword) in [
            ("class", "class"),
            ("interface", "interface"),
            ("enum", "enum"),
            ("annotation", "@interface"),
        ] {
            if let Some(name) = java_type_on_line(line, keyword) {
                current_class = Some(name.clone());
                let qualified = match &pkg_prefix {
                    Some(prefix) => format!("{prefix}{name}"),
                    None => name.clone(),
                };
                let col = line.find(&name).map(|i| i as u32 + 1).unwrap_or(1);
                symbols.push(IndexedSymbol {
                    name,
                    qualified,
                    kind: kind.to_string(),
                    path: rel_path.to_string(),
                    line: line_no,
                    column: col,
                });
            }
        }

        if let Some(class) = &current_class {
            if index_methods {
                if let Some(method) = super::symbols::java_method_name_on_line(line) {
                    let qualified = match &pkg_prefix {
                        Some(prefix) => format!("{prefix}{class}.{method}"),
                        None => format!("{class}.{method}"),
                    };
                    let col = line.find(&method).map(|i| i as u32 + 1).unwrap_or(1);
                    symbols.push(IndexedSymbol {
                        name: method,
                        qualified,
                        kind: "method".into(),
                        path: rel_path.to_string(),
                        line: line_no,
                        column: col,
                    });
                }
                if let Some(field) = java_field_name_on_line(line) {
                    let qualified = match &pkg_prefix {
                        Some(prefix) => format!("{prefix}{class}.{field}"),
                        None => format!("{class}.{field}"),
                    };
                    let col = line.find(&field).map(|i| i as u32 + 1).unwrap_or(1);
                    symbols.push(IndexedSymbol {
                        name: field,
                        qualified,
                        kind: "field".into(),
                        path: rel_path.to_string(),
                        line: line_no,
                        column: col,
                    });
                }
            }
        }
    }
}

fn java_field_name_on_line(line: &str) -> Option<String> {
    let trimmed = line.split("//").next()?.trim();
    if trimmed.is_empty()
        || trimmed.contains('(')
        || trimmed.ends_with('{')
        || trimmed.ends_with('}')
        || !trimmed.ends_with(';')
    {
        return None;
    }
    if trimmed.starts_with("import ")
        || trimmed.starts_with("package ")
        || trimmed.starts_with('@')
    {
        return None;
    }
    let without_semi = trimmed.trim_end_matches(';').trim();
    let before_assign = without_semi.split('=').next()?.trim();
    let name = before_assign
        .rsplit(|c: char| c.is_whitespace())
        .next()?
        .trim();
    if name.is_empty() || super::symbols::is_keyword(name) {
        return None;
    }
    if name.chars().any(|c| c == '<' || c == '>') {
        return None;
    }
    Some(name.to_string())
}

fn java_type_on_line(line: &str, keyword: &str) -> Option<String> {
    let trimmed = line.split("//").next()?.trim();
    if trimmed.starts_with("package ") || trimmed.starts_with("import ") {
        return None;
    }
    let pattern = if keyword == "@interface" {
        "@interface"
    } else {
        keyword
    };
    let re_needle = format!("{pattern} ");
    let pos = trimmed.find(&re_needle)?;
    let rest = &trimmed[pos + re_needle.len()..];
    let name: String = rest
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    if name.is_empty() {
        return None;
    }
    Some(name)
}

fn find_package(source: &str) -> Option<String> {
    for line in source.lines() {
        let trimmed = line.split("//").next()?.trim();
        if let Some(rest) = trimmed.strip_prefix("package ") {
            if let Some(pkg) = rest.strip_suffix(';') {
                let pkg = pkg.trim();
                if !pkg.is_empty() {
                    return Some(pkg.to_string());
                }
            }
        }
    }
    None
}

fn package_from_source_or_path(content: &str, rel_path: &str) -> Option<String> {
    find_package(content).or_else(|| infer_package_from_java_path(rel_path))
}

fn infer_package_from_java_path(rel_path: &str) -> Option<String> {
    let norm = rel_path.replace('\\', "/");
    for marker in ["src/main/java/", "src/test/java/"] {
        if let Some(rest) = norm.split_once(marker).map(|(_, tail)| tail) {
            if let Some((pkg_path, file)) = rest.rsplit_once('/') {
                if file.ends_with(".java") && !pkg_path.is_empty() {
                    return Some(pkg_path.replace('/', "."));
                }
            }
        }
    }
    let parts: Vec<&str> = norm.split('/').collect();
    for (i, part) in parts.iter().enumerate() {
        if !matches!(*part, "org" | "com" | "javax" | "jakarta" | "java" | "kotlin") {
            continue;
        }
        if i + 1 >= parts.len() {
            continue;
        }
        let file = parts[parts.len() - 1];
        if !file.ends_with(".java") {
            continue;
        }
        let pkg_parts = &parts[i..parts.len() - 1];
        if !pkg_parts.is_empty() {
            return Some(pkg_parts.join("."));
        }
    }
    None
}

fn parse_imports(content: &str) -> ImportMap {
    let mut explicit = HashMap::new();
    let mut wildcards = Vec::new();
    for line in content.lines() {
        let trimmed = line.split("//").next().unwrap_or(line).trim();
        if let Some(rest) = trimmed.strip_prefix("import static ") {
            if rest.contains('*') {
                continue;
            }
            if let Some(fqcn) = rest.strip_suffix(';').map(str::trim) {
                if let Some(simple) = fqcn.rsplit('.').next() {
                    explicit.insert(simple.to_string(), fqcn.to_string());
                }
            }
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("import ") {
            if rest.contains('*') {
                if let Some(prefix) = rest.strip_suffix(';').and_then(|s| s.strip_suffix(".*")) {
                    let prefix = prefix.trim();
                    if !prefix.is_empty() {
                        wildcards.push(prefix.to_string());
                    }
                }
                continue;
            }
            if let Some(fqcn) = rest.strip_suffix(';').map(str::trim) {
                if let Some(simple) = fqcn.rsplit('.').next() {
                    explicit.insert(simple.to_string(), fqcn.to_string());
                }
            }
        }
    }
    ImportMap { explicit, wildcards }
}

#[derive(Clone)]
struct ImportMap {
    explicit: HashMap<String, String>,
    wildcards: Vec<String>,
}

fn lookup_imported_symbol<'a>(
    lookup: &'a IndexLookup,
    gradle_root: &Path,
    symbol: &str,
    imports: &ImportMap,
) -> Option<&'a IndexedSymbol> {
    resolve_type_fqcn(lookup, symbol, imports, gradle_root)
        .and_then(|fqcn| lookup.type_by_qualified(&fqcn))
}

fn import_fqcns(symbol: &str, imports: &ImportMap) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(fqcn) = imports.explicit.get(symbol) {
        out.push(fqcn.clone());
    }
    for prefix in &imports.wildcards {
        out.push(format!("{prefix}.{symbol}"));
    }
    out
}

fn is_library_fqcn(fqcn: &str) -> bool {
    fqcn.starts_with("java.")
        || fqcn.starts_with("javax.")
        || fqcn.starts_with("jakarta.")
        || fqcn.starts_with("org.springframework.")
        || fqcn.starts_with("kotlin.")
}

fn is_java_lang_simple_type(symbol: &str) -> bool {
    matches!(
        symbol,
        "String"
            | "Object"
            | "Integer"
            | "Long"
            | "Boolean"
            | "Character"
            | "Byte"
            | "Short"
            | "Float"
            | "Double"
            | "Class"
            | "Throwable"
            | "Exception"
            | "RuntimeException"
            | "Error"
            | "System"
            | "Math"
            | "StringBuilder"
            | "StringBuffer"
            | "Void"
            | "Enum"
            | "Record"
            | "Thread"
            | "Runnable"
            | "Comparable"
            | "Iterable"
            | "AutoCloseable"
            | "Cloneable"
    )
}

fn is_java_util_simple_type(symbol: &str) -> bool {
    matches!(
        symbol,
        "List"
            | "ArrayList"
            | "LinkedList"
            | "Set"
            | "HashSet"
            | "LinkedHashSet"
            | "TreeSet"
            | "Map"
            | "HashMap"
            | "LinkedHashMap"
            | "TreeMap"
            | "Collection"
            | "Iterable"
            | "Queue"
            | "Deque"
            | "ArrayDeque"
            | "Optional"
            | "Iterator"
            | "Stream"
            | "Objects"
            | "Collections"
            | "Comparator"
            | "UUID"
            | "Date"
            | "Calendar"
    )
}

fn import_match_priority(fqcn: &str, imports: &ImportMap) -> u8 {
    if imports.explicit.values().any(|v| v == fqcn) {
        return 0;
    }
    if imports
        .wildcards
        .iter()
        .any(|prefix| fqcn.starts_with(&format!("{prefix}.")))
    {
        return 1;
    }
    4
}

fn library_source_dirs(gradle_root: &Path) -> Vec<PathBuf> {
    let key = gradle_root
        .canonicalize()
        .unwrap_or_else(|_| gradle_root.to_path_buf())
        .display()
        .to_string();
    if let Ok(guard) = LIBRARY_SOURCE_DIRS_CACHE.lock() {
        if let Some(dirs) = guard.get(&key) {
            return dirs.clone();
        }
    }

    let sources = reaper_dir(gradle_root).join("java-sources");
    let mut dirs = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&sources) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() && entry.file_name() != "jdk" {
                dirs.push(path);
            }
        }
    }

    if let Ok(mut guard) = LIBRARY_SOURCE_DIRS_CACHE.lock() {
        guard.insert(key, dirs.clone());
    }
    dirs
}

fn cache_fqcn_location(cache_key: &str, loc: &SymbolLocation) {
    let Ok(mut guard) = JDK_LOCATION_CACHE.lock() else {
        return;
    };
    if guard.len() >= DEFINITION_CACHE_MAX {
        guard.clear();
    }
    guard.insert(cache_key.to_string(), loc.clone());
}

fn resolve_type_by_fqcn(
    ws: &Path,
    root: &Path,
    lookup: &IndexLookup,
    fqcn: &str,
    symbol: &str,
) -> Option<SymbolLocation> {
    let cache_key = format!("{}:{}", root.display(), fqcn);
    if let Ok(guard) = JDK_LOCATION_CACHE.lock() {
        if let Some(loc) = guard.get(&cache_key) {
            return Some(loc.clone());
        }
    }

    if let Some(hit) = lookup.type_by_qualified(fqcn) {
        let loc = to_location(ws, root, hit);
        cache_fqcn_location(&cache_key, &loc);
        return Some(loc);
    }

    if is_library_fqcn(fqcn) {
        if let Some(loc) = resolve_library_type_location(ws, root, fqcn, symbol) {
            cache_fqcn_location(&cache_key, &loc);
            return Some(loc);
        }
    }

    None
}

fn resolve_imported_type(
    ws: &Path,
    root: &Path,
    lookup: &IndexLookup,
    symbol: &str,
    imports: &ImportMap,
) -> Option<SymbolLocation> {
    for fqcn in import_fqcns(symbol, imports) {
        if let Some(loc) = resolve_type_by_fqcn(ws, root, lookup, &fqcn, symbol) {
            return Some(loc);
        }
    }
    None
}

fn resolve_library_type_location(
    ws: &Path,
    gradle_root: &Path,
    fqcn: &str,
    symbol: &str,
) -> Option<SymbolLocation> {
    let _ = ensure_navigation_sources(ws, gradle_root);
    let dirs = library_source_dirs(gradle_root);
    if dirs.is_empty() {
        return None;
    }
    let source_path = find_java_source_for_fqcn(ws, gradle_root, &dirs, fqcn)?;
    let rel = rel_path_for(ws, &source_path)
        .map(|p| normalize_index_path(ws, gradle_root, &p))
        .unwrap_or_else(|_| normalize_index_path(ws, gradle_root, &source_path.to_string_lossy()));
    let simple = fqcn.rsplit('.').next().unwrap_or(symbol);
    let (line, column) = read_type_line_in_java_source(&source_path, simple);
    Some(SymbolLocation {
        name: symbol.to_string(),
        kind: "class".into(),
        path: rel,
        line,
        column,
    })
}

fn resolve_type_fqcn(
    lookup: &IndexLookup,
    symbol: &str,
    imports: &ImportMap,
    gradle_root: &Path,
) -> Option<String> {
    if let Some(fqcn) = imports.explicit.get(symbol) {
        return Some(fqcn.clone());
    }

    let jdk = reaper_dir(gradle_root).join("java-sources/jdk");

    let lang = format!("java.lang.{symbol}");
    if lookup.type_by_qualified(&lang).is_some()
        || jdk_source_exists(&jdk, &lang)
        || is_java_lang_simple_type(symbol)
    {
        return Some(lang);
    }

    let util = format!("java.util.{symbol}");
    if lookup.type_by_qualified(&util).is_some()
        || jdk_source_exists(&jdk, &util)
        || is_java_util_simple_type(symbol)
    {
        return Some(util);
    }

    for prefix in &imports.wildcards {
        let fqcn = format!("{prefix}.{symbol}");
        if lookup.type_by_qualified(&fqcn).is_some()
            || is_library_fqcn(&fqcn)
            || jdk_source_exists(&jdk, &fqcn)
        {
            return Some(fqcn);
        }
    }

    let mut candidates: Vec<String> = lookup
        .types_named(symbol)
        .map(|sym| sym.qualified.clone())
        .collect();
    if !candidates.is_empty() {
        candidates.sort_by(|a, b| {
            import_match_priority(a, imports)
                .cmp(&import_match_priority(b, imports))
                .then_with(|| spring_priority(a).cmp(&spring_priority(b)))
        });
        return Some(candidates[0].clone());
    }

    resolve_fqcn_from_jdk_files(gradle_root, symbol, imports)
        .or_else(|| well_known_import(symbol).map(str::to_string))
}

fn find_method_in_index<'a>(
    lookup: &'a IndexLookup,
    fqcn: &str,
    method: &str,
) -> Option<&'a IndexedSymbol> {
    lookup.method_by_qualified(&format!("{fqcn}.{method}"))
}

pub(crate) fn is_java_like(path: &str) -> bool {
    let lower = path.to_lowercase();
    lower.ends_with(".java") || lower.ends_with(".kt") || lower.ends_with(".kts")
}

fn is_annotation_context(content: &str, line: u32, column: u32) -> bool {
    let line_text = match content.lines().nth(line.saturating_sub(1) as usize) {
        Some(l) => l,
        None => return false,
    };
    let col = column.saturating_sub(1) as usize;
    let before = &line_text[..col.min(line_text.len())];
    before.rfind('@').is_some_and(|at| {
        before[at + 1..]
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
    })
}

fn spring_priority(qualified: &str) -> u8 {
    if qualified.starts_with("org.springframework.boot.") {
        0
    } else if qualified.starts_with("org.springframework.") {
        1
    } else if qualified.starts_with("jakarta.") || qualified.starts_with("javax.") {
        2
    } else if qualified.starts_with("java.") || qualified.starts_with("jdk.") {
        3
    } else {
        4
    }
}

fn to_location(ws: &Path, gradle_root: &Path, hit: &IndexedSymbol) -> SymbolLocation {
    SymbolLocation {
        name: hit.name.clone(),
        kind: hit.kind.clone(),
        path: normalize_index_path(ws, gradle_root, &hit.path),
        line: hit.line,
        column: hit.column,
    }
}

/// Legacy indexes stored `.reaper/...` or absolute paths; normalize to workspace-relative.
fn normalize_index_path(ws: &Path, gradle_root: &Path, path: &str) -> String {
    let rel = path.replace('\\', "/");
    if rel.starts_with('/') {
        if let Ok(stripped) = rel_path_for(ws, Path::new(&rel)) {
            return stripped;
        }
    }
    if rel.starts_with(".reaper/") {
        if let Ok(prefix) = rel_path_for(ws, gradle_root) {
            if !prefix.is_empty() {
                return format!("{prefix}/{rel}");
            }
        }
    }
    rel
}

fn rel_path_for(ws: &Path, path: &Path) -> Result<String> {
    let ws_canon = ws.canonicalize()?;
    let path_canon = path.canonicalize()?;
    Ok(path_canon
        .strip_prefix(&ws_canon)
        .with_context(|| "path outside workspace")?
        .to_string_lossy()
        .replace('\\', "/"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn write_minimal_jdk_sources(ws: &Path) {
        let jdk = ws.join(".reaper/java-sources/jdk");
        std::fs::create_dir_all(jdk.join("java.base/java/lang")).unwrap();
        std::fs::create_dir_all(jdk.join("java.base/java/io")).unwrap();
        std::fs::write(
            jdk.join("java.base/java/lang/System.java"),
            "package java.lang;\n\nimport java.io.PrintStream;\n\npublic final class System {\n    public static final PrintStream out = null;\n    public static final java.io.InputStream in = null;\n    public static final PrintStream err = null;\n}\n",
        )
        .unwrap();
        std::fs::write(
            jdk.join("java.base/java/io/PrintStream.java"),
            "package java.io;\n\npublic class PrintStream {\n    public void println(String x) {}\n    public void print(String x) {}\n    public void flush() {}\n}\n",
        )
        .unwrap();
        std::fs::write(jdk.join(".extracted"), "test").unwrap();
    }

    #[test]
    fn discovers_gradle_generated_output_dirs() {
        let root = std::env::temp_dir().join(format!(
            "reaper-gradle-out-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("build/classes/java/main/com/example")).unwrap();
        std::fs::write(
            root.join("build/classes/java/main/com/example/Generated.class"),
            b"",
        )
        .unwrap();
        std::fs::create_dir_all(
            root.join("build/generated/sources/annotationProcessor/java/main/com/example"),
        )
        .unwrap();
        std::fs::write(
            root.join("build/generated/sources/annotationProcessor/java/main/com/example/Generated.java"),
            "package com.example;\npublic class Generated {}\n",
        )
        .unwrap();

        let (classes, sources) = discover_gradle_output_dirs(&root);
        assert!(classes.iter().any(|p| p.ends_with("build/classes/java/main")));
        assert!(sources.iter().any(|p| p.ends_with("java/main")));

        let entries = list_dir_class_entries(&classes[0]).expect("class entries");
        assert!(entries.iter().any(|(fqcn, _)| fqcn == "com.example.Generated"));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn discovers_maven_generated_output_dirs() {
        let root = std::env::temp_dir().join(format!(
            "reaper-maven-out-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("target/classes/com/example")).unwrap();
        std::fs::create_dir_all(
            root.join("target/generated-sources/annotations/com/example"),
        )
        .unwrap();
        std::fs::write(
            root.join("target/generated-sources/annotations/com/example/Generated.java"),
            "package com.example;\npublic class Generated {}\n",
        )
        .unwrap();

        let (classes, sources) = discover_project_output_dirs(&root);
        assert!(classes.iter().any(|p| p.ends_with("target/classes")));
        assert!(sources.iter().any(|p| p.ends_with("annotations")));

        let _ = std::fs::remove_dir_all(&root);
    }

    fn plain_java_completion_workspace(name: &str) -> PathBuf {
        let ws = std::env::temp_dir().join(name);
        let _ = std::fs::remove_dir_all(&ws);
        std::fs::create_dir_all(ws.join("src")).unwrap();
        std::fs::write(
            ws.join("src/HelloWorld.java"),
            "public class HelloWorld {\n  public static void main(String[] args) {\n    System.out.println(\"hi\");\n  }\n}\n",
        )
        .unwrap();
        write_minimal_jdk_sources(&ws);
        warm_index(&ws).expect("warm_index");
        ws
    }

    #[test]
    fn plain_java_system_dot_member_completions() {
        let ws = plain_java_completion_workspace("reaper-plain-java-system-dot");
        let path = "src/HelloWorld.java";
        let content = "public class HelloWorld {\n  public static void main(String[] args) {\n    System.\n  }\n}\n";
        let items = java_completions(&ws, path, 3, 12, content, "", &[]).expect("completions");
        let labels: Vec<_> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            labels.iter().any(|l| *l == "out"),
            "expected out in System. completions, got {:?}",
            labels
        );
        assert!(
            labels.iter().any(|l| *l == "in"),
            "expected in in System. completions, got {:?}",
            labels
        );
        assert!(
            labels.iter().any(|l| *l == "err"),
            "expected err in System. completions, got {:?}",
            labels
        );
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn plain_java_system_out_dot_member_completions() {
        let ws = plain_java_completion_workspace("reaper-plain-java-system-out-dot");
        let path = "src/HelloWorld.java";
        let content = "public class HelloWorld {\n  public static void main(String[] args) {\n    System.out.\n  }\n}\n";
        let items = java_completions(&ws, path, 3, 16, content, "", &[]).expect("completions");
        let labels: Vec<_> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            labels.iter().any(|l| *l == "println"),
            "expected println in System.out. completions, got {:?}",
            labels
        );
        assert!(
            labels.iter().any(|l| *l == "print"),
            "expected print in System.out. completions, got {:?}",
            labels
        );
        let _ = std::fs::remove_dir_all(&ws);
    }


    #[test]
    fn classpath_includes_junit_jars() {
        let jars = vec![PathBuf::from(
            "/home/.m2/repository/org/junit/jupiter/junit-jupiter-api/5.11.4/junit-jupiter-api-5.11.4.jar",
        )];
        assert!(classpath_includes_test_deps(&jars));
        assert!(!classpath_includes_test_deps(&[PathBuf::from(
            "/home/.m2/repository/com/google/guava/guava/31.1-jre/guava-31.1-jre.jar"
        )]));
    }

    #[test]
    fn file_needs_test_classpath_from_path_and_content() {
        assert!(file_needs_test_classpath(
            "src/test/java/com/example/AppTest.java",
            "class AppTest {}"
        ));
        assert!(file_needs_test_classpath(
            "src/main/java/com/example/App.java",
            "import org.junit.jupiter.api.Test;\nclass App { @Test void x() {} }"
        ));
        assert!(!file_needs_test_classpath(
            "src/main/java/com/example/App.java",
            "class App { void x() {} }"
        ));
    }

    #[test]
    fn parses_gradle_coordinates() {
        let text = r#"
            implementation "com.google.guava:guava:31.1-jre"
            api 'org.junit.jupiter:junit-jupiter:5.9.3'
        "#;
        let coords = parse_gradle_coordinates(
            text,
            &GradleVersionCatalog::default(),
            &HashMap::new(),
            Path::new("/nonexistent"),
        );
        assert_eq!(coords.len(), 2);
        assert_eq!(coords[0].0, "com.google.guava");
        assert_eq!(coords[1].1, "junit-jupiter");
    }

    #[test]
    fn parses_gradle_plugin_declarations() {
        let build = r#"
plugins {
    id 'java'
    id 'org.springframework.boot' version '3.2.0'
    id("org.jetbrains.kotlin.jvm") version "1.9.22"
}
"#;
        let decls = parse_gradle_plugin_declarations(build, &GradleVersionCatalog::default());
        assert!(decls.iter().any(|p| p.id == "org.springframework.boot" && p.version == "3.2.0"));
        assert!(decls.iter().any(|p| p.id == "org.jetbrains.kotlin.jvm" && p.version == "1.9.22"));
        assert!(!decls.iter().any(|p| p.id == "java"));
    }

    #[test]
    fn plugin_import_boms_maps_known_plugins() {
        let spring = plugin_import_boms("org.springframework.boot", "3.2.0");
        assert_eq!(spring.len(), 1);
        assert_eq!(spring[0].0, "org.springframework.boot");
        assert_eq!(spring[0].1, "spring-boot-dependencies");

        let kotlin = plugin_import_boms("org.jetbrains.kotlin.jvm", "1.9.22");
        assert_eq!(kotlin[0].1, "kotlin-bom");

        assert!(plugin_import_boms("java", "1.0").is_empty());
    }

    #[test]
    fn parses_dependency_management_maven_bom() {
        let build = r#"
plugins {
    id 'io.spring.dependency-management' version '1.1.4'
}
dependencyManagement {
    imports {
        mavenBom "org.springframework.boot:spring-boot-dependencies:3.2.0"
    }
}
"#;
        let boms = collect_dependency_management_boms(build, &GradleVersionCatalog::default());
        assert!(boms.iter().any(|(g, a, v)| {
            g == "org.springframework.boot" && a == "spring-boot-dependencies" && v == "3.2.0"
        }));
    }

    #[test]
    fn parses_gradle_plugin_catalog_alias() {
        let catalog = parse_gradle_version_catalog(
            r#"
[plugins]
spring-boot = { id = "org.springframework.boot", version = "3.2.0" }
"#,
        );
        let build = r#"
plugins {
    alias(libs.plugins.spring.boot)
}
"#;
        let decls = parse_gradle_plugin_declarations(&build, &catalog);
        assert_eq!(decls.len(), 1);
        assert_eq!(decls[0].id, "org.springframework.boot");
        assert_eq!(decls[0].version, "3.2.0");
    }

    #[test]
    fn resolves_gradle_versionless_coordinates_from_bom() {
        let mut management = HashMap::new();
        management.insert(
            "org.springframework.boot:spring-boot-starter-web".into(),
            "3.2.0".into(),
        );
        management.insert(
            "org.springframework.boot:spring-boot-starter-test".into(),
            "3.2.0".into(),
        );
        let build = r#"
dependencies {
    implementation 'org.springframework.boot:spring-boot-starter-web'
    testImplementation 'org.springframework.boot:spring-boot-starter-test'
}
"#;
        let coords = parse_gradle_coordinates(
            build,
            &GradleVersionCatalog::default(),
            &management,
            Path::new("/nonexistent"),
        );
        assert!(coords.iter().any(|(g, a, v)| {
            g == "org.springframework.boot" && a == "spring-boot-starter-web" && v == "3.2.0"
        }));
        assert!(coords.iter().any(|(g, a, v)| {
            g == "org.springframework.boot" && a == "spring-boot-starter-test" && v == "3.2.0"
        }));
    }

    #[test]
    fn parses_gradle_version_catalog_and_libs_references() {
        let catalog = parse_gradle_version_catalog(
            r#"
[versions]
junit-jupiter = "5.11.1"
guava = "33.3.1-jre"

[libraries]
junit-jupiter = { module = "org.junit.jupiter:junit-jupiter", version.ref = "junit-jupiter" }
guava = { module = "com.google.guava:guava", version.ref = "guava" }
"#,
        );
        assert_eq!(
            catalog.resolve("junit.jupiter"),
            Some((
                "org.junit.jupiter".into(),
                "junit-jupiter".into(),
                "5.11.1".into()
            ))
        );
        let build = r#"
dependencies {
    testImplementation libs.junit.jupiter
    implementation libs.guava
    testRuntimeOnly 'org.junit.platform:junit-platform-launcher'
}
"#;
        let coords = parse_gradle_coordinates(
            &build,
            &catalog,
            &HashMap::new(),
            Path::new("/nonexistent"),
        );
        assert!(coords.iter().any(|(g, a, v)| g == "org.junit.jupiter" && a == "junit-jupiter" && v == "5.11.1"));
        assert!(coords.iter().any(|(g, a, _)| g == "com.google.guava" && a == "guava"));
        assert!(coords.iter().any(|(g, a, _)| g == "org.junit.platform" && a == "junit-platform-launcher"));
        assert_eq!(coords.len(), 3);
    }

    #[test]
    fn parses_imports() {
        let src = "package com.example;\n\nimport org.springframework.web.bind.annotation.*;\nimport org.springframework.web.bind.annotation.RestController;\n";
        let map = parse_imports(src);
        assert_eq!(
            map.explicit.get("RestController"),
            Some(&"org.springframework.web.bind.annotation.RestController".to_string())
        );
        assert_eq!(
            map.wildcards,
            vec!["org.springframework.web.bind.annotation".to_string()]
        );
    }

    #[test]
    fn resolves_java_lang_implicit() {
        let index = JavaIndex {
            project_root: ".".into(),
            symbols: vec![IndexedSymbol {
                name: "String".into(),
                qualified: "java.lang.String".into(),
                kind: "class".into(),
                path: ".reaper/java-sources/jdk/java/lang/String.java".into(),
                line: 1,
                column: 1,
            }],
        };
        let imports = parse_imports("package com.example;\n");
        let lookup = IndexLookup::from_index(index);
        let hit = lookup_imported_symbol(&lookup, Path::new("."), "String", &imports);
        assert_eq!(hit.map(|s| s.qualified.as_str()), Some("java.lang.String"));
    }

    #[test]
    fn resolves_string_fqcn_without_index_entry() {
        let lookup = IndexLookup::from_index(JavaIndex {
            project_root: ".".into(),
            symbols: vec![],
        });
        let imports = parse_imports("package com.example;\n");
        let fqcn = resolve_type_fqcn(&lookup, "String", &imports, Path::new("."));
        assert_eq!(fqcn.as_deref(), Some("java.lang.String"));
        let list = resolve_type_fqcn(&lookup, "List", &imports, Path::new("."));
        assert_eq!(list.as_deref(), Some("java.util.List"));
    }

    #[test]
    fn class_kind_detects_annotation_from_classfile() {
        let dir = std::env::temp_dir().join(format!("reaper-ann-kind-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("Sample.java"),
            "public @interface Sample { String value() default \"\"; }\n",
        )
        .unwrap();
        let status = Command::new("javac")
            .arg(dir.join("Sample.java"))
            .status()
            .unwrap();
        if !status.success() {
            let _ = std::fs::remove_dir_all(&dir);
            return;
        }
        let bytes = std::fs::read(dir.join("Sample.class")).unwrap();
        assert_eq!(class_kind_from_bytes(&bytes, "Sample"), "annotation");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn is_annotation_index_symbol_matches_library_annotations() {
        let spring = IndexedSymbol {
            name: "GetMapping".into(),
            qualified: "org.springframework.web.bind.annotation.GetMapping".into(),
            kind: "annotation".into(),
            path: "jar".into(),
            line: 1,
            column: 1,
        };
        assert!(is_annotation_index_symbol(&spring));
        let plain = IndexedSymbol {
            name: "Hello".into(),
            qualified: "com.example.Hello".into(),
            kind: "class".into(),
            path: "src".into(),
            line: 1,
            column: 1,
        };
        assert!(!is_annotation_index_symbol(&plain));
    }

    #[test]
    fn finds_annotation_on_line() {
        assert_eq!(
            java_type_on_line("@interface RestController {", "@interface"),
            Some("RestController".to_string())
        );
    }

    #[test]
    fn parses_field_name_with_assignment() {
        assert_eq!(
            java_field_name_on_line("    public static final PrintStream out = null;"),
            Some("out".to_string())
        );
    }

    #[test]
    fn resolves_system_out_field_type() {
        assert_eq!(
            java_field_type_on_line("    public static final PrintStream out = null;", "out"),
            Some("PrintStream".to_string())
        );
    }

    #[test]
    fn gradle_for_paren_type_completions() {
        let ws = Path::new("/Users/sunny/Library/Application Support/Reaper/workspaces/Hello-world-gradle");
        if !ws.is_dir() {
            return;
        }
        let path = "src/main/java/com/example/HelloWorld.java";
        let content = "package com.example;\n\npublic class HelloWorld {\n    public static void main(String[] args) {\n        for(\n    }\n}";
        let items = java_completions(ws, path, 5, 13, content, "", &[]).unwrap_or_default();
        assert!(
            items.iter().any(|i| i.label == "int" || i.label == "String"),
            "expected type completions at for(, got {:?}",
            items.iter().map(|i| i.label.clone()).take(12).collect::<Vec<_>>()
        );
    }

    #[test]
    fn gradle_system_out_member_completions() {
        let ws = Path::new("/Users/sunny/Library/Application Support/Reaper/workspaces/Hello-world-gradle");
        if !ws.is_dir() {
            return;
        }
        let path = "src/main/java/com/example/HelloWorld.java";
        let content = "package com.example;\n\npublic class HelloWorld {\n    public static void main(String[] args) {\n        System.out.\n    }\n}";
        let items = java_completions(ws, path, 5, 20, content, "", &[]).unwrap_or_default();
        assert!(
            items.iter().any(|i| i.label == "println"),
            "expected println in System.out completions, got {:?}",
            items.iter().map(|i| i.label.clone()).take(12).collect::<Vec<_>>()
        );
    }

    #[test]
    fn indexes_plain_java_src_layout() {
        let ws = std::env::temp_dir().join("reaper-plain-java-index");
        let _ = std::fs::remove_dir_all(&ws);
        std::fs::create_dir_all(ws.join("src")).unwrap();
        std::fs::write(
            ws.join("src/HelloWorld.java"),
            "public class HelloWorld {\n  public static void main(String[] args) {\n    System.out.println(\"hi\");\n  }\n}\n",
        )
        .unwrap();
        std::fs::create_dir_all(ws.join(".reaper/java-sources/jdk/java.base/java/lang")).unwrap();
        std::fs::write(
            ws.join(".reaper/java-sources/jdk/java.base/java/lang/String.java"),
            "package java.lang;\n\npublic final class String {\n}\n",
        )
        .unwrap();
        std::fs::write(ws.join(".reaper/java-sources/jdk/.extracted"), "test").unwrap();

        let status = warm_index(&ws).expect("warm_index");
        assert!(
            status.symbol_count > 0,
            "expected symbols, got {}",
            status.symbol_count
        );
        let index_text =
            std::fs::read_to_string(ws.join(".reaper/java-index.json")).expect("java-index.json");
        assert!(index_text.contains("HelloWorld"));
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn maven_java_member_completions() {
        let ws = Path::new("/Users/sunny/Library/Application Support/Reaper/workspaces/Hello-world");
        if !ws.is_dir() {
            return;
        }
        let path = "src/main/java/com/helloworld/HelloWorld.java";
        let content = std::fs::read_to_string(ws.join(path)).unwrap_or_default();
        let items = java_completions(ws, path, 7, 11, &content, "", &[]).unwrap_or_default();
        let string_dot = {
            let line = "String.";
            java_completions(ws, path, 1, 7, line, "", &[]).unwrap_or_default()
        };
        assert!(items.len() >= 5, "expected index members for a., got {}", items.len());
        assert!(
            string_dot.len() >= 5,
            "expected String members, got {}",
            string_dot.len()
        );
    }

    #[test]
    fn warm_maven_hello_workspace() {
        let ws = Path::new("/Users/sunny/Library/Application Support/Reaper/workspaces/Hello-world");
        if !ws.is_dir() {
            return;
        }
        let status = warm_index(ws).expect("warm_index");
        eprintln!(
            "indexed={} symbols={} jars={} jdk={}",
            status.indexed,
            status.symbol_count,
            status.dependency_jars,
            status.jdk_symbols
        );
        assert!(status.symbol_count > 0, "expected symbols");
    }

    #[test]
    fn warm_spring_boot_workspace() {
        let ws = Path::new("/Users/sunny/Library/Application Support/Reaper/workspaces/somayaj/spring-boot");
        if !ws.is_dir() {
            return;
        }
        let status = warm_index(ws).expect("warm_index");
        eprintln!(
            "indexed={} symbols={} jars={} jdk={}",
            status.indexed,
            status.symbol_count,
            status.dependency_jars,
            status.jdk_symbols
        );
        assert!(status.symbol_count > 0, "expected symbols");
    }

    #[test]
    fn normalizes_absolute_index_path() {
        let ws = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data/workspaces/somayaj");
        let root = ws.join("repo-1");
        let abs = root.join(".reaper/java-sources/jdk/java.base/java/lang/String.java");
        if !abs.is_file() {
            return;
        }
        let normalized = normalize_index_path(&ws, &root, &abs.to_string_lossy());
        assert!(
            !normalized.starts_with('/'),
            "expected workspace-relative path, got {normalized}"
        );
        assert!(normalized.contains("repo-1/.reaper/java-sources"));
    }

    #[test]
    fn indexes_jdk_flat_and_modular_layouts() {
        assert!(should_index_file(
            "repo/.reaper/java-sources/jdk/java.base/java/lang/String.java"
        ));
        assert!(should_index_file(
            "repo/.reaper/java-sources/jdk/java/lang/String.java"
        ));
        assert!(!should_index_file(
            "repo/.reaper/java-sources/jdk/jdk.internal/misc/Unsafe.java"
        ));
    }

    #[test]
    fn resolves_jdk_type_from_materialized_sources() {
        let ws = std::env::temp_dir().join("reaper-jdk-nav-test");
        let _ = std::fs::remove_dir_all(&ws);
        std::fs::create_dir_all(ws.join(".reaper/java-sources/jdk/java.base/java/lang")).unwrap();
        std::fs::write(
            ws.join(".reaper/java-sources/jdk/java.base/java/lang/String.java"),
            "package java.lang;\n\npublic final class String {\n}\n",
        )
        .unwrap();
        std::fs::write(ws.join(".reaper/java-sources/jdk/.extracted"), "test").unwrap();
        std::fs::write(ws.join("build.gradle"), "plugins { id 'java' }\n").unwrap();

        let imports = parse_imports("package com.example;\n");
        let loc = resolve_jdk_type_location(&ws, &ws, "String", &imports)
            .expect("lookup ok")
            .expect("String location");
        assert!(loc.path.contains("String.java"));
        assert_eq!(loc.line, 3);
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn resolves_spring_type_from_dependency_sources() {
        let ws = std::env::temp_dir().join("reaper-spring-nav-test");
        let _ = std::fs::remove_dir_all(&ws);
        std::fs::create_dir_all(
            ws.join(".reaper/java-sources/deps/spring_web/org/springframework/web/bind/annotation"),
        )
        .unwrap();
        std::fs::write(
            ws.join(".reaper/java-sources/deps/spring_web/org/springframework/web/bind/annotation/RestController.java"),
            "package org.springframework.web.bind.annotation;\n\npublic @interface RestController {\n}\n",
        )
        .unwrap();
        std::fs::write(ws.join("build.gradle"), "plugins { id 'java' }\n").unwrap();
        std::fs::create_dir_all(ws.join("src/main/java/com/example")).unwrap();
        std::fs::write(
            ws.join("src/main/java/com/example/App.java"),
            "package com.example;\n\nimport org.springframework.web.bind.annotation.RestController;\n\n@RestController\npublic class App {\n}\n",
        )
        .unwrap();

        let content = std::fs::read_to_string(ws.join("src/main/java/com/example/App.java")).unwrap();
        let hit = find_external_definition(&ws, "src/main/java/com/example/App.java", 5, 2, &content)
            .expect("lookup ok")
            .expect("RestController location");
        assert!(hit.path.contains("RestController.java"));
        assert_eq!(hit.line, 3);

        let again = find_external_definition(&ws, "src/main/java/com/example/App.java", 5, 2, &content)
            .expect("cached lookup ok")
            .expect("cached RestController");
        assert_eq!(hit.path, again.path);
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn definition_lookup_is_cached() {
        let ws = std::env::temp_dir().join("reaper-jdk-def-cache");
        let _ = std::fs::remove_dir_all(&ws);
        std::fs::create_dir_all(ws.join(".reaper/java-sources/jdk/java.base/java/lang")).unwrap();
        std::fs::write(
            ws.join(".reaper/java-sources/jdk/java.base/java/lang/String.java"),
            "package java.lang;\n\npublic final class String {\n}\n",
        )
        .unwrap();
        std::fs::write(ws.join(".reaper/java-sources/jdk/.extracted"), "test").unwrap();
        std::fs::write(ws.join("build.gradle"), "plugins { id 'java' }\n").unwrap();
        std::fs::create_dir_all(ws.join("src/main/java")).unwrap();
        std::fs::write(
            ws.join("src/main/java/App.java"),
            "package app;\n\nclass App {\n  String name;\n}\n",
        )
        .unwrap();

        let content = std::fs::read_to_string(ws.join("src/main/java/App.java")).unwrap();
        let first = find_external_definition(&ws, "src/main/java/App.java", 4, 5, &content)
            .expect("lookup ok")
            .expect("String location");
        let second = find_external_definition(&ws, "src/main/java/App.java", 4, 5, &content)
            .expect("lookup ok")
            .expect("cached String location");
        assert_eq!(first.path, second.path);
        assert_eq!(first.line, second.line);
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn tooling_classpath_cache_preferred_over_tree_walk() {
        let ws = std::env::temp_dir().join("reaper-tooling-classpath-pref");
        let _ = std::fs::remove_dir_all(&ws);
        std::fs::create_dir_all(ws.join(".reaper")).unwrap();
        std::fs::write(ws.join("build.gradle"), "plugins { id 'java' }\n").unwrap();

        let tooling_only = ws.join("slf4j-api-tooling.jar");
        std::fs::write(&tooling_only, b"PK").unwrap();
        save_classpath_jars_cache(&ws, &[tooling_only.clone()]).unwrap();
        mark_tooling_classpath_done(&ws).unwrap();

        assert!(tooling_classpath_resolved(&ws));
        let jars = resolve_classpath_jars_preferring_build_tree(&ws, true);
        assert!(
            jars.iter().any(|p| p == &tooling_only),
            "expected tooling compile classpath JAR, got {:?}",
            jars
        );
        assert!(needs_tooling_classpath_resolve(&ws) == false);
        std::fs::write(ws.join("build.gradle"), "plugins { id 'java' }\ndependencies { implementation 'org.slf4j:slf4j-api:2.0.9' }\n").unwrap();
        assert!(
            needs_tooling_classpath_resolve(&ws),
            "build file change should invalidate tooling classpath"
        );
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn should_index_mockito_and_slf4j_jar_classes() {
        assert!(should_index_jar_class("org.mockito.Mock"));
        assert!(should_index_jar_class("org.mockito.junit.jupiter.MockitoExtension"));
        assert!(should_index_jar_class("org.slf4j.Logger"));
        assert!(should_index_jar_class("lombok.extern.slf4j.Slf4j"));
    }

    #[test]
    fn build_file_tree_merged_with_tooling_cache() {
        let ws = std::env::temp_dir().join("reaper-classpath-tree-merge");
        let _ = std::fs::remove_dir_all(&ws);
        std::fs::create_dir_all(ws.join(".reaper")).unwrap();
        std::fs::write(
            ws.join("build.gradle"),
            r#"
plugins { id 'java' }
dependencies {
    testImplementation 'org.mockito:mockito-core:5.14.2'
}
"#,
        )
        .unwrap();

        let tooling_only = ws.join("guava-tooling.jar");
        std::fs::write(&tooling_only, b"PK").unwrap();
        save_classpath_jars_cache(&ws, &[tooling_only.clone()]).unwrap();
        mark_tooling_classpath_done(&ws).unwrap();

        // Mockito may not resolve offline without M2 cache; at minimum tooling JAR is kept.
        let jars = resolve_classpath_jars_preferring_build_tree(&ws, true);
        assert!(
            jars.iter().any(|p| p == &tooling_only),
            "expected tooling jar preserved, got {:?}",
            jars
        );
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn spring_data_deps_requires_commons_not_starter() {
        let starter = PathBuf::from("/m2/spring-boot-starter-data-jpa-3.2.0.jar");
        assert!(!classpath_includes_spring_data_deps(&[starter]));
        let commons = PathBuf::from("/m2/spring-data-commons-3.2.0.jar");
        assert!(classpath_includes_spring_data_deps(&[commons]));
    }

    #[test]
    fn jar_index_skips_jdk_classes_and_prioritizes_spring_data() {
        assert!(!should_index_jar_class("java.lang.String"));
        assert!(should_index_jar_class("org.springframework.data.domain.PageRequest"));
        assert!(jar_is_index_priority("spring-data-commons-3.2.0.jar"));
        let mut jars: Vec<PathBuf> = (0..121)
            .map(|i| PathBuf::from(format!("/tmp/filler-{i}.jar")))
            .collect();
        jars.push(PathBuf::from("/tmp/spring-data-commons-3.2.0.jar"));
        let ordered = prioritize_jars_for_fallback(&jars);
        assert!(ordered[0].to_string_lossy().contains("spring-data"));
    }
}
