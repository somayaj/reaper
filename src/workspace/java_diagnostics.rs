use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::classpath;
use super::diagnostics::Diagnostic;
use super::java_javac_inflight::{self, CancellableOutput};
use super::gradle::{self, find_gradle_root};
use super::maven::find_maven_root;
use super::{safe_join};

const DIAG_ROOT: &str = ".reaper/java-diagnostics";
const DIAG_OUT: &str = ".reaper/java-diagnostics-out";
const TEST_COMPILE_CACHE: &str = "test-compile-cache.json";
const TEST_COMPILE_TTL: Duration = Duration::from_secs(120);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum JavaDiagScope {
    /// Active-file keystroke path: single-file javac, no Gradle test compile, no tab overlays.
    #[default]
    Typing,
    /// Save / classpath refresh: companions, Gradle test compile, all open-tab overlays.
    Full,
}

pub type JavaDiagnostic = Diagnostic;

fn run_cancellable_javac(
    ws: &Path,
    rel_path: &str,
    content: &str,
    args: &[&str],
) -> Result<CancellableOutput> {
    java_javac_inflight::run_cancellable_java_command(
        ws,
        rel_path,
        "javac",
        args,
        fingerprint(content),
    )
}

fn fingerprint(content: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    hasher.finish()
}

pub fn check_java(
    ws: &Path,
    rel_path: &str,
    content: &str,
    overlays: &[(String, String)],
    scope: JavaDiagScope,
) -> Result<(Vec<Diagnostic>, bool)> {
    if !rel_path.ends_with(".java") {
        return Ok((Vec::new(), false));
    }

    let _ = safe_join(ws, rel_path)?;

    let project_root = find_gradle_root(ws, rel_path)?.or(find_maven_root(ws, rel_path)?);
    if let Some(root) = project_root.as_deref() {
        check_project_java(ws, root, rel_path, content, overlays, scope)
    } else {
        check_plain_java(ws, rel_path, content)
    }
}

fn check_project_java(
    ws: &Path,
    project_root: &Path,
    rel_path: &str,
    content: &str,
    overlays: &[(String, String)],
    scope: JavaDiagScope,
) -> Result<(Vec<Diagnostic>, bool)> {
    if scope == JavaDiagScope::Full {
        let _ = classpath::ensure_test_classpath_for_file(project_root, rel_path, content);

        if classpath::file_needs_test_classpath(rel_path, content)
            && workspace_file_matches_disk(ws, rel_path, content)
            && gradle::find_gradle_wrapper_root(project_root).join("gradlew").is_file()
        {
            if let Some(diags) =
                gradle_test_compile_diagnostics(ws, project_root, rel_path, content)?
            {
                if !diags.is_empty() {
                    return Ok((diags, false));
                }
            }
        }
    }

    let classpath_entries =
        classpath::resolve_javac_classpath_for_file(project_root, rel_path, content);
    let jars: Vec<PathBuf> = classpath_entries
        .iter()
        .filter(|p| p.is_file())
        .cloned()
        .collect();
    if jars.is_empty() && classpath_entries.is_empty() {
        tracing::debug!(
            "Project classpath not resolved yet for {} — skipping dependency false positives",
            rel_path
        );
    }

    let overlay_root =
        sync_java_diagnostics_overlays(ws, project_root, rel_path, content, overlays, scope)?;

    // -sourcepath resolves cross-module project types (e.g. ApiResponse in libs:common)
    // without compiling the whole tree — only the active file is passed to javac.
    let sourcepath = classpath::project_java_sourcepath(ws, project_root, &overlay_root);

    let out_dir = ws.join(DIAG_OUT);
    std::fs::create_dir_all(&out_dir)?;

    let cp = classpath_entries
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(if cfg!(windows) { ";" } else { ":" });

    let mut args = vec![
        "-encoding".to_string(),
        "UTF-8".to_string(),
        "-proc:none".to_string(),
        "-d".to_string(),
        out_dir.to_string_lossy().into_owned(),
    ];
    if !cp.is_empty() {
        args.push("-classpath".to_string());
        args.push(cp);
    }
    if !sourcepath.is_empty() {
        args.push("-sourcepath".to_string());
        args.push(
            sourcepath
                .iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join(if cfg!(windows) { ";" } else { ":" }),
        );
    }
    append_javac_release_args(&mut args, project_root);
    // Compile only the active overlay file; -sourcepath resolves cross-file types without
    // spawning a multi-file javac that can stall the server on large multi-module projects.
    args.push(overlay_root.join(rel_path).to_string_lossy().into_owned());

    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let content_fp = fingerprint(content);
    let out = java_javac_inflight::with_workspace_java_lock(ws, || {
        run_cancellable_javac(ws, rel_path, content, &arg_refs)
    })?;
    let out = if out.cancelled {
        java_javac_inflight::peek_cached(ws, rel_path, content_fp).unwrap_or(out)
    } else {
        out
    };
    if out.cancelled {
        return Ok((Vec::new(), true));
    }

    Ok((
        filter_project_javac_diags(
            &out,
            ws,
            rel_path,
            content,
            project_root,
            &jars,
            overlays,
        ),
        false,
    ))
}

fn filter_project_javac_diags(
    out: &java_javac_inflight::CancellableOutput,
    ws: &Path,
    rel_path: &str,
    content: &str,
    project_root: &Path,
    jars: &[PathBuf],
    overlays: &[(String, String)],
) -> Vec<Diagnostic> {
    let mut diags = parse_compiler_output(&out.stderr, ws, rel_path, content);
    if diags.is_empty() {
        diags = parse_compiler_output(&out.stdout, ws, rel_path, content);
    }
    diags = filter_stale_dependency_diags(diags, project_root, rel_path, content, jars);
    diags = filter_javac_classpath_visibility_false_positives(
        diags,
        project_root,
        rel_path,
        content,
    );
    diags = filter_spring_data_javac_false_positives(diags, project_root, content);
    diags = filter_project_method_false_positives(
        diags,
        ws,
        project_root,
        content,
        overlays,
    );
    diags = filter_project_type_false_positives(
        diags,
        ws,
        project_root,
        content,
        overlays,
    );
    diags = filter_project_import_false_positives(
        diags,
        ws,
        project_root,
        content,
        overlays,
    );
    enrich_missing_dependency_diags(&mut diags, project_root, content);
    enrich_static_import_diags(&mut diags, content);
    diags.extend(local_missing_import_type_diags(
        ws,
        Some(project_root),
        rel_path,
        content,
        overlays,
        &diags,
    ));
    diags
}

fn sync_java_diagnostics_overlays(
    ws: &Path,
    project_root: &Path,
    rel_path: &str,
    content: &str,
    overlays: &[(String, String)],
    scope: JavaDiagScope,
) -> Result<PathBuf> {
    let overlay_root = ws.join(DIAG_ROOT).join("overlay");
    let mut seen = std::collections::HashSet::new();
    for (path, text) in std::iter::once((rel_path.to_string(), content.to_string()))
        .chain(overlays.iter().cloned())
    {
        if !path.ends_with(".java") || path.starts_with(".reaper/") {
            continue;
        }
        let _ = safe_join(ws, &path)?;
        if !seen.insert(path.clone()) {
            continue;
        }
        let overlay_file = overlay_root.join(&path);
        if let Some(parent) = overlay_file.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&overlay_file, text)?;
    }

    if scope == JavaDiagScope::Full {
        for disk_path in collect_imported_project_source_files(ws, project_root, content, overlays) {
            let rel = disk_path
                .strip_prefix(ws)
                .or_else(|_| disk_path.strip_prefix(project_root))
                .unwrap_or(&disk_path)
                .to_string_lossy()
                .replace('\\', "/");
            if !rel.ends_with(".java") || !seen.insert(rel.clone()) {
                continue;
            }
            if let Ok(text) = std::fs::read_to_string(&disk_path) {
                let overlay_file = overlay_root.join(&rel);
                if let Some(parent) = overlay_file.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let _ = std::fs::write(overlay_file, text);
            }
        }
    }

    Ok(overlay_root)
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct TestCompileCache {
    stamp: String,
    success: bool,
    #[serde(default)]
    log: String,
    checked_at_ms: u64,
}

fn test_compile_cache_path(project_root: &Path) -> PathBuf {
    project_root.join(".reaper").join(TEST_COMPILE_CACHE)
}

fn load_test_compile_cache(project_root: &Path) -> Option<TestCompileCache> {
    let text = std::fs::read_to_string(test_compile_cache_path(project_root)).ok()?;
    serde_json::from_str(&text).ok()
}

fn save_test_compile_cache(
    project_root: &Path,
    stamp: &str,
    success: bool,
    log: &str,
) -> Result<()> {
    let reaper = project_root.join(".reaper");
    std::fs::create_dir_all(&reaper)?;
    let checked_at_ms = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let cache = TestCompileCache {
        stamp: stamp.to_string(),
        success,
        log: log.to_string(),
        checked_at_ms,
    };
    std::fs::write(
        test_compile_cache_path(project_root),
        serde_json::to_string(&cache)?,
    )?;
    Ok(())
}

fn test_compile_cache_fresh(project_root: &Path, stamp: &str) -> bool {
    let Some(cache) = load_test_compile_cache(project_root) else {
        return false;
    };
    if cache.stamp != stamp {
        return false;
    }
    let age_ms = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
        .saturating_sub(cache.checked_at_ms);
    age_ms < TEST_COMPILE_TTL.as_millis() as u64
}

fn workspace_file_matches_disk(ws: &Path, rel_path: &str, content: &str) -> bool {
    safe_join(ws, rel_path)
        .ok()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .is_some_and(|disk| disk == content)
}

/// Use Gradle `compileTestJava` as the source of truth for saved test sources (cached ~2 min).
fn gradle_test_compile_diagnostics(
    ws: &Path,
    project_root: &Path,
    rel_path: &str,
    content: &str,
) -> Result<Option<Vec<Diagnostic>>> {
    let stamp = classpath::classpath_cache_stamp(project_root).unwrap_or_default();
    let (success, log) = if test_compile_cache_fresh(project_root, &stamp) {
        let cache = load_test_compile_cache(project_root).expect("fresh cache");
        (cache.success, cache.log)
    } else {
        let out = match gradle::run_gradle_compile_test_java(project_root) {
            Ok(out) => out,
            Err(e) => {
                tracing::debug!(
                    "compileTestJava diagnostics skipped for {}: {e:#}",
                    project_root.display()
                );
                return Ok(None);
            }
        };
        let success = out.exit_code == 0;
        let log = if out.stderr.is_empty() {
            out.stdout
        } else if out.stdout.is_empty() {
            out.stderr
        } else {
            format!("{}\n{}", out.stdout, out.stderr)
        };
        save_test_compile_cache(project_root, &stamp, success, &log)?;
        (success, log)
    };

    if success {
        return Ok(Some(Vec::new()));
    }

    Ok(Some(parse_compiler_output(&log, ws, rel_path, content)))
}

