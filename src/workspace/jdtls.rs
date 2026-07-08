//! Eclipse JDT Language Server (jdtls) client — auto-enabled when bundled in Reaper.app,
//! when `jdtls` is on PATH, or via `REAPER_USE_JDTLS=1`. Falls back to the custom Java index on failure.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::lsp;
use super::symbols::{self, HoverInfo, SymbolLocation};

pub use super::lsp::{FileTextEdits, ReferenceLocation, RenameRange, SignatureHelp};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JdtlsCodeAction {
    pub title: String,
    pub kind: Option<String>,
    pub edits: Vec<FileTextEdits>,
}

const INIT_TIMEOUT: Duration = Duration::from_secs(90);
const QUERY_TIMEOUT: Duration = Duration::from_secs(12);
const REFERENCE_TIMEOUT: Duration = Duration::from_secs(25);
const SESSION_IDLE: Duration = Duration::from_secs(300);
/// Debounce before pushing didChange to jdtls.
const SYNC_DEBOUNCE: Duration = Duration::from_millis(300);
const COMPLETION_CACHE_TTL: Duration = Duration::from_secs(2);

static SESSIONS: LazyLock<Mutex<HashMap<PathBuf, JdtlsSession>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static NEXT_ID: AtomicU64 = AtomicU64::new(10);

struct OpenDoc {
    version: i64,
    content_hash: u64,
    pending_hash: Option<u64>,
    pending_content: Option<String>,
    last_change_at: Option<Instant>,
}

#[derive(Hash, Eq, PartialEq, Clone)]
struct CompletionCacheKey {
    uri: String,
    line: u32,
    column: u32,
    prefix: String,
    content_hash: u64,
}

struct JdtlsSession {
    child: Child,
    last_used: Instant,
    open_docs: HashMap<String, OpenDoc>,
    service_ready: bool,
    ready_at: Instant,
    /// Latest publishDiagnostics per file URI (typing path).
    diagnostics: HashMap<String, Vec<super::diagnostics::Diagnostic>>,
    completion_cache: HashMap<CompletionCacheKey, (Vec<super::classpath::CompletionItem>, Instant)>,
}

fn new_open_doc(version: i64, content_hash: u64) -> OpenDoc {
    OpenDoc {
        version,
        content_hash,
        pending_hash: None,
        pending_content: None,
        last_change_at: None,
    }
}

fn new_jdtls_session(child: Child, now: Instant) -> JdtlsSession {
    JdtlsSession {
        child,
        last_used: now,
        open_docs: HashMap::new(),
        service_ready: true,
        ready_at: now,
        diagnostics: HashMap::new(),
        completion_cache: HashMap::new(),
    }
}

fn content_fingerprint(content: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    content.hash(&mut hasher);
    hasher.finish()
}

pub fn is_enabled() -> bool {
    match std::env::var("REAPER_USE_JDTLS").as_deref() {
        Ok("0") | Ok("false") | Ok("no") => false,
        Ok("1") | Ok("true") | Ok("yes") => true,
        _ => crate::toolchain::resolve_program("jdtls").is_some(),
    }
}

/// Start jdtls when a Java workspace opens.
pub fn warm_workspace(ws: &Path) -> Result<()> {
    if !is_enabled() {
        return Ok(());
    }
    let ws = ws
        .canonicalize()
        .with_context(|| format!("resolve workspace {}", ws.display()))?;
    let mut map = SESSIONS.lock().expect("jdtls sessions");
    purge_stale_sessions(&mut map);
    if session_alive(map.get_mut(&ws)) {
        return Ok(());
    }
    map.remove(&ws);
    let mut child = spawn_jdtls(&ws)?;
    let root_uri = file_uri(&ws)?;
    initialize_session(&mut child, &root_uri, &ws)?;
    let now = Instant::now();
    map.insert(ws, new_jdtls_session(child, now));
    Ok(())
}

/// True when a warm jdtls session is alive for this workspace.
pub fn workspace_ready(ws: &Path) -> bool {
    let Ok(ws) = ws.canonicalize() else {
        return false;
    };
    let Ok(mut map) = SESSIONS.lock() else {
        return false;
    };
    map.get_mut(&ws)
        .is_some_and(|s| s.service_ready && session_alive(Some(s)))
}

