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
const INDEX_VERSION: u32 = 6;

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

/// Drop cached Java indexes so the next warm_index rebuilds (e.g. after branch checkout).
pub fn invalidate_caches(ws: &Path) -> Result<()> {
    for root in find_all_index_roots(ws)? {
        invalidate_lookup_cache(&root);
        let stamp = reaper_dir(&root).join("classpath.stamp");
        let _ = std::fs::remove_file(stamp);
    }
    Ok(())
}

fn reaper_dir(gradle_root: &Path) -> PathBuf {
    gradle_root.join(".reaper")
}

/// Resolved compile/runtime JAR paths for javac (Spring, JDK libs, etc.).
const CLASSPATH_JARS_CACHE: &str = "classpath-jars.json";

/// Cached dependency JARs from the last successful index — never runs Gradle.
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

pub fn compile_classpath_jars(gradle_root: &Path) -> Result<Vec<PathBuf>> {
    let cached = cached_classpath_jars(gradle_root);
    if !cached.is_empty() {
        return Ok(cached);
    }
    Ok(resolve_gradle_classpath(gradle_root)?.jars)
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
        if at_annotation && sym.kind != "annotation" {
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
        });
    }
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

        if items.len() < 8 {
            for item in members_from_type_source(ws, root, fqcn, member_prefix)? {
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
                for item in members_from_type_source(ws, root, &obj_fqcn, member_prefix)? {
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

    Ok(items)
}

fn member_source_dirs(gradle_root: &Path) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    for rel in ["src/main/java", "src/test/java", "src"] {
        let p = gradle_root.join(rel);
        if p.is_dir() {
            dirs.push(p);
        }
    }
    dirs.extend(library_source_dirs(gradle_root));
    let jdk = reaper_dir(gradle_root).join("java-sources/jdk");
    if jdk.is_dir() {
        dirs.push(jdk);
    }
    dirs
}

fn members_from_type_source(
    ws: &Path,
    gradle_root: &Path,
    fqcn: &str,
    member_prefix: &str,
) -> Result<Vec<CompletionItem>> {
    let dirs = member_source_dirs(gradle_root);
    let Some(source_path) = find_java_source_for_fqcn(ws, gradle_root, &dirs, fqcn) else {
        return Ok(Vec::new());
    };
    let content = std::fs::read_to_string(&source_path)?;
    let rel = rel_path_for(ws, &source_path).unwrap_or_else(|_| {
        source_path
            .strip_prefix(ws)
            .unwrap_or(&source_path)
            .to_string_lossy()
            .replace('\\', "/")
    });
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
        items.push(CompletionItem {
            label: sym.name.clone(),
            kind: sym.kind.clone(),
            detail: Some(fqcn.to_string()),
            insert: None,
            path: Some(normalize_index_path(ws, gradle_root, &sym.path)),
            line: Some(sym.line),
            column: Some(sym.column),
        });
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
                    meta.modified()?.elapsed()?.as_nanos()
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
            "No dependency JARs resolved for {} — indexing project sources and JDK only (no Gradle run)",
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
        materialize_sources(ws, gradle_root, &classpath.jars, &classpath.source_jars)?;
    if !classpath.jars.is_empty() {
        if let Err(e) = super::spring_props::build_index(ws, gradle_root, &classpath.jars) {
            tracing::warn!("Spring properties index failed for {}: {e:#}", gradle_root.display());
        }
    }

    let mut symbols = Vec::new();
    report(progress, "indexing", 0);
    for dir in &source_dirs {
        index_java_dir(ws, dir, &mut symbols, progress)?;
        report(progress, "indexing", symbols.len());
    }

    index_all_java_source_trees(ws, gradle_root, &mut symbols, progress)?;

    index_jar_classpath_fallback(ws, gradle_root, &classpath.jars, &source_dirs, &mut symbols)?;
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
) -> Result<(Vec<PathBuf>, bool)> {
    let dest_root = reaper_dir(gradle_root).join("java-sources");
    std::fs::create_dir_all(&dest_root)?;

    let mut dirs = Vec::new();
    let mut extracted = HashSet::new();

    for sources in source_jars {
        if let Some(dir) = extract_sources_jar(ws, &dest_root.join("deps"), sources, &mut extracted)? {
            dirs.push(dir);
        }
    }

    for jar in jars {
        let key = jar.to_string_lossy().to_string();
        if extracted.contains(&key) {
            continue;
        }
        if let Some(sources) = find_sources_jar(jar) {
            if let Some(dir) =
                extract_sources_jar(ws, &dest_root.join("deps"), &sources, &mut extracted)?
            {
                dirs.push(dir);
            }
        }
    }

    let mut jdk_sources = false;
    if let Some(jdk_dir) = materialize_jdk_sources(&dest_root.join("jdk"))? {
        jdk_sources = true;
        dirs.push(jdk_dir);
    }

    Ok((dirs, jdk_sources))
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
    log: String,
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

/// Resolve dependency JARs without running Gradle/Maven when possible (cache file, local caches).
fn resolve_classpath_for_index(gradle_root: &Path) -> GradleClasspath {
    if super::maven::is_maven_project_root(gradle_root) {
        return resolve_classpath_for_maven(gradle_root);
    }
    let cached = cached_classpath_jars(gradle_root);
    if !cached.is_empty() {
        tracing::info!(
            "Using {} cached classpath JARs for {}",
            cached.len(),
            gradle_root.display()
        );
        let source_jars = discover_source_jars_for_jars(&cached);
        return GradleClasspath {
            jars: cached,
            source_jars,
            log: "from classpath-jars.json cache".into(),
        };
    }

    let offline = resolve_classpath_from_gradle_cache(gradle_root);
    if !offline.jars.is_empty() {
        tracing::info!(
            "Resolved {} JARs from local Gradle cache (no Gradle run) for {}",
            offline.jars.len(),
            gradle_root.display()
        );
        return offline;
    }

    if std::env::var("REAPER_INDEX_USE_GRADLE").as_deref() == Ok("1") {
        if let Ok(gradle) = resolve_gradle_classpath(gradle_root) {
            if !gradle.jars.is_empty() {
                tracing::info!(
                    "Resolved {} JARs via Gradle for {}",
                    gradle.jars.len(),
                    gradle_root.display()
                );
                return gradle;
            }
        }
    }

    GradleClasspath::default()
}

fn resolve_classpath_for_maven(maven_root: &Path) -> GradleClasspath {
    let cached = cached_classpath_jars(maven_root);
    if !cached.is_empty() {
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
        };
    }

    let offline = resolve_classpath_from_m2(maven_root);
    if !offline.jars.is_empty() {
        tracing::info!(
            "Resolved {} JARs from local Maven repository (no mvn run) for {}",
            offline.jars.len(),
            maven_root.display()
        );
        return offline;
    }

    if std::env::var("REAPER_INDEX_USE_MAVEN").as_deref() == Ok("1") {
        if let Ok(maven) = resolve_maven_classpath_via_mvn(maven_root) {
            if !maven.jars.is_empty() {
                tracing::info!(
                    "Resolved {} JARs via Maven for {}",
                    maven.jars.len(),
                    maven_root.display()
                );
                return maven;
            }
        }
    }

    GradleClasspath::default()
}

fn resolve_classpath_from_m2(maven_root: &Path) -> GradleClasspath {
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

fn resolve_maven_classpath_via_mvn(maven_root: &Path) -> Result<GradleClasspath> {
    let out_file = reaper_dir(maven_root).join("maven-classpath.txt");
    std::fs::create_dir_all(reaper_dir(maven_root))?;
    let output = std::process::Command::new("mvn")
        .current_dir(maven_root)
        .args([
            "-q",
            "dependency:build-classpath",
            "-Dmdep.includeScope=compile",
            "-Dmdep.outputFile",
            out_file.to_str().context("classpath output path")?,
        ])
        .output()
        .context("spawn mvn")?;
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
    let files_root = gradle_user_home().join("caches/modules-2/files-2.1");
    if !files_root.is_dir() {
        return GradleClasspath::default();
    }

    let mut jars = HashSet::new();
    for (group, artifact, version) in collect_dependency_coordinates(gradle_root) {
        if let Some(jar) = find_cached_jar(&files_root, &group, &artifact, &version) {
            jars.insert(jar);
        }
        if jars.len() >= MAX_OFFLINE_CLASSPATH_JARS {
            break;
        }
    }

    if super::gradle::is_spring_boot_project(gradle_root)
        || super::maven::is_spring_boot_project(gradle_root)
    {
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
    }

    let jar_list: Vec<PathBuf> = jars.into_iter().take(MAX_OFFLINE_CLASSPATH_JARS).collect();
    GradleClasspath {
        jars: jar_list.clone(),
        source_jars: discover_source_jars_for_jars(&jar_list),
        log: format!("from Gradle dependency cache ({} JARs)", jar_list.len()),
    }
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
    if super::maven::is_maven_project_root(gradle_root) {
        return super::maven::collect_dependency_coordinates(gradle_root);
    }
    let mut coords = Vec::new();
    let mut seen = HashSet::new();
    collect_coordinates_from_dir(gradle_root, 0, 5, &mut coords, &mut seen);
    coords
}

fn collect_coordinates_from_dir(
    dir: &Path,
    depth: usize,
    max_depth: usize,
    out: &mut Vec<(String, String, String)>,
    seen: &mut HashSet<String>,
) {
    if depth > max_depth || !dir.is_dir() {
        return;
    }
    for name in ["build.gradle", "build.gradle.kts"] {
        let path = dir.join(name);
        if path.is_file() {
            if let Ok(text) = std::fs::read_to_string(&path) {
                for coord in parse_gradle_coordinates(&text) {
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
        collect_coordinates_from_dir(&path, depth + 1, max_depth, out, seen);
    }
}

fn parse_gradle_coordinates(content: &str) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    for line in content.lines() {
        let line = line.split("//").next().unwrap_or(line);
        for token in extract_quoted_tokens(line) {
            if let Some(coord) = parse_coordinate_token(&token) {
                out.push(coord);
            }
        }
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

fn resolve_gradle_classpath(gradle_root: &Path) -> Result<GradleClasspath> {
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
        "-I".into(),
        init_str.to_string(),
    ]);

    let mut gradle_log = String::new();

    // Resolve/download dependency JARs before printing classpath.
    let warm_args: Vec<&str> = args
        .iter()
        .map(String::as_str)
        .chain(["compileJava", "-q", "--console=plain"].iter().copied())
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
        }
    }
    Ok(GradleClasspath {
        jars: jars.into_iter().collect(),
        source_jars: source_jars.into_iter().collect(),
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

fn index_jar_classpath_fallback(
    ws: &Path,
    gradle_root: &Path,
    jars: &[PathBuf],
    source_dirs: &[PathBuf],
    symbols: &mut Vec<IndexedSymbol>,
) -> Result<()> {
    let mut known: HashSet<String> = symbols.iter().map(|s| s.qualified.clone()).collect();
    let mut added = 0usize;

    for jar in jars {
        if !jar.is_file() {
            continue;
        }
        let entries = list_jar_class_entries(jar)?;
        for (fqcn, kind) in entries {
            if known.contains(&fqcn) {
                continue;
            }
            if !should_index_jar_class(&fqcn) {
                continue;
            }
            let name = fqcn.rsplit('.').next().unwrap_or(&fqcn).to_string();
            let Some(source_path) = find_java_source_for_fqcn(ws, gradle_root, source_dirs, &fqcn) else {
                continue;
            };
            if let Ok(content) = std::fs::read_to_string(&source_path) {
                let rel = rel_path_for(ws, &source_path).unwrap_or_else(|_| {
                    source_path
                        .strip_prefix(ws)
                        .unwrap_or(&source_path)
                        .to_string_lossy()
                        .replace('\\', "/")
                });
                let before = symbols.len();
                index_java_content(&content, &rel, should_index_methods(&rel), symbols);
                for sym in &symbols[before..] {
                    known.insert(sym.qualified.clone());
                }
                added += symbols.len().saturating_sub(before);
                continue;
            }
            symbols.push(IndexedSymbol {
                name,
                qualified: fqcn.clone(),
                kind,
                path: rel_path_for(ws, &source_path).unwrap_or_else(|_| {
                    source_path
                        .strip_prefix(ws)
                        .unwrap_or(&source_path)
                        .to_string_lossy()
                        .replace('\\', "/")
                }),
                line: 1,
                column: 1,
            });
            known.insert(fqcn);
            added += 1;
        }
    }

    if added > 0 {
        tracing::info!("Indexed {added} additional Java symbols from classpath JAR fallback");
    }
    Ok(())
}

fn should_index_jar_class(fqcn: &str) -> bool {
    fqcn.starts_with("org.springframework.")
        || fqcn.starts_with("jakarta.")
        || fqcn.starts_with("javax.")
        || fqcn.starts_with("java.")
        || fqcn.starts_with("kotlin.")
        || fqcn.starts_with("org.junit.")
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
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        let line = line.trim();
        if !line.ends_with(".class") || line.contains('$') {
            continue;
        }
        let fqcn = line.trim_end_matches(".class").replace('/', ".");
        let kind = if fqcn.contains(".") {
            let simple = fqcn.rsplit('.').next().unwrap_or("");
            if simple.ends_with("Exception") || simple.ends_with("Error") {
                "class".to_string()
            } else {
                "class".to_string()
            }
        } else {
            continue;
        };
        entries.push((fqcn, kind));
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
                if source_matches_fqcn(&content, fqcn) {
                    return Some(found);
                }
            }
        }
    }
    let reaper_sources = reaper_dir(gradle_root).join("java-sources");
    if reaper_sources.is_dir() {
        if let Some(found) = find_file_by_name(&reaper_sources, file_name) {
            if let Ok(content) = std::fs::read_to_string(&found) {
                if source_matches_fqcn(&content, fqcn) {
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

fn source_matches_fqcn(content: &str, fqcn: &str) -> bool {
    let Some(pkg) = fqcn.rsplit_once('.') else {
        return true;
    };
    let (pkg, class_name) = pkg;
    find_package(content).is_some_and(|p| p == pkg)
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
            if symbols.len() % 64 == 0 {
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
    if rel.contains("/src/") && rel.ends_with(".java") && !rel.contains("/build/") {
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
    let package = find_package(content);
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
        let items = java_completions(&ws, path, 3, 12, content, "").expect("completions");
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
        let items = java_completions(&ws, path, 3, 16, content, "").expect("completions");
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
    fn parses_gradle_coordinates() {
        let text = r#"
            implementation "com.google.guava:guava:31.1-jre"
            api 'org.junit.jupiter:junit-jupiter:5.9.3'
        "#;
        let coords = parse_gradle_coordinates(text);
        assert_eq!(coords.len(), 2);
        assert_eq!(coords[0].0, "com.google.guava");
        assert_eq!(coords[1].1, "junit-jupiter");
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
        let items = java_completions(ws, path, 5, 13, content, "").unwrap_or_default();
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
        let items = java_completions(ws, path, 5, 20, content, "").unwrap_or_default();
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
        let items = java_completions(ws, path, 7, 11, &content, "").unwrap_or_default();
        let string_dot = {
            let line = "String.";
            java_completions(ws, path, 1, 7, line, "").unwrap_or_default()
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
}