fn filter_javac_classpath_visibility_false_positives(
    diags: Vec<Diagnostic>,
    project_root: &Path,
    rel_path: &str,
    content: &str,
) -> Vec<Diagnostic> {
    if !classpath::file_needs_test_classpath(rel_path, content) {
        return diags;
    }
    let stamp = classpath::classpath_cache_stamp(project_root).unwrap_or_default();
    let gradle_test_ok = load_test_compile_cache(project_root)
        .is_some_and(|c| c.success && c.stamp == stamp);
    if !gradle_test_ok {
        return diags;
    }
    diags
        .into_iter()
        .filter(|d| !is_classpath_visibility_false_positive(&d.message))
        .collect()
}

fn is_classpath_visibility_false_positive(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("cannot access")
        || (lower.contains("inaccessible") && lower.contains("interface"))
        || (lower.contains("bad class file") && lower.contains("class file has wrong version"))
}

/// Source roots for javac — every module under the Gradle/Maven project root.
fn build_java_sourcepath(ws: &Path, project_root: &Path, overlay_root: &Path) -> Vec<PathBuf> {
    classpath::project_java_sourcepath(ws, project_root, overlay_root)
}

fn filter_stale_dependency_diags(
    diags: Vec<Diagnostic>,
    project_root: &Path,
    rel_path: &str,
    content: &str,
    jars: &[PathBuf],
) -> Vec<Diagnostic> {
    let markers = super::java_ecosystem::project_build_markers(project_root);
    let declares_spring_data = super::java_ecosystem::project_declares_spring_data(project_root);
    let needs_test = classpath::file_needs_test_classpath(rel_path, content);
    let tooling_pending = classpath::needs_tooling_classpath_resolve(project_root);

    diags
        .into_iter()
        .filter(|d| {
            !is_stale_declared_dependency_diag(
                &d.message,
                content,
                &markers,
                needs_test,
                declares_spring_data,
                jars,
                tooling_pending,
            )
        })
        .collect()
}

fn dependency_unresolved(on_classpath: bool, jars: &[PathBuf], tooling_pending: bool) -> bool {
    jars.is_empty() || tooling_pending || !on_classpath
}

fn is_stale_declared_dependency_diag(
    message: &str,
    content: &str,
    markers: &super::java_ecosystem::JavaBuildMarkers,
    needs_test: bool,
    declares_spring_data: bool,
    jars: &[PathBuf],
    tooling_pending: bool,
) -> bool {
    let lower = message.to_ascii_lowercase();
    let missing_package = lower.contains("package") && lower.contains("does not exist");
    let missing_symbol = lower.contains("cannot find symbol");

    if super::java_psi::stale_imported_dependency_diag(message, content, |pkg| {
        classpath::classpath_includes_package(jars, pkg)
    }) {
        return true;
    }

    if (markers.slf4j || super::java_ecosystem::file_uses_slf4j(content))
        && dependency_unresolved(classpath::classpath_includes_slf4j(jars), jars, tooling_pending)
    {
        if lower.contains("org.slf4j")
            || (missing_package && lower.contains("slf4j"))
            || (missing_symbol && lower.contains("logger"))
        {
            return true;
        }
    }

    if (markers.lombok || super::java_ecosystem::file_uses_lombok(content))
        && is_lombok_proc_none_false_positive(
            message,
            content,
            &lower,
            missing_package,
            missing_symbol,
            jars,
            tooling_pending,
        )
    {
        return true;
    }

    if markers.junit
        && needs_test
        && uses_junit(content)
        && dependency_unresolved(classpath::classpath_includes_junit(jars), jars, tooling_pending)
    {
        if lower.contains("org.junit")
            || (missing_package && lower.contains("junit"))
            || (missing_symbol && (lower.contains(" test") || lower.contains("symbol:   class test")))
        {
            return true;
        }
        if lower.contains("static import only from classes and interfaces")
            && content.contains("import static org.junit")
        {
            return true;
        }
        if missing_symbol && uses_junit_assertions(content) {
            if lower.contains("assertnotnull")
                || lower.contains("assertequals")
                || lower.contains("asserttrue")
                || lower.contains("assertfalse")
                || lower.contains("assertthrows")
                || lower.contains("assertions")
            {
                return true;
            }
        }
    }

    if markers.spring
        && uses_spring(content)
        && dependency_unresolved(classpath::classpath_includes_spring_deps(jars), jars, tooling_pending)
    {
        if lower.contains("org.springframework")
            || (missing_package && lower.contains("springframework"))
        {
            return true;
        }
        if missing_symbol && lower.contains("springframework") {
            return true;
        }
    }

    if (markers.spring || declares_spring_data)
        && uses_spring_data_types(content)
        && dependency_unresolved(
            classpath::classpath_includes_spring_data_deps(jars),
            jars,
            tooling_pending,
        )
    {
        if lower.contains("org.springframework.data")
            || (missing_package && lower.contains("springframework.data"))
            || (missing_symbol && spring_data_symbol_in_message(&lower, content))
        {
            return true;
        }
    }

    if markers.spring_test
        && needs_test
        && uses_spring(content)
        && dependency_unresolved(classpath::classpath_includes_spring_deps(jars), jars, tooling_pending)
    {
        if lower.contains("org.springframework")
            || (missing_package && lower.contains("springframework"))
        {
            return true;
        }
    }

    if (markers.mockito || needs_test)
        && uses_mockito(content)
        && dependency_unresolved(classpath::classpath_includes_mockito(jars), jars, tooling_pending)
    {
        if lower.contains("org.mockito")
            || (missing_package && lower.contains("mockito"))
            || (missing_symbol
                && (lower.contains("mock")
                    || lower.contains("injectmocks")
                    || lower.contains("mockito")))
        {
            return true;
        }
    }

    false
}

fn uses_mockito(content: &str) -> bool {
    content.contains("org.mockito")
        || content.contains("@Mock")
        || content.contains("@InjectMocks")
        || content.contains("@Spy")
        || content.contains("MockitoExtension")
}

fn uses_junit(content: &str) -> bool {
    content.contains("org.junit.jupiter")
        || content.contains("@Test")
        || content.contains("@ParameterizedTest")
        || content.contains("@SpringBootTest")
}

fn uses_spring(content: &str) -> bool {
    content.contains("org.springframework")
}

fn uses_spring_data_types(content: &str) -> bool {
    content.contains("org.springframework.data")
        || classpath::well_known_spring_data_simple_names()
            .any(|name| content.contains(name))
}

fn spring_data_symbol_in_message(lower_message: &str, content: &str) -> bool {
    classpath::well_known_spring_data_simple_names().any(|name| {
        content.contains(name) && lower_message.contains(&name.to_ascii_lowercase())
    })
}

fn lombok_symbol_in_message(message: &str, content: &str) -> bool {
    super::java_psi::lombok_symbol_in_message(message, content)
}

/// javac runs with `-proc:none`, so Lombok annotations and generated members are not resolved.
fn is_lombok_proc_none_false_positive(
    message: &str,
    content: &str,
    lower: &str,
    missing_package: bool,
    missing_symbol: bool,
    jars: &[PathBuf],
    tooling_pending: bool,
) -> bool {
    let annotations = super::java_psi::annotation_simple_names(content);
    if annotations.iter().any(|n| n == "Slf4j")
        && missing_symbol
        && (lower.contains("variable log") || lower.contains("class slf4j"))
    {
        return true;
    }

    let lombok_on_classpath = classpath::classpath_includes_lombok(jars);
    let lombok_unresolved =
        dependency_unresolved(lombok_on_classpath, jars, tooling_pending);

    if lombok_unresolved {
        return lower.contains("lombok")
            || (missing_package && lower.contains("lombok"))
            || (missing_symbol && lombok_symbol_in_message(message, content));
    }

    missing_symbol
        && lombok_on_classpath
        && lombok_symbol_in_message(message, content)
}

/// Drop javac false positives for Spring Data types when the project declares them and Gradle tests compile.
fn filter_spring_data_javac_false_positives(
    diags: Vec<Diagnostic>,
    project_root: &Path,
    content: &str,
) -> Vec<Diagnostic> {
    let declares_spring_data = super::java_ecosystem::project_declares_spring_data(project_root);
    if !declares_spring_data && !uses_spring_data_types(content) {
        return diags;
    }
    diags
        .into_iter()
        .filter(|d| {
            !is_spring_data_well_known_false_positive(&d.message, content, declares_spring_data)
        })
        .collect()
}

fn is_spring_data_well_known_false_positive(
    message: &str,
    content: &str,
    declares_spring_data: bool,
) -> bool {
    if !declares_spring_data && !uses_spring_data_types(content) {
        return false;
    }
    let lower = message.to_ascii_lowercase();
    if !lower.contains("cannot find symbol") {
        return false;
    }
    spring_data_symbol_in_message(&lower, content)
        || (lower.contains("org.springframework.data") && uses_spring_data_types(content))
}

/// Inherited Spring Data repository API methods that javac may miss offline even when Gradle tests pass.
const SPRING_DATA_REPOSITORY_METHODS: &[&str] = &[
    "save",
    "saveAll",
    "saveAndFlush",
    "saveAllAndFlush",
    "findById",
    "findAll",
    "findAllById",
    "existsById",
    "count",
    "delete",
    "deleteById",
    "deleteAll",
    "deleteAllById",
    "deleteAllInBatch",
    "deleteInBatch",
    "flush",
    "getOne",
    "getById",
    "getReferenceById",
];

fn filter_project_method_false_positives(
    diags: Vec<Diagnostic>,
    ws: &Path,
    project_root: &Path,
    content: &str,
    overlays: &[(String, String)],
) -> Vec<Diagnostic> {
    diags
        .into_iter()
        .filter(|d| {
            !is_project_method_false_positive(ws, project_root, content, overlays, &d.message)
        })
        .collect()
}

fn filter_project_type_false_positives(
    diags: Vec<Diagnostic>,
    ws: &Path,
    project_root: &Path,
    content: &str,
    overlays: &[(String, String)],
) -> Vec<Diagnostic> {
    diags
        .into_iter()
        .filter(|d| {
            !is_project_type_false_positive(ws, project_root, content, overlays, &d.message)
        })
        .collect()
}

