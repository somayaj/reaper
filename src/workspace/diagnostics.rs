use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::Deserialize;
use serde::Serialize;

use super::exec::{run_command, run_shell_argv, run_tool_command};
use super::java_diagnostics;
use super::safe_join;

const OVERLAY_PREFIX: &str = ".reaper/diagnostics/overlay/";

#[derive(Debug, Clone, Serialize)]
pub struct Diagnostic {
    pub path: String,
    pub line: u32,
    pub column: u32,
    pub end_line: Option<u32>,
    pub end_column: Option<u32>,
    pub message: String,
    pub severity: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct FileDiagnosticsResult {
    pub diagnostics: Vec<Diagnostic>,
    #[serde(default, skip_serializing_if = "FileDiagnosticsResult::omit_cancelled")]
    pub cancelled: bool,
}

impl FileDiagnosticsResult {
    fn omit_cancelled(cancelled: &bool) -> bool {
        !*cancelled
    }

    pub fn ready(diagnostics: Vec<Diagnostic>) -> Self {
        Self {
            diagnostics,
            cancelled: false,
        }
    }

    pub fn cancelled() -> Self {
        Self {
            diagnostics: Vec::new(),
            cancelled: true,
        }
    }
}

pub fn check_file(
    ws: &Path,
    rel_path: &str,
    content: &str,
    overlays: &[(String, String)],
    scope: java_diagnostics::JavaDiagScope,
) -> Result<Vec<Diagnostic>> {
    if rel_path.starts_with(".reaper/") {
        return Ok(Vec::new());
    }
    let _ = safe_join(ws, rel_path)?;

    let lower = rel_path.to_lowercase();
    let ext = file_extension(&lower);

    if lower.ends_with(".java") {
        let (diagnostics, cancelled) =
            java_diagnostics::check_java(ws, rel_path, content, overlays, scope)?;
        if cancelled {
            return Ok(Vec::new());
        }
        return Ok(diagnostics);
    }
    if ext == "rs" {
        return check_rust(ws, rel_path, content);
    }
    if matches!(ext, "py" | "pyw") {
        return check_python(ws, rel_path, content);
    }
    if ext == "go" {
        return check_go(ws, rel_path, content);
    }
    if matches!(ext, "json") {
        return check_json(ws, rel_path, content);
    }
    if matches!(ext, "js" | "mjs" | "cjs" | "jsx") {
        return check_javascript(ws, rel_path, content);
    }
    if matches!(ext, "ts" | "tsx") {
        return check_typescript(ws, rel_path, content);
    }
    if matches!(ext, "kt" | "kts") || lower.ends_with(".gradle.kts") {
        return check_kotlin(ws, rel_path, content);
    }
    if matches!(ext, "yaml" | "yml") {
        return check_yaml(ws, rel_path, content);
    }
    if matches!(ext, "xml" | "html" | "htm") {
        return check_xml(ws, rel_path, content);
    }
    if matches!(ext, "vue" | "svelte") {
        return check_component_file(ws, rel_path, content);
    }
    if ext == "r" {
        return check_r_lang(ws, rel_path, content);
    }
    if ext == "toml" {
        return check_toml(ws, rel_path, content);
    }
    if matches!(ext, "css" | "scss" | "less") {
        return check_stylelint(ws, rel_path, content);
    }
    if ext == "rb" {
        return check_ruby(ws, rel_path, content);
    }
    if ext == "php" {
        return check_php(ws, rel_path, content);
    }
    if matches!(ext, "c" | "h" | "cpp" | "cc" | "cxx" | "hpp" | "hh") {
        return check_c(ws, rel_path, content);
    }
    if ext == "swift" {
        return check_swift(ws, rel_path, content);
    }
    if lower.ends_with(".gradle") || ext == "groovy" {
        return check_groovy(ws, rel_path, content);
    }
    if matches!(ext, "jsonc") {
        return check_json(ws, rel_path, &strip_jsonc_comments(content));
    }
    if matches!(ext, "ini" | "properties") || lower.ends_with(".gradle.properties") {
        return Ok(check_ini(rel_path, content));
    }
    if matches!(ext, "sh" | "bash" | "zsh") {
        return check_shell(ws, rel_path, content);
    }
    if ext == "lua" {
        return check_lua(ws, rel_path, content);
    }
    if ext == "cs" {
        return check_csharp(ws, rel_path, content);
    }
    if ext == "dart" {
        return check_dart(ws, rel_path, content);
    }
    if matches!(ext, "sql") {
        return check_sql(ws, rel_path, content);
    }
    if base_name_is(&lower, "dockerfile") || ext == "dockerfile" {
        return check_dockerfile(ws, rel_path, content);
    }
    if matches!(ext, "proto") {
        return check_protobuf(ws, rel_path, content);
    }
    if matches!(ext, "graphql" | "gql") {
        return check_graphql(ws, rel_path, content);
    }
    if ext == "md" || ext == "mdx" {
        return check_markdown(ws, rel_path, content);
    }
    if base_name_is(&lower, "makefile") || base_name_is(&lower, "gnumakefile") {
        return check_makefile(ws, rel_path, content);
    }
    if base_name_is(&lower, "cmakelists.txt") {
        return check_cmake(ws, rel_path, content);
    }

    Ok(Vec::new())
}

pub fn diagnose_file(
    ws: &Path,
    rel_path: &str,
    content: &str,
    overlays: &[(String, String)],
    scope: java_diagnostics::JavaDiagScope,
) -> Result<FileDiagnosticsResult> {
    if rel_path.starts_with(".reaper/") {
        return Ok(FileDiagnosticsResult::ready(Vec::new()));
    }
    let lower = rel_path.to_lowercase();
    if lower.ends_with(".java") {
        let _ = safe_join(ws, rel_path)?;
        let (diagnostics, cancelled) =
            java_diagnostics::check_java(ws, rel_path, content, overlays, scope)?;
        if cancelled {
            // Do not fall back to jdtls publishDiagnostics — cache is stale after supersede
            // and paints incorrect squiggles while the next javac run is in flight.
            return Ok(FileDiagnosticsResult::cancelled());
        }
        return Ok(FileDiagnosticsResult::ready(diagnostics));
    }
    Ok(FileDiagnosticsResult::ready(check_file(
        ws, rel_path, content, overlays, scope,
    )?))
}

fn base_name_is(lower_path: &str, name: &str) -> bool {
    lower_path.rsplit('/').next().is_some_and(|b| b == name)
}

fn strip_jsonc_comments(content: &str) -> String {
    content
        .lines()
        .map(|line| {
            if let Some(idx) = line.find("//") {
                &line[..idx]
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn check_ini(rel_path: &str, content: &str) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for (i, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with(';') {
            continue;
        }
        if trimmed.starts_with('[') {
            if !trimmed.ends_with(']') || trimmed.len() < 3 {
                diags.push(diag(
                    rel_path,
                    (i + 1) as u32,
                    1,
                    "Invalid INI section header",
                    "error",
                ));
            }
            continue;
        }
        if !trimmed.contains('=') && !trimmed.contains(':') {
            diags.push(diag(
                rel_path,
                (i + 1) as u32,
                1,
                "Expected key=value property",
                "error",
            ));
        }
    }
    diags
}

fn check_shell(ws: &Path, rel_path: &str, content: &str) -> Result<Vec<Diagnostic>> {
    write_overlay(ws, rel_path, content)?;
    let rel = overlay_rel(rel_path);
    for prog in ["bash", "sh", "zsh"] {
        let out = if prog == "bash" {
            run_tool_command(ws, "bash", &["-n", &rel])
        } else {
            run_command(ws, prog, &["-n", &rel])
        };
        if out.is_err() {
            continue;
        }
        let out = out?;
        if out.exit_code == 0 {
            return Ok(Vec::new());
        }
        let text = format!("{}\n{}", out.stderr, out.stdout);
        let line = text
            .lines()
            .find_map(|l| l.split(':').nth(1).and_then(|s| s.trim().parse().ok()))
            .unwrap_or(1);
        let message = text.lines().next().unwrap_or("Shell syntax error").to_string();
        return Ok(vec![diag(rel_path, line, 1, message, "error")]);
    }
    Ok(Vec::new())
}

fn check_lua(ws: &Path, rel_path: &str, content: &str) -> Result<Vec<Diagnostic>> {
    write_overlay(ws, rel_path, content)?;
    let rel = overlay_rel(rel_path);
    let out = run_tool_command(ws, "luac", &["-p", &rel]);
    let Ok(out) = out else {
        return Ok(Vec::new());
    };
    if out.exit_code == 0 {
        return Ok(Vec::new());
    }
    let text = format!("{}\n{}", out.stderr, out.stdout);
    let line = text
        .lines()
        .find_map(|l| {
            l.split(':')
                .nth(1)
                .and_then(|s| s.trim().parse().ok())
        })
        .unwrap_or(1);
    Ok(vec![diag(
        rel_path,
        line,
        1,
        text.lines().next().unwrap_or("Lua syntax error"),
        "error",
    )])
}

fn check_csharp(ws: &Path, rel_path: &str, content: &str) -> Result<Vec<Diagnostic>> {
    write_overlay(ws, rel_path, content)?;
    let rel = overlay_rel(rel_path);
    let out = run_tool_command(ws, "csc", &["/nologo", "/t:library", &rel]);
    let Ok(out) = out else {
        return Ok(Vec::new());
    };
    if out.exit_code == 0 {
        return Ok(Vec::new());
    }
    Ok(parse_clang_output(
        &format!("{}\n{}", out.stderr, out.stdout),
        rel_path,
    ))
}

fn check_dart(ws: &Path, rel_path: &str, content: &str) -> Result<Vec<Diagnostic>> {
    write_overlay(ws, rel_path, content)?;
    let rel = overlay_rel(rel_path);
    let out = run_tool_command(ws, "dart", &["analyze", &rel]);
    let Ok(out) = out else {
        return Ok(Vec::new());
    };
    if out.exit_code == 0 {
        return Ok(Vec::new());
    }
    let mut diags = Vec::new();
    for line in format!("{}\n{}", out.stderr, out.stdout).lines() {
        // info • file.dart:12:3 • message
        let parts: Vec<&str> = line.split('•').map(str::trim).collect();
        if parts.len() < 3 {
            continue;
        }
        let severity = if parts[0].contains("error") {
            "error"
        } else if parts[0].contains("warning") {
            "warning"
        } else {
            continue;
        };
        if let Some((path, loc)) = parts[1].split_once(':') {
            let line_no: u32 = loc.split(':').next().and_then(|s| s.parse().ok()).unwrap_or(1);
            let col: u32 = loc.split(':').nth(1).and_then(|s| s.parse().ok()).unwrap_or(1);
            if matches_focus(path, rel_path) {
                diags.push(diag(
                    &strip_overlay_prefix(path),
                    line_no,
                    col,
                    parts[2..].join(" • "),
                    severity,
                ));
            }
        }
    }
    Ok(diags)
}

fn check_sql(ws: &Path, rel_path: &str, content: &str) -> Result<Vec<Diagnostic>> {
    use super::exec::run_tool_command;

    write_overlay(ws, rel_path, content)?;
    let rel = overlay_rel(rel_path);
    let out = run_tool_command(ws, "sqlfluff", &["lint", &rel, "--format", "human"]);
    let Ok(out) = out else {
        return Ok(Vec::new());
    };
    if out.exit_code == 0 {
        return Ok(Vec::new());
    }
    Ok(parse_stylelint_output(
        &format!("{}\n{}", out.stderr, out.stdout),
        rel_path,
    ))
}

fn check_dockerfile(ws: &Path, rel_path: &str, content: &str) -> Result<Vec<Diagnostic>> {
    write_overlay(ws, rel_path, content)?;
    let rel = overlay_rel(rel_path);
    let out = run_command(ws, "hadolint", &[&rel]);
    let Ok(out) = out else {
        return Ok(Vec::new());
    };
    if out.exit_code == 0 {
        return Ok(Vec::new());
    }
    let mut diags = Vec::new();
    for line in format!("{}\n{}", out.stderr, out.stdout).lines() {
        // hadolint: DL3006 warning: ...
        if let Some(rest) = line.strip_prefix("hadolint: ") {
            diags.push(diag(rel_path, 1, 1, rest, "warning"));
        }
    }
    Ok(diags)
}

fn check_protobuf(ws: &Path, rel_path: &str, content: &str) -> Result<Vec<Diagnostic>> {
    write_overlay(ws, rel_path, content)?;
    let rel = overlay_rel(rel_path);
    let out = run_command(ws, "protoc", &["--proto_path=.", &rel, "--descriptor_set_out=/dev/null"]);
    let Ok(out) = out else {
        return Ok(Vec::new());
    };
    if out.exit_code == 0 {
        return Ok(Vec::new());
    }
    Ok(vec![diag(
        rel_path,
        1,
        1,
        out.stderr.lines().next().unwrap_or("Protobuf error"),
        "error",
    )])
}

fn check_graphql(ws: &Path, rel_path: &str, content: &str) -> Result<Vec<Diagnostic>> {
    write_overlay(ws, rel_path, content)?;
    let abs = ws.join(overlay_rel(rel_path));
    let abs = abs.to_string_lossy().to_string();
    let out = run_command(
        ws,
        "npx",
        &["--yes", "graphql-schema-linter", &abs],
    );
    let Ok(out) = out else {
        return Ok(Vec::new());
    };
    if out.exit_code == 0 {
        return Ok(Vec::new());
    }
    let mut diags = Vec::new();
    for line in format!("{}\n{}", out.stderr, out.stdout).lines() {
        // path:12:3 message
        if let Some(d) = parse_location_colon_error(line.trim(), rel_path) {
            diags.push(d);
        }
    }
    Ok(diags)
}

fn check_markdown(ws: &Path, rel_path: &str, content: &str) -> Result<Vec<Diagnostic>> {
    write_overlay(ws, rel_path, content)?;
    let rel = overlay_rel(rel_path);
    let out = run_command(ws, "npx", &["--yes", "markdownlint-cli2", rel.as_str()]);
    let Ok(out) = out else {
        return Ok(Vec::new());
    };
    if out.exit_code == 0 {
        return Ok(Vec::new());
    }
    let mut diags = Vec::new();
    for line in format!("{}\n{}", out.stderr, out.stdout).lines() {
        // file.md:12 error MD041 ...
        let parts: Vec<&str> = line.splitn(3, ':').collect();
        if parts.len() >= 3 {
            if let Ok(line_no) = parts[1].trim().parse::<u32>() {
                if matches_focus(parts[0], rel_path) {
                    diags.push(diag(
                        &strip_overlay_prefix(parts[0]),
                        line_no,
                        1,
                        parts[2].trim(),
                        "warning",
                    ));
                }
            }
        }
    }
    Ok(diags)
}

fn check_makefile(ws: &Path, rel_path: &str, content: &str) -> Result<Vec<Diagnostic>> {
    write_overlay(ws, rel_path, content)?;
    let dir = ws.join(OVERLAY_PREFIX).join(rel_path);
    let dir = dir.parent().unwrap_or(ws);
    let out = run_command(dir, "make", &["-n", "-f", rel_path.rsplit('/').next().unwrap_or(rel_path)]);
    let Ok(out) = out else {
        return Ok(Vec::new());
    };
    if out.exit_code == 0 {
        return Ok(Vec::new());
    }
    Ok(vec![diag(
        rel_path,
        1,
        1,
        out.stderr.lines().next().unwrap_or("Makefile error"),
        "error",
    )])
}

fn check_cmake(ws: &Path, rel_path: &str, content: &str) -> Result<Vec<Diagnostic>> {
    write_overlay(ws, rel_path, content)?;
    let rel = overlay_rel(rel_path);
    let out = run_command(ws, "cmake", &["--warn-uninitialized", "-P", &rel]);
    let Ok(out) = out else {
        return Ok(Vec::new());
    };
    if out.exit_code == 0 {
        return Ok(Vec::new());
    }
    Ok(vec![diag(
        rel_path,
        1,
        1,
        out.stderr.lines().next().unwrap_or("CMake error"),
        "error",
    )])
}

fn file_extension(lower_path: &str) -> &str {
    let base = lower_path.rsplit('/').next().unwrap_or(lower_path);
    base.rsplit('.').next().unwrap_or("")
}

fn write_overlay(ws: &Path, rel_path: &str, content: &str) -> Result<PathBuf> {
    let path = ws.join(OVERLAY_PREFIX).join(rel_path);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, content)?;
    Ok(path)
}

fn overlay_rel(rel_path: &str) -> String {
    format!("{OVERLAY_PREFIX}{rel_path}")
}

fn diag(path: &str, line: u32, column: u32, message: impl Into<String>, severity: &str) -> Diagnostic {
    Diagnostic {
        path: path.to_string(),
        line: line.max(1),
        column: column.max(1),
        end_line: None,
        end_column: None,
        message: message.into(),
        severity: severity.to_string(),
    }
}

fn matches_focus(path: &str, focus: &str) -> bool {
    let path = strip_overlay_prefix(&path.replace('\\', "/"));
    let focus = focus.replace('\\', "/");
    path == focus || focus.ends_with(&path) || path.ends_with(&focus)
}

fn strip_overlay_prefix(path: &str) -> String {
    path.strip_prefix(OVERLAY_PREFIX)
        .or_else(|| path.strip_prefix(".reaper/java-diagnostics/overlay/"))
        .unwrap_or(path)
        .to_string()
}

fn check_json(ws: &Path, rel_path: &str, content: &str) -> Result<Vec<Diagnostic>> {
    write_overlay(ws, rel_path, content)?;
    let mut diags = Vec::new();
    diags.extend(check_json_syntax(rel_path, content));
    diags.extend(check_json_jsonlint(ws, rel_path));
    if looks_like_json_schema(rel_path, content) {
        diags.extend(check_json_ajv_schema(ws, rel_path));
    }
    Ok(diags)
}

fn check_json_syntax(rel_path: &str, content: &str) -> Vec<Diagnostic> {
    if content.trim().is_empty() {
        return Vec::new();
    }
    match serde_json::from_str::<serde_json::Value>(content) {
        Ok(_) => Vec::new(),
        Err(e) => {
            let line = e.line().max(1) as u32;
            let column = e.column().max(1) as u32;
            vec![diag(rel_path, line, column, e.to_string(), "error")]
        }
    }
}

fn looks_like_json_schema(rel_path: &str, content: &str) -> bool {
    let path_lower = rel_path.replace('\\', "/").to_lowercase();
    let lower = content.to_lowercase();
    let has_schema_marker = lower.contains("\"$schema\"")
        || lower.contains("\"$ref\"")
        || path_lower.ends_with(".schema.json")
        || path_lower.contains("/schemas/");
    let has_schema_shape = lower.contains("\"properties\"")
        || lower.contains("\"definitions\"")
        || lower.contains("\"$defs\"");
    has_schema_marker && has_schema_shape
}

fn check_json_jsonlint(ws: &Path, rel_path: &str) -> Vec<Diagnostic> {
    let rel = overlay_rel(rel_path);
    let out = run_tool_command(ws, "jsonlint", &[&rel])
        .or_else(|_| run_shell_argv(ws, "jsonlint", &[&rel]));
    let Ok(out) = out else {
        return Vec::new();
    };
    if out.exit_code == 0 {
        return Vec::new();
    }
    parse_jsonlint_output(&format!("{}\n{}", out.stderr, out.stdout), rel_path)
}

fn parse_jsonlint_output(text: &str, focus: &str) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('^') || line.chars().all(|c| c == '-') {
            continue;
        }
        if let Some(d) = parse_location_colon_error(line, focus) {
            diags.push(d);
            continue;
        }
        if let Some(d) = parse_jsonlint_parse_error_line(line, focus) {
            diags.push(d);
        }
    }
    if diags.is_empty() {
        if let Some(d) = parse_jsonlint_parse_error_block(text, focus) {
            diags.push(d);
        }
    }
    diags
}

fn parse_jsonlint_parse_error_line(line: &str, focus: &str) -> Option<Diagnostic> {
    let line = line.strip_prefix("Error: ").unwrap_or(line);
    const MARKER: &str = "Parse error on line ";
    let idx = line.find(MARKER)?;
    let head = line[..idx].trim();
    let rest = &line[idx + MARKER.len()..];
    let line_no: u32 = rest.split(':').next()?.trim().parse().ok()?;
    if !head.is_empty() && !matches_focus(head.trim_end_matches(':'), focus) {
        return None;
    }
    Some(diag(focus, line_no, 1, line.to_string(), "error"))
}

fn parse_jsonlint_parse_error_block(text: &str, focus: &str) -> Option<Diagnostic> {
    for line in text.lines() {
        if let Some(d) = parse_jsonlint_parse_error_line(line.trim(), focus) {
            return Some(d);
        }
    }
    let message = text
        .lines()
        .map(str::trim)
        .find(|l| {
            !l.is_empty()
                && !l.starts_with('^')
                && !l.chars().all(|c| c == '-')
                && !l.contains("Parse error on line ")
        })
        .unwrap_or("Invalid JSON");
    Some(diag(focus, 1, 1, message, "error"))
}

fn check_json_ajv_schema(ws: &Path, rel_path: &str) -> Vec<Diagnostic> {
    let rel = overlay_rel(rel_path);
    let out = run_tool_command(ws, "ajv", &["compile", "-s", &rel])
        .or_else(|_| run_shell_argv(ws, "ajv", &["compile", "-s", &rel]));
    let Ok(out) = out else {
        return Vec::new();
    };
    if out.exit_code == 0 {
        return Vec::new();
    }
    parse_ajv_compile_output(&format!("{}\n{}", out.stderr, out.stdout), rel_path)
}

fn parse_ajv_compile_output(text: &str, focus: &str) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(d) = parse_location_colon_error(line, focus) {
            diags.push(d);
            continue;
        }
        let severity = if line.to_ascii_lowercase().contains("warning") {
            "warning"
        } else {
            "error"
        };
        diags.push(diag(focus, 1, 1, format!("ajv: {line}"), severity));
    }
    diags
}

