use std::collections::{HashMap, HashSet};
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
const INDEX_VERSION: u32 = 2;

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
}

struct CachedLookup {
    mtime: SystemTime,
    stamp: String,
    lookup: Arc<IndexLookup>,
}

static LOOKUP_CACHE: LazyLock<Mutex<HashMap<String, CachedLookup>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn invalidate_lookup_cache(gradle_root: &Path) {
    if let Ok(key) = gradle_root.canonicalize() {
        if let Ok(mut guard) = LOOKUP_CACHE.lock() {
            guard.remove(&key.display().to_string());
        }
    }
}

fn get_lookup(ws: &Path, gradle_root: &Path) -> Result<Arc<IndexLookup>> {
    let key = gradle_root
        .canonicalize()
        .unwrap_or_else(|_| gradle_root.to_path_buf())
        .display()
        .to_string();

    let cache_path = reaper_dir(gradle_root).join("java-index.json");
    let (mtime, stamp) = if cache_path.is_file() {
        (
            std::fs::metadata(&cache_path)?.modified()?,
            std::fs::read_to_string(reaper_dir(gradle_root).join("classpath.stamp"))
                .unwrap_or_default(),
        )
    } else {
        (SystemTime::UNIX_EPOCH, String::new())
    };

    if let Ok(guard) = LOOKUP_CACHE.lock() {
        if let Some(entry) = guard.get(&key) {
            if entry.mtime == mtime && entry.stamp == stamp {
                return Ok(Arc::clone(&entry.lookup));
            }
        }
    }

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
}

pub fn is_gradle_workspace(ws: &Path) -> bool {
    super::gradle::find_all_gradle_roots(ws)
        .map(|roots| !roots.is_empty())
        .unwrap_or(false)
}

fn reaper_dir(gradle_root: &Path) -> PathBuf {
    gradle_root.join(".reaper")
}

/// Resolved compile/runtime JAR paths for javac (Spring, JDK libs, etc.).
pub fn compile_classpath_jars(gradle_root: &Path) -> Result<Vec<PathBuf>> {
    Ok(resolve_gradle_classpath(gradle_root)?.jars)
}

pub fn warm_index(ws: &Path) -> Result<WarmIndexStatus> {
    let roots = super::gradle::find_all_gradle_roots(ws)?;
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
    };

    for root in roots {
        let cached = is_index_cached(ws, &root)?;
        let index = if cached {
            load_index(ws, &root)?
        } else {
            build_index(ws, &root)?
        };
        let meta = index_meta(&root);
        combined.indexed = true;
        combined.project_root = Some(index.project_root.clone());
        combined.symbol_count += index.symbols.len();
        combined.cached = combined.cached && cached;
        combined.dependency_jars += meta.dependency_jars;
        combined.source_jars += meta.source_jars;
        combined.jdk_sources = combined.jdk_sources || meta.jdk_sources;
    }

    Ok(combined)
}

/// Read index status from disk without building (for UI polling).
pub fn peek_index_status(ws: &Path) -> Result<WarmIndexStatus> {
    let roots = super::gradle::find_all_gradle_roots(ws)?;
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
        }
    }

    Ok(combined)
}