fn is_project_type_false_positive(
    ws: &Path,
    project_root: &Path,
    content: &str,
    overlays: &[(String, String)],
    message: &str,
) -> bool {
    let Some(type_name) = parse_missing_class_symbol(message) else {
        return false;
    };
    if read_project_type_source(ws, project_root, &type_name, content, overlays).is_some() {
        return true;
    }
    if read_project_type_source(ws, ws, &type_name, content, overlays).is_some() {
        return true;
    }
    if is_well_known_external_type(&type_name, content) {
        return false;
    }
    false
}

fn simple_type_name(type_name: &str) -> &str {
    let base = type_name
        .split('<')
        .next()
        .unwrap_or(type_name)
        .trim();
    base.rsplit('.').next().unwrap_or(base)
}

fn parse_missing_class_symbol(message: &str) -> Option<String> {
    if !message.contains("cannot find symbol") {
        return None;
    }
    for line in message.lines() {
        if let Some(type_name) = parse_class_symbol_on_line(line) {
            return Some(type_name);
        }
    }
    None
}

/// `symbol: class Foo`, or `symbol: variable Foo` when Foo is PascalCase (static/type reference).
fn parse_class_symbol_on_line(line: &str) -> Option<String> {
    let sym_idx = line.find("symbol:")?;
    let after = line[sym_idx + "symbol:".len()..].trim();
    let type_part = if let Some(rest) = after.strip_prefix("class") {
        rest.trim()
    } else if let Some(rest) = after.strip_prefix("variable") {
        let name = simple_type_name(rest.split_whitespace().next().unwrap_or(rest));
        if name.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
            return Some(name.to_string());
        }
        return None;
    } else {
        return None;
    };
    let type_name = simple_type_name(type_part.split_whitespace().next().unwrap_or(type_part));
    if type_name.is_empty() {
        return None;
    }
    Some(type_name.to_string())
}

fn is_well_known_external_type(type_name: &str, content: &str) -> bool {
    if matches!(
        type_name,
        "String"
            | "Integer"
            | "Long"
            | "Boolean"
            | "Double"
            | "Float"
            | "Object"
            | "List"
            | "Map"
            | "Set"
            | "Optional"
            | "Stream"
            | "Logger"
    ) {
        return true;
    }
    for line in content.lines() {
        let t = line.trim();
        if !t.starts_with("import ") || t.starts_with("import static ") {
            continue;
        }
        let imp = t
            .trim_start_matches("import ")
            .trim_end_matches(';')
            .trim();
        if imp.ends_with(&format!(".{type_name}"))
            && (imp.starts_with("java.")
                || imp.starts_with("javax.")
                || imp.starts_with("jakarta.")
                || imp.starts_with("org.")
                || imp.starts_with("com.sun."))
        {
            return true;
        }
    }
    false
}

fn is_project_method_false_positive(
    ws: &Path,
    project_root: &Path,
    content: &str,
    overlays: &[(String, String)],
    message: &str,
) -> bool {
    let Some((method, type_name)) = parse_missing_method_on_type(message) else {
        return false;
    };
    if is_spring_data_repository_method(&type_name, &method, project_root) {
        return true;
    }
    read_project_type_source(ws, project_root, &type_name, content, overlays)
        .is_some_and(|src| source_declares_method(&src, &method))
}

fn parse_missing_method_on_type(message: &str) -> Option<(String, String)> {
    if !message.contains("cannot find symbol") {
        return None;
    }
    let method_name = message.lines().find_map(parse_method_symbol_on_line)?;
    let type_name = message.lines().find_map(parse_javac_location_type)?;
    Some((method_name, type_name))
}

fn parse_method_symbol_on_line(line: &str) -> Option<String> {
    let sym_idx = line.find("symbol:")?;
    let after = line[sym_idx + "symbol:".len()..].trim();
    if !after.contains("method ") {
        return None;
    }
    let method_part = after.split("method ").nth(1)?.trim();
    let method_name = method_part.split('(').next()?.trim();
    if method_name.is_empty() {
        return None;
    }
    Some(method_name.to_string())
}

fn parse_javac_location_type(line: &str) -> Option<String> {
    let loc_idx = line.find("location:")?;
    let after = line[loc_idx + "location:".len()..].trim();
    if let Some(of_type) = after.rsplit(" of type ").next() {
        let type_name = of_type.trim();
        if !type_name.is_empty() {
            return Some(type_name.to_string());
        }
    }
    let type_name = after
        .strip_prefix("class ")
        .or_else(|| after.strip_prefix("type "))
        .or_else(|| after.strip_prefix("interface "))
        .or_else(|| after.strip_prefix("enum "))
        .or_else(|| after.strip_prefix("record "))
        .unwrap_or(after)
        .trim();
    if type_name.is_empty() {
        return None;
    }
    Some(type_name.to_string())
}

fn is_spring_data_repository_method(
    type_name: &str,
    method_name: &str,
    project_root: &Path,
) -> bool {
    if !type_name.ends_with("Repository") {
        return false;
    }
    if !super::java_ecosystem::project_declares_spring_data(project_root)
        && !super::gradle::is_spring_boot_project(project_root)
        && !super::maven::is_spring_boot_project(project_root)
    {
        return false;
    }
    SPRING_DATA_REPOSITORY_METHODS.contains(&method_name)
}

fn resolve_project_type_fqcn(content: &str, type_name: &str) -> Option<String> {
    if type_name.contains('.') {
        return Some(type_name.to_string());
    }
    for line in content.lines() {
        let t = line.trim();
        if !t.starts_with("import ") || t.starts_with("import static ") {
            continue;
        }
        let imp = t
            .trim_start_matches("import ")
            .trim_end_matches(';')
            .trim();
        if imp.ends_with(&format!(".{type_name}")) {
            return Some(imp.to_string());
        }
    }
    let pkg = content.lines().find_map(|line| {
        line.trim()
            .strip_prefix("package ")
            .map(|p| p.trim_end_matches(';').trim())
    })?;
    Some(format!("{pkg}.{type_name}"))
}

fn java_rel_path_for_fqcn(fqcn: &str) -> String {
    format!("{}.java", fqcn.replace('.', "/"))
}

fn read_java_at_rel(
    ws: &Path,
    rel: &str,
    overlays: &[(String, String)],
) -> Option<String> {
    let rel = rel.replace('\\', "/");
    for (path, text) in overlays {
        let p = path.replace('\\', "/");
        if p == rel || p.ends_with(&format!("/{rel}")) {
            return Some(text.clone());
        }
    }
    let disk = ws.join(&rel);
    if disk.is_file() {
        return std::fs::read_to_string(disk).ok();
    }
    None
}

fn all_source_prefixes(ws: &Path, project_root: &Path) -> Vec<String> {
    let mut prefixes = super::java_sources::discover_source_prefixes(ws);
    prefixes.extend(super::java_sources::discover_source_prefixes(project_root));
    prefixes.sort();
    prefixes.dedup();
    prefixes
}

fn fqcn_candidates(content: &str, type_name: &str) -> Vec<String> {
    let simple = simple_type_name(type_name);
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut push = |fqcn: String| {
        if seen.insert(fqcn.clone()) {
            out.push(fqcn);
        }
    };
    if simple.contains('.') {
        push(simple.to_string());
    }
    if let Some(fqcn) = resolve_project_type_fqcn(content, simple) {
        push(fqcn);
    }
    for line in content.lines() {
        let t = line.trim();
        if !t.starts_with("import ") || t.starts_with("import static ") {
            continue;
        }
        let imp = t
            .trim_start_matches("import ")
            .trim_end_matches(';')
            .trim();
        if imp.ends_with(".*") {
            let pkg = imp.trim_end_matches(".*").trim();
            if !pkg.is_empty() {
                push(format!("{pkg}.{simple}"));
            }
            continue;
        }
        if simple.contains('.') {
            if imp == simple || imp.ends_with(&format!(".{simple}")) {
                push(imp.to_string());
            }
        } else if imp.ends_with(&format!(".{simple}")) {
            push(imp.to_string());
        }
    }
    out
}

fn project_type_extensions() -> [&'static str; 3] {
    ["java", "kt", "kts"]
}

fn rel_paths_for_fqcn(fqcn: &str) -> Vec<String> {
    let path = fqcn.replace('.', "/");
    let mut out = vec![format!("{path}.java")];
    for ext in ["kt", "kts"] {
        out.push(format!("{path}.{ext}"));
    }
    out
}

fn collect_imported_project_source_files(
    ws: &Path,
    project_root: &Path,
    content: &str,
    overlays: &[(String, String)],
) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for imp in super::java_psi::type_import_fqcns(&super::java_psi::parse_imports(content)) {
        if let Some(path) = resolve_project_type_file_path(ws, project_root, &imp, overlays) {
            if path.extension().and_then(|e| e.to_str()) == Some("java") && seen.insert(path.clone())
            {
                out.push(path);
            }
        }
    }
    out
}

fn resolve_project_type_file_path(
    ws: &Path,
    project_root: &Path,
    fqcn: &str,
    overlays: &[(String, String)],
) -> Option<PathBuf> {
    for rel in rel_paths_for_fqcn(fqcn) {
        if let Some(text) = read_java_at_rel(ws, &rel, overlays) {
            if source_defines_type(&text, fqcn.rsplit('.').next().unwrap_or(fqcn)) {
                return Some(ws.join(&rel));
            }
        }
        for prefix in all_source_prefixes(ws, project_root) {
            let nested = format!("{prefix}/{rel}");
            if let Some(text) = read_java_at_rel(ws, &nested, overlays) {
                if source_defines_type(&text, fqcn.rsplit('.').next().unwrap_or(fqcn)) {
                    return Some(ws.join(&nested));
                }
            }
            let disk = project_root.join(&nested);
            if disk.is_file() {
                return Some(disk);
            }
            let disk_ws = ws.join(&nested);
            if disk_ws.is_file() {
                return Some(disk_ws);
            }
        }
    }
    None
}