fn check_rust(ws: &Path, rel_path: &str, content: &str) -> Result<Vec<Diagnostic>> {
    write_overlay(ws, rel_path, content)?;
    let rel = overlay_rel(rel_path);
    let Ok(out) = run_tool_command(
        ws,
        "rustc",
        &[
            "--error-format=human",
            "--crate-type",
            "lib",
            "--edition",
            "2021",
            &rel,
        ],
    ) else {
        return Ok(Vec::new());
    };
    Ok(parse_rustc_output(&out.stderr, rel_path))
}

fn parse_rustc_output(text: &str, focus: &str) -> Vec<Diagnostic> {
    let lines: Vec<&str> = text.lines().collect();
    let mut diags = Vec::new();
    let mut pending_msg: Option<(String, &str)> = None;

    for line in lines {
        let trimmed = line.trim();
        if trimmed.starts_with("error") || trimmed.starts_with("warning") {
            let severity = if trimmed.starts_with("warning") {
                "warning"
            } else {
                "error"
            };
            pending_msg = Some((trimmed.to_string(), severity));
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("--> ") {
            if let Some((path, line_no, col)) = parse_location_tail(rest) {
                if let Some((message, severity)) = pending_msg.take() {
                    if matches_focus(&path, focus) {
                        diags.push(diag(&strip_overlay_prefix(&path), line_no, col, message, severity));
                    }
                }
            }
        }
    }
    diags
}

fn parse_location_tail(rest: &str) -> Option<(String, u32, u32)> {
    // path:line:col or path:line:col: ...
    let (path, tail) = rest.split_once(':')?;
    let mut nums = tail.split(':');
    let line: u32 = nums.next()?.trim().parse().ok()?;
    let col: u32 = nums.next()?.trim().parse().ok()?;
    Some((path.trim().to_string(), line, col))
}

fn check_python(ws: &Path, rel_path: &str, content: &str) -> Result<Vec<Diagnostic>> {
    write_overlay(ws, rel_path, content)?;
    let rel = overlay_rel(rel_path);
    let Ok(out) = run_tool_command(ws, "python", &["-m", "py_compile", &rel]) else {
        return Ok(Vec::new());
    };
    if out.exit_code == 0 {
        return Ok(Vec::new());
    }
    Ok(parse_python_output(
        &format!("{}\n{}", out.stderr, out.stdout),
        rel_path,
    ))
}

fn parse_python_output(text: &str, focus: &str) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    let lines: Vec<&str> = text.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i].trim();
        if let Some(idx) = line.find("File \"") {
            let rest = &line[idx + 6..];
            if let Some(end) = rest.find('"') {
                let path = &rest[..end];
                let line_no = line
                    .split(", line ")
                    .nth(1)
                    .and_then(|s| s.split(',').next())
                    .and_then(|s| s.trim().parse().ok())
                    .unwrap_or(1);
                if matches_focus(path, focus) {
                    let mut message = String::new();
                    i += 1;
                    while i < lines.len() {
                        let next = lines[i].trim();
                        if next.starts_with("File \"") {
                            break;
                        }
                        if next.starts_with("SyntaxError:")
                            || next.starts_with("IndentationError:")
                            || next.starts_with("TabError:")
                        {
                            message = next.to_string();
                            break;
                        }
                        i += 1;
                    }
                    if message.is_empty() {
                        message = "Syntax error".into();
                    }
                    diags.push(diag(
                        &strip_overlay_prefix(path),
                        line_no,
                        1,
                        message,
                        "error",
                    ));
                }
            }
        }
        i += 1;
    }
    diags
}

