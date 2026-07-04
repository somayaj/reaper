use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

use super::languages;
use super::lsp::{self, ReferenceLocation, RenameRange, FileTextEdits, SignatureHelp};
use super::symbols::{self, HoverInfo, SymbolLocation};

const LSP_TIMEOUT: Duration = Duration::from_secs(20);
const SESSION_IDLE: Duration = Duration::from_secs(120);

static SESSIONS: LazyLock<Mutex<HashMap<PathBuf, ClangdSession>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static NEXT_ID: AtomicU64 = AtomicU64::new(10);

struct ClangdSession {
    child: Child,
    last_used: Instant,
}

pub fn find_hover(
    ws: &Path,
    rel_path: &str,
    line: u32,
    column: u32,
    content: &str,
) -> Result<Option<HoverInfo>> {
    if !languages::is_c_like_path(rel_path) {
        return Ok(None);
    }

    let ws = ws
        .canonicalize()
        .with_context(|| format!("resolve workspace {}", ws.display()))?;
    let abs_file = ws.join(rel_path);
    if !abs_file.is_file() {
        return Ok(None);
    }

    let uri = file_uri(&abs_file)?;
    let deadline = Instant::now() + LSP_TIMEOUT;

    let hover = match query_hover(&ws, rel_path, &uri, line, column, content, deadline) {
        Ok(r) => r,
        Err(e) => {
            tracing::debug!("clangd hover failed: {e:#}");
            return Ok(None);
        }
    };

    if hover.is_null() {
        return Ok(None);
    }

    let Some(text) = hover_text(&hover) else {
        return Ok(None);
    };

    let mut info = parse_clangd_hover(&text, rel_path);
    if info.name.is_empty() {
        if let Some(word) = symbols::word_at(content, line, column) {
            info.name = word;
        }
    }

    if let Ok(def) = query_definition(&ws, rel_path, &uri, line, column, content, deadline) {
        if let Some(loc) = parse_definition_location(&ws, rel_path, content, line, column, &def) {
            info.path = Some(loc.path);
            info.line = Some(loc.line);
            if !loc.name.is_empty() {
                info.name = loc.name;
            }
            if info.kind == "symbol" && !loc.kind.is_empty() {
                info.kind = loc.kind;
            }
        }
    }

    Ok(Some(info))
}

pub fn find_definition(
    ws: &Path,
    rel_path: &str,
    line: u32,
    column: u32,
    content: &str,
) -> Result<Option<SymbolLocation>> {
    if !languages::is_c_like_path(rel_path) {
        return Ok(None);
    }

    let ws = ws
        .canonicalize()
        .with_context(|| format!("resolve workspace {}", ws.display()))?;
    let abs_file = ws.join(rel_path);
    if !abs_file.is_file() {
        return Ok(None);
    }

    let uri = file_uri(&abs_file)?;
    let deadline = Instant::now() + LSP_TIMEOUT;

    let result = match query_definition(&ws, rel_path, &uri, line, column, content, deadline) {
        Ok(r) => r,
        Err(e) => {
            tracing::debug!("clangd definition failed: {e:#}");
            return Ok(None);
        }
    };

    Ok(parse_definition_location(
        &ws, rel_path, content, line, column, &result,
    ))
}

pub fn find_references(
    ws: &Path,
    rel_path: &str,
    line: u32,
    column: u32,
    content: &str,
) -> Result<Vec<ReferenceLocation>> {
    if !languages::is_c_like_path(rel_path) {
        return Ok(Vec::new());
    }
    let ws = ws
        .canonicalize()
        .with_context(|| format!("resolve workspace {}", ws.display()))?;
    let abs_file = ws.join(rel_path);
    if !abs_file.is_file() {
        return Ok(Vec::new());
    }
    let uri = file_uri(&abs_file)?;
    let deadline = Instant::now() + LSP_TIMEOUT;
    let result = match query_references(&ws, rel_path, &uri, line, column, content, deadline) {
        Ok(r) => r,
        Err(e) => {
            tracing::debug!("clangd references failed: {e:#}");
            return Ok(Vec::new());
        }
    };
    Ok(lsp::parse_reference_locations(&ws, &result))
}