fn read_project_type_source(
    ws: &Path,
    project_root: &Path,
    type_name: &str,
    content: &str,
    overlays: &[(String, String)],
) -> Option<String> {
    let simple = simple_type_name(type_name);
    for fqcn in fqcn_candidates(content, simple) {
        for rel in rel_paths_for_fqcn(&fqcn) {
            if let Some(text) = read_java_at_rel(ws, &rel, overlays) {
                if source_defines_type(&text, simple) {
                    return Some(text);
                }
            }
            for prefix in all_source_prefixes(ws, project_root) {
                let nested = format!("{prefix}/{rel}");
                if let Some(text) = read_java_at_rel(ws, &nested, overlays) {
                    if source_defines_type(&text, simple) {
                        return Some(text);
                    }
                }
            }
        }
    }

    for (path, text) in overlays {
        if is_project_source_path(path) && source_defines_type(text, simple) {
            return Some(text.clone());
        }
    }

    for ext in project_type_extensions() {
        let file_name = format!("{simple}.{ext}");
        for prefix in all_source_prefixes(ws, project_root) {
            for base in [ws, project_root] {
                let root = base.join(&prefix);
                if !root.is_dir() {
                    continue;
                }
                if let Some(path) = find_file_named_recursive(&root, &file_name, 0) {
                    if let Some(text) = read_java_path(ws, path, overlays) {
                        if source_defines_type(&text, simple) {
                            return Some(text);
                        }
                    }
                }
            }
        }
    }
    None
}

fn is_project_source_path(path: &str) -> bool {
    let p = path.replace('\\', "/").to_lowercase();
    project_type_extensions()
        .iter()
        .any(|ext| p.ends_with(&format!(".{ext}")))
}

fn read_java_path(
    ws: &Path,
    path: std::path::PathBuf,
    overlays: &[(String, String)],
) -> Option<String> {
    if let Ok(rel) = path.strip_prefix(ws) {
        let rel = rel.to_string_lossy().replace('\\', "/");
        if let Some(text) = overlays.iter().find(|(p, _)| p == &rel).map(|(_, t)| t.clone()) {
            return Some(text);
        }
    }
    std::fs::read_to_string(path).ok()
}

fn filter_project_import_false_positives(
    diags: Vec<Diagnostic>,
    ws: &Path,
    project_root: &Path,
    content: &str,
    overlays: &[(String, String)],
) -> Vec<Diagnostic> {
    diags
        .into_iter()
        .filter(|d| {
            !is_project_import_false_positive(ws, project_root, content, overlays, &d.message)
        })
        .collect()
}

fn is_project_import_false_positive(
    ws: &Path,
    project_root: &Path,
    content: &str,
    overlays: &[(String, String)],
    message: &str,
) -> bool {
    let lower = message.to_ascii_lowercase();
    if !(lower.contains("package") && lower.contains("does not exist")) {
        return false;
    }
    let Some(missing_pkg) = parse_missing_package(message) else {
        return false;
    };
    for line in content.lines() {
        let t = line.trim();
        if !t.starts_with("import ") || t.starts_with("import static ") {
            continue;
        }
        let imp = t
            .trim_start_matches("import ")
            .trim_end_matches(';')
            .trim();
        if !imp.starts_with(&format!("{missing_pkg}.")) && imp != missing_pkg {
            continue;
        }
        if read_project_type_source(ws, project_root, imp, content, overlays).is_some() {
            return true;
        }
    }
    false
}

fn parse_missing_package(message: &str) -> Option<String> {
    for line in message.lines() {
        let t = line.trim();
        if !t.contains("package") || !t.contains("does not exist") {
            continue;
        }
        let rest = t
            .strip_prefix("package ")
            .or_else(|| t.split("package ").nth(1))?;
        let pkg = rest.split_whitespace().next()?.trim_end_matches('.');
        if !pkg.is_empty() && pkg.contains('.') {
            return Some(pkg.to_string());
        }
    }
    None
}

fn source_defines_type(content: &str, type_name: &str) -> bool {
    let simple = simple_type_name(type_name);
    for line in content.lines() {
        let t = line.trim();
        if t.is_empty() || t.starts_with("//") || t.starts_with('*') {
            continue;
        }
        for keyword in [
            "class ",
            "interface ",
            "enum ",
            "record ",
            "data class ",
            "object ",
        ] {
            let Some(pos) = t.find(keyword) else {
                continue;
            };
            let rest = &t[pos + keyword.len()..];
            let name: String = rest
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            if name == simple {
                return true;
            }
        }
    }
    false
}

fn source_declares_method(content: &str, method_name: &str) -> bool {
    content.lines().any(|line| {
        super::symbols::java_method_name_on_line(line).as_deref() == Some(method_name)
    })
}

fn should_skip_project_search_dir(name: &str) -> bool {
    matches!(
        name,
        "build" | "target" | ".gradle" | "node_modules" | ".git" | ".reaper" | "out" | "bin"
    )
}

fn find_file_named_recursive(dir: &Path, file_name: &str, depth: usize) -> Option<PathBuf> {
    if depth > 16 || !dir.is_dir() {
        return None;
    }
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if path.is_file() {
            if name_str == file_name {
                return Some(path);
            }
        } else if path.is_dir() && !should_skip_project_search_dir(&name_str) {
            if let Some(found) = find_file_named_recursive(&path, file_name, depth + 1) {
                return Some(found);
            }
        }
    }
    None
}

fn uses_junit_assertions(content: &str) -> bool {
    content.contains("import static org.junit.jupiter.api.Assertions")
        || content.contains("org.junit.jupiter.api.Assertions.")
        || content.contains("assertNotNull")
        || content.contains("assertEquals")
        || content.contains("assertTrue")
        || content.contains("assertFalse")
        || content.contains("assertThrows")
}

fn has_invalid_junit_assertion_import(content: &str) -> bool {
    content.lines().any(|line| {
        let t = line.trim();
        t.starts_with("import ")
            && !t.starts_with("import static ")
            && t.contains(".Assertions.")
            && !t.ends_with("Assertions;")
    })
}

fn check_plain_java(ws: &Path, rel_path: &str, content: &str) -> Result<(Vec<Diagnostic>, bool)> {
    let overlay_file = ws.join(DIAG_ROOT).join("overlay").join(rel_path);
    if let Some(parent) = overlay_file.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&overlay_file, content)?;

    let out_dir = ws.join(DIAG_OUT);
    std::fs::create_dir_all(&out_dir)?;

    let rel = overlay_file
        .strip_prefix(ws)
        .with_context(|| "overlay path outside workspace")?
        .to_string_lossy()
        .replace('\\', "/");

    let content_fp = fingerprint(content);
    let mut args = vec![
        "-encoding".to_string(),
        "UTF-8".to_string(),
        "-proc:none".to_string(),
        "-d".to_string(),
        out_dir.to_string_lossy().into_owned(),
    ];
    append_javac_release_args(&mut args, ws);
    args.push(rel);
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let out = java_javac_inflight::with_workspace_java_lock(ws, || {
        run_cancellable_javac(ws, rel_path, content, &arg_refs)
    })?;
    let out = if out.cancelled {
        java_javac_inflight::peek_cached(ws, rel_path, content_fp).unwrap_or(out)
    } else {
        out
    };
    if out.cancelled {
        return Ok((Vec::new(), true));
    }

    let mut diags = parse_compiler_output(&out.stderr, ws, rel_path, content);
    if diags.is_empty() {
        diags = parse_compiler_output(&out.stdout, ws, rel_path, content);
    }
    Ok((diags, false))
}

fn enrich_missing_dependency_diags(
    diags: &mut [Diagnostic],
    project_root: &Path,
    content: &str,
) {
    if !uses_junit(content) {
        return;
    }

    let Some(hint) = missing_junit_dependency_hint(project_root) else {
        return;
    };

    for d in diags.iter_mut() {
        let lower = d.message.to_ascii_lowercase();
        if lower.contains("org.junit")
            || (lower.contains("package") && lower.contains("does not exist"))
            || (lower.contains("cannot find symbol") && lower.contains("test"))
        {
            if !d.message.contains("pom.xml") && !d.message.contains("build.gradle") {
                d.message = format!("{} — {hint}", d.message);
            }
        }
    }
}

fn missing_junit_dependency_hint(project_root: &Path) -> Option<String> {
    if super::maven::is_maven_project_root(project_root) {
        let pom = std::fs::read_to_string(project_root.join("pom.xml")).ok()?;
        let markers = super::java_ecosystem::scan_maven_pom(&pom);
        if markers.junit {
            return None;
        }
        return Some(
            "add JUnit to pom.xml (<dependency> org.junit.jupiter:junit-jupiter test scope), then run ./mvnw dependency:resolve".into(),
        );
    }
    if super::gradle::is_gradle_project_dir(project_root) {
        let markers = super::java_ecosystem::scan_gradle_project(project_root);
        if markers.junit {
            return None;
        }
        return Some(
            "add JUnit to build.gradle (testImplementation 'org.junit.jupiter:junit-jupiter'), then sync Gradle".into(),
        );
    }
    None
}

fn enrich_static_import_diags(diags: &mut [Diagnostic], content: &str) {
    let bad_assertion_import = has_invalid_junit_assertion_import(content);
    let assertion_import_hint =
        "use import static org.junit.jupiter.api.Assertions.assertNotNull; (or import org.junit.jupiter.api.Assertions and call Assertions.assertNotNull(...))";

    for d in diags.iter_mut() {
        let lower = d.message.to_ascii_lowercase();

        if bad_assertion_import
            && lower.contains("cannot find symbol")
            && (lower.contains("assertnotnull") || lower.contains("assertions"))
        {
            d.message = format!("{} — {assertion_import_hint}", d.message);
            continue;
        }

        if !lower.contains("static import only from classes and interfaces") {
            continue;
        }
        let line_idx = d.line.saturating_sub(1) as usize;
        let Some(line) = content.lines().nth(line_idx) else {
            continue;
        };
        if !line.contains("import static") {
            if line.contains(".Assertions.") {
                d.message = format!("{} — {assertion_import_hint}", d.message);
            }
            continue;
        }
        if line.contains(".Test")
            || line.contains(".SpringBootTest")
            || line.contains(".ParameterizedTest")
            || line.contains(".BeforeEach")
            || line.contains(".AfterEach")
            || line.contains(".BeforeAll")
            || line.contains(".AfterAll")
        {
            d.message = format!(
                "{} — annotations use a regular import (e.g. import org.junit.jupiter.api.Test), not import static",
                d.message
            );
        }
    }
}

fn append_javac_release_args(args: &mut Vec<String>, gradle_root: &Path) {
    let release = javac_release_flag(gradle_root);
    let use_legacy = crate::jdk::effective_java_home()
        .ok()
        .and_then(|home| crate::jdk::java_major_version(&home))
        .is_some_and(|major| major <= 8);
    if use_legacy {
        args.push("-source".into());
        args.push(release.clone());
        args.push("-target".into());
        args.push(release);
    } else {
        args.push("--release".into());
        args.push(release);
    }
}

fn javac_release_flag(gradle_root: &Path) -> String {
    effective_java_release(gradle_root)
}