fn check_go(ws: &Path, rel_path: &str, content: &str) -> Result<Vec<Diagnostic>> {
    write_overlay(ws, rel_path, content)?;
    let rel = overlay_rel(rel_path);
    let null_out = if cfg!(windows) { "NUL" } else { "/dev/null" };
    let Ok(out) = run_tool_command(ws, "go", &["build", "-o", null_out, &rel]) else {
        return Ok(Vec::new());
    };
    Ok(parse_go_output(&format!("{}\n{}", out.stderr, out.stdout), rel_path))
}

fn parse_go_output(text: &str, focus: &str) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        // file.go:12:3: message
        let Some((head, message)) = line.rsplit_once(": ") else {
            continue;
        };
        let parts: Vec<&str> = head.split(':').collect();
        if parts.len() < 3 {
            continue;
        }
        let path = parts[..parts.len() - 2].join(":");
        let line_no: u32 = parts[parts.len() - 2].parse().unwrap_or(1);
        let col: u32 = parts[parts.len() - 1].parse().unwrap_or(1);
        if !matches_focus(&path, focus) {
            continue;
        }
        let severity = if message.contains("warning") {
            "warning"
        } else {
            "error"
        };
        diags.push(diag(
            &strip_overlay_prefix(&path),
            line_no,
            col,
            message,
            severity,
        ));
    }
    diags
}

