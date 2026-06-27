use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use super::classpath;
use super::diagnostics::Diagnostic;
use super::exec::run_java_command;
use super::gradle::find_gradle_root;
use super::{safe_join};

const DIAG_ROOT: &str = ".reaper/java-diagnostics";
const DIAG_OUT: &str = ".reaper/java-diagnostics-out";

pub type JavaDiagnostic = Diagnostic;

pub fn check_java(ws: &Path, rel_path: &str, content: &str) -> Result<Vec<Diagnostic>> {
    if !rel_path.ends_with(".java") {
        return Ok(Vec::new());
    }

    let _ = safe_join(ws, rel_path)?;

    if let Some(gradle_root) = find_gradle_root(ws, rel_path)? {
        return check_gradle_java(ws, &gradle_root, rel_path, content);
    }

    check_plain_java(ws, rel_path, content)
}

fn check_gradle_java(
    ws: &Path,
    gradle_root: &Path,
    rel_path: &str,
    content: &str,
) -> Result<Vec<Diagnostic>> {
    let jars = classpath::cached_classpath_jars(gradle_root);
    let classpath_ready = !jars.is_empty();
    if !classpath_ready {
        tracing::debug!(
            "Gradle classpath not resolved yet for {} — running syntax-only javac",
            rel_path
        );
    }

    let overlay_root = ws.join(DIAG_ROOT).join("overlay");
    let overlay_file = overlay_root.join(rel_path);
    if let Some(parent) = overlay_file.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&overlay_file, content)?;

    let mut sourcepath = Vec::new();
    for rel in ["src/main/java", "src/test/java"] {
        if rel_path.starts_with(rel) {
            sourcepath.push(overlay_root.join(rel));
        }
        let dir = gradle_root.join(rel);
        if dir.is_dir() {
            sourcepath.push(dir);
        }
    }

    let out_dir = ws.join(DIAG_OUT);
    std::fs::create_dir_all(&out_dir)?;

    let cp = jars
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(if cfg!(windows) { ";" } else { ":" });

    let mut args = vec![
        "-encoding".to_string(),
        "UTF-8".to_string(),
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
    append_javac_release_args(&mut args, gradle_root);
    args.push(overlay_file.to_string_lossy().into_owned());

    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let out = run_java_command(ws, "javac", &arg_refs)?;

    let mut diags = parse_compiler_output(&out.stderr, ws, rel_path, content);
    if diags.is_empty() {
        diags = parse_compiler_output(&out.stdout, ws, rel_path, content);
    }
    Ok(filter_compiler_diagnostics(diags, classpath_ready))
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
    Ok(filter_compiler_diagnostics(diags, false))
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

fn detect_java_release(gradle_root: &Path) -> String {
    for name in ["build.gradle.kts", "build.gradle"] {
        let path = gradle_root.join(name);
        if let Ok(text) = std::fs::read_to_string(&path) {
            if let Some(v) = extract_release_version(&text) {
                return v;
            }
        }
    }
    "17".into()
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
            i += 1;
            while i < lines.len() {
                let next = lines[i].trim();
                if parse_diagnostic_line(next, &ws_canon).is_some() || next.is_empty() {
                    break;
                }
                if next.starts_with("symbol:")
                    || next.starts_with("location:")
                    || next.starts_with("^")
                    || next.starts_with("Note:")
                {
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
                    ..diag
                });
            }
            continue;
        }
        i += 1;
    }

    diags
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

/// Single-file javac floods imports with missing-symbol noise; show real compiler errors.
fn filter_compiler_diagnostics(diags: Vec<Diagnostic>, classpath_ready: bool) -> Vec<Diagnostic> {
    diags
        .into_iter()
        .filter(|d| should_show_java_diag(d, classpath_ready))
        .collect()
}

fn should_show_java_diag(diag: &Diagnostic, classpath_ready: bool) -> bool {
    let msg = diag.message.to_lowercase();

    if diag.severity == "warning" {
        return true;
    }

    if is_javac_syntax_error(&msg) {
        return true;
    }

    if is_dependency_resolution_error(&msg) {
        return false;
    }

    classpath_ready
}

fn is_javac_syntax_error(msg: &str) -> bool {
    msg.contains("';' expected")
        || msg.contains("')' expected")
        || msg.contains("'(' expected")
        || msg.contains("'{' expected")
        || msg.contains("'}' expected")
        || msg.contains("'[' expected")
        || msg.contains("']' expected")
        || msg.contains("illegal start of expression")
        || msg.contains("illegal start of type")
        || msg.contains("class, interface, enum, or record expected")
        || msg.contains("reached end of file while parsing")
        || msg.contains("not a statement")
        || msg.contains("orphaned")
        || msg.contains("<identifier> expected")
}

fn is_dependency_resolution_error(msg: &str) -> bool {
    msg.contains("cannot find symbol")
        || msg.contains("does not exist")
        || msg.contains("cannot access")
        || msg.contains("package")
        || msg.contains("cannot infer type arguments")
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn hides_import_symbol_noise_without_classpath() {
        let diags = vec![Diagnostic {
            path: "App.java".into(),
            line: 1,
            column: 1,
            end_line: None,
            end_column: None,
            message: "cannot find symbol".into(),
            severity: "error".into(),
        }];
        assert!(filter_compiler_diagnostics(diags, false).is_empty());
    }

    #[test]
    fn keeps_syntax_errors() {
        let diags = vec![Diagnostic {
            path: "App.java".into(),
            line: 1,
            column: 24,
            end_line: None,
            end_column: None,
            message: "';' expected".into(),
            severity: "error".into(),
        }];
        assert_eq!(filter_compiler_diagnostics(diags, false).len(), 1);
    }

    #[test]
    fn keeps_semantic_errors_when_classpath_ready() {
        let diags = vec![Diagnostic {
            path: "App.java".into(),
            line: 5,
            column: 9,
            end_line: None,
            end_column: None,
            message: "incompatible types: int cannot be converted to java.lang.String".into(),
            severity: "error".into(),
        }];
        assert_eq!(filter_compiler_diagnostics(diags, true).len(), 1);
    }
}