/// jdtls publishDiagnostics for the typing path (no javac while editing).
pub fn typing_diagnostics(
    ws: &Path,
    rel_path: &str,
    content: &str,
) -> Result<Vec<super::diagnostics::Diagnostic>> {
    if !is_enabled() || !is_jdtls_path(rel_path) {
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
    // Sync + drain notifications via a cheap hover at the file start.
    let _ = find_hover(&ws, rel_path, 1, 1, content);
    let map = SESSIONS.lock().expect("jdtls sessions");
    Ok(map
        .get(&ws)
        .and_then(|s| s.diagnostics.get(&uri).cloned())
        .unwrap_or_default())
}

/// Drop a cached jdtls process (project reload, branch switch, classpath invalidation).
pub fn drop_workspace_session(ws: &Path) {
    let Ok(ws) = ws.canonicalize() else {
        return;
    };
    let mut map = SESSIONS.lock().expect("jdtls sessions");
    if let Some(mut session) = map.remove(&ws) {
        let _ = session.child.kill();
        let _ = session.child.wait();
    }
}

pub fn find_hover(
    ws: &Path,
    rel_path: &str,
    line: u32,
    column: u32,
    content: &str,
) -> Result<Option<HoverInfo>> {
    if !is_enabled() || !is_jdtls_path(rel_path) {
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
    let deadline = Instant::now() + QUERY_TIMEOUT;

    let hover = match query_hover(&ws, rel_path, &uri, line, column, content, deadline) {
        Ok(r) => r,
        Err(e) => {
            tracing::debug!("jdtls hover failed: {e:#}");
            return Ok(None);
        }
    };

    if hover.is_null() {
        return Ok(None);
    }

    let Some(text) = hover_text(&hover) else {
        return Ok(None);
    };

    let info = finalize_hover_info(parse_lsp_hover(&text, rel_path), content, line, column);

    Ok(Some(info))
}

fn finalize_hover_info(
    mut info: HoverInfo,
    content: &str,
    line: u32,
    column: u32,
) -> HoverInfo {
    if info.name.is_empty() {
        if let Some(word) = symbols::word_at(content, line, column) {
            info.name = word;
        }
    }
    info.documentation = info
        .documentation
        .and_then(|doc| sanitize_hover_documentation(&doc));
    if info
        .signature
        .as_deref()
        .is_some_and(is_jdtls_source_line)
    {
        info.signature = None;
    }
    info.kind = normalize_hover_kind(&info.kind);
    info
}

fn is_jdtls_source_line(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return false;
    }
    let lower = trimmed.to_lowercase();
    if lower.starts_with("source:")
        && (trimmed.contains("jdt.ls") || trimmed.contains("file://") || trimmed.contains("jdt://"))
    {
        return true;
    }
    trimmed.contains("jdt.ls-java-project") || trimmed.contains("jdt://contents")
}

fn sanitize_hover_documentation(doc: &str) -> Option<String> {
    let trimmed = scrub_jdtls_branding(doc).trim().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

fn file_label_from_jdtls_url(url: &str) -> Option<String> {
    let (path_part, line_part) = url
        .strip_prefix("file://")
        .map(|rest| {
            rest.split_once('#')
                .map(|(path, line)| (path, Some(line)))
                .unwrap_or((rest, None))
        })
        .unwrap_or((url, None));
    let file = path_part.rsplit('/').next()?.split('\\').next()?.trim();
    if file.is_empty() {
        return None;
    }
    let line = line_part.and_then(|l| l.parse::<u32>().ok());
    Some(if let Some(l) = line {
        format!("{file}:{l}")
    } else {
        file.to_string()
    })
}

fn scrub_jdtls_branding(doc: &str) -> String {
    let mut out = doc.to_string();
    for marker in ["[jdt.ls-java-project](", "[jdt.ls]("] {
        loop {
            let Some(start) = out.to_ascii_lowercase().find(marker) else {
                break;
            };
            let Some(rel) = out[start..].find(')') else {
                break;
            };
            let end = start + rel + 1;
            let segment = &out[start..end];
            let Some(url_start) = segment.find("file:") else {
                break;
            };
            let url = &segment[url_start..segment.len() - 1];
            let replacement = file_label_from_jdtls_url(url)
                .map(|label| format!("[{label}]({url})"))
                .unwrap_or_else(|| format!("({url})"));
            out.replace_range(start..end, &replacement);
        }
    }
    for token in ["jdt.ls-java-project", "jdt.ls", "jdtls"] {
        out = out.replace(token, "");
    }
    out.replace("Source:  ", "Source: ")
}

fn normalize_hover_kind(kind: &str) -> String {
    if kind == "symbol" { String::new() } else { kind.to_string() }
}

pub fn find_definition(
    ws: &Path,
    rel_path: &str,
    line: u32,
    column: u32,
    content: &str,
) -> Result<Option<SymbolLocation>> {
    if !is_enabled() || !is_jdtls_path(rel_path) {
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
    let deadline = Instant::now() + QUERY_TIMEOUT;

    let result = match query_definition(&ws, rel_path, &uri, line, column, content, deadline) {
        Ok(r) => r,
        Err(e) => {
            tracing::debug!("jdtls definition failed: {e:#}");
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
    if !is_enabled() || !is_jdtls_path(rel_path) {
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
    let deadline = Instant::now() + REFERENCE_TIMEOUT;
    let result = match query_references(&ws, rel_path, &uri, line, column, content, deadline) {
        Ok(r) => r,
        Err(e) => {
            tracing::debug!("jdtls references failed: {e:#}");
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
    if !is_enabled() || !is_jdtls_path(rel_path) {
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
    let deadline = Instant::now() + QUERY_TIMEOUT;
    let result = match query_prepare_rename(&ws, rel_path, &uri, line, column, content, deadline) {
        Ok(r) => r,
        Err(e) => {
            tracing::debug!("jdtls prepareRename failed: {e:#}");
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
    if !is_enabled() || !is_jdtls_path(rel_path) || new_name.trim().is_empty() {
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
    let deadline = Instant::now() + QUERY_TIMEOUT;
    let result = match query_rename(
        &ws,
        rel_path,
        &uri,
        line,
        column,
        content,
        new_name.trim(),
        deadline,
    ) {
        Ok(r) => r,
        Err(e) => {
            tracing::debug!("jdtls rename failed: {e:#}");
            return Ok(Vec::new());
        }
    };
    lsp::parse_workspace_edit(&ws, &result)
}

pub fn code_actions(
    ws: &Path,
    rel_path: &str,
    line: u32,
    column: u32,
    content: &str,
    only: &[&str],
) -> Result<Vec<JdtlsCodeAction>> {
    if !is_enabled() || !is_jdtls_path(rel_path) {
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
    let deadline = Instant::now() + QUERY_TIMEOUT;
    let result = match query_code_actions(
        &ws, rel_path, &uri, line, column, content, only, deadline,
    ) {
        Ok(r) => r,
        Err(e) => {
            tracing::debug!("jdtls codeAction failed: {e:#}");
            return Ok(Vec::new());
        }
    };
    Ok(parse_code_actions(&ws, &result))
}

pub fn signature_help(
    ws: &Path,
    rel_path: &str,
    line: u32,
    column: u32,
    content: &str,
) -> Result<Option<SignatureHelp>> {
    if !is_enabled() || !is_jdtls_path(rel_path) {
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
    let deadline = Instant::now() + QUERY_TIMEOUT;
    let result = match query_signature_help(
        &ws, rel_path, &uri, line, column, content, deadline,
    ) {
        Ok(r) => r,
        Err(e) => {
            tracing::debug!("jdtls signatureHelp failed: {e:#}");
            return Ok(None);
        }
    };
    Ok(lsp::parse_signature_help(&result))
}

/// Project-aware Java completions from jdtls (`textDocument/completion`).
pub fn find_completions(
    ws: &Path,
    rel_path: &str,
    line: u32,
    column: u32,
    content: &str,
) -> Result<Vec<super::classpath::CompletionItem>> {
    if !is_enabled() || !is_jdtls_path(rel_path) {
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
    let deadline = Instant::now() + QUERY_TIMEOUT;
    let trigger = completion_trigger_character(content, line, column);
    let content_hash = content_fingerprint(content);
    let cache_key = CompletionCacheKey {
        uri: uri.clone(),
        line,
        column,
        prefix: String::new(), // prefix filtering happens client-side on cached jdtls items
        content_hash,
    };
    {
        let map = SESSIONS.lock().expect("jdtls sessions");
        if let Some(session) = map.get(&ws) {
            if let Some((items, at)) = session.completion_cache.get(&cache_key) {
                if at.elapsed() < COMPLETION_CACHE_TTL {
                    return Ok(items.clone());
                }
            }
        }
    }
    let result = match query_completion(
        &ws, rel_path, &uri, line, column, content, trigger, deadline,
    ) {
        Ok(r) => r,
        Err(e) => {
            tracing::debug!("jdtls completion failed: {e:#}");
            return Ok(Vec::new());
        }
    };
    let items = parse_jdtls_completion_items(&result, rel_path);
    if let Ok(mut map) = SESSIONS.lock() {
        if let Some(session) = map.get_mut(&ws) {
            session
                .completion_cache
                .insert(cache_key, (items.clone(), Instant::now()));
        }
    }
    Ok(items)
}

fn is_jdtls_path(path: &str) -> bool {
    path.ends_with(".java")
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
    let pos = lsp_position(line, column);
    lsp_request_with_retry(
        ws,
        rel_path,
        uri,
        content,
        deadline,
        |id| {
            json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": "textDocument/hover",
                "params": {
                    "textDocument": { "uri": uri },
                    "position": pos
                }
            })
        },
        true,
    )
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
    lsp_text_document_request(
        ws,
        rel_path,
        uri,
        content,
        deadline,
        "textDocument/definition",
        line,
        column,
    )
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
    let pos = lsp_position(line, column);
    lsp_request_with_retry(
        ws,
        rel_path,
        uri,
        content,
        deadline,
        |id| {
            json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": "textDocument/references",
                "params": {
                    "textDocument": { "uri": uri },
                    "position": pos,
                    "context": { "includeDeclaration": true }
                }
            })
        },
        true,
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
    lsp_text_document_request(
        ws,
        rel_path,
        uri,
        content,
        deadline,
        "textDocument/prepareRename",
        line,
        column,
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
    let pos = lsp_position(line, column);
    let new_name = new_name.to_string();
    lsp_request_with_retry(
        ws,
        rel_path,
        uri,
        content,
        deadline,
        move |id| {
            json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": "textDocument/rename",
                "params": {
                    "textDocument": { "uri": uri },
                    "position": pos,
                    "newName": new_name
                }
            })
        },
        true,
    )
}

fn query_code_actions(
    ws: &Path,
    rel_path: &str,
    uri: &str,
    line: u32,
    column: u32,
    content: &str,
    only: &[&str],
    deadline: Instant,
) -> Result<Value> {
    let (start_line, start_col, end_line, end_col) = word_range(content, line, column)
        .unwrap_or((line, column, line, column));
    let start = lsp_position(start_line, start_col);
    let end = lsp_position(end_line, end_col);
    let only: Vec<String> = only.iter().map(|s| s.to_string()).collect();
    lsp_request_with_retry(
        ws,
        rel_path,
        uri,
        content,
        deadline,
        move |id| {
            json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": "textDocument/codeAction",
                "params": {
                    "textDocument": { "uri": uri },
                    "range": { "start": start, "end": end },
                    "context": {
                        "diagnostics": [],
                        "only": only
                    }
                }
            })
        },
        true,
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
    let pos = lsp_position(line, column);
    let trigger = lsp::signature_help_trigger(content, line, column);
    lsp_request_with_retry(
        ws,
        rel_path,
        uri,
        content,
        deadline,
        move |id| {
            let mut params = json!({
                "textDocument": { "uri": uri },
                "position": pos
            });
            if let Some((trigger_kind, trigger_character)) = trigger {
                params["context"] = json!({
                    "triggerKind": trigger_kind,
                    "triggerCharacter": trigger_character,
                    "isRetrigger": false
                });
            }
            json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": "textDocument/signatureHelp",
                "params": params
            })
        },
        true,
    )
}

fn query_completion(
    ws: &Path,
    rel_path: &str,
    uri: &str,
    line: u32,
    column: u32,
    content: &str,
    trigger_character: Option<char>,
    deadline: Instant,
) -> Result<Value> {
    let pos = lsp_position(line, column);
    lsp_request_with_retry(
        ws,
        rel_path,
        uri,
        content,
        deadline,
        move |id| {
            let mut params = json!({
                "textDocument": { "uri": uri },
                "position": pos
            });
            if let Some(ch) = trigger_character {
                params["context"] = json!({
                    "triggerKind": 2,
                    "triggerCharacter": ch.to_string()
                });
            }
            json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": "textDocument/completion",
                "params": params
            })
        },
        true,
    )
}

fn completion_trigger_character(content: &str, line: u32, column: u32) -> Option<char> {
    let line_text = content.lines().nth(line.saturating_sub(1) as usize)?;
    let col0 = column.saturating_sub(1) as usize;
    if col0 == 0 || col0 > line_text.len() {
        return None;
    }
    match line_text.as_bytes()[col0 - 1] as char {
        '.' | '@' => Some(line_text.as_bytes()[col0 - 1] as char),
        _ => None,
    }
}

fn parse_jdtls_completion_items(
    result: &Value,
    from_path: &str,
) -> Vec<super::classpath::CompletionItem> {
    lsp::parse_completion_items(result)
        .into_iter()
        .map(|item| super::classpath::CompletionItem {
            label: item.label,
            kind: item.kind,
            detail: item.detail,
            insert: item.insert,
            path: Some(from_path.to_string()),
            line: None,
            column: None,
            documentation: item.documentation,
        })
        .collect()
}

fn lsp_text_document_request(
    ws: &Path,
    rel_path: &str,
    uri: &str,
    content: &str,
    deadline: Instant,
    method: &str,
    line: u32,
    column: u32,
) -> Result<Value> {
    let pos = lsp_position(line, column);
    let method = method.to_string();
    lsp_request_with_retry(
        ws,
        rel_path,
        uri,
        content,
        deadline,
        move |id| {
            json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": method,
                "params": {
                    "textDocument": { "uri": uri },
                    "position": pos
                }
            })
        },
        true,
    )
}

fn lsp_position(line: u32, column: u32) -> Value {
    json!({
        "line": line.saturating_sub(1),
        "character": column.saturating_sub(1)
    })
}

fn lsp_request(
    ws: &Path,
    _rel_path: &str,
    uri: &str,
    content: &str,
    deadline: Instant,
    id: u64,
    request: Value,
) -> Result<Value> {
    let mut map = SESSIONS.lock().expect("jdtls sessions");
    purge_stale_sessions(&mut map);

    if !session_alive(map.get_mut(ws)) {
        map.remove(ws);
        let mut child = spawn_jdtls(ws)?;
        let root_uri = file_uri(ws)?;
        initialize_session(&mut child, &root_uri, ws)?;
        let now = Instant::now();
        map.insert(ws.to_path_buf(), new_jdtls_session(child, now));
    }

    let session = map.get_mut(ws).context("jdtls session")?;
    session.last_used = Instant::now();

    let stdin = session.child.stdin.as_mut().context("jdtls stdin")?;
    sync_document(stdin, &mut session.open_docs, uri, content, true)?;
    let stdout = session.child.stdout.as_mut().context("jdtls stdout")?;

    write_message(stdin, &request)?;
    wait_for_id(stdout, id, deadline, Some(ws))
}

fn sync_document(
    stdin: &mut impl Write,
    open_docs: &mut HashMap<String, OpenDoc>,
    uri: &str,
    content: &str,
    force: bool,
) -> Result<()> {
    let hash = content_fingerprint(content);
    if let Some(doc) = open_docs.get_mut(uri) {
        if doc.content_hash == hash {
            doc.pending_hash = None;
            doc.pending_content = None;
            doc.last_change_at = None;
            return Ok(());
        }
        doc.pending_hash = Some(hash);
        doc.pending_content = Some(content.to_string());
        if doc.last_change_at.is_none() {
            doc.last_change_at = Some(Instant::now());
        }
        if !force && doc.last_change_at.is_some_and(|t| t.elapsed() < SYNC_DEBOUNCE) {
            return Ok(());
        }
        let ver = doc.version + 1;
        let text = doc
            .pending_content
            .take()
            .unwrap_or_else(|| content.to_string());
        let flushed_hash = doc.pending_hash.take().unwrap_or(hash);
        open_docs.insert(
            uri.to_string(),
            OpenDoc {
                version: ver,
                content_hash: flushed_hash,
                pending_hash: None,
                pending_content: None,
                last_change_at: None,
            },
        );
        write_message(
            stdin,
            &json!({
                "jsonrpc": "2.0",
                "method": "textDocument/didChange",
                "params": {
                    "textDocument": { "uri": uri, "version": ver },
                    "contentChanges": [{ "text": text }]
                }
            }),
        )
    } else {
        let version = NEXT_ID.fetch_add(1, Ordering::Relaxed) as i64;
        open_docs.insert(uri.to_string(), new_open_doc(version, hash));
        write_message(
            stdin,
            &json!({
                "jsonrpc": "2.0",
                "method": "textDocument/didOpen",
                "params": {
                    "textDocument": {
                        "uri": uri,
                        "languageId": "java",
                        "version": version,
                        "text": content
                    }
                }
            }),
        )
    }
}

fn session_alive(session: Option<&mut JdtlsSession>) -> bool {
    let Some(session) = session else {
        return false;
    };
    if session.last_used.elapsed() > SESSION_IDLE {
        return false;
    }
    matches!(session.child.try_wait(), Ok(None))
}

fn purge_stale_sessions(map: &mut HashMap<PathBuf, JdtlsSession>) {
    map.retain(|_, session| {
        session.last_used.elapsed() <= SESSION_IDLE
            && matches!(session.child.try_wait(), Ok(None))
    });
}

fn initialize_session(child: &mut Child, root_uri: &str, ws: &Path) -> Result<()> {
    let deadline = Instant::now() + INIT_TIMEOUT;
    let stdin = child.stdin.as_mut().context("jdtls stdin")?;
    let stdout = child.stdout.as_mut().context("jdtls stdout")?;

    let folder_name = ws
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("workspace");

    write_message(
        stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "processId": std::process::id(),
                "rootUri": root_uri,
                "workspaceFolders": [
                    { "uri": root_uri, "name": folder_name }
                ],
                "capabilities": {
                    "workspace": {
                        "workspaceFolders": true
                    },
                    "textDocument": {
                        "hover": { "contentFormat": ["markdown", "plaintext"] },
                        "definition": { "linkSupport": true },
                        "references": {},
                        "rename": { "prepareSupport": true },
                        "codeAction": {
                            "codeActionLiteralSupport": {
                                "codeActionKind": {
                                    "valueSet": [
                                        "quickfix",
                                        "refactor",
                                        "source",
                                        "source.organizeImports"
                                    ]
                                }
                            }
                        },
                        "signatureHelp": {
                            "signatureInformation": {
                                "documentationFormat": ["markdown", "plaintext"],
                                "parameterInformation": { "labelOffsetSupport": true }
                            }
                        },
                        "completion": {
                            "completionItem": {
                                "snippetSupport": true,
                                "documentationFormat": ["markdown", "plaintext"],
                                "resolveSupport": {
                                    "properties": ["documentation", "detail", "additionalTextEdits"]
                                }
                            },
                            "contextSupport": true
                        },
                        "publishDiagnostics": { "relatedInformation": false }
                    }
                }
            }
        }),
    )?;

    wait_for_id(stdout, 1, deadline, Some(ws))?;

    write_message(
        stdin,
        &json!({
            "jsonrpc": "2.0",
            "method": "initialized",
            "params": {}
        }),
    )?;

    configure_workspace(stdin, ws)?;

    wait_for_service_ready(stdout, deadline, ws)?;

    Ok(())
}

fn is_service_ready_notification(msg: &Value) -> bool {
    msg.get("method").and_then(|v| v.as_str()) == Some("language/status")
        && msg.get("params")
            .and_then(|p| p.get("type"))
            .and_then(|t| t.as_str())
            .is_some_and(|t| t == "ServiceReady")
}

fn java_se_runtime_name(major: u32) -> String {
    if major <= 8 {
        "JavaSE-1.8".into()
    } else {
        format!("JavaSE-{major}")
    }
}

fn workspace_project_java_release(ws: &Path) -> u32 {
    let probe = project_settings_probe_path(ws);
    super::java_diagnostics::project_java_release(ws, &probe).unwrap_or(17)
}

/// Eclipse compiler prefs so jdtls uses pom/Gradle release before Maven import finishes.
fn ensure_jdtls_compiler_prefs(ws: &Path, release: u32) -> Result<()> {
    let settings_dir = ws.join(".settings");
    std::fs::create_dir_all(&settings_dir)?;
    let prefs_path = settings_dir.join("org.eclipse.jdt.core.prefs");
    let compliance_key = format!("org.eclipse.jdt.core.compiler.compliance={release}");
    let needs_write = match std::fs::read_to_string(&prefs_path) {
        Ok(existing) => !existing.lines().any(|line| line.trim() == compliance_key),
        Err(_) => true,
    };
    if !needs_write {
        return Ok(());
    }
    let contents = format!(
        "eclipse.preferences.version=1\n\
         org.eclipse.jdt.core.compiler.codegen.targetPlatform={release}\n\
         org.eclipse.jdt.core.compiler.compliance={release}\n\
         org.eclipse.jdt.core.compiler.source={release}\n"
    );
    std::fs::write(&prefs_path, contents)?;
    tracing::info!(
        "jdtls compiler compliance Java {release} → {}",
        prefs_path.display()
    );
    Ok(())
}

fn jdtls_runtime_settings(ws: &Path) -> Value {
    let release = workspace_project_java_release(ws);
    if let Err(e) = ensure_jdtls_compiler_prefs(ws, release) {
        tracing::warn!("jdtls compiler prefs: {e:#}");
    }

    let mut runtimes: Vec<Value> = Vec::new();
    match crate::jdk::project_java_home_for_release(release) {
        Ok(project_home) => {
            tracing::info!(
                "jdtls project runtime: {} → {} (Java {release} from pom/Gradle + Settings → Java)",
                java_se_runtime_name(release),
                project_home.display()
            );
            runtimes.push(json!({
                "name": java_se_runtime_name(release),
                "path": project_home,
                "default": true,
            }));
        }
        Err(e) => {
            tracing::warn!("jdtls project runtime JDK {release}: {e:#}");
        }
    }

    if let Ok(launcher_home) = crate::jdk::jdtls_java_home() {
        tracing::debug!(
            "jdtls process JVM (not a project runtime): {}",
            launcher_home.display()
        );
    }

    json!({
        "java.configuration.runtimes": runtimes,
        "java.configuration.updateBuildConfiguration": "automatic",
        "java.import.maven.enabled": true,
        "java.import.gradle.enabled": true,
        "java.import.gradle.wrapper.enabled": true,
        "java.completion.enabled": true,
        "java.completion.overwrite": true,
        "java.completion.guessMethodArguments": true,
        "java.sources.download": true,
        "java.eclipse.downloadSources": true,
    })
}

fn project_settings_probe_path(ws: &Path) -> String {
    for rel in ["pom.xml", "build.gradle.kts", "build.gradle"] {
        if ws.join(rel).is_file() {
            return rel.to_string();
        }
    }
    ".".to_string()
}

fn configure_workspace(stdin: &mut impl Write, ws: &Path) -> Result<()> {
    write_message(
        stdin,
        &json!({
            "jsonrpc": "2.0",
            "method": "workspace/didChangeConfiguration",
            "params": {
                "settings": jdtls_runtime_settings(ws),
            }
        }),
    )
}

fn wait_for_service_ready(r: &mut impl Read, deadline: Instant, ws: &Path) -> Result<()> {
    while Instant::now() < deadline {
        let msg = read_message(r)?;
        if is_service_ready_notification(&msg) {
            return Ok(());
        }
        ingest_notification(ws, &msg);
        if msg.get("id").is_some() {
            tracing::debug!("unexpected jdtls response during warm-up: {msg}");
        }
    }
    tracing::warn!("jdtls ServiceReady not seen before deadline; continuing");
    Ok(())
}

fn lsp_result_empty(result: &Value) -> bool {
    result.is_null()
        || result
            .as_array()
            .is_some_and(|items| items.is_empty())
}

fn empty_result_retry_limit(session: &JdtlsSession) -> u32 {
    if !session.service_ready {
        return 4;
    }
    if session.ready_at.elapsed() < Duration::from_secs(90) {
        1
    } else {
        0
    }
}

fn empty_result_retry_delay(session: &JdtlsSession) -> Duration {
    if session.service_ready {
        Duration::from_millis(300)
    } else {
        Duration::from_millis(1500)
    }
}

fn lsp_request_with_retry(
    ws: &Path,
    rel_path: &str,
    uri: &str,
    content: &str,
    deadline: Instant,
    build_request: impl Fn(u64) -> Value,
    retry_empty: bool,
) -> Result<Value> {
    let mut attempt = 0u32;
    loop {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let per_attempt = Duration::from_secs(8);
        let attempt_deadline = (Instant::now() + per_attempt).min(deadline);
        let result = lsp_request(
            ws,
            rel_path,
            uri,
            content,
            attempt_deadline,
            id,
            build_request(id),
        )?;
        let max_retries = if retry_empty {
            SESSIONS
                .lock()
                .ok()
                .and_then(|map| map.get(ws).map(empty_result_retry_limit))
                .unwrap_or(0)
        } else {
            0
        };
        if !retry_empty || !lsp_result_empty(&result) || attempt >= max_retries {
            return Ok(result);
        }
        let delay = SESSIONS
            .lock()
            .ok()
            .and_then(|map| map.get(ws).map(empty_result_retry_delay))
            .unwrap_or(Duration::from_millis(300));
        attempt += 1;
        std::thread::sleep(delay);
    }
}

fn bundled_jdtls_root() -> Option<PathBuf> {
    let bin = crate::config::bundled_jdtls()?;
    let root = bin.parent()?.parent()?.to_path_buf();
    (root.join("plugins").is_dir()).then_some(root)
}

fn jdtls_shared_config_dir(base: &Path) -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        return if crate::platform::macos_host_arch() == "arm64" {
            base.join("config_mac_arm")
        } else {
            base.join("config_mac")
        };
    }
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    {
        return base.join("config_linux_arm");
    }
    #[cfg(all(target_os = "linux", not(target_arch = "aarch64")))]
    {
        return base.join("config_linux");
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        return base.join("config_linux");
    }
}