fn check_javascript(ws: &Path, rel_path: &str, content: &str) -> Result<Vec<Diagnostic>> {
    write_overlay(ws, rel_path, content)?;
    let rel = overlay_rel(rel_path);
    let Ok(out) = run_tool_command(ws, "node", &["--check", &rel]) else {
        return Ok(Vec::new());
    };
    if out.exit_code == 0 {
        return Ok(Vec::new());
    }
    Ok(parse_node_output(&format!("{}\n{}", out.stderr, out.stdout), rel_path))
}

fn parse_node_output(text: &str, focus: &str) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    let lines: Vec<&str> = text.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("SyntaxError:") || trimmed.starts_with("ReferenceError:") {
            let message = trimmed.to_string();
            let (line_no, col) = lines
                .get(i.saturating_sub(1))
                .and_then(|prev| parse_node_location(prev))
                .unwrap_or((1, 1));
            diags.push(diag(focus, line_no, col, message, "error"));
        } else if let Some((path, line_no, col)) = parse_node_path_line(trimmed) {
            if matches_focus(&path, focus) {
                let message = lines
                    .get(i + 1)
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| "Syntax error".into());
                diags.push(diag(
                    &strip_overlay_prefix(&path),
                    line_no,
                    col,
                    message,
                    "error",
                ));
            }
        }
    }
    diags
}

fn parse_node_location(line: &str) -> Option<(u32, u32)> {
    // at path:line:col or path:line
    let (_, tail) = line.rsplit_once(':')?;
    let col: u32 = tail.trim().parse().ok()?;
    let (head, line_str) = line.rsplit_once(':')?;
    let line_no: u32 = line_str.trim().parse().ok()?;
    let _ = head;
    Some((line_no, col))
}

fn parse_node_path_line(line: &str) -> Option<(String, u32, u32)> {
    // file.js:10:5 or file.js:10
    if !line.contains('.') {
        return None;
    }
    let (path, tail) = line.rsplit_once(':')?;
    if let Ok(col) = tail.trim().parse::<u32>() {
        if let Some((path, line_str)) = path.rsplit_once(':') {
            if let Ok(line_no) = line_str.trim().parse::<u32>() {
                return Some((path.to_string(), line_no, col));
            }
        }
        if let Some(line_no) = path.rsplit_once(':').and_then(|(_, l)| l.trim().parse().ok()) {
            return Some((path.to_string(), line_no, 1));
        }
    }
    None
}

fn check_typescript(ws: &Path, rel_path: &str, content: &str) -> Result<Vec<Diagnostic>> {
    write_overlay(ws, rel_path, content)?;
    let rel = overlay_rel(rel_path);
    let out = run_tool_command(
        ws,
        "tsc",
        &[
            "--noEmit",
            "--pretty",
            "false",
            "--target",
            "ES2020",
            "--module",
            "commonjs",
            "--moduleResolution",
            "node",
            "--skipLibCheck",
            &rel,
        ],
    );
    let Ok(out) = out else {
        return Ok(Vec::new());
    };
    if out.exit_code == 0 {
        return Ok(Vec::new());
    }
    Ok(parse_tsc_output(&format!("{}\n{}", out.stderr, out.stdout), rel_path))
}