pub fn prepare_rename(
    ws: &Path,
    rel_path: &str,
    line: u32,
    column: u32,
    content: &str,
) -> Result<Option<RenameRange>> {
    if !languages::is_c_like_path(rel_path) {
        return Ok(None);
    }
    let ws = ws
        .canonicalize()
        .with_context(|| format!("resolve workspace {}", ws.display()))?;
    let abs_file = ws.join(rel_path);
    if !abs_file.is_file() {
        return Ok(None);
    }
    let uri = file_uri(&abs_file)?;
    let deadline = Instant::now() + LSP_TIMEOUT;
    let result = match query_prepare_rename(&ws, rel_path, &uri, line, column, content, deadline) {
        Ok(r) => r,
        Err(e) => {
            tracing::debug!("clangd prepareRename failed: {e:#}");
            return Ok(None);
        }
    };
    Ok(lsp::parse_rename_range(&result))
}

pub fn rename_symbol(
    ws: &Path,
    rel_path: &str,
    line: u32,
    column: u32,
    content: &str,
    new_name: &str,
) -> Result<Vec<FileTextEdits>> {
    if !languages::is_c_like_path(rel_path) || new_name.trim().is_empty() {
        return Ok(Vec::new());
    }
    let ws = ws
        .canonicalize()
        .with_context(|| format!("resolve workspace {}", ws.display()))?;
    let abs_file = ws.join(rel_path);
    if !abs_file.is_file() {
        return Ok(Vec::new());
    }
    let uri = file_uri(&abs_file)?;
    let deadline = Instant::now() + LSP_TIMEOUT;
    let result = query_rename(
        &ws,
        rel_path,
        &uri,
        line,
        column,
        content,
        new_name.trim(),
        deadline,
    )?;
    lsp::parse_workspace_edit(&ws, &result)
}

pub fn signature_help(
    ws: &Path,
    rel_path: &str,
    line: u32,
    column: u32,
    content: &str,
) -> Result<Option<SignatureHelp>> {
    if !languages::is_c_like_path(rel_path) {
        return Ok(None);
    }
    let ws = ws
        .canonicalize()
        .with_context(|| format!("resolve workspace {}", ws.display()))?;
    let abs_file = ws.join(rel_path);
    if !abs_file.is_file() {
        return Ok(None);
    }
    let uri = file_uri(&abs_file)?;
    let deadline = Instant::now() + LSP_TIMEOUT;
    let result = match query_signature_help(&ws, rel_path, &uri, line, column, content, deadline)
    {
        Ok(r) => r,
        Err(e) => {
            tracing::debug!("clangd signatureHelp failed: {e:#}");
            return Ok(None);
        }
    };
    Ok(lsp::parse_signature_help(&result))
}

fn query_references(
    ws: &Path,
    rel_path: &str,
    uri: &str,
    line: u32,
    column: u32,
    content: &str,
    deadline: Instant,
) -> Result<Value> {
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    lsp_request(
        ws,
        rel_path,
        uri,
        content,
        deadline,
        id,
        json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "textDocument/references",
            "params": {
                "textDocument": { "uri": uri },
                "position": {
                    "line": line.saturating_sub(1),
                    "character": column.saturating_sub(1)
                },
                "context": { "includeDeclaration": true }
            }
        }),
    )
}

fn query_prepare_rename(
    ws: &Path,
    rel_path: &str,
    uri: &str,
    line: u32,
    column: u32,
    content: &str,
    deadline: Instant,
) -> Result<Value> {
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    lsp_request(
        ws,
        rel_path,
        uri,
        content,
        deadline,
        id,
        json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "textDocument/prepareRename",
            "params": {
                "textDocument": { "uri": uri },
                "position": {
                    "line": line.saturating_sub(1),
                    "character": column.saturating_sub(1)
                }
            }
        }),
    )
}

fn query_rename(
    ws: &Path,
    rel_path: &str,
    uri: &str,
    line: u32,
    column: u32,
    content: &str,
    new_name: &str,
    deadline: Instant,
) -> Result<Value> {
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    lsp_request(
        ws,
        rel_path,
        uri,
        content,
        deadline,
        id,
        json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "textDocument/rename",
            "params": {
                "textDocument": { "uri": uri },
                "position": {
                    "line": line.saturating_sub(1),
                    "character": column.saturating_sub(1)
                },
                "newName": new_name
            }
        }),
    )
}