/// Resolved Java `--release` for editor javac: project → settings → configured JDK.
pub fn javac_release_for_path(ws: &Path, path: &str) -> u32 {
    let project_root = find_gradle_root(ws, path)
        .ok()
        .flatten()
        .or_else(|| find_maven_root(ws, path).ok().flatten());
    let release = project_root
        .as_ref()
        .map(|root| effective_java_release(root))
        .unwrap_or_else(|| effective_java_release(ws));
    release.parse().unwrap_or_else(|_| configured_jdk_major())
}

fn effective_java_release(project_root: &Path) -> String {
    let release = java_release_from_project(project_root)
        .or_else(|| crate::jdk::configured_java_release().map(|v| v.to_string()))
        .unwrap_or_else(|| configured_jdk_major().to_string());
    if let Ok(home) = crate::jdk::effective_java_home() {
        if crate::jdk::java_major_version(&home).is_some_and(|major| major <= 8) {
            return "8".into();
        }
    }
    release
}

fn detect_java_release(project_root: &Path) -> String {
    effective_java_release(project_root)
}

fn java_release_from_project(project_root: &Path) -> Option<String> {
    if super::maven::is_maven_project_root(project_root) {
        if let Ok(text) = std::fs::read_to_string(project_root.join("pom.xml")) {
            if let Some(v) = extract_maven_java_version(&text) {
                return Some(v);
            }
        }
    }
    java_release_from_gradle_tree(project_root)
}

/// Walk from the module Gradle root up to the wrapper root so `subprojects { … }`
/// toolchain/source settings on the repo root apply to submodule sources.
fn java_release_from_gradle_tree(project_root: &Path) -> Option<String> {
    let wrapper_root = gradle::find_gradle_wrapper_root(project_root);
    let mut dir = project_root.to_path_buf();
    loop {
        for name in ["build.gradle.kts", "build.gradle"] {
            let path = dir.join(name);
            if let Ok(text) = std::fs::read_to_string(&path) {
                if let Some(v) = extract_release_version(&text) {
                    return Some(v);
                }
            }
        }
        if dir == wrapper_root {
            break;
        }
        let Some(parent) = dir.parent() else {
            break;
        };
        dir = parent.to_path_buf();
    }
    None
}

fn extract_maven_java_version(pom: &str) -> Option<String> {
    if let Some(section) = extract_xml_block(pom, "properties") {
        for key in ["java.version", "maven.compiler.release", "maven.compiler.source"] {
            if let Some(v) = tag_value(&section, key) {
                let digits: String = v.chars().take_while(|c| c.is_ascii_digit()).collect();
                if !digits.is_empty() {
                    return Some(digits);
                }
            }
        }
    }
    None
}

fn extract_xml_block(raw: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = raw.find(&open)? + open.len();
    let end = raw[start..].find(&close)? + start;
    Some(raw[start..end].to_string())
}

fn tag_value(section: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = section.find(&open)? + open.len();
    let end = section[start..].find(&close)? + start;
    let value = section[start..end].trim();
    if value.is_empty() {
        return None;
    }
    Some(value.to_string())
}

/// Major Java version from Settings → Compiler → Java (`effective_java_home`).
pub fn configured_jdk_major() -> u32 {
    crate::jdk::effective_java_home()
        .ok()
        .and_then(|h| crate::jdk::java_major_version(&h))
        .unwrap_or(17)
}

/// Java language level for completions: max(configured JDK, project, settings release).
pub fn completion_java_level(ws: &Path, path: &str) -> u32 {
    let jdk = configured_jdk_major();
    let project = project_java_release(ws, path);
    let configured = crate::jdk::configured_java_release();
    [Some(jdk), project, configured]
        .into_iter()
        .flatten()
        .max()
        .unwrap_or(jdk)
}

/// Project `sourceCompatibility` / Maven `java.version` when a build root exists.
/// Informational and for javac `-source`/`--release` — not used to cap inline completions.
pub fn project_java_release(ws: &Path, path: &str) -> Option<u32> {
    let project_root = find_gradle_root(ws, path)
        .ok()
        .flatten()
        .or_else(|| find_maven_root(ws, path).ok().flatten())?;
    java_release_from_project(&project_root)
        .and_then(|v| v.parse().ok())
}

fn extract_release_version(text: &str) -> Option<String> {
    for line in text.lines() {
        let line = line.split("//").next()?.trim();
        if let Some(idx) = line.find("JavaVersion.VERSION_") {
            let rest = &line[idx + "JavaVersion.VERSION_".len()..];
            let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            if !digits.is_empty() {
                return Some(digits);
            }
        }
        if let Some(rest) = line.strip_prefix("sourceCompatibility = ") {
            let v = rest
                .trim()
                .trim_end_matches('}')
                .trim()
                .trim_matches('"')
                .trim_matches('\'');
            if let Some(ver) = v.strip_prefix("JavaVersion.VERSION_") {
                let digits: String = ver.chars().take_while(|c| c.is_ascii_digit()).collect();
                if !digits.is_empty() {
                    return Some(digits);
                }
            }
            if v.chars().next().is_some_and(|c| c.is_ascii_digit()) {
                let digits: String = v.chars().take_while(|c| c.is_ascii_digit()).collect();
                return Some(digits);
            }
        }
        if line.contains("JavaLanguageVersion.of(") {
            if let Some(start) = line.find("JavaLanguageVersion.of(") {
                let rest = &line[start + "JavaLanguageVersion.of(".len()..];
                let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
                if !digits.is_empty() {
                    return Some(digits);
                }
            }
        }
    }
    None
}

fn parse_compiler_output(text: &str, ws: &Path, focus_path: &str, _content: &str) -> Vec<Diagnostic> {
    let ws_canon = ws.canonicalize().unwrap_or_else(|_| ws.to_path_buf());
    let focus = focus_path.replace('\\', "/");
    let mut diags = Vec::new();
    let lines: Vec<&str> = text.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i].trim();
        if let Some(diag) = parse_diagnostic_line(line, &ws_canon) {
            let mut message = diag.message;
            let mut column = diag.column;
            let mut end_column = diag.end_column;
            i += 1;
            while i < lines.len() {
                let raw = lines[i];
                let next = raw.trim();
                if parse_diagnostic_line(next, &ws_canon).is_some() {
                    break;
                }
                if next.is_empty() {
                    i += 1;
                    continue;
                }
                if raw.contains('^') {
                    if let Some(caret_idx) = raw.find('^') {
                        column = caret_idx as u32 + 1;
                    }
                    i += 1;
                    continue;
                }
                if next.starts_with("symbol:")
                    || next.starts_with("location:")
                    || next.starts_with("Note:")
                {
                    if next.starts_with("symbol:") {
                        if let Some(name) = parse_javac_symbol_name(next) {
                            end_column = Some(column + name.len() as u32);
                        }
                    }
                    if !message.is_empty() {
                        message.push(' ');
                    }
                    message.push_str(next);
                }
                i += 1;
            }
            if diag.path.replace('\\', "/") == focus
                || focus.ends_with(&diag.path)
                || diag.path.ends_with(&focus)
            {
                diags.push(Diagnostic {
                    message,
                    column,
                    end_column,
                    ..diag
                });
            }
            continue;
        }
        i += 1;
    }

    diags
}

fn parse_javac_symbol_name(line: &str) -> Option<String> {
    let rest = line.trim().strip_prefix("symbol:")?.trim();
    let name = rest.split_whitespace().last()?.trim();
    if name.is_empty() {
        return None;
    }
    Some(name.to_string())
}

fn parse_diagnostic_line(line: &str, ws: &Path) -> Option<Diagnostic> {
    // path:line: error: message
    // path:line:column: error: message
    let (head, severity, message) = split_severity(line)?;
    let mut parts = head.split(':');
    let file_part = parts.next()?;
    if !file_part.ends_with(".java") {
        return None;
    }
    let line_no: u32 = parts.next()?.trim().parse().ok()?;
    let mut column = 1u32;
    if let Some(maybe_col) = parts.next() {
        if let Ok(col) = maybe_col.trim().parse::<u32>() {
            column = col;
        }
    }

    let path = normalize_diag_path(file_part, ws)?;
    Some(Diagnostic {
        path,
        line: line_no.max(1),
        column: column.max(1),
        end_line: None,
        end_column: None,
        message: message.trim().to_string(),
        severity: severity.to_string(),
    })
}

fn split_severity(line: &str) -> Option<(&str, &str, &str)> {
    for (needle, sev) in [
        (": error: ", "error"),
        (": warning: ", "warning"),
        (": note: ", "warning"),
    ] {
        if let Some(idx) = line.find(needle) {
            return Some((&line[..idx], sev, &line[idx + needle.len()..]));
        }
    }
    None
}

fn normalize_diag_path(raw: &str, ws: &Path) -> Option<String> {
    let path = PathBuf::from(raw);
    if path.is_absolute() {
        if let Ok(canon) = path.canonicalize() {
            let ws_canon = ws.canonicalize().unwrap_or_else(|_| ws.to_path_buf());
            if let Ok(rel) = canon.strip_prefix(&ws_canon) {
                let rel = rel.to_string_lossy().replace('\\', "/");
                return Some(strip_diag_overlay_prefix(&rel));
            }
            // javac may report path outside ws canonicalization; strip overlay segment.
            let lossy = canon.to_string_lossy().replace('\\', "/");
            if let Some(rest) = lossy.split("/.reaper/java-diagnostics/overlay/").nth(1) {
                return Some(rest.to_string());
            }
        }
    }

    let normalized = raw.replace('\\', "/");
    Some(strip_diag_overlay_prefix(&normalized))
}

fn strip_diag_overlay_prefix(path: &str) -> String {
    path.strip_prefix(".reaper/java-diagnostics/overlay/")
        .unwrap_or(path)
        .to_string()
}

#[derive(Debug, Clone)]
struct PublicTypeDecl {
    line: u32,
    column: u32,
    name: String,
}

/// Public top-level type name must match the `.java` file stem (javac rule).
fn merge_diagnostics(mut base: Vec<Diagnostic>, extra: Vec<Diagnostic>) -> Vec<Diagnostic> {
    for d in extra {
        let dup = base.iter().any(|existing| {
            existing.line == d.line
                && existing.severity == d.severity
                && (existing.message.contains(&d.message)
                    || d.message.contains(&existing.message)
                    || same_missing_symbol(&existing.message, &d.message))
        });
        if !dup {
            base.push(d);
        }
    }
    base
}

fn same_missing_symbol(a: &str, b: &str) -> bool {
    parse_missing_class_symbol(a)
        .is_some_and(|sym| parse_missing_class_symbol(b).is_some_and(|other| sym == other))
}