fn parse_tsc_output(text: &str, focus: &str) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        // file.ts(12,5): error TS2304: message
        let Some((head, rest)) = line.split_once("): ") else {
            continue;
        };
        let Some((path_part, loc)) = head.split_once('(') else {
            continue;
        };
        let loc = loc.trim_end_matches(')');
        let (line_no, col) = loc
            .split_once(',')
            .map(|(l, c)| {
                (
                    l.trim().parse().unwrap_or(1),
                    c.trim().parse().unwrap_or(1),
                )
            })
            .unwrap_or((1, 1));
        if !matches_focus(path_part, focus) {
            continue;
        }
        let severity = if rest.contains("error TS") {
            "error"
        } else if rest.contains("warning TS") {
            "warning"
        } else {
            "error"
        };
        diags.push(diag(
            &strip_overlay_prefix(path_part),
            line_no,
            col,
            rest,
            severity,
        ));
    }
    diags
}

fn check_kotlin(ws: &Path, rel_path: &str, content: &str) -> Result<Vec<Diagnostic>> {
    write_overlay(ws, rel_path, content)?;
    let rel = overlay_rel(rel_path);
    let out_dir = ws.join(".reaper/diagnostics/kotlin-out");
    std::fs::create_dir_all(&out_dir)?;
    let out = run_tool_command(
        ws,
        "kotlin",
        &[
            "-nowarn",
            "-d",
            out_dir.to_str().unwrap_or(".reaper/diagnostics/kotlin-out"),
            &rel,
        ],
    );
    let Ok(out) = out else {
        return Ok(Vec::new());
    };
    if out.exit_code == 0 {
        return Ok(Vec::new());
    }
    Ok(parse_kotlin_output(&format!("{}\n{}", out.stderr, out.stdout), rel_path))
}

fn parse_kotlin_output(text: &str, focus: &str) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        // file.kt: (12, 5): error: message
        if let Some(idx) = line.find(": (") {
            let path = &line[..idx];
            let rest = &line[idx + 3..];
            if let Some((loc, message)) = rest.split_once("): ") {
                let (line_no, col) = loc
                    .split_once(',')
                    .map(|(l, c)| {
                        (
                            l.trim().parse().unwrap_or(1),
                            c.trim().parse().unwrap_or(1),
                        )
                    })
                    .unwrap_or((1, 1));
                if matches_focus(path, focus) {
                    diags.push(diag(
                        &strip_overlay_prefix(path),
                        line_no,
                        col,
                        message,
                        if message.contains("warning") {
                            "warning"
                        } else {
                            "error"
                        },
                    ));
                }
            }
        } else if let Some(d) = parse_location_colon_error(line, focus) {
            diags.push(d);
        }
    }
    diags
}

fn parse_location_colon_error(line: &str, focus: &str) -> Option<Diagnostic> {
    for (needle, severity) in [(": error: ", "error"), (": warning: ", "warning")] {
        if let Some(idx) = line.find(needle) {
            let head = &line[..idx];
            let message = &line[idx + needle.len()..];
            let mut parts = head.rsplit(':');
            let col: u32 = parts.next()?.trim().parse().ok()?;
            let line_no: u32 = parts.next()?.trim().parse().ok()?;
            let path = parts.collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join(":");
            if matches_focus(&path, focus) {
                return Some(diag(
                    &strip_overlay_prefix(&path),
                    line_no,
                    col,
                    message,
                    severity,
                ));
            }
        }
    }
    None
}

fn check_yaml(ws: &Path, rel_path: &str, content: &str) -> Result<Vec<Diagnostic>> {
    write_overlay(ws, rel_path, content)?;
    let mut diags = Vec::new();
    diags.extend(check_yaml_syntax(rel_path, content));
    diags.extend(check_yaml_yamllint(ws, rel_path));
    if is_github_workflow(rel_path) {
        diags.extend(check_yaml_actionlint(ws, rel_path));
    }
    if looks_like_k8s_manifest(content) {
        diags.extend(check_yaml_kubeconform(ws, rel_path));
    }
    Ok(diags)
}

fn check_yaml_syntax(rel_path: &str, content: &str) -> Vec<Diagnostic> {
    if content.trim().is_empty() {
        return Vec::new();
    }
    match serde_yaml::from_str::<serde_yaml::Value>(content) {
        Ok(_) => Vec::new(),
        Err(e) => {
            let line = e
                .location()
                .map(|loc| loc.line().max(1) as u32)
                .unwrap_or(1);
            let column = e
                .location()
                .map(|loc| loc.column().max(1) as u32)
                .unwrap_or(1);
            vec![diag(rel_path, line, column, e.to_string(), "error")]
        }
    }
}

fn is_github_workflow(rel_path: &str) -> bool {
    let p = rel_path.replace('\\', "/").to_lowercase();
    p.contains(".github/workflows/") && (p.ends_with(".yml") || p.ends_with(".yaml"))
}

fn looks_like_k8s_manifest(content: &str) -> bool {
    let mut has_api = false;
    let mut has_kind = false;
    for line in content.lines() {
        let t = line.trim();
        if t.starts_with("apiVersion:") {
            has_api = true;
        }
        if t.starts_with("kind:") {
            has_kind = true;
        }
        if has_api && has_kind {
            return true;
        }
    }
    false
}

fn check_yaml_yamllint(ws: &Path, rel_path: &str) -> Vec<Diagnostic> {
    let rel = overlay_rel(rel_path);
    let out = run_tool_command(ws, "yamllint", &["-f", "parsable", &rel])
        .or_else(|_| run_shell_argv(ws, "yamllint", &["-f", "parsable", &rel]));
    let Ok(out) = out else {
        return Vec::new();
    };
    if out.exit_code == 0 {
        return Vec::new();
    }
    parse_yamllint_parsable(&format!("{}\n{}", out.stderr, out.stdout), rel_path)
}

fn parse_yamllint_parsable(text: &str, focus: &str) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // path:line:col: [error|warning] message (rule)
        let Some(colon_idx) = line.rfind(": [") else {
            continue;
        };
        let head = &line[..colon_idx];
        let tail = &line[colon_idx + 2..];
        let severity = if tail.starts_with("[error]") {
            "error"
        } else if tail.starts_with("[warning]") {
            "warning"
        } else {
            "error"
        };
        let message = tail
            .strip_prefix("[error]")
            .or_else(|| tail.strip_prefix("[warning]"))
            .map(|s| s.trim())
            .unwrap_or(tail);
        // path:line:col — split numeric suffix from the right so paths stay intact.
        let Some(col_sep) = head.rfind(':') else {
            continue;
        };
        let col = head[col_sep + 1..].trim().parse().unwrap_or(1);
        let rest = &head[..col_sep];
        let Some(line_sep) = rest.rfind(':') else {
            continue;
        };
        let line_no = rest[line_sep + 1..].trim().parse().unwrap_or(1);
        let path = &rest[..line_sep];
        if matches_focus(path, focus) {
            diags.push(diag(
                &strip_overlay_prefix(path),
                line_no,
                col,
                message,
                severity,
            ));
        }
    }
    diags
}

fn check_yaml_actionlint(ws: &Path, rel_path: &str) -> Vec<Diagnostic> {
    let rel = overlay_rel(rel_path);
    let format = "{{range $err := .}}{{json $err}}{{end}}";
    let out = run_shell_argv(ws, "actionlint", &["-format", format, &rel]);
    let Ok(out) = out else {
        return Vec::new();
    };
    if out.exit_code == 0 {
        return Vec::new();
    }
    parse_actionlint_jsonlines(&format!("{}\n{}", out.stderr, out.stdout), rel_path)
}

