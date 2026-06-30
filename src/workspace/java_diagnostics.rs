use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use super::classpath;
use super::diagnostics::Diagnostic;
use super::exec::run_java_command;
use super::gradle::find_gradle_root;
use super::maven::find_maven_root;
use super::{safe_join};

const DIAG_ROOT: &str = ".reaper/java-diagnostics";
const DIAG_OUT: &str = ".reaper/java-diagnostics-out";

pub type JavaDiagnostic = Diagnostic;

pub fn check_java(
    ws: &Path,
    rel_path: &str,
    content: &str,
    overlays: &[(String, String)],
) -> Result<Vec<Diagnostic>> {
    if !rel_path.ends_with(".java") {
        return Ok(Vec::new());
    }

    let _ = safe_join(ws, rel_path)?;

    let project_root = find_gradle_root(ws, rel_path)?.or(find_maven_root(ws, rel_path)?);

    let javac_diags = if let Some(root) = project_root {
        check_project_java(ws, &root, rel_path, content, overlays)?
    } else {
        check_plain_java(ws, rel_path, content)?
    };

    let local = local_file_class_name_diags(rel_path, content);
    Ok(merge_file_name_diags(javac_diags, local))
}

fn check_project_java(
    ws: &Path,
    project_root: &Path,
    rel_path: &str,
    content: &str,
    overlays: &[(String, String)],
) -> Result<Vec<Diagnostic>> {
    let classpath_entries =
        classpath::resolve_dependency_jars_for_java_file(project_root, rel_path, content);
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

    let overlay_root = sync_java_diagnostics_overlays(ws, rel_path, content, overlays)?;

    let sourcepath = classpath::project_java_sourcepath(project_root, &overlay_root);

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
    args.push(overlay_root.join(rel_path).to_string_lossy().into_owned());

    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let out = run_java_command(ws, "javac", &arg_refs)?;

    let mut diags = parse_compiler_output(&out.stderr, ws, rel_path, content);
    if diags.is_empty() {
        diags = parse_compiler_output(&out.stdout, ws, rel_path, content);
    }
    diags = filter_stale_dependency_diags(diags, project_root, rel_path, content, &jars);
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
    enrich_missing_dependency_diags(&mut diags, project_root, content);
    enrich_static_import_diags(&mut diags, content);
    Ok(diags)
}

fn sync_java_diagnostics_overlays(
    ws: &Path,
    rel_path: &str,
    content: &str,
    overlays: &[(String, String)],
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
    Ok(overlay_root)
}