fn find_equinox_launcher(base: &Path) -> Result<PathBuf> {
    let plugins = base.join("plugins");
    let mut jars = std::fs::read_dir(&plugins)
        .with_context(|| format!("read {}", plugins.display()))?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| {
                    name.starts_with("org.eclipse.equinox.launcher_") && name.ends_with(".jar")
                })
        })
        .collect::<Vec<_>>();
    jars.sort();
    jars.into_iter()
        .next()
        .with_context(|| format!("equinox launcher jar not found in {}", plugins.display()))
}

fn bundled_jdtls_configuration_ready(base: &Path) -> bool {
    base.join("configuration/org.eclipse.osgi").is_dir()
}

fn ensure_bundled_jdtls_configuration(base: &Path) -> Result<()> {
    if bundled_jdtls_configuration_ready(base) {
        return Ok(());
    }

    static WARM: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));
    let _guard = WARM.lock().expect("jdtls warm");
    if bundled_jdtls_configuration_ready(base) {
        return Ok(());
    }

    let java_home = crate::jdk::jdtls_java_home().context("JDK 21+ required for jdtls")?;
    let java = java_home.join("bin").join("java");
    if !java.is_file() {
        bail!("java executable not found at {}", java.display());
    }

    let jar = find_equinox_launcher(base)?;
    let config = jdtls_shared_config_dir(base);
    let config_area = format!(
        "-Dosgi.sharedConfiguration.area={}",
        config.to_string_lossy()
    );
    let warm_data = std::env::temp_dir().join(format!(
        "reaper-jdtls-warm-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&warm_data);
    std::fs::create_dir_all(&warm_data)?;

    let mut cmd = Command::new(&java);
    cmd.current_dir(base);
    cmd.args([
        "-Declipse.application=org.eclipse.jdt.ls.core.id1",
        "-Dosgi.bundles.defaultStartLevel=4",
        "-Declipse.product=org.eclipse.jdt.ls.core.product",
        "-Dosgi.checkConfiguration=true",
        config_area.as_str(),
        "-Dosgi.sharedConfiguration.area.readOnly=true",
        "-Dosgi.configuration.cascaded=true",
        "-Xms256m",
        "--add-modules=ALL-SYSTEM",
        "--add-opens",
        "java.base/java.util=ALL-UNNAMED",
        "--add-opens",
        "java.base/java.lang=ALL-UNNAMED",
    ])
    .arg("-jar")
    .arg(&jar)
    .arg("-data")
    .arg(&warm_data)
    .stdin(Stdio::null())
    .stdout(Stdio::null())
    .stderr(Stdio::null());
    crate::jdk::apply_jdtls_java_env(&mut cmd);

    let mut child = cmd
        .spawn()
        .with_context(|| format!("failed to warm-start bundled jdtls ({})", java.display()))?;

    let deadline = Instant::now() + INIT_TIMEOUT;
    while Instant::now() < deadline {
        if bundled_jdtls_configuration_ready(base) {
            let _ = child.kill();
            let _ = child.wait();
            let _ = std::fs::remove_dir_all(&warm_data);
            return Ok(());
        }
        if child.try_wait()?.is_some() {
            let _ = std::fs::remove_dir_all(&warm_data);
            bail!("bundled jdtls warm-start exited before configuration was ready");
        }
        std::thread::sleep(Duration::from_millis(500));
    }

    let _ = child.kill();
    let _ = child.wait();
    let _ = std::fs::remove_dir_all(&warm_data);
    bail!("bundled jdtls warm-start timed out before configuration was ready")
}

fn spawn_bundled_jdtls_java(base: &Path, ws: &Path, data_dir: &Path) -> Result<Child> {
    ensure_bundled_jdtls_configuration(base)?;

    let java_home = crate::jdk::jdtls_java_home().context("JDK 21+ required for jdtls")?;
    let java = java_home.join("bin").join("java");
    if !java.is_file() {
        bail!("java executable not found at {}", java.display());
    }

    let jar = find_equinox_launcher(base)?;
    let config = jdtls_shared_config_dir(base);
    let config_area = format!(
        "-Dosgi.sharedConfiguration.area={}",
        config.to_string_lossy()
    );

    let mut cmd = Command::new(&java);
    cmd.current_dir(ws);
    cmd.args([
        "-Declipse.application=org.eclipse.jdt.ls.core.id1",
        "-Dosgi.bundles.defaultStartLevel=4",
        "-Declipse.product=org.eclipse.jdt.ls.core.product",
        "-Dosgi.checkConfiguration=true",
        config_area.as_str(),
        "-Dosgi.sharedConfiguration.area.readOnly=true",
        "-Dosgi.configuration.cascaded=true",
        "-Xms1G",
        "--add-modules=ALL-SYSTEM",
        "--add-opens",
        "java.base/java.util=ALL-UNNAMED",
        "--add-opens",
        "java.base/java.lang=ALL-UNNAMED",
    ])
    .arg("-jar")
    .arg(&jar)
    .arg("-data")
    .arg(data_dir)
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::null());
    crate::jdk::apply_jdtls_java_env(&mut cmd);

    cmd.spawn()
        .with_context(|| format!("failed to start bundled jdtls ({})", java.display()))
}