#[derive(Debug, Deserialize)]
struct ActionlintError {
    message: Option<String>,
    line: Option<u64>,
    column: Option<u64>,
}

fn parse_actionlint_jsonlines(text: &str, focus: &str) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || !line.starts_with('{') {
            continue;
        }
        let Ok(err) = serde_json::from_str::<ActionlintError>(line) else {
            continue;
        };
        let message = err.message.unwrap_or_else(|| "actionlint error".to_string());
        let line_no = err.line.unwrap_or(1) as u32;
        let col = err.column.unwrap_or(1) as u32;
        diags.push(diag(focus, line_no, col, format!("actionlint: {message}"), "error"));
    }
    diags
}

fn check_yaml_kubeconform(ws: &Path, rel_path: &str) -> Vec<Diagnostic> {
    let abs = ws.join(overlay_rel(rel_path));
    let abs = abs.to_string_lossy().to_string();
    let out = run_shell_argv(
        ws,
        "kubeconform",
        &[
            "-output",
            "json",
            "-ignore-missing-schemas",
            "-summary",
            &abs,
        ],
    );
    let Ok(out) = out else {
        return Vec::new();
    };
    if out.exit_code == 0 {
        return Vec::new();
    }
    parse_kubeconform_json(&format!("{}\n{}", out.stderr, out.stdout), rel_path)
}

#[derive(Debug, Deserialize)]
struct KubeconformOutput {
    resources: Option<Vec<KubeconformResource>>,
}

#[derive(Debug, Deserialize)]
struct KubeconformResource {
    status: String,
    msg: Option<String>,
    kind: Option<String>,
}

fn parse_kubeconform_json(text: &str, focus: &str) -> Vec<Diagnostic> {
    let json_start = text.find('{').unwrap_or(0);
    let json_text = &text[json_start..];
    let Ok(parsed) = serde_json::from_str::<KubeconformOutput>(json_text) else {
        return Vec::new();
    };
    let mut diags = Vec::new();
    for res in parsed.resources.unwrap_or_default() {
        if res.status == "VALID" || res.status == "Skipped" {
            continue;
        }
        let kind = res.kind.unwrap_or_else(|| "resource".to_string());
        let msg = res
            .msg
            .unwrap_or_else(|| "schema validation failed".to_string());
        diags.push(diag(
            focus,
            1,
            1,
            format!("kubeconform ({kind}): {msg}"),
            "error",
        ));
    }
    diags
}

fn check_xml(ws: &Path, rel_path: &str, content: &str) -> Result<Vec<Diagnostic>> {
    write_overlay(ws, rel_path, content)?;
    let rel = overlay_rel(rel_path);
    let out = run_command(ws, "xmllint", &["--noout", &rel]);
    let Ok(out) = out else {
        return Ok(Vec::new());
    };
    if out.exit_code == 0 {
        return Ok(Vec::new());
    }
    let text = format!("{}\n{}", out.stderr, out.stdout);
    let line = text
        .lines()
        .find_map(|l| {
            l.split(':')
                .nth(1)
                .and_then(|s| s.trim().parse().ok())
        })
        .unwrap_or(1);
    let message = text.lines().next().unwrap_or("Invalid XML/HTML").to_string();
    Ok(vec![diag(rel_path, line, 1, message, "error")])
}

/// Vue / Svelte: syntax-check embedded `<script>` blocks and markup remainder.
fn check_component_file(ws: &Path, rel_path: &str, content: &str) -> Result<Vec<Diagnostic>> {
    let mut diags = Vec::new();
    for (line_offset, script_body) in extract_script_blocks(content) {
        let script_rel = overlay_script_path(rel_path);
        write_overlay(ws, &script_rel, &script_body)?;
        diags.extend(check_overlay_script_with_node(
            ws,
            &script_rel,
            rel_path,
            line_offset,
        )?);
    }

    let markup = strip_component_blocks(content);
    if markup.trim().len() > 3 {
        write_overlay(ws, rel_path, &markup)?;
        diags.extend(check_xml(ws, rel_path, &markup)?);
    }

    Ok(diags)
}

fn overlay_script_path(rel_path: &str) -> String {
    if let Some((base, _)) = rel_path.rsplit_once('.') {
        format!("{base}.reaper-script.js")
    } else {
        format!("{rel_path}.reaper-script.js")
    }
}

fn extract_script_blocks(content: &str) -> Vec<(u32, String)> {
    let lower = content.to_lowercase();
    let mut blocks = Vec::new();
    let mut search = 0usize;
    while let Some(rel) = lower[search..].find("<script") {
        let open = search + rel;
        let tag_end = content[open..]
            .find('>')
            .map(|i| open + i + 1)
            .unwrap_or(content.len());
        let close_lower = lower[tag_end..].find("</script>");
        if let Some(close_rel) = close_lower {
            let close = tag_end + close_rel;
            let body = content[tag_end..close].trim();
            if !body.is_empty() {
                let body_start_line = content[..tag_end].lines().count() as u32;
                blocks.push((body_start_line.max(1), body.to_string()));
            }
            search = close + "</script>".len();
        } else {
            break;
        }
    }
    blocks
}

fn strip_component_blocks(content: &str) -> String {
    let lower = content.to_lowercase();
    let mut out = String::new();
    let mut i = 0usize;
    while i < content.len() {
        let script = lower[i..].find("<script");
        let style = lower[i..].find("<style");
        let next = match (script, style) {
            (Some(s), Some(st)) => Some(i + s.min(st)),
            (Some(s), None) => Some(i + s),
            (None, Some(st)) => Some(i + st),
            (None, None) => None,
        };
        if let Some(start) = next {
            out.push_str(&content[i..start]);
            let is_script = lower[start..].starts_with("<script");
            let close_tag = if is_script { "</script>" } else { "</style>" };
            if let Some(close_rel) = lower[start..].find(close_tag) {
                i = start + close_rel + close_tag.len();
            } else {
                break;
            }
        } else {
            out.push_str(&content[i..]);
            break;
        }
    }
    out
}

fn check_overlay_script_with_node(
    ws: &Path,
    script_rel: &str,
    focus_path: &str,
    line_offset: u32,
) -> Result<Vec<Diagnostic>> {
    let rel = overlay_rel(script_rel);
    let Ok(out) = run_tool_command(ws, "node", &["--check", &rel]) else {
        return Ok(Vec::new());
    };
    if out.exit_code == 0 {
        return Ok(Vec::new());
    }
    let mut diags = parse_node_output(
        &format!("{}\n{}", out.stderr, out.stdout),
        script_rel,
    );
    let shift = line_offset.saturating_sub(1);
    for d in &mut diags {
        d.path = focus_path.to_string();
        d.line = d.line.saturating_add(shift);
        if let Some(end_line) = d.end_line {
            d.end_line = Some(end_line.saturating_add(shift));
        }
    }
    Ok(diags)
}

fn check_r_lang(ws: &Path, rel_path: &str, content: &str) -> Result<Vec<Diagnostic>> {
    write_overlay(ws, rel_path, content)?;
    let abs = ws.join(overlay_rel(rel_path));
    let path_arg = abs.to_string_lossy().replace('\\', "/").replace('\'', "\\'");
    let expr = format!(
        "tryCatch(parse(file='{path_arg}'), error=function(e) quit(status=1, conditionMessage(e)))"
    );
    for prog in ["Rscript", "R"] {
        let out = run_command(ws, prog, &["--vanilla", "-e", &expr]);
        if let Ok(out) = out {
            if out.exit_code == 0 {
                return Ok(Vec::new());
            }
            return Ok(parse_r_output(
                &format!("{}\n{}", out.stderr, out.stdout),
                rel_path,
            ));
        }
    }
    Ok(Vec::new())
}