fn query_signature_help(
    ws: &Path,
    rel_path: &str,
    uri: &str,
    line: u32,
    column: u32,
    content: &str,
    deadline: Instant,
) -> Result<Value> {
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let mut params = json!({
        "textDocument": { "uri": uri },
        "position": {
            "line": line.saturating_sub(1),
            "character": column.saturating_sub(1)
        }
    });
    if let Some((trigger_kind, trigger_character)) = lsp::signature_help_trigger(content, line, column)
    {
        params["context"] = json!({
            "triggerKind": trigger_kind,
            "triggerCharacter": trigger_character,
            "isRetrigger": false
        });
    }
    lsp_request(
        ws,
        rel_path,
        uri,
        content,
        deadline,
        id,
        json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "textDocument/signatureHelp",
            "params": params
        }),
    )
}

fn query_hover(
    ws: &Path,
    rel_path: &str,
    uri: &str,
    line: u32,
    column: u32,
    content: &str,
    deadline: Instant,
) -> Result<Value> {
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let result = lsp_request(
        ws,
        rel_path,
        uri,
        content,
        deadline,
        id,
        json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "textDocument/hover",
            "params": {
                "textDocument": { "uri": uri },
                "position": {
                    "line": line.saturating_sub(1),
                    "character": column.saturating_sub(1)
                }
            }
        }),
    )?;
    Ok(result)
}

fn query_definition(
    ws: &Path,
    rel_path: &str,
    uri: &str,
    line: u32,
    column: u32,
    content: &str,
    deadline: Instant,
) -> Result<Value> {
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    lsp_request(
        ws,
        rel_path,
        uri,
        content,
        deadline,
        id,
        json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "textDocument/definition",
            "params": {
                "textDocument": { "uri": uri },
                "position": {
                    "line": line.saturating_sub(1),
                    "character": column.saturating_sub(1)
                }
            }
        }),
    )
}

fn lsp_request(
    ws: &Path,
    rel_path: &str,
    uri: &str,
    content: &str,
    deadline: Instant,
    id: u64,
    request: Value,
) -> Result<Value> {
    let mut map = SESSIONS.lock().expect("clangd sessions");
    purge_stale_sessions(&mut map);

    if !session_alive(map.get_mut(ws)) {
        map.remove(ws);
        let mut child = spawn_clangd(ws, rel_path)?;
        let root_uri = file_uri(ws)?;
        initialize_session(&mut child, &root_uri, deadline)?;
        map.insert(
            ws.to_path_buf(),
            ClangdSession {
                child,
                last_used: Instant::now(),
            },
        );
    }

    let session = map.get_mut(ws).context("clangd session")?;
    session.last_used = Instant::now();

    let open_id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let stdin = session.child.stdin.as_mut().context("clangd stdin")?;
    let stdout = session.child.stdout.as_mut().context("clangd stdout")?;

    write_message(
        stdin,
        &json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": {
                "textDocument": {
                    "uri": uri,
                    "languageId": languages::c_language_id(rel_path),
                    "version": open_id as i64,
                    "text": content
                }
            }
        }),
    )?;

    write_message(stdin, &request)?;
    wait_for_id(stdout, id, deadline)
}

fn session_alive(session: Option<&mut ClangdSession>) -> bool {
    let Some(session) = session else {
        return false;
    };
    if session.last_used.elapsed() > SESSION_IDLE {
        return false;
    }
    matches!(session.child.try_wait(), Ok(None))
}

fn purge_stale_sessions(map: &mut HashMap<PathBuf, ClangdSession>) {
    map.retain(|_, session| {
        session.last_used.elapsed() <= SESSION_IDLE
            && matches!(session.child.try_wait(), Ok(None))
    });
}

fn initialize_session(child: &mut Child, root_uri: &str, deadline: Instant) -> Result<()> {
    let stdin = child.stdin.as_mut().context("clangd stdin")?;
    let stdout = child.stdout.as_mut().context("clangd stdout")?;

    write_message(
        stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "processId": std::process::id(),
                "rootUri": root_uri,
                "capabilities": {
                    "textDocument": {
                        "hover": { "contentFormat": ["markdown", "plaintext"] },
                        "definition": { "linkSupport": true },
                        "references": {},
                        "rename": { "prepareSupport": true },
                        "signatureHelp": {
                            "signatureInformation": {
                                "documentationFormat": ["markdown", "plaintext"]
                            }
                        }
                    }
                }
            }
        }),
    )?;

    wait_for_id(stdout, 1, deadline)?;

    write_message(
        stdin,
        &json!({
            "jsonrpc": "2.0",
            "method": "initialized",
            "params": {}
        }),
    )?;

    Ok(())
}