fn spawn_jdtls(ws: &Path) -> Result<Child> {
    let data_dir = ws.join(".reaper/jdtls-data");
    std::fs::create_dir_all(&data_dir)?;

    if let Some(base) = bundled_jdtls_root() {
        return spawn_bundled_jdtls_java(&base, ws, &data_dir);
    }

    let jdtls = crate::toolchain::resolve_program("jdtls").with_context(|| {
        "jdtls not found — rebuild Reaper.app to bundle jdtls, install with `brew install jdtls`, \
         or set REAPER_JDTLS in Settings → Compilers"
    })?;

    let mut cmd = Command::new(&jdtls);
    cmd.arg("-data").arg(data_dir.as_os_str());
    if let Ok(home) = crate::jdk::jdtls_java_home() {
        let java = home.join("bin").join("java");
        if java.is_file() {
            cmd.arg("--java-executable").arg(java);
        }
    }
    cmd.current_dir(ws);
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    crate::jdk::apply_jdtls_java_env(&mut cmd);

    cmd.spawn()
        .with_context(|| format!("failed to start jdtls ({})", jdtls.display()))
}

fn write_message(w: &mut impl Write, body: &Value) -> Result<()> {
    let json = serde_json::to_string(body)?;
    write!(w, "Content-Length: {}\r\n\r\n{}", json.len(), json)?;
    w.flush()?;
    Ok(())
}