fn parse_r_output(text: &str, focus: &str) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // bad.R:3:1: unexpected symbol
        if let Some((head, message)) = line.rsplit_once(": ") {
            let parts: Vec<&str> = head.rsplit(':').collect();
            if parts.len() >= 2 {
                if let Ok(line_no) = parts[parts.len() - 2].trim().parse::<u32>() {
                    let col = parts[parts.len() - 1].trim().parse::<u32>().unwrap_or(1);
                    diags.push(diag(focus, line_no, col, message, "error"));
                    continue;
                }
            }
        }
        diags.push(diag(focus, 1, 1, line, "error"));
    }
    if diags.len() > 1 {
        diags.truncate(1);
    }
    diags
}

fn check_toml(ws: &Path, rel_path: &str, content: &str) -> Result<Vec<Diagnostic>> {
    write_overlay(ws, rel_path, content)?;
    let rel = overlay_rel(rel_path);
    let out = run_command(ws, "taplo", &["check", &rel]);
    let Ok(out) = out else {
        return Ok(Vec::new());
    };
    if out.exit_code == 0 {
        return Ok(Vec::new());
    }
    Ok(parse_taplo_output(&format!("{}\n{}", out.stderr, out.stdout), rel_path))
}

fn parse_taplo_output(text: &str, focus: &str) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for line in text.lines() {
        if let Some(d) = parse_location_colon_error(line.trim(), focus) {
            diags.push(d);
        }
    }
    diags
}

fn check_stylelint(ws: &Path, rel_path: &str, content: &str) -> Result<Vec<Diagnostic>> {
    write_overlay(ws, rel_path, content)?;
    let rel = overlay_rel(rel_path);
    let out = run_command(
        ws,
        "npx",
        &["--yes", "stylelint", rel.as_str(), "--formatter", "compact"],
    );
    let Ok(out) = out else {
        return Ok(Vec::new());
    };
    if out.exit_code == 0 {
        return Ok(Vec::new());
    }
    Ok(parse_stylelint_output(&format!("{}\n{}", out.stderr, out.stdout), rel_path))
}

fn parse_stylelint_output(text: &str, focus: &str) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        // path: line X, col Y, error/warning - message
        if !line.contains(": line ") {
            continue;
        }
        let Some((path, rest)) = line.split_once(": line ") else {
            continue;
        };
        if !matches_focus(path, focus) {
            continue;
        }
        let Some(first) = rest.split(',').next() else {
            continue;
        };
        let Some(line_no) = first.trim().parse().ok() else {
            continue;
        };
        let col = rest
            .split("col ")
            .nth(1)
            .and_then(|s| s.split(',').next())
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(1);
        let severity = if rest.contains(" warning ") {
            "warning"
        } else {
            "error"
        };
        let message = rest
            .split(" - ")
            .nth(1)
            .unwrap_or("Stylelint error")
            .to_string();
        diags.push(diag(
            &strip_overlay_prefix(path),
            line_no,
            col,
            message,
            severity,
        ));
    }
    diags
}

fn check_ruby(ws: &Path, rel_path: &str, content: &str) -> Result<Vec<Diagnostic>> {
    write_overlay(ws, rel_path, content)?;
    let rel = overlay_rel(rel_path);
    let Ok(out) = run_tool_command(ws, "ruby", &["-c", &rel]) else {
        return Ok(Vec::new());
    };
    if out.exit_code == 0 {
        return Ok(Vec::new());
    }
    Ok(parse_ruby_output(&format!("{}\n{}", out.stderr, out.stdout), rel_path))
}

fn parse_ruby_output(text: &str, focus: &str) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        // file.rb:12: syntax error, unexpected ...
        if let Some((head, message)) = line.split_once(": syntax error") {
            let path = head.rsplit_once(':').map(|(p, _)| p).unwrap_or(head);
            let line_no: u32 = head
                .rsplit_once(':')
                .and_then(|(_, l)| l.parse().ok())
                .unwrap_or(1);
            if matches_focus(path, focus) {
                diags.push(diag(
                    &strip_overlay_prefix(path),
                    line_no,
                    1,
                    format!("syntax error{message}"),
                    "error",
                ));
            }
        }
    }
    diags
}

fn check_php(ws: &Path, rel_path: &str, content: &str) -> Result<Vec<Diagnostic>> {
    write_overlay(ws, rel_path, content)?;
    let rel = overlay_rel(rel_path);
    let Ok(out) = run_tool_command(ws, "php", &["-l", &rel]) else {
        return Ok(Vec::new());
    };
    if out.exit_code == 0 {
        return Ok(Vec::new());
    }
    let text = format!("{}\n{}", out.stderr, out.stdout);
    let line = text
        .lines()
        .find_map(|l| {
            l.split("on line ")
                .nth(1)
                .and_then(|s| s.split_whitespace().next())
                .and_then(|s| s.parse().ok())
        })
        .unwrap_or(1);
    let message = text.lines().next().unwrap_or("PHP syntax error").to_string();
    Ok(vec![diag(rel_path, line, 1, message, "error")])
}

fn check_c(ws: &Path, rel_path: &str, content: &str) -> Result<Vec<Diagnostic>> {
    write_overlay(ws, rel_path, content)?;
    let rel = overlay_rel(rel_path);
    let out = run_tool_command(ws, "clang", &["-fsyntax-only", "-x", "c", &rel]);
    let out = match out {
        Ok(o) => o,
        Err(_) => run_tool_command(ws, "gcc", &["-fsyntax-only", "-x", "c", &rel])?,
    };
    if out.exit_code == 0 {
        return Ok(Vec::new());
    }
    Ok(parse_clang_output(&format!("{}\n{}", out.stderr, out.stdout), rel_path))
}

fn parse_clang_output(text: &str, focus: &str) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for line in text.lines() {
        if let Some(d) = parse_location_colon_error(line.trim(), focus) {
            diags.push(d);
        }
    }
    diags
}

fn check_swift(ws: &Path, rel_path: &str, content: &str) -> Result<Vec<Diagnostic>> {
    write_overlay(ws, rel_path, content)?;
    let rel = overlay_rel(rel_path);
    let out = run_tool_command(ws, "swiftc", &["-typecheck", &rel]);
    let Ok(out) = out else {
        return Ok(Vec::new());
    };
    if out.exit_code == 0 {
        return Ok(Vec::new());
    }
    Ok(parse_swift_output(&format!("{}\n{}", out.stderr, out.stdout), rel_path))
}

fn parse_swift_output(text: &str, focus: &str) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("error: ") {
            diags.push(diag(focus, 1, 1, rest, "error"));
        } else if let Some(rest) = line.strip_prefix("warning: ") {
            diags.push(diag(focus, 1, 1, rest, "warning"));
        } else if let Some(d) = parse_location_colon_error(line, focus) {
            diags.push(d);
        }
    }
    diags
}

