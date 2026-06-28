use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::Serialize;

use super::exec::{run_command, run_tool_command};
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

pub fn check_file(ws: &Path, rel_path: &str, content: &str) -> Result<Vec<Diagnostic>> {
    if rel_path.starts_with(".reaper/") {
        return Ok(Vec::new());
    }
    let _ = safe_join(ws, rel_path)?;

    let lower = rel_path.to_lowercase();
    let ext = file_extension(&lower);

    if lower.ends_with(".java") {
        return Ok(java_diagnostics::check_java(ws, rel_path, content)?);
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
        return Ok(check_json(rel_path, content));
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
        return Ok(check_json(rel_path, &strip_jsonc_comments(content)));
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
    write_overlay(ws, rel_path, content)?;
    let rel = overlay_rel(rel_path);
    let out = run_command(ws, "sqlfluff", &["lint", &rel, "--format", "human"]);
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

fn check_json(rel_path: &str, content: &str) -> Vec<Diagnostic> {
    match serde_json::from_str::<serde_json::Value>(content) {
        Ok(_) => Vec::new(),
        Err(e) => {
            let line = e.line().max(1) as u32;
            let column = e.column().max(1) as u32;
            vec![diag(rel_path, line, column, e.to_string(), "error")]
        }
    }
}

fn check_rust(ws: &Path, rel_path: &str, content: &str) -> Result<Vec<Diagnostic>> {
    write_overlay(ws, rel_path, content)?;
    let rel = overlay_rel(rel_path);
    let out = run_tool_command(
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
    )?;
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
    let out = run_tool_command(ws, "go", &["build", "-o", null_out, &rel])?;
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
    let out = run_tool_command(ws, "node", &["--check", &rel])?;
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
    let abs = ws.join(overlay_rel(rel_path));
    let abs = abs.to_string_lossy();
    let script = format!(
        "import sys\n\
         try:\n\
             import yaml\n\
         except ImportError:\n\
             sys.exit(0)\n\
         try:\n\
             yaml.safe_load(open(sys.argv[1]))\n\
         except yaml.YAMLError as e:\n\
             print(e)\n\
             sys.exit(1)\n"
    );
    let Ok(out) = run_tool_command(ws, "python", &["-c", &script, &abs]) else {
        return Ok(Vec::new());
    };
    if out.exit_code == 0 {
        return Ok(Vec::new());
    }
    let msg = out.stdout.trim();
    let line = msg
        .lines()
        .find_map(|l| {
            l.split("line ")
                .nth(1)
                .and_then(|s| s.split(',').next())
                .and_then(|s| s.trim().parse().ok())
        })
        .unwrap_or(1);
    Ok(vec![diag(rel_path, line, 1, msg, "error")])
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
    let out = run_tool_command(ws, "ruby", &["-c", &rel])?;
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
    let out = run_tool_command(ws, "php", &["-l", &rel])?;
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
        let diags = check_json("x.json", "{ invalid");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].severity, "error");
        assert!(diags[0].line >= 1);
    }

    #[test]
    fn valid_json_has_no_diagnostics() {
        assert!(check_json("x.json", r#"{"ok": true}"#).is_empty());
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
}