pub fn search_indexed_classes(ws: &Path, query: &str, limit: usize) -> Result<Vec<ClassSearchHit>> {
    let roots = super::gradle::find_all_gradle_roots(ws)?;
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
            if query.trim().is_empty()
                && (path_norm.contains(".reaper/") || path_norm.contains("/org/springframework/"))
            {
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

fn indexed_class_priority(path: &str, qualified: &str) -> u32 {
    let path = path.replace('\\', "/");
    if path.contains(".reaper/")
        || path.contains("/org/springframework/")
        || qualified.starts_with("java.")
        || qualified.starts_with("jdk.")
    {
        0
    } else if path.contains("/src/") || !qualified.contains('.') {
        300
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

    let Some(root) = find_gradle_root(ws, from_path)? else {
        return Ok(None);
    };

    let lookup = get_lookup(ws, &root)?;
    let symbol = match super::symbols::word_at(content, line, column) {
        Some(s) if !s.is_empty() && !super::symbols::is_keyword(&s) => s,
        _ => return Ok(None),
    };

    let imports = parse_imports(content);

    if let Some(type_name) =
        super::symbols::java_member_qualifier(content, line, column, &symbol)
    {
        if let Some(fqcn) = resolve_type_fqcn(&lookup, &type_name, &imports) {
            if let Some(hit) = find_method_in_index(&lookup, &fqcn, &symbol) {
                return Ok(Some(to_location(ws, &root, hit)));
            }
        }
    }

    if let Some(fqcn) = super::symbols::java_class_from_source_path(from_path) {
        if let Some(hit) = find_method_in_index(&lookup, &fqcn, &symbol) {
            return Ok(Some(to_location(ws, &root, hit)));
        }
    }

    if let Some(hit) = lookup_imported_symbol(&lookup, &symbol, &imports) {
        return Ok(Some(to_location(ws, &root, hit)));
    }

    let mut candidates: Vec<&IndexedSymbol> = lookup.types_named(&symbol).collect();

    if candidates.is_empty() {
        let mut methods: Vec<&IndexedSymbol> = lookup.methods_named(&symbol).collect();
        if methods.is_empty() {
            return Ok(None);
        }
        methods.sort_by_key(|s| spring_priority(&s.qualified));
        return Ok(Some(to_location(ws, &root, methods[0])));
    }

    if candidates.len() == 1 {
        return Ok(Some(to_location(ws, &root, candidates[0])));
    }

    candidates.sort_by_key(|s| spring_priority(&s.qualified));
    Ok(Some(to_location(ws, &root, candidates[0])))
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

    let Some(root) = find_gradle_root(ws, from_path)? else {
        return Ok(Vec::new());
    };

    let lookup = get_lookup(ws, &root)?;
    let at_annotation = is_annotation_context(content, line, column);
    let prefix = if prefix.is_empty() {
        super::symbols::word_at(content, line, column).unwrap_or_default()
    } else {
        prefix.to_string()
    };

    if prefix.is_empty() && !at_annotation {
        return Ok(Vec::new());
    }

    let prefix_lower = prefix.to_lowercase();

    let mut seen = HashSet::new();
    let mut items = Vec::new();

    for sym in lookup.symbols.iter() {
        if at_annotation && sym.kind != "annotation" {
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
        items.push(CompletionItem {
            label: sym.name.clone(),
            kind: sym.kind.clone(),
            detail: Some(sym.qualified.clone()),
            path: Some(normalize_index_path(ws, &root, &sym.path)),
            line: Some(sym.line),
            column: Some(sym.column),
        });
        if items.len() >= 80 {
            break;
        }
    }

    items.sort_by(|a, b| a.label.len().cmp(&b.label.len()).then_with(|| a.label.cmp(&b.label)));
    Ok(items)
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

    if super::gradle::is_spring_boot_project(gradle_root) {
        let meta = index_meta(gradle_root);
        if meta.dependency_jars == 0 {
            return Ok(false);
        }
        if !super::spring_props::has_cached_properties(gradle_root) {
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
    if let Ok(jdk) = java_home() {
        parts.push(format!("jdk:{}", jdk.display()));
    }
    Ok(parts.join("|"))
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct IndexMeta {
    dependency_jars: usize,
    source_jars: usize,
    jdk_sources: bool,
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

fn build_index(ws: &Path, gradle_root: &Path) -> Result<JavaIndex> {
    invalidate_lookup_cache(gradle_root);
    let project_root = rel_path_for(ws, gradle_root)?;
    let classpath = resolve_gradle_classpath(gradle_root).unwrap_or_else(|e| {
        tracing::warn!("Gradle classpath resolution failed for {}: {e:#}", gradle_root.display());
        GradleClasspath::default()
    });
    if classpath.jars.is_empty() {
        tracing::warn!(
            "No dependency JARs resolved for {} — Java/Spring indexing will be incomplete",
            gradle_root.display()
        );
    }
    if classpath.source_jars.is_empty() {
        tracing::warn!(
            "No source JARs resolved for {} — run ./gradlew build or check network access to Maven sources",
            gradle_root.display()
        );
    }

    let (source_dirs, jdk_sources) =
        materialize_sources(ws, gradle_root, &classpath.jars, &classpath.source_jars)?;
    if let Err(e) = super::spring_props::build_index(ws, gradle_root, &classpath.jars) {
        tracing::warn!("Spring properties index failed for {}: {e:#}", gradle_root.display());
    }

    let mut symbols = Vec::new();
    for dir in &source_dirs {
        index_java_dir(ws, dir, &mut symbols)?;
    }

    for rel in ["src/main/java", "src/test/java"] {
        let project_src = gradle_root.join(rel);
        if project_src.is_dir() {
            index_java_dir(ws, &project_src, &mut symbols)?;
        }
    }

    symbols.sort_by(|a, b| a.qualified.cmp(&b.qualified));

    let index = JavaIndex {
        project_root,
        symbols,
    };

    std::fs::create_dir_all(reaper_dir(gradle_root))?;
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
            index_version: INDEX_VERSION,
        },
    )?;

    invalidate_lookup_cache(gradle_root);

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
        "-I".into(),
        init_str.to_string(),
        "reaperPrintClasspath".into(),
        "-q".into(),
        "--console=plain".into(),
    ]);
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();

    let out = run_gradle_with_command(&cmd, &arg_refs)?;
    if !out.success() {
        tracing::warn!(
            "reaperPrintClasspath exited {} for {}: {}",
            out.exit_code,
            gradle_root.display(),
            out.stderr.trim()
        );
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
        for candidate in [home.join("lib/src.zip"), home.join("src.zip")] {
            if candidate.is_file() {
                return Ok(Some(candidate));
            }
        }
    }
    Ok(None)
}

fn jdk_home_candidates() -> Result<Vec<PathBuf>> {
    let mut homes = Vec::new();

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

fn java_home() -> Result<PathBuf> {
    crate::jdk::effective_java_home()
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

fn index_java_dir(ws: &Path, dir: &Path, symbols: &mut Vec<IndexedSymbol>) -> Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    index_java_dir_inner(ws, dir, dir, symbols)
}

fn index_java_dir_inner(
    ws: &Path,
    _root: &Path,
    dir: &Path,
    symbols: &mut Vec<IndexedSymbol>,
) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            index_java_dir_inner(ws, _root, &path, symbols)?;
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
    }
    Ok(())
}

/// Skip heavy JDK modules (desktop, swing, etc.) — java.base covers String, Object, etc.
fn should_index_file(rel_path: &str) -> bool {
    let rel = rel_path.replace('\\', "/");
    if !rel.contains(".reaper/java-sources/jdk/") {
        return true;
    }
    rel.contains(".reaper/java-sources/jdk/java.base/")
}

/// Methods are indexed for project code and Spring Framework only (not the full JDK).
fn should_index_methods(rel_path: &str) -> bool {
    let rel = rel_path.replace('\\', "/");
    if rel.contains("src/main/java/") || rel.contains("src/test/java/") {
        return true;
    }
    rel.contains("/org/springframework/")
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
            }
        }
    }
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

struct ImportMap {
    explicit: HashMap<String, String>,
    wildcards: Vec<String>,
}

fn lookup_imported_symbol<'a>(
    lookup: &'a IndexLookup,
    symbol: &str,
    imports: &ImportMap,
) -> Option<&'a IndexedSymbol> {
    resolve_type_fqcn(lookup, symbol, imports).and_then(|fqcn| lookup.type_by_qualified(&fqcn))
}

fn resolve_type_fqcn(lookup: &IndexLookup, symbol: &str, imports: &ImportMap) -> Option<String> {
    if let Some(fqcn) = imports.explicit.get(symbol) {
        return Some(fqcn.clone());
    }

    let lang = format!("java.lang.{symbol}");
    if lookup.type_by_qualified(&lang).is_some() {
        return Some(lang);
    }

    for prefix in &imports.wildcards {
        let fqcn = format!("{prefix}.{symbol}");
        if lookup.type_by_qualified(&fqcn).is_some() {
            return Some(fqcn);
        }
    }

    None
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
        let hit = lookup_imported_symbol(&lookup, "String", &imports);
        assert_eq!(hit.map(|s| s.qualified.as_str()), Some("java.lang.String"));
    }

    #[test]
    fn finds_annotation_on_line() {
        assert_eq!(
            java_type_on_line("@interface RestController {", "@interface"),
            Some("RestController".to_string())
        );
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
}