fn spawn_clangd(ws: &Path, rel_path: &str) -> Result<Child> {
    let clangd = crate::toolchain::resolve_program("clangd")
        .with_context(|| "clangd not found — install clangd or set REAPER_CLANGD in Settings → Compilers")?;

    let mut cmd = Command::new(&clangd);
    cmd.arg("--background-index");
    cmd.arg("--clang-tidy=false");
    if let Some(dir) = find_compile_commands_dir(ws, rel_path) {
        cmd.arg(format!("--compile-commands-dir={}", dir.display()));
    }
    cmd.current_dir(ws);
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());

    cmd.spawn()
        .with_context(|| format!("failed to start clangd ({})", clangd.display()))
}

fn find_compile_commands_dir(ws: &Path, rel_path: &str) -> Option<PathBuf> {
    for dir in walk_search_dirs(ws, rel_path) {
        if dir.join("compile_commands.json").is_file() {
            return Some(dir);
        }
        let build = dir.join("build");
        if build.join("compile_commands.json").is_file() {
            return Some(build);
        }
    }
    None
}

fn walk_search_dirs(ws: &Path, rel_path: &str) -> Vec<PathBuf> {
    let rel = rel_path.replace('\\', "/");
    let parent = rel.rsplit_once('/').map(|(p, _)| p).unwrap_or("");
    let mut current = if parent.is_empty() {
        ws.to_path_buf()
    } else {
        ws.join(parent)
    };
    let mut dirs = Vec::new();
    loop {
        dirs.push(current.clone());
        if current == ws {
            break;
        }
        if !current.pop() {
            dirs.push(ws.to_path_buf());
            break;
        }
    }
    dirs
}

fn write_message(w: &mut impl Write, body: &Value) -> Result<()> {
    let json = serde_json::to_string(body)?;
    write!(w, "Content-Length: {}\r\n\r\n{}", json.len(), json)?;
    w.flush()?;
    Ok(())
}

fn wait_for_id(r: &mut impl Read, id: u64, deadline: Instant) -> Result<Value> {
    loop {
        if Instant::now() >= deadline {
            bail!("clangd timed out");
        }
        let msg = read_message(r)?;
        if msg.get("id").and_then(|v| v.as_u64()) == Some(id) {
            if let Some(err) = msg.get("error") {
                bail!("clangd error: {err}");
            }
            return Ok(msg.get("result").cloned().unwrap_or(Value::Null));
        }
    }
}

fn read_message(r: &mut impl Read) -> Result<Value> {
    let mut header = String::new();
    let mut buf = [0u8; 1];
    loop {
        header.clear();
        loop {
            r.read_exact(&mut buf)?;
            header.push(buf[0] as char);
            if header.ends_with("\r\n\r\n") {
                break;
            }
            if header.len() > 8192 {
                bail!("invalid LSP header");
            }
        }

        let mut content_length = None;
        for line in header.lines() {
            if let Some(rest) = line.strip_prefix("Content-Length:") {
                content_length = rest.trim().parse().ok();
            }
        }
        let len = content_length.context("missing Content-Length")?;
        let mut body = vec![0u8; len];
        r.read_exact(&mut body)?;
        let msg: Value = serde_json::from_slice(&body)?;
        return Ok(msg);
    }
}

fn hover_text(result: &Value) -> Option<String> {
    let contents = result.get("contents")?;
    if let Some(text) = contents.as_str() {
        return Some(text.to_string());
    }
    if let Some(value) = contents.get("value").and_then(|v| v.as_str()) {
        return Some(value.to_string());
    }
    if let Some(items) = contents.as_array() {
        let joined = items
            .iter()
            .filter_map(|item| {
                item.as_str()
                    .or_else(|| item.get("value").and_then(|v| v.as_str()))
            })
            .collect::<Vec<_>>()
            .join("\n\n");
        return (!joined.is_empty()).then_some(joined);
    }
    None
}

pub(crate) fn parse_clangd_hover(text: &str, from_path: &str) -> HoverInfo {
    let trimmed = text.trim();
    if trimmed.contains("```") {
        return parse_clangd_markdown_hover(trimmed, from_path);
    }

    let lines: Vec<&str> = trimmed.lines().collect();
    let first = lines.first().copied().unwrap_or("").trim();
    let (kind, name) = parse_kind_name(first);

    let signature = lines
        .iter()
        .rev()
        .find(|line| looks_like_signature_line(line.trim(), first))
        .map(|line| line.trim().to_string());

    let mut doc_lines = Vec::new();
    for line in lines.iter().skip(1) {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if signature.as_deref() == Some(t) {
            continue;
        }
        doc_lines.push(t.to_string());
    }

    HoverInfo {
        name,
        kind,
        signature,
        documentation: (!doc_lines.is_empty()).then_some(doc_lines.join("\n")),
        path: Some(from_path.to_string()),
        line: None,
    }
}