fn check_groovy(ws: &Path, rel_path: &str, content: &str) -> Result<Vec<Diagnostic>> {
    write_overlay(ws, rel_path, content)?;
    let rel = overlay_rel(rel_path);
    if let Ok(out) = run_tool_command(ws, "groovy", &["-d", ".reaper/diagnostics/groovy-out", &rel]) {
        if out.exit_code == 0 {
            return Ok(Vec::new());
        }
        return Ok(parse_javac_like(&format!("{}\n{}", out.stderr, out.stdout), rel_path));
    }
    if let Some(gradle_root) = super::gradle::find_gradle_root(ws, rel_path)? {
        let cmd = super::gradle::resolve_gradle_command(&gradle_root)?;
        let out = super::gradle::run_gradle_with_command(&cmd, &["compileGroovy", "-q"])?;
        if out.exit_code == 0 {
            return Ok(Vec::new());
        }
        return Ok(parse_javac_like(
            &format!("{}\n{}", out.stderr, out.stdout),
            rel_path,
        ));
    }
    Ok(Vec::new())
}

fn parse_javac_like(text: &str, focus: &str) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for line in text.lines() {
        if let Some(d) = parse_location_colon_error(line.trim(), focus) {
            diags.push(d);
        }
    }
    diags
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_syntax_error_has_line() {
        let diags = check_json_syntax("x.json", "{ invalid");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].severity, "error");
        assert!(diags[0].line >= 1);
    }

    #[test]
    fn valid_json_has_no_diagnostics() {
        assert!(check_json_syntax("x.json", r#"{"ok": true}"#).is_empty());
    }

    #[test]
    fn jsonlint_parse_error_parsed() {
        let text = "Error: Parse error on line 2:\n{ \"a\": }\n-------^\nExpecting 'STRING'";
        let diags = parse_jsonlint_output(text, "x.json");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 2);
        assert_eq!(diags[0].severity, "error");
    }

    #[test]
    fn json_schema_detected() {
        let schema = r#"{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"name":{"type":"string"}}}"#;
        assert!(looks_like_json_schema("models/user.schema.json", schema));
        assert!(!looks_like_json_schema("package.json", r#"{"name":"demo"}"#));
    }

    #[test]
    fn tsc_output_parsed() {
        let text = ".reaper/diagnostics/overlay/src/a.ts(12,5): error TS2304: Cannot find name 'x'.";
        let diags = parse_tsc_output(text, "src/a.ts");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 12);
        assert_eq!(diags[0].column, 5);
    }

    #[test]
    fn rustc_output_parsed() {
        let text = "error[E0412]: cannot find type `Foo`\n  --> .reaper/diagnostics/overlay/lib.rs:3:5";
        let diags = parse_rustc_output(text, "lib.rs");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 3);
    }

    #[test]
    fn yaml_syntax_error_has_line() {
        let diags = check_yaml_syntax("openapi.yaml", "foo: [bar\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].severity, "error");
        assert!(diags[0].line >= 1);
    }

    #[test]
    fn yamllint_parsable_output_parsed() {
        let text = ".reaper/diagnostics/overlay/deploy.yml:12:3: [error] trailing spaces (trailing-spaces)";
        let diags = parse_yamllint_parsable(text, "deploy.yml");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 12);
        assert_eq!(diags[0].column, 3);
        assert_eq!(diags[0].severity, "error");
    }

    #[test]
    fn actionlint_jsonlines_parsed() {
        let text = r#"{"message":"unknown key","line":9,"column":4,"filepath":".github/workflows/ci.yml"}"#;
        let diags = parse_actionlint_jsonlines(text, ".github/workflows/ci.yml");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 9);
        assert_eq!(diags[0].column, 4);
        assert!(diags[0].message.contains("actionlint"));
    }

    #[test]
    fn kubeconform_json_parsed() {
        let text = r#"{
  "resources": [
    {
      "filename": "pod.yaml",
      "kind": "Pod",
      "status": "INVALID",
      "msg": "Additional property foo is not allowed"
    }
  ]
}"#;
        let diags = parse_kubeconform_json(text, "pod.yaml");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("kubeconform"));
        assert!(diags[0].message.contains("Pod"));
    }

    #[test]
    fn github_workflow_path_detected() {
        assert!(is_github_workflow(".github/workflows/ci.yml"));
        assert!(!is_github_workflow("k8s/deployment.yaml"));
    }

    #[test]
    fn k8s_manifest_content_detected() {
        let yaml = "apiVersion: v1\nkind: Pod\nmetadata:\n  name: x";
        assert!(looks_like_k8s_manifest(yaml));
        assert!(!looks_like_k8s_manifest("on: push\njobs:\n  build:\n"));
    }

    #[test]
    fn bad_ini_returns_error() {
        let diags = check_ini("app.properties", "not_a_property\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].severity, "error");
    }

    #[test]
    fn vue_and_r_checkers_wired() {
        let ws = std::env::temp_dir().join("reaper-diag-gap");
        let _ = std::fs::remove_dir_all(&ws);
        std::fs::create_dir_all(&ws).unwrap();
        let blocks = extract_script_blocks("<script>\nconst x = \n</script><template></template>");
        assert_eq!(blocks.len(), 1);
        assert!(blocks[0].1.contains("const x"));

        let r_diags = parse_r_output("bad.R:2:1: unexpected end of input", "bad.R");
        assert_eq!(r_diags.len(), 1);
        assert_eq!(r_diags[0].line, 2);

        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn integration_java_without_disk_file() {
        let ws = std::env::temp_dir().join("reaper-diag-java-new");
        let _ = std::fs::remove_dir_all(&ws);
        std::fs::create_dir_all(&ws).unwrap();
        let content = "public class RightName {\n}\n";
        let diags = check_file(
            &ws,
            "WrongFile.java",
            content,
            &[],
            crate::workspace::JavaDiagScope::Full,
        )
        .unwrap();
        assert!(
            diags.iter().any(|d| d.message.contains("should be declared in a file named")),
            "unsaved Java files should still get file/class diagnostics on save: {:?}",
            diags
        );
        let typing = check_file(
            &ws,
            "WrongFile.java",
            content,
            &[],
            crate::workspace::JavaDiagScope::Typing,
        )
        .unwrap();
        assert!(
            typing.iter().any(|d| d.message.contains("should be declared in a file named")),
            "typing scope runs single-file javac for Java: {:?}",
            typing
        );
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn integration_bad_json_and_yaml_syntax() {
        let ws = std::env::temp_dir().join("reaper-diag-smoke");
        let _ = std::fs::remove_dir_all(&ws);
        std::fs::create_dir_all(&ws).unwrap();
        let json_diags = check_file(
            &ws,
            "bad.json",
            "{ invalid",
            &[],
            crate::workspace::JavaDiagScope::Typing,
        )
        .unwrap();
        assert!(!json_diags.is_empty(), "JSON syntax errors should surface");
        assert_eq!(json_diags[0].severity, "error");

        let yaml_diags = check_file(
            &ws,
            "bad.yaml",
            "foo: [bar\n",
            &[],
            crate::workspace::JavaDiagScope::Typing,
        )
        .unwrap();
        assert!(!yaml_diags.is_empty(), "YAML syntax errors should surface");
        assert_eq!(yaml_diags[0].severity, "error");

        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn integration_java_file_class_mismatch() {
        let ws = std::env::temp_dir().join("reaper-diag-java");
        let _ = std::fs::remove_dir_all(&ws);
        std::fs::create_dir_all(&ws).unwrap();
        let content = "public class RightName {\n}\n";
        std::fs::write(ws.join("WrongFile.java"), content).unwrap();
        let diags = check_file(
            &ws,
            "WrongFile.java",
            content,
            &[],
            crate::workspace::JavaDiagScope::Full,
        )
        .unwrap();
        assert!(
            diags.iter().any(|d| d.message.contains("should be declared in a file named")),
            "Java file/class mismatch should show in editor diagnostics on save: {:?}",
            diags
        );
        let _ = std::fs::remove_dir_all(&ws);
    }
}