fn local_missing_import_type_diags(
    ws: &Path,
    project_root: Option<&Path>,
    rel_path: &str,
    content: &str,
    overlays: &[(String, String)],
    existing: &[Diagnostic],
) -> Vec<Diagnostic> {
    let unit = super::java_psi::parse_compilation_unit(content);
    let imported: std::collections::HashSet<String> = unit.imports.explicit.keys().cloned().collect();
    let declared = declared_type_names(&unit);
    let package = unit.package.as_deref();
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut classpath_known: std::collections::HashMap<String, bool> = std::collections::HashMap::new();

    for (idx, line) in content.lines().enumerate() {
        let code = line.split("//").next().unwrap_or(line);
        if code.trim().starts_with("import ") || code.trim().starts_with("package ") {
            continue;
        }
        let mut candidates: Vec<String> = extract_used_type_names(code);
        for ann in super::java_psi::annotation_simple_names(code) {
            if !candidates.iter().any(|c| c == &ann) {
                candidates.push(ann);
            }
        }
        for simple in candidates {
            if simple.len() < 2 || imported.contains(&simple) || declared.contains(&simple) {
                continue;
            }
            if classpath::is_java_lang_public_type(&simple) {
                continue;
            }
            if is_well_known_external_type(&simple, content) {
                continue;
            }
            let known = *classpath_known.entry(simple.clone()).or_insert_with(|| {
                classpath::symbol_known_on_classpath(ws, rel_path, content, &simple)
            });
            if known {
                continue;
            }
            if project_root.is_some_and(|root| {
                type_visible_in_project(ws, root, package, content, overlays, &simple)
            }) {
                continue;
            }
            if existing.iter().any(|d| {
                d.line == idx as u32 + 1 && d.message.contains(&simple)
            }) {
                continue;
            }
            if !seen.insert((idx as u32 + 1, simple.clone())) {
                continue;
            }
            let column = line
                .find(&simple)
                .map(|i| i as u32 + 1)
                .unwrap_or(1);
            let end_column = column + simple.len() as u32;
            out.push(Diagnostic {
                path: rel_path.to_string(),
                line: idx as u32 + 1,
                column,
                end_line: Some(idx as u32 + 1),
                end_column: Some(end_column),
                message: format!(
                    "cannot find symbol\n  symbol:   class {simple}\n  location: class {}",
                    declared.iter().next().map(String::as_str).unwrap_or("file")
                ),
                severity: "error".to_string(),
            });
        }
    }
    out
}

fn declared_type_names(unit: &super::java_psi::CompilationUnit) -> std::collections::HashSet<String> {
    let mut out = std::collections::HashSet::new();
    let mut stack: Vec<_> = unit.types.iter().collect();
    while let Some(ty) = stack.pop() {
        out.insert(ty.name.clone());
        for nested in &ty.nested {
            stack.push(nested);
        }
    }
    out
}

fn extract_used_type_names(line: &str) -> Vec<String> {
    let mut names = Vec::new();
    let bytes = line.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        let c = bytes[i] as char;
        if !c.is_ascii_alphabetic() && c != '_' {
            i += 1;
            continue;
        }
        let start = i;
        i += 1;
        while i < bytes.len() {
            let c = bytes[i] as char;
            if c.is_ascii_alphanumeric() || c == '_' {
                i += 1;
            } else {
                break;
            }
        }
        let token = &line[start..i];
        if !(token.as_bytes()[0] as char).is_ascii_uppercase() {
            continue;
        }
        let before = line[..start].trim_end();
        let next = line[i..].trim_start();
        let usage = next.starts_with('.')
            || next.starts_with('(')
            || before.ends_with('@')
            || before.ends_with("new")
            || before.ends_with("extends")
            || before.ends_with("implements");
        if usage {
            names.push(token.to_string());
        }
    }
    names
}

fn type_visible_in_project(
    ws: &Path,
    project_root: &Path,
    package: Option<&str>,
    content: &str,
    overlays: &[(String, String)],
    simple: &str,
) -> bool {
    if read_project_type_source(ws, project_root, simple, content, overlays).is_some() {
        return true;
    }
    if let Some(pkg) = package {
        let fqcn = format!("{pkg}.{simple}");
        if read_project_type_source(ws, project_root, &fqcn, content, overlays).is_some() {
            return true;
        }
    }
    false
}