fn parse_clangd_markdown_hover(text: &str, from_path: &str) -> HoverInfo {
    let mut signature = None;
    let mut doc_parts = Vec::new();
    let mut in_fence = false;
    let mut fence_buf = String::new();

    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            if in_fence {
                let sig = fence_buf.trim().to_string();
                if !sig.is_empty() {
                    signature = Some(sig);
                }
                fence_buf.clear();
                in_fence = false;
            } else {
                in_fence = true;
            }
            continue;
        }
        if in_fence {
            if !fence_buf.is_empty() {
                fence_buf.push('\n');
            }
            fence_buf.push_str(line);
        } else {
            let t = line.trim();
            if !t.is_empty() {
                doc_parts.push(t.to_string());
            }
        }
    }

    let (kind, name) = signature
        .as_deref()
        .map(signature_kind_name)
        .unwrap_or(("symbol".into(), String::new()));

    HoverInfo {
        name,
        kind,
        signature,
        documentation: (!doc_parts.is_empty()).then_some(doc_parts.join("\n")),
        path: Some(from_path.to_string()),
        line: None,
    }
}

fn parse_kind_name(first: &str) -> (String, String) {
    if let Some((kind, name)) = first.split_once(' ') {
        return (kind.to_string(), name.trim().to_string());
    }
    ("symbol".into(), first.to_string())
}

fn looks_like_signature_line(line: &str, first: &str) -> bool {
    if line.is_empty() || line == first {
        return false;
    }
    if line.starts_with("→") || line.starts_with('-') || line == "Parameters:" {
        return false;
    }
    line.contains('(') || line.contains('#') || line.ends_with(';') || line.contains("::")
}

fn signature_kind_name(sig: &str) -> (String, String) {
    let trimmed = sig.trim();
    if let Some(name) = trimmed
        .split('(')
        .next()
        .and_then(|head| head.split_whitespace().last())
    {
        let name = name.rsplit("::").next().unwrap_or(name).trim_start_matches('*');
        return ("symbol".into(), name.to_string());
    }
    ("symbol".into(), String::new())
}

fn parse_definition_location(
    ws: &Path,
    _from_path: &str,
    content: &str,
    line: u32,
    column: u32,
    result: &Value,
) -> Option<SymbolLocation> {
    if result.is_null() {
        return None;
    }

    let loc = if result.is_array() {
        result.get(0)?
    } else {
        result
    };

    let uri = loc
        .get("targetUri")
        .or_else(|| loc.get("uri"))
        .and_then(|v| v.as_str())?;
    let range = loc
        .get("targetSelectionRange")
        .or_else(|| loc.get("targetRange"))
        .or_else(|| loc.get("range"));
    let (def_line, def_col) = range
        .and_then(|r| r.get("start"))
        .map(|start| {
            (
                start.get("line").and_then(|v| v.as_u64()).unwrap_or(0) as u32 + 1,
                start.get("character").and_then(|v| v.as_u64()).unwrap_or(0) as u32 + 1,
            )
        })
        .unwrap_or((1, 1));

    let path = uri_to_path(ws, uri).ok()?;
    let name = symbols::word_at(content, line, column)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "symbol".into());

    Some(SymbolLocation {
        name,
        kind: "definition".into(),
        path,
        line: def_line,
        column: def_col,
    })
}

fn file_uri(path: &Path) -> Result<String> {
    url::Url::from_file_path(path)
        .map(|u| u.to_string())
        .map_err(|_| anyhow::anyhow!("invalid path for URI: {}", path.display()))
}