fn wait_for_id(r: &mut impl Read, id: u64, deadline: Instant, ws: Option<&Path>) -> Result<Value> {
    loop {
        if Instant::now() >= deadline {
            bail!("jdtls timed out");
        }
        let msg = read_message(r)?;
        if is_service_ready_notification(&msg) {
            continue;
        }
        if let Some(ws) = ws {
            ingest_notification(ws, &msg);
        }
        if msg.get("id").and_then(|v| v.as_u64()) == Some(id) {
            if let Some(err) = msg.get("error") {
                bail!("jdtls error: {err}");
            }
            return Ok(msg.get("result").cloned().unwrap_or(Value::Null));
        }
        if msg.get("id").is_some() {
            tracing::debug!("unexpected jdtls response while waiting for id {id}: {msg}");
        }
    }
}

fn ingest_notification(ws: &Path, msg: &Value) {
    if msg.get("method").and_then(|v| v.as_str()) != Some("textDocument/publishDiagnostics") {
        return;
    }
    let Some(params) = msg.get("params") else {
        return;
    };
    let uri = params
        .get("uri")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if uri.is_empty() {
        return;
    }
    let diags = params
        .get("diagnostics")
        .and_then(|v| v.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|d| parse_lsp_diagnostic(ws, &uri, d))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let Ok(ws_canon) = ws.canonicalize() else {
        return;
    };
    let Ok(mut map) = SESSIONS.lock() else {
        return;
    };
    if let Some(session) = map.get_mut(&ws_canon) {
        session.diagnostics.insert(uri, diags);
    }
}