/// Source roots for javac — every module under the Gradle/Maven project root.
fn build_java_sourcepath(project_root: &Path, overlay_root: &Path) -> Vec<PathBuf> {
    classpath::project_java_sourcepath(project_root, overlay_root)
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

    if (markers.lombok || content.contains("@Slf4j"))
        && content.contains("@Slf4j")
        && missing_symbol
        && lower.contains("variable log")
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

    if markers.lombok
        && content.contains('@')
        && dependency_unresolved(classpath::classpath_includes_lombok(jars), jars, tooling_pending)
        && lower.contains("lombok")
    {
        return missing_package || missing_symbol;
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
    if is_well_known_external_type(&type_name, content) {
        return false;
    }
    read_project_type_source(ws, project_root, &type_name, content, overlays).is_some()
}

fn parse_missing_class_symbol(message: &str) -> Option<String> {
    if !message.contains("cannot find symbol") {
        return None;
    }
    let class_line = message
        .lines()
        .find(|l| l.contains("symbol:") && l.contains("class "))?;
    let type_name = class_line.split("class ").nth(1)?.trim();
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
    let method_line = message
        .lines()
        .find(|l| l.contains("symbol:") && l.contains("method "))?;
    let method_part = method_line.split("method ").nth(1)?.trim();
    let method_name = method_part.split('(').next()?.trim();
    if method_name.is_empty() {
        return None;
    }
    let loc_line = message
        .lines()
        .find(|l| l.contains("location:") && l.contains("type "))?;
    let type_name = loc_line.rsplit("type ").next()?.trim();
    if type_name.is_empty() {
        return None;
    }
    Some((method_name.to_string(), type_name.to_string()))
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

fn read_project_type_source(
    ws: &Path,
    project_root: &Path,
    type_name: &str,
    content: &str,
    overlays: &[(String, String)],
) -> Option<String> {
    if let Some(fqcn) = resolve_project_type_fqcn(content, type_name) {
        let rel = format!("{}.java", fqcn.replace('.', "/"));
        for (path, text) in overlays {
            if path.replace('\\', "/").ends_with(&rel) {
                return Some(text.clone());
            }
        }
        let disk = ws.join(&rel);
        if disk.is_file() {
            return std::fs::read_to_string(disk).ok();
        }
    }

    for (path, text) in overlays {
        if path.ends_with(".java") && source_defines_type(text, type_name) {
            return Some(text.clone());
        }
    }

    for prefix in super::java_sources::discover_source_prefixes(project_root) {
        let root = project_root.join(prefix);
        if let Some(path) = find_file_named_recursive(&root, &format!("{type_name}.java"), 0) {
            if let Ok(rel) = path.strip_prefix(ws) {
                let rel = rel.to_string_lossy().replace('\\', "/");
                if let Some(text) = overlays.iter().find(|(p, _)| p == &rel).map(|(_, t)| t.clone())
                {
                    return Some(text);
                }
            }
            return std::fs::read_to_string(path).ok();
        }
    }
    None
}

fn source_defines_type(content: &str, type_name: &str) -> bool {
    content.lines().any(|line| {
        let t = line.trim();
        t.contains(&format!("interface {type_name}"))
            || t.contains(&format!("class {type_name}"))
            || t.contains(&format!("enum {type_name}"))
            || t.contains(&format!("record {type_name}"))
    })
}

fn source_declares_method(content: &str, method_name: &str) -> bool {
    content.lines().any(|line| {
        super::symbols::java_method_name_on_line(line).as_deref() == Some(method_name)
            || line.split("//").next().unwrap_or(line).contains(&format!("{method_name}("))
    })
}

fn find_file_named_recursive(dir: &Path, file_name: &str, depth: usize) -> Option<PathBuf> {
    if depth > 14 || !dir.is_dir() {
        return None;
    }
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() {
            if path.file_name().and_then(|n| n.to_str()) == Some(file_name) {
                return Some(path);
            }
        } else if path.is_dir() {
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

fn check_plain_java(ws: &Path, rel_path: &str, content: &str) -> Result<Vec<Diagnostic>> {
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

    let out = run_java_command(
        ws,
        "javac",
        &[
            "-encoding",
            "UTF-8",
            "-d",
            out_dir.to_str().context("invalid out dir")?,
            &rel,
        ],
    )?;

    let mut diags = parse_compiler_output(&out.stderr, ws, rel_path, content);
    if diags.is_empty() {
        diags = parse_compiler_output(&out.stdout, ws, rel_path, content);
    }
    Ok(diags)
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
    let project = detect_java_release(gradle_root);
    if let Ok(home) = crate::jdk::effective_java_home() {
        if crate::jdk::java_major_version(&home).is_some_and(|major| major <= 8) {
            return "8".into();
        }
    }
    project
}

fn detect_java_release(project_root: &Path) -> String {
    if super::maven::is_maven_project_root(project_root) {
        if let Ok(text) = std::fs::read_to_string(project_root.join("pom.xml")) {
            if let Some(v) = extract_maven_java_version(&text) {
                return v;
            }
        }
    }
    for name in ["build.gradle.kts", "build.gradle"] {
        let path = project_root.join(name);
        if let Ok(text) = std::fs::read_to_string(&path) {
            if let Some(v) = extract_release_version(&text) {
                return v;
            }
        }
    }
    "17".into()
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

/// Effective Java source level for a file: min(selected JDK major, project sourceCompatibility).
pub fn java_language_level(ws: &Path, path: &str) -> u32 {
    let jdk_major = crate::jdk::effective_java_home()
        .ok()
        .and_then(|h| crate::jdk::java_major_version(&h))
        .unwrap_or(17);
    let project_root = find_gradle_root(ws, path)
        .ok()
        .flatten()
        .or_else(|| find_maven_root(ws, path).ok().flatten());
    if let Some(root) = project_root {
        let project = detect_java_release(&root)
            .parse()
            .unwrap_or(jdk_major);
        return jdk_major.min(project);
    }
    jdk_major
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
}