fn local_file_class_name_diags(rel_path: &str, content: &str) -> Vec<Diagnostic> {
    let stem = Path::new(rel_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    if stem.is_empty() {
        return Vec::new();
    }

    find_public_top_level_declarations(content)
        .into_iter()
        .filter(|decl| decl.name != stem)
        .map(|decl| {
            let end_col = decl.column + decl.name.len() as u32;
            Diagnostic {
                path: rel_path.to_string(),
                line: decl.line,
                column: decl.column,
                end_line: Some(decl.line),
                end_column: Some(end_col),
                message: format!(
                    "class {} is public, should be declared in a file named {}.java",
                    decl.name, decl.name
                ),
                severity: "error".to_string(),
            }
        })
        .collect()
}

fn find_public_top_level_declarations(content: &str) -> Vec<PublicTypeDecl> {
    let prefixes: &[&str] = &[
        "public abstract class ",
        "public final class ",
        "public class ",
        "public interface ",
        "public enum ",
        "public @interface ",
        "public record ",
    ];

    let mut decls = Vec::new();
    let mut depth = 0usize;
    for (idx, line) in content.lines().enumerate() {
        let code = line.split("//").next().unwrap_or(line);
        if depth == 0 {
            let trimmed = code.trim();
            for prefix in prefixes {
                if let Some(rest) = trimmed.strip_prefix(prefix) {
                    let name: String = rest
                        .trim_start()
                        .chars()
                        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                        .collect();
                    if !name.is_empty() {
                        let column = line
                            .find(&name)
                            .map(|i| i as u32 + 1)
                            .unwrap_or(1);
                        decls.push(PublicTypeDecl {
                            line: idx as u32 + 1,
                            column,
                            name,
                        });
                    }
                    break;
                }
            }
        }
        for ch in code.chars() {
            match ch {
                '{' => depth += 1,
                '}' => depth = depth.saturating_sub(1),
                _ => {}
            }
        }
    }
    decls
}

fn merge_file_name_diags(javac: Vec<Diagnostic>, local: Vec<Diagnostic>) -> Vec<Diagnostic> {
    if local.is_empty() {
        return javac;
    }
    let mut out = javac;
    for loc in local {
        let existing = out.iter().position(|d| {
            d.line == loc.line && d.message.contains("should be declared in a file named")
        });
        if let Some(i) = existing {
            out[i] = loc;
        } else {
            out.push(loc);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::java_sources;

    #[test]
    fn parses_javac_error_line() {
        let ws = PathBuf::from("/tmp/project");
        let line = "/tmp/project/src/main/java/com/example/App.java:12: error: cannot find symbol";
        let diag = parse_diagnostic_line(line, &ws).expect("diag");
        assert_eq!(diag.line, 12);
        assert_eq!(diag.severity, "error");
        assert!(diag.message.contains("cannot find symbol"));
    }

    #[test]
    fn parses_column_form() {
        let ws = PathBuf::from("/repo");
        let line = "src/main/java/App.java:5:10: error: ';' expected";
        let diag = parse_diagnostic_line(line, &ws).expect("diag");
        assert_eq!(diag.column, 10);
    }

    #[test]
    fn detects_java_release() {
        let text = r#"
plugins { id 'java' }
java { sourceCompatibility = JavaVersion.VERSION_21 }
"#;
        assert_eq!(extract_release_version(text).as_deref(), Some("21"));
    }

    #[test]
    fn java_release_from_gradle_tree_reads_root_subprojects_toolchain() {
        let root = std::env::temp_dir().join("reaper-diag-gradle-release-tree");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("libs/common")).unwrap();
        std::fs::write(
            root.join("build.gradle"),
            r#"subprojects {
    java {
        toolchain {
            languageVersion = JavaLanguageVersion.of(21)
        }
    }
}
"#,
        )
        .unwrap();
        std::fs::write(
            root.join("libs/common/build.gradle"),
            "plugins { id 'java-library' }\n",
        )
        .unwrap();
        std::fs::write(root.join("settings.gradle"), "rootProject.name = 'demo'\n").unwrap();
        std::fs::write(root.join("gradlew"), "#!/bin/sh\n").unwrap();
        assert_eq!(
            java_release_from_gradle_tree(&root.join("libs/common")).as_deref(),
            Some("21")
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn effective_java_release_uses_settings_when_project_missing() {
        crate::jdk::set_configured_java_release(Some(21));
        let root = std::env::temp_dir().join("reaper-diag-plain-java-release");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        assert_eq!(effective_java_release(&root), "21");
        crate::jdk::set_configured_java_release(None);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn parses_caret_column_from_javac_context() {
        let ws = PathBuf::from("/repo");
        let text = r#"src/App.java:5: error: cannot find symbol
        List<String> s= Arrays.asList(args);
                        ^
  symbol:   variable Arrays
  location: class HelloWorld"#;
        let diags = parse_compiler_output(text, &ws, "src/App.java", "");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 5);
        assert_eq!(diags[0].column, 25);
        assert!(diags[0].message.contains("Arrays"));
    }

    #[test]
    fn local_file_class_name_mismatch_on_class_name() {
        let content = "public class RightName {\n    public static void main(String[] args) {}\n}\n";
        let diags = local_file_class_name_diags("WrongFile.java", content);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 1);
        assert_eq!(diags[0].column, 14);
        assert_eq!(diags[0].end_column, Some(23));
        assert!(diags[0].message.contains("RightName.java"));
    }

    #[test]
    fn local_file_class_name_ok_when_names_match() {
        let content = "public class App {\n}\n";
        assert!(local_file_class_name_diags("App.java", content).is_empty());
    }

    #[test]
    fn ignores_inner_public_class_for_file_name() {
        let content = "public class Outer {\n    public static class Inner {\n    }\n}\n";
        assert!(local_file_class_name_diags("Outer.java", content).is_empty());
    }

    #[test]
    fn local_missing_import_flags_slf4j_annotation_without_import() {
        let content = r#"package com.example;

@SpringBootApplication
@Slf4j
public class App {
    public static void main(String[] args) {}
}
"#;
        let diags = local_missing_import_type_diags(
            Path::new("/repo"),
            None,
            "App.java",
            content,
            &[],
            &[],
        );
        assert!(
            diags.iter().any(|d| d.message.contains("Slf4j")),
            "expected Slf4j missing-import diagnostic, got {diags:?}"
        );
    }

    #[test]
    fn local_missing_import_flags_spring_application_usage() {
        let content = r#"package com.example;

@SpringBootApplication
public class App {
    public static void main(String[] args) {
        SpringApplication.run(App.class, args);
    }
}
"#;
        let diags = local_missing_import_type_diags(
            Path::new("/repo"),
            None,
            "App.java",
            content,
            &[],
            &[],
        );
        assert!(
            diags.iter().any(|d| d.message.contains("SpringApplication")),
            "expected SpringApplication missing-import diagnostic, got {diags:?}"
        );
    }

    #[test]
    fn local_missing_import_skips_java_lang_runtime_exception() {
        let content = r#"package com.example.common.exception;

public class NotFoundException extends RuntimeException {
    public NotFoundException(String message) {
        super(message);
    }
}
"#;
        let diags = local_missing_import_type_diags(
            Path::new("/repo"),
            None,
            "NotFoundException.java",
            content,
            &[],
            &[],
        );
        assert!(
            !diags.iter().any(|d| d.message.contains("RuntimeException")),
            "java.lang.RuntimeException must not need import: {diags:?}"
        );
    }

    #[test]
    fn local_missing_import_does_not_treat_create_temp_file_as_temp_file_class() {
        let content = r#"package com.example;
public class App {
    void x() {
        File.createTempFile("a", ".tmp");
    }
}
"#;
        let diags = local_missing_import_type_diags(
            Path::new("/repo"),
            None,
            "App.java",
            content,
            &[],
            &[],
        );
        assert!(
            !diags.iter().any(|d| d.message.contains("TempFile")),
            "createTempFile must not produce a false TempFile class error: {diags:?}"
        );
    }

    #[test]
    fn discovers_all_module_java_source_roots() {
        let root = std::env::temp_dir().join("reaper-diag-src-roots");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("api/src/main/java")).unwrap();
        std::fs::create_dir_all(root.join("core/src/main/java")).unwrap();
        std::fs::create_dir_all(root.join("src/test/java")).unwrap();
        let prefixes = java_sources::discover_source_prefixes(&root);
        assert!(prefixes.contains(&"api/src/main/java".to_string()));
        assert!(prefixes.contains(&"core/src/main/java".to_string()));
        assert!(prefixes.contains(&"src/test/java".to_string()));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn detects_gradle_test_source_prefix() {
        assert_eq!(
            java_sources::detect_file_source_prefix("app/src/test/java/com/example/AppTest.java").as_deref(),
            Some("app/src/test/java")
        );
        assert_eq!(
            java_sources::detect_file_source_prefix("src/test/java/com/example/AppTest.java").as_deref(),
            Some("src/test/java")
        );
    }

    #[test]
    fn filters_validation_imports_when_api_missing_but_jakarta_annotation_present() {
        let markers = crate::workspace::java_ecosystem::JavaBuildMarkers {
            spring: true,
            ..Default::default()
        };
        let content = r#"
import jakarta.validation.constraints.NotBlank;
import jakarta.validation.constraints.NotNull;
import jakarta.validation.constraints.Positive;
public record CreateProductRequest(@NotBlank String name, @NotNull @Positive java.math.BigDecimal price) {}
"#;
        let ann = PathBuf::from(
            "/cache/files-2.1/jakarta.annotation/jakarta.annotation-api/2.1.1/x/jakarta.annotation-api-2.1.1.jar",
        );
        assert!(is_stale_declared_dependency_diag(
            "error: cannot find symbol\n  symbol:   class NotBlank\n  location: class CreateProductRequest",
            content,
            &markers,
            false,
            false,
            &[ann],
            false,
        ));
    }

    #[test]
    fn filters_junit_diag_when_pom_declares_junit_but_classpath_empty() {
        let markers = crate::workspace::java_ecosystem::JavaBuildMarkers {
            junit: true,
            ..Default::default()
        };
        let content = "import org.junit.jupiter.api.Test;\nclass T { @Test void x() {} }\n";
        assert!(is_stale_declared_dependency_diag(
            "package org.junit.jupiter.api does not exist",
            content,
            &markers,
            true,
            false,
            &[],
            false,
        ));
        assert!(!is_stale_declared_dependency_diag(
            "';' expected",
            content,
            &markers,
            true,
            false,
            &[],
            false,
        ));
    }

    #[test]
    fn filters_static_import_junit_diag_when_classpath_unresolved() {
        let markers = crate::workspace::java_ecosystem::JavaBuildMarkers {
            junit: true,
            ..Default::default()
        };
        let content = "import static org.junit.jupiter.api.Assertions.assertEquals;\nclass T {}\n";
        assert!(is_stale_declared_dependency_diag(
            "static import only from classes and interfaces",
            content,
            &markers,
            true,
            false,
            &[],
            false,
        ));
    }

    #[test]
    fn filters_assertnotnull_when_junit_declared_but_classpath_unresolved() {
        let markers = crate::workspace::java_ecosystem::JavaBuildMarkers {
            junit: true,
            ..Default::default()
        };
        let content = r#"
import org.junit.jupiter.api.Test;
import static org.junit.jupiter.api.Assertions.assertNotNull;
class AppTest {
    @Test void x() { assertNotNull("hi"); }
}
"#;
        assert!(is_stale_declared_dependency_diag(
            "cannot find symbol\n  symbol:   method assertNotNull(String)\n  location: class AppTest",
            content,
            &markers,
            true,
            false,
            &[],
            false,
        ));
    }

    #[test]
    fn filters_spring_data_domain_when_spring_declared_but_classpath_unresolved() {
        let markers = crate::workspace::java_ecosystem::JavaBuildMarkers {
            spring: true,
            ..Default::default()
        };
        let content = "import org.springframework.data.domain.Page;\nclass Repo {}\n";
        assert!(is_stale_declared_dependency_diag(
            "package org.springframework.data.domain does not exist",
            content,
            &markers,
            false,
            true,
            &[],
            false,
        ));
    }

    #[test]
    fn filters_slf4j_when_declared_but_missing_from_classpath() {
        let markers = crate::workspace::java_ecosystem::JavaBuildMarkers {
            slf4j: true,
            ..Default::default()
        };
        let content = "import org.slf4j.Logger;\nclass App { Logger log; }\n";
        let junit_only = vec![PathBuf::from("/tmp/junit-jupiter-api-5.10.jar")];
        assert!(is_stale_declared_dependency_diag(
            "package org.slf4j does not exist",
            content,
            &markers,
            false,
            false,
            &junit_only,
            false,
        ));
    }

    #[test]
    fn filters_mockito_when_junit_on_classpath_but_mockito_missing() {
        let markers = crate::workspace::java_ecosystem::JavaBuildMarkers {
            junit: true,
            mockito: true,
            ..Default::default()
        };
        let content = "import org.mockito.Mock;\nclass T { @Mock Object o; }\n";
        let junit_only = vec![PathBuf::from("/tmp/junit-jupiter-api-5.10.jar")];
        assert!(is_stale_declared_dependency_diag(
            "package org.mockito does not exist",
            content,
            &markers,
            true,
            false,
            &junit_only,
            false,
        ));
    }

    #[test]
    fn filters_lombok_required_args_constructor_when_classpath_unresolved() {
        let markers = crate::workspace::java_ecosystem::JavaBuildMarkers {
            lombok: true,
            ..Default::default()
        };
        let content = r#"
import lombok.RequiredArgsConstructor;
@RequiredArgsConstructor
class GatewayController {
    private final String id;
}
"#;
        assert!(is_stale_declared_dependency_diag(
            "error: cannot find symbol\n  symbol:   class RequiredArgsConstructor\n  location: class GatewayController",
            content,
            &markers,
            false,
            false,
            &[],
            false,
        ));
        assert!(!is_stale_declared_dependency_diag(
            "';' expected",
            content,
            &markers,
            false,
            false,
            &[],
            false,
        ));
    }

    #[test]
    fn filters_lombok_slf4j_annotation_without_import_when_lombok_on_classpath() {
        let markers = crate::workspace::java_ecosystem::JavaBuildMarkers {
            lombok: true,
            ..Default::default()
        };
        let content = r#"
@SpringBootApplication
@Slf4j
public class OrderServiceApplication {
    public static void main(String[] args) {}
}
"#;
        let lombok = PathBuf::from("/tmp/lombok-1.18.36.jar");
        assert!(is_stale_declared_dependency_diag(
            "error: cannot find symbol\n  symbol:   class Slf4j\n  location: class OrderServiceApplication",
            content,
            &markers,
            false,
            false,
            &[lombok],
            false,
        ));
    }

    #[test]
    fn filters_lombok_slf4j_log_variable_without_annotation_processing() {
        let markers = crate::workspace::java_ecosystem::JavaBuildMarkers {
            lombok: true,
            ..Default::default()
        };
        let content = "@Slf4j\nclass App { void x() { log.info(\"hi\"); } }\n";
        assert!(is_stale_declared_dependency_diag(
            "cannot find symbol\n  symbol:   variable log\n  location: class App",
            content,
            &markers,
            false,
            false,
            &[PathBuf::from("/tmp/lombok-1.18.30.jar")],
            false,
        ));
    }

    #[test]
    fn filters_project_class_when_source_exists_in_workspace() {
        let ws = std::env::temp_dir().join("reaper-diag-missing-class");
        let _ = std::fs::remove_dir_all(&ws);
        std::fs::create_dir_all(ws.join("src/main/java/com/example")).unwrap();
        std::fs::write(
            ws.join("src/main/java/com/example/ScheduledTask.java"),
            "package com.example;\nclass ScheduledTask {}\n",
        )
        .unwrap();
        let content = r#"
package com.example;
class Worker {
    void x() { new ScheduledTask(); }
}
"#;
        let msg = "error: cannot find symbol\n  symbol:   class ScheduledTask\n  location: class Worker";
        assert!(is_project_type_false_positive(&ws, &ws, content, &[], msg));
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn filters_project_class_under_gradle_module_src_root() {
        let ws = std::env::temp_dir().join("reaper-diag-module-class");
        let _ = std::fs::remove_dir_all(&ws);
        std::fs::create_dir_all(ws.join("app/src/main/java/com/example")).unwrap();
        std::fs::write(
            ws.join("app/src/main/java/com/example/ScheduledTask.java"),
            "package com.example;\nclass ScheduledTask {}\n",
        )
        .unwrap();
        let content = r#"
package com.example;
import com.example.ScheduledTask;
class Worker {
    void x() { new ScheduledTask(); }
}
"#;
        let msg = "error: cannot find symbol\n  symbol:   class ScheduledTask\n  location: class Worker";
        assert!(is_project_type_false_positive(&ws, &ws, content, &[], msg));
        let import_msg = "package com.example does not exist";
        assert!(is_project_import_false_positive(
            &ws,
            &ws,
            content,
            &[],
            import_msg,
        ));
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn filters_api_response_variable_symbol_when_type_in_project() {
        let ws = std::env::temp_dir().join("reaper-diag-api-response-var");
        let _ = std::fs::remove_dir_all(&ws);
        std::fs::create_dir_all(ws.join("libs/common/src/main/java/com/example/common/model")).unwrap();
        std::fs::create_dir_all(ws.join("services/gateway/src/main/java/com/example/gateway/web")).unwrap();
        std::fs::write(
            ws.join("libs/common/src/main/java/com/example/common/model/ApiResponse.java"),
            "package com.example.common.model;\npublic record ApiResponse<T>(T data) {\n  public static <T> ApiResponse<T> ok(T data) { return new ApiResponse<>(data); }\n}\n",
        )
        .unwrap();
        let content = r#"
package com.example.gateway.web;
import com.example.common.model.ApiResponse;
public class GatewayController {
    ApiResponse<String> ok() { return ApiResponse.ok("x"); }
}
"#;
        let msg = "error: cannot find symbol\n  symbol:   variable ApiResponse\n  location: class GatewayController";
        assert!(is_project_type_false_positive(
            &ws,
            &ws.join("services/gateway"),
            content,
            &[],
            msg,
        ));
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn parse_class_symbol_accepts_pascal_case_variable() {
        assert_eq!(
            parse_missing_class_symbol(
                "cannot find symbol\n  symbol:   variable ApiResponse\n  location: class GatewayController"
            )
            .as_deref(),
            Some("ApiResponse")
        );
        assert!(parse_missing_class_symbol(
            "cannot find symbol\n  symbol:   variable fike\n  location: class App"
        )
        .is_none());
    }

    #[test]
    fn filters_api_response_for_gateway_controller_across_dto_package() {
        let ws = std::env::temp_dir().join("reaper-diag-api-response");
        let _ = std::fs::remove_dir_all(&ws);
        std::fs::create_dir_all(ws.join("api/src/main/java/com/example/dto")).unwrap();
        std::fs::create_dir_all(ws.join("api/src/main/java/com/example/web")).unwrap();
        std::fs::write(
            ws.join("api/src/main/java/com/example/dto/ApiResponse.java"),
            "package com.example.dto;\npublic class ApiResponse<T> { }\n",
        )
        .unwrap();
        let content = r#"
package com.example.web;
import com.example.dto.ApiResponse;
public class GatewayController {
    ApiResponse<String> ok() { return new ApiResponse<>(); }
}
"#;
        let msg = "error: cannot find symbol\n  symbol:   class ApiResponse\n  location: class GatewayController";
        assert!(is_project_type_false_positive(&ws, &ws, content, &[], msg));
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn filters_api_response_from_kotlin_source() {
        let ws = std::env::temp_dir().join("reaper-diag-api-response-kt");
        let _ = std::fs::remove_dir_all(&ws);
        std::fs::create_dir_all(ws.join("src/main/kotlin/com/example/dto")).unwrap();
        std::fs::write(
            ws.join("src/main/kotlin/com/example/dto/ApiResponse.kt"),
            "package com.example.dto\ndata class ApiResponse<T>(val data: T)\n",
        )
        .unwrap();
        let content = r#"
package com.example.web;
import com.example.dto.ApiResponse;
class GatewayController {
    ApiResponse<String> ok() { return null; }
}
"#;
        let msg = "error: cannot find symbol\n  symbol:   class ApiResponse\n  location: class GatewayController";
        assert!(is_project_type_false_positive(&ws, &ws, content, &[], msg));
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn filters_spring_data_when_pom_declares_starter_without_markers_spring() {
        let markers = crate::workspace::java_ecosystem::JavaBuildMarkers::default();
        let content = "import org.springframework.data.domain.PageRequest;\nclass Repo {}\n";
        assert!(is_stale_declared_dependency_diag(
            "package org.springframework.data.domain does not exist",
            content,
            &markers,
            false,
            true,
            &[],
            false,
        ));
    }

    #[test]
    fn filters_pagerequest_variable_false_positive_when_spring_data_declared() {
        let content = r#"
import org.springframework.data.domain.PageRequest;
class ScheduledTaskServiceIntegrationTest {
    void x() { PageRequest.of(0, 10); }
}
"#;
        assert!(is_spring_data_well_known_false_positive(
            "error: cannot find symbol\n  symbol:   variable PageRequest\n  location: class ScheduledTaskServiceIntegrationTest",
            content,
            true,
        ));
    }

    #[test]
    fn filters_custom_repository_save_when_method_in_project_source() {
        let ws = std::env::temp_dir().join("reaper-diag-repo-save");
        let _ = std::fs::remove_dir_all(&ws);
        std::fs::create_dir_all(ws.join("src/main/java/com/example")).unwrap();
        std::fs::write(
            ws.join("src/main/java/com/example/ScheduledTaskRepository.java"),
            "package com.example;\ninterface ScheduledTaskRepository {\n  ScheduledTask save(ScheduledTask task);\n}\n",
        )
        .unwrap();
        let test_content = r#"
package com.example;
class ScheduledTaskServiceIntegrationTest {
    ScheduledTaskRepository taskRepository;
    void x() { taskRepository.save(new ScheduledTask()); }
}
class ScheduledTask {}
"#;
        let msg = "error: cannot find symbol\n  symbol:   method save(ScheduledTask)\n  location: variable taskRepository of type ScheduledTaskRepository";
        assert!(is_project_method_false_positive(
            &ws,
            &ws,
            test_content,
            &[],
            msg,
        ));
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn filters_inherited_repository_save_for_spring_boot_project() {
        let root = std::env::temp_dir().join("reaper-diag-spring-repo");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("pom.xml"),
            "<project><dependencies><dependency><artifactId>spring-boot-starter-data-jpa</artifactId></dependency></dependencies></project>",
        )
        .unwrap();
        assert!(is_spring_data_repository_method(
            "ScheduledTaskRepository",
            "save",
            &root,
        ));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn parse_class_symbol_ignores_variable_and_location_class() {
        assert_eq!(
            parse_missing_class_symbol(
                "cannot find symbol symbol:   variable fike location: class AnalyticsServiceApplication"
            ),
            None
        );
        assert_eq!(
            parse_missing_class_symbol(
                "cannot find symbol\n  symbol:   class File\n  location: class App"
            ),
            Some("File".to_string())
        );
    }

    #[test]
    fn does_not_filter_fike_or_bare_exists_as_project_false_positive() {
        let ws = std::env::temp_dir().join("reaper-diag-analytics-fp");
        let _ = std::fs::remove_dir_all(&ws);
        std::fs::create_dir_all(ws.join("src/main/java/com/example")).unwrap();
        let content = r#"package com.example;
public class App {
  void m() {
    System.out.println(fike.exists());
    exists();
  }
}
"#;
        std::fs::write(
            ws.join("src/main/java/com/example/App.java"),
            content,
        )
        .unwrap();
        let fike_msg = "cannot find symbol symbol:   variable fike location: class App";
        assert!(!is_project_type_false_positive(&ws, &ws, content, &[], fike_msg));
        let exists_msg =
            "cannot find symbol symbol:   method exists() location: class App";
        assert!(!is_project_method_false_positive(
            &ws, &ws, content, &[], exists_msg
        ));
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn analytics_app_reports_real_javac_errors() {
        let ws = Path::new("/Users/sunny/reaper/workspaces/Spring-maven-complicated");
        if !ws.is_dir() {
            return;
        }
        let rel = "services/analytics-service/src/main/java/com/enterprise/analytics/AnalyticsServiceApplication.java";
        // Intentional bugs — do not read from disk; workspace copy may be edited while editing.
        let content = r#"package com.enterprise.analytics;

import org.springframework.boot.autoconfigure.SpringBootApplication;
import org.springframework.cloud.client.discovery.EnableDiscoveryClient;
import org.springframework.boot.SpringApplication;

@SpringBootApplication(
    scanBasePackages = {"com.enterprise.analytics", "com.enterprise.data", "com.enterprise.web"})
@EnableDiscoveryClient
public class AnalyticsServiceApplication {
  public static void main(String[] args) throws Exception {
    File file = new File("file");
    File.createTempFile("file", null);
    if (file.exists()) {
      System.out.println("file exists: " + fike.exists());
    }
    exists();
    SpringApplication.run(AnalyticsServiceApplication.class, args);
  }
}
"#;
        let (diags, cancelled) =
            check_java(ws, rel, content, &[], JavaDiagScope::Full).unwrap();
        eprintln!("cancelled={cancelled} count={}", diags.len());
        for d in &diags {
            eprintln!("L{}: {}", d.line, d.message.lines().next().unwrap_or(""));
        }
        assert!(
            !cancelled,
            "javac diagnostics should complete for analytics app (cancelled with no cache)"
        );
        let joined = diags
            .iter()
            .map(|d| d.message.to_ascii_lowercase())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            joined.contains("fike") || joined.contains("exists()") || joined.contains("null"),
            "expected javac errors for fike/exists/null, got: {joined}"
        );
    }

    fn javac_finds_missing_files_class() {
        let ws = std::env::temp_dir().join("reaper-diag-files-class");
        let _ = std::fs::remove_dir_all(&ws);
        let rel = "src/main/java/com/example/App.java";
        std::fs::create_dir_all(ws.join("src/main/java/com/example")).unwrap();
        let content = r#"package com.example;
public class App {
  void m() {
    var files = new Files();
  }
}
"#;
        std::fs::write(ws.join(rel), content).unwrap();
        let (diags, cancelled) = check_java(&ws, rel, content, &[], JavaDiagScope::Full).unwrap();
        assert!(!cancelled, "single-file javac should complete");
        let joined = diags
            .iter()
            .map(|d| d.message.to_ascii_lowercase())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            joined.contains("files") && joined.contains("cannot find symbol"),
            "expected missing Files class diagnostic, got: {joined}"
        );
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn classpath_visibility_false_positive_matches_cannot_access() {
        assert!(is_classpath_visibility_false_positive(
            "cannot access Versioned\n  class file for com.fasterxml.jackson.core.Versioned not found"
        ));
        assert!(!is_classpath_visibility_false_positive(
            "cannot find symbol\n  symbol:   variable foo"
        ));
    }
}