fn uri_to_path(ws: &Path, uri: &str) -> Result<String> {
    let url = url::Url::parse(uri).with_context(|| format!("parse uri {uri}"))?;
    let path = url
        .to_file_path()
        .map_err(|_| anyhow::anyhow!("uri is not a file path: {uri}"))?;
    let rel = path
        .strip_prefix(ws)
        .unwrap_or(&path)
        .to_string_lossy()
        .replace('\\', "/");
    Ok(rel)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_clangd_plaintext_hover() {
        let text = "function add\n\n→ int\nParameters:\n- int a\n- int b\n\nint add(int a, int b)";
        let info = parse_clangd_hover(text, "math.c");
        assert_eq!(info.name, "add");
        assert_eq!(info.kind, "function");
        assert_eq!(
            info.signature.as_deref(),
            Some("int add(int a, int b)")
        );
        assert!(info.documentation.as_ref().is_some_and(|d| d.contains("Parameters")));
    }

    #[test]
    fn parses_clangd_markdown_hover() {
        let text = "```cpp\nvoid Widget::draw() const\n```\n\nDraws the widget.";
        let info = parse_clangd_hover(text, "widget.cpp");
        assert_eq!(info.name, "draw");
        assert!(info
            .signature
            .as_ref()
            .is_some_and(|s| s.contains("Widget::draw")));
        assert_eq!(info.documentation.as_deref(), Some("Draws the widget."));
    }

    #[test]
    fn clangd_hover_live_when_available() {
        if crate::toolchain::resolve_program("clangd").is_none() {
            return;
        }
        let ws = std::env::temp_dir().join(format!("reaper-clangd-hover-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&ws);
        std::fs::create_dir_all(&ws).unwrap();
        std::fs::write(
            ws.join("demo.c"),
            "int add(int a, int b) { return a + b; }\nint main() { return add(1, 2); }\n",
        )
        .unwrap();
        let content = std::fs::read_to_string(ws.join("demo.c")).unwrap();
        let info = find_hover(&ws, "demo.c", 2, 21, &content)
            .expect("hover query")
            .expect("hover result");
        assert_eq!(info.name, "add");
        assert!(info.signature.as_ref().is_some_and(|s| s.contains("add")));
        let _ = std::fs::remove_dir_all(&ws);
    }

    const CLANGD_DEMO: &str = "int add(int a, int b) { return a + b; }\nint main() { return add(1, 2); }\n";
    const CLANGD_REL: &str = "demo.c";

    fn setup_clangd_workspace() -> Option<std::path::PathBuf> {
        if crate::toolchain::resolve_program("clangd").is_none() {
            return None;
        }
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let ws = std::env::temp_dir().join(format!(
            "reaper-clangd-lsp-{}-{}",
            std::process::id(),
            id
        ));
        let _ = std::fs::remove_dir_all(&ws);
        std::fs::create_dir_all(&ws).ok()?;
        std::fs::write(ws.join(CLANGD_REL), CLANGD_DEMO).ok()?;
        Some(ws)
    }

    #[test]
    fn clangd_definition_regression() {
        let Some(ws) = setup_clangd_workspace() else {
            return;
        };
        let def = find_definition(&ws, CLANGD_REL, 2, 21, CLANGD_DEMO)
            .expect("definition query")
            .expect("definition for add");
        assert!(def.path.contains("demo.c"));
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn clangd_find_references_regression() {
        let Some(ws) = setup_clangd_workspace() else {
            return;
        };
        let refs = find_references(&ws, CLANGD_REL, 2, 21, CLANGD_DEMO).expect("references query");
        assert!(
            refs.len() >= 2,
            "expected declaration + usage references, got {}",
            refs.len()
        );
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn clangd_prepare_rename_regression() {
        let Some(ws) = setup_clangd_workspace() else {
            return;
        };
        let prep = prepare_rename(&ws, CLANGD_REL, 2, 21, CLANGD_DEMO).expect("prepareRename query");
        if let Some(prep) = prep {
            assert!(prep.column > 0);
        }
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn clangd_rename_symbol_regression() {
        let Some(ws) = setup_clangd_workspace() else {
            return;
        };
        let edits = rename_symbol(&ws, CLANGD_REL, 2, 21, CLANGD_DEMO, "sum")
            .expect("rename query");
        assert!(!edits.is_empty(), "rename should produce workspace edits");
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn clangd_signature_help_regression() {
        let Some(ws) = setup_clangd_workspace() else {
            return;
        };
        let help = signature_help(&ws, CLANGD_REL, 2, 28, CLANGD_DEMO)
            .expect("signatureHelp query");
        if let Some(help) = help {
            assert!(
                help.signatures.iter().any(|s| s.label.contains("add")),
                "expected add() signature, got {:?}",
                help
            );
        }
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn clangd_peek_definition_regression() {
        let Some(ws) = setup_clangd_workspace() else {
            return;
        };
        let def = find_definition(&ws, CLANGD_REL, 2, 21, CLANGD_DEMO)
            .expect("peek definition query")
            .expect("definition location for peek");
        assert!(def.path.contains("demo.c"));
        let _ = std::fs::remove_dir_all(&ws);
    }
}