fn parse_lsp_diagnostic(
    ws: &Path,
    uri: &str,
    diag: &Value,
) -> Option<super::diagnostics::Diagnostic> {
    let path = lsp::uri_to_workspace_path(ws, uri).ok()?;
    let range = diag.get("range")?;
    let start = range.get("start")?;
    let end = range.get("end")?;
    let line = start
        .get("line")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32
        + 1;
    let column = start
        .get("character")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32
        + 1;
    let end_line = end
        .get("line")
        .and_then(|v| v.as_u64())
        .map(|v| v as u32 + 1);
    let end_column = end
        .get("character")
        .and_then(|v| v.as_u64())
        .map(|v| v as u32 + 1);
    let message = diag
        .get("message")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let severity = match diag.get("severity").and_then(|v| v.as_u64()) {
        Some(2) => "warning",
        Some(3) | Some(4) => "warning",
        _ => "error",
    }
    .to_string();
    Some(super::diagnostics::Diagnostic {
        path,
        line: line.max(1),
        column: column.max(1),
        end_line,
        end_column,
        message,
        severity,
    })
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

fn parse_lsp_hover(text: &str, from_path: &str) -> HoverInfo {
    let trimmed = text.trim();
    if trimmed.contains("```") {
        return parse_markdown_hover(trimmed, from_path);
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

fn parse_markdown_hover(text: &str, from_path: &str) -> HoverInfo {
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
    if is_jdtls_source_line(line) {
        return false;
    }
    if line.starts_with("→") || line.starts_with('-') || line == "Parameters:" {
        return false;
    }
    line.contains('(') || line.ends_with(';')
}

fn signature_kind_name(sig: &str) -> (String, String) {
    let trimmed = sig.trim();
    let tokens: Vec<&str> = trimmed.split_whitespace().collect();
    for (i, tok) in tokens.iter().enumerate() {
        let kind = match *tok {
            "class" => "class",
            "interface" => "interface",
            "enum" => "enum",
            "record" => "record",
            "@interface" => "annotation",
            _ => continue,
        };
        if let Some(name) = tokens.get(i + 1) {
            let name = name.trim_start_matches('*').trim_end_matches('<');
            return (kind.into(), name.to_string());
        }
    }

    if let Some(head) = trimmed.split('(').next() {
        if trimmed.contains('(') {
            if let Some(name) = head.split_whitespace().last() {
                let name = name.trim_start_matches('*');
                return ("method".into(), name.to_string());
            }
        } else if let Some(name) = head.split_whitespace().last() {
            let name = name.trim_start_matches('*');
            if !name.is_empty() {
                return ("symbol".into(), name.to_string());
            }
        }
    }
    ("symbol".into(), String::new())
}

fn parse_definition_location(
    ws: &Path,
    from_path: &str,
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

    let name = symbols::word_at(content, line, column)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "symbol".into());

    if let Ok(path) = lsp::uri_to_workspace_path(ws, uri) {
        if super::classpath::definition_path_is_openable(ws, &path) {
            return Some(SymbolLocation {
                name,
                kind: "definition".into(),
                path,
                line: def_line,
                column: def_col,
            });
        }
    }

    if let Some(fqcn) = fqcn_from_lsp_uri(uri) {
        if let Ok(Some(loc)) =
            super::classpath::resolve_java_library_definition(ws, from_path, &fqcn, &name)
        {
            return Some(loc);
        }
    }

    None
}

fn fqcn_from_lsp_uri(uri: &str) -> Option<String> {
    if let Some((_, entry)) = split_jar_uri(uri) {
        return class_entry_to_fqcn(&entry);
    }
    if let Some(entry) = uri.strip_prefix("jdt://contents/") {
        let path = entry.split('?').next().unwrap_or(entry);
        let class_part = path.split('/').skip(1).collect::<Vec<_>>().join("/");
        if !class_part.is_empty() {
            return class_entry_to_fqcn(&class_part);
        }
    }
    None
}

fn split_jar_uri(uri: &str) -> Option<(PathBuf, String)> {
    let rest = uri.strip_prefix("jar:file:").or_else(|| uri.strip_prefix("jar:"))?;
    let (jar_part, entry) = rest.split_once('!')?;
    let jar_url = if jar_part.starts_with("file:") {
        jar_part.to_string()
    } else {
        format!("file:{jar_part}")
    };
    let jar_path = url::Url::parse(&jar_url).ok()?.to_file_path().ok()?;
    let entry = entry.trim_start_matches('/').to_string();
    Some((jar_path, entry))
}

fn class_entry_to_fqcn(entry: &str) -> Option<String> {
    let entry = entry.trim_start_matches('/');
    let path = entry.split('?').next().unwrap_or(entry);
    let path = path.strip_suffix(".java").or_else(|| path.strip_suffix(".class"))?;
    if path.contains('/') {
        Some(path.replace('/', "."))
    } else if path.contains('.') {
        Some(path.to_string())
    } else {
        None
    }
}

fn parse_code_actions(ws: &Path, result: &Value) -> Vec<JdtlsCodeAction> {
    let Some(items) = result.as_array() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for item in items {
        let Some(title) = item.get("title").and_then(|v| v.as_str()) else {
            continue;
        };
        let kind = item
            .get("kind")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        let mut edits = Vec::new();
        if let Some(edit) = item.get("edit") {
            if let Ok(parsed) = lsp::parse_workspace_edit(ws, edit) {
                edits = parsed;
            }
        }
        if edits.is_empty() {
            continue;
        }
        out.push(JdtlsCodeAction {
            title: title.to_string(),
            kind,
            edits,
        });
    }
    out
}

fn word_range(content: &str, line: u32, column: u32) -> Option<(u32, u32, u32, u32)> {
    let word = symbols::word_at(content, line, column)?;
    let line_text = content.lines().nth(line.saturating_sub(1) as usize)?;
    let col0 = column.saturating_sub(1) as usize;
    let start = line_text[..col0.min(line_text.len())]
        .rfind(&word)
        .map(|i| (i + 1) as u32)
        .unwrap_or(column);
    let end = start + word.len() as u32;
    Some((line, start, line, end))
}

fn file_uri(path: &Path) -> Result<String> {
    url::Url::from_file_path(path)
        .map(|u| u.to_string())
        .map_err(|_| anyhow::anyhow!("invalid path for URI: {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::OnceLock;
    use std::time::Duration;

    const REGRESSION_SOURCE: &str = "package com.example;\n\nimport java.util.List;\n\npublic class App {\n  public static void greet() {}\n\n  public static int add(int a, int b) {\n    return a + b;\n  }\n\n  public static void main(String[] args) {\n    greet();\n    add(1, 2);\n  }\n}\n";
    const REGRESSION_REL: &str = "src/main/java/com/example/App.java";
    const GREET_USAGE_LINE: u32 = 13;
    const GREET_USAGE_COL: u32 = 5;

    static JDTLS_REGRESSION: OnceLock<bool> = OnceLock::new();

    fn create_regression_workspace_dir() -> Option<std::path::PathBuf> {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let ws = std::env::temp_dir().join(format!(
            "reaper-jdtls-lsp-{}-{}",
            std::process::id(),
            id
        ));
        let _ = std::fs::remove_dir_all(&ws);
        std::fs::create_dir_all(ws.join("src/main/java/com/example")).ok()?;
        std::fs::write(
            ws.join("pom.xml"),
            r#"<?xml version="1.0" encoding="UTF-8"?>
<project xmlns="http://maven.apache.org/POM/4.0.0">
  <modelVersion>4.0.0</modelVersion>
  <groupId>com.example</groupId>
  <artifactId>app</artifactId>
  <version>1.0</version>
  <properties>
    <maven.compiler.source>17</maven.compiler.source>
    <maven.compiler.target>17</maven.compiler.target>
  </properties>
</project>
"#,
        )
        .ok()?;
        std::fs::write(ws.join(REGRESSION_REL), REGRESSION_SOURCE).ok()?;
        Some(ws)
    }

    fn jdtls_regression_probe(ws: &Path) -> bool {
        for _ in 0..12 {
            match find_hover(
                ws,
                REGRESSION_REL,
                GREET_USAGE_LINE,
                GREET_USAGE_COL,
                REGRESSION_SOURCE,
            ) {
                Ok(Some(h)) => {
                    if h.name.contains("greet")
                        || h.signature.as_deref().is_some_and(|s| s.contains("greet"))
                    {
                        return true;
                    }
                }
                Ok(None) => {}
                Err(_) => return false,
            }
            std::thread::sleep(Duration::from_millis(500));
        }
        false
    }

    fn jdtls_regression_enabled() -> bool {
        *JDTLS_REGRESSION.get_or_init(|| {
            if !is_enabled() {
                return false;
            }
            let Some(ws) = create_regression_workspace_dir() else {
                return false;
            };
            drop_workspace_session(&ws);
            let ok = jdtls_regression_probe(&ws);
            let _ = std::fs::remove_dir_all(&ws);
            ok
        })
    }

    fn setup_regression_workspace() -> Option<std::path::PathBuf> {
        if !jdtls_regression_enabled() {
            return None;
        }
        let ws = create_regression_workspace_dir()?;
        drop_workspace_session(&ws);
        Some(ws)
    }

    #[test]
    fn disabled_without_jdtls_on_path() {
        if std::env::var("REAPER_USE_JDTLS").is_ok() {
            return;
        }
        if crate::toolchain::resolve_program("jdtls").is_some() {
            return;
        }
        assert!(!is_enabled());
    }

    #[test]
    fn sanitize_hover_documentation_scrubs_jdtls_branding() {
        let doc = "Some javadoc\n\nSource: *[jdt.ls-java-project](file:///foo/Bar.java#3)*";
        assert_eq!(
            sanitize_hover_documentation(doc).as_deref(),
            Some("Some javadoc\n\nSource: *[Bar.java:3](file:///foo/Bar.java#3)*")
        );
    }

    #[test]
    fn looks_like_signature_line_ignores_jdtls_source_with_line_anchor() {
        let source = "Source: *[jdt.ls-java-project](file:///foo/Bar.java#11)*";
        assert!(!looks_like_signature_line(source, "class SpringApplication"));
    }

    #[test]
    fn parse_lsp_hover_does_not_use_jdtls_source_as_signature() {
        let text = "class SpringApplication\n\nSome javadoc\n\nSource: *[jdt.ls-java-project](file:///foo/Bar.java#11)*";
        let info = finalize_hover_info(parse_lsp_hover(text, "Foo.java"), "", 1, 1);
        assert_eq!(
            info.documentation.as_deref(),
            Some("Some javadoc\n\nSource: *[Bar.java:11](file:///foo/Bar.java#11)*")
        );
        assert!(info.signature.is_none());
    }

    #[test]
    fn normalize_hover_kind_drops_symbol() {
        assert_eq!(normalize_hover_kind("symbol"), "");
        assert_eq!(normalize_hover_kind("class"), "class");
    }

    #[test]
    fn signature_kind_name_parses_java_declarations() {
        assert_eq!(
            signature_kind_name("public class SpringApplication"),
            ("class".into(), "SpringApplication".into())
        );
        assert_eq!(
            signature_kind_name("void run(String... args)"),
            ("method".into(), "run".into())
        );
        assert_eq!(
            signature_kind_name("public interface ApplicationContext"),
            ("interface".into(), "ApplicationContext".into())
        );
    }

    #[test]
    fn class_entry_to_fqcn_from_jar_and_jdt_paths() {
        assert_eq!(
            class_entry_to_fqcn("org/springframework/boot/SpringApplication.class"),
            Some("org.springframework.boot.SpringApplication".into())
        );
        assert_eq!(
            class_entry_to_fqcn("org.springframework.boot.SpringApplication.class"),
            Some("org.springframework.boot.SpringApplication".into())
        );
    }

    #[test]
    fn fqcn_from_jdt_contents_uri() {
        assert_eq!(
            fqcn_from_lsp_uri(
                "jdt://contents/spring-boot-3.3.5.jar/org.springframework.boot.SpringApplication.class"
            ),
            Some("org.springframework.boot.SpringApplication".into())
        );
    }

    #[test]
    fn jdtls_hover_regression() {
        let Some(ws) = setup_regression_workspace() else {
            return;
        };
        let hover = find_hover(
            &ws,
            REGRESSION_REL,
            GREET_USAGE_LINE,
            GREET_USAGE_COL,
            REGRESSION_SOURCE,
        )
        .expect("hover query")
        .expect("hover result");
        assert!(
            hover.name.contains("greet")
                || hover
                    .signature
                    .as_deref()
                    .is_some_and(|s| s.contains("greet")),
            "hover should describe greet(): {:?}",
            hover
        );
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn jdtls_completions_regression() {
        let Some(ws) = setup_regression_workspace() else {
            return;
        };
        let items = find_completions(
            &ws,
            REGRESSION_REL,
            GREET_USAGE_LINE,
            GREET_USAGE_COL,
            REGRESSION_SOURCE,
        )
        .expect("completion query");
        assert!(
            items.iter().any(|i| i.label == "greet" || i.label.starts_with("greet")),
            "expected greet in jdtls completions, got: {:?}",
            items.iter().map(|i| &i.label).collect::<Vec<_>>()
        );
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn jdtls_definition_regression() {
        let Some(ws) = setup_regression_workspace() else {
            return;
        };
        let def = find_definition(
            &ws,
            REGRESSION_REL,
            GREET_USAGE_LINE,
            GREET_USAGE_COL,
            REGRESSION_SOURCE,
        )
        .expect("definition query")
        .expect("definition for greet");
        assert!(def.path.contains("App.java"));
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn jdtls_definition_with_project_java_17() {
        #[cfg(not(target_os = "macos"))]
        return;

        let jdk17 = PathBuf::from("/opt/homebrew/opt/openjdk@17/libexec/openjdk.jdk/Contents/Home");
        if !jdk17.is_dir() {
            return;
        }
        if crate::jdk::jdtls_java_home().is_err() {
            return;
        }

        std::env::set_var("REAPER_JAVA_HOME", &jdk17);
        crate::jdk::set_configured_java_home(Some(jdk17));

        let Some(ws) = setup_regression_workspace() else {
            return;
        };
        let def = find_definition(
            &ws,
            REGRESSION_REL,
            GREET_USAGE_LINE,
            GREET_USAGE_COL,
            REGRESSION_SOURCE,
        )
        .expect("definition query")
        .expect("definition for greet with project JDK 17");
        assert!(def.path.contains("App.java"));
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn jdtls_find_references_regression() {
        let Some(ws) = setup_regression_workspace() else {
            return;
        };
        let refs = find_references(
            &ws,
            REGRESSION_REL,
            GREET_USAGE_LINE,
            GREET_USAGE_COL,
            REGRESSION_SOURCE,
        )
        .expect("references query");
        assert!(
            refs.len() >= 2,
            "expected declaration + usage references, got {}",
            refs.len()
        );
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn jdtls_prepare_rename_regression() {
        let Some(ws) = setup_regression_workspace() else {
            return;
        };
        let prep = prepare_rename(
            &ws,
            REGRESSION_REL,
            GREET_USAGE_LINE,
            GREET_USAGE_COL,
            REGRESSION_SOURCE,
        )
        .expect("prepareRename query")
        .expect("prepareRename range for greet");
        assert!(prep.column > 0);
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn jdtls_rename_symbol_regression() {
        let Some(ws) = setup_regression_workspace() else {
            return;
        };
        let edits = rename_symbol(
            &ws,
            REGRESSION_REL,
            GREET_USAGE_LINE,
            GREET_USAGE_COL,
            REGRESSION_SOURCE,
            "hello",
        )
        .expect("rename query");
        assert!(!edits.is_empty(), "rename should produce workspace edits");
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn jdtls_signature_help_regression() {
        let Some(ws) = setup_regression_workspace() else {
            return;
        };
        let help = signature_help(&ws, REGRESSION_REL, 14, 8, REGRESSION_SOURCE)
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
    fn jdtls_peek_definition_regression() {
        let Some(ws) = setup_regression_workspace() else {
            return;
        };
        let def = find_definition(
            &ws,
            REGRESSION_REL,
            GREET_USAGE_LINE,
            GREET_USAGE_COL,
            REGRESSION_SOURCE,
        )
        .expect("peek definition query")
        .expect("definition location for peek");
        assert!(super::super::classpath::definition_path_is_openable(&ws, &def.path));
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn jdtls_organize_imports_regression() {
        let Some(ws) = setup_regression_workspace() else {
            return;
        };
        let actions = code_actions(
            &ws,
            REGRESSION_REL,
            3,
            8,
            REGRESSION_SOURCE,
            &["source.organizeImports"],
        )
        .expect("codeAction query");
        assert!(
            actions.iter().any(|a| !a.edits.is_empty()),
            "organize imports should offer edits for unused java.util.List"
        );
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn workspace_project_java_release_reads_pom() {
        let Some(ws) = create_regression_workspace_dir() else {
            return;
        };
        assert_eq!(super::workspace_project_java_release(&ws), 17);
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn ensure_jdtls_compiler_prefs_writes_release() {
        let ws = std::env::temp_dir().join(format!(
            "reaper-jdtls-prefs-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&ws);
        std::fs::create_dir_all(&ws).unwrap();
        super::ensure_jdtls_compiler_prefs(&ws, 17).unwrap();
        let prefs = std::fs::read_to_string(ws.join(".settings/org.eclipse.jdt.core.prefs")).unwrap();
        assert!(prefs.contains("compiler.compliance=17"));
        assert!(prefs.contains("compiler.source=17"));
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn jdtls_runtime_settings_project_jdk_not_launcher_jdk() {
        let Some(ws) = create_regression_workspace_dir() else {
            return;
        };
        let settings = super::jdtls_runtime_settings(&ws);
        let runtimes = settings
            .get("java.configuration.runtimes")
            .and_then(|v| v.as_array())
            .expect("runtimes array");
        if runtimes.is_empty() {
            let _ = std::fs::remove_dir_all(&ws);
            return;
        }
        assert_eq!(runtimes.len(), 1, "only project JDK should be registered");
        let rt = &runtimes[0];
        assert_eq!(rt.get("name").and_then(|v| v.as_str()), Some("JavaSE-17"));
        assert_eq!(rt.get("default").and_then(|v| v.as_bool()), Some(true));
        if let Ok(launcher) = crate::jdk::jdtls_java_home() {
            let path = rt.get("path").and_then(|v| v.as_str()).unwrap();
            assert_ne!(
                PathBuf::from(path),
                launcher,
                "JDK 21 launcher must not be the project runtime"
            );
        }
        let _ = std::fs::remove_dir_all(&ws);
    }
}
