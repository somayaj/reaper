use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

use super::symbols::SymbolLocation;

const LSP_TIMEOUT: Duration = Duration::from_secs(20);
const SESSION_IDLE: Duration = Duration::from_secs(120);

static SESSIONS: LazyLock<Mutex<HashMap<PathBuf, SolargraphSession>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static NEXT_ID: AtomicU64 = AtomicU64::new(10);

struct SolargraphSession {
    child: Child,
    last_used: Instant,
}

pub fn find_definition(
    ws: &Path,
    rel_path: &str,
    line: u32,
    column: u32,
    content: &str,
) -> Result<Option<SymbolLocation>> {
    if !rel_path.to_lowercase().ends_with(".rb") {
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

    let response = match query_definition(&ws, &uri, line, column, content, deadline) {
        Ok(r) => r,
        Err(e) => {
            tracing::debug!("solargraph definition failed: {e:#}");
            return Ok(None);
        }
    };

    parse_definition_response(&ws, rel_path, &response)
}

fn query_definition(
    ws: &Path,
    uri: &str,
    line: u32,
    column: u32,
    content: &str,
    deadline: Instant,
) -> Result<Value> {
    let mut map = SESSIONS.lock().expect("solargraph sessions");
    purge_stale_sessions(&mut map);

    if !session_alive(map.get_mut(ws)) {
        map.remove(ws);
        let mut child = spawn_solargraph(ws)?;
        let root_uri = file_uri(ws)?;
        initialize_session(&mut child, &root_uri, deadline)?;
        map.insert(
            ws.to_path_buf(),
            SolargraphSession {
                child,
                last_used: Instant::now(),
            },
        );
    }

    let session = map.get_mut(ws).context("solargraph session")?;
    session.last_used = Instant::now();
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);

    let stdin = session.child.stdin.as_mut().context("solargraph stdin")?;
    let stdout = session.child.stdout.as_mut().context("solargraph stdout")?;

    write_message(
        stdin,
        &json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": {
                "textDocument": {
                    "uri": uri,
                    "languageId": "ruby",
                    "version": id as i64,
                    "text": content
                }
            }
        }),
    )?;

    write_message(
        stdin,
        &json!({
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
    )?;

    wait_for_id(stdout, id, deadline)
}

fn session_alive(session: Option<&mut SolargraphSession>) -> bool {
    let Some(session) = session else {
        return false;
    };
    if session.last_used.elapsed() > SESSION_IDLE {
        return false;
    }
    match session.child.try_wait() {
        Ok(None) => true,
        _ => false,
    }
}

fn purge_stale_sessions(map: &mut HashMap<PathBuf, SolargraphSession>) {
    map.retain(|_, session| {
        session.last_used.elapsed() <= SESSION_IDLE
            && matches!(session.child.try_wait(), Ok(None))
    });
}

fn initialize_session(child: &mut Child, root_uri: &str, deadline: Instant) -> Result<()> {
    let stdin = child.stdin.as_mut().context("solargraph stdin")?;
    let stdout = child.stdout.as_mut().context("solargraph stdout")?;

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
                        "definition": { "linkSupport": true }
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

fn spawn_solargraph(ws: &Path) -> Result<Child> {
    let use_bundle = ws.join("Gemfile").is_file();
    let script = if use_bundle {
        "bundle exec solargraph stdio 2>/dev/null"
    } else {
        "solargraph stdio 2>/dev/null"
    };

    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".into());
    let mut cmd = Command::new(&shell);
    cmd.arg("-lc").arg(script).current_dir(ws);
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());

    cmd.spawn()
        .with_context(|| "failed to start solargraph (install: gem install solargraph)")
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
            bail!("solargraph timed out");
        }
        let msg = read_message(r)?;
        if msg.get("id").and_then(|v| v.as_u64()) == Some(id) {
            if let Some(err) = msg.get("error") {
                bail!("solargraph error: {err}");
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

fn parse_definition_response(
    ws: &Path,
    from_path: &str,
    result: &Value,
) -> Result<Option<SymbolLocation>> {
    if result.is_null() {
        return Ok(None);
    }

    let loc = if result.is_array() {
        result.get(0)
    } else {
        Some(result)
    };

    let Some(loc) = loc else {
        return Ok(None);
    };

    let uri = loc
        .get("targetUri")
        .or_else(|| loc.get("uri"))
        .and_then(|v| v.as_str());
    let range = loc
        .get("targetSelectionRange")
        .or_else(|| loc.get("targetRange"))
        .or_else(|| loc.get("range"));

    let Some(uri) = uri else {
        return Ok(None);
    };

    let (line, column) = range
        .and_then(|r| r.get("start"))
        .map(|start| {
            (
                start.get("line").and_then(|v| v.as_u64()).unwrap_or(0) as u32 + 1,
                start.get("character").and_then(|v| v.as_u64()).unwrap_or(0) as u32 + 1,
            )
        })
        .unwrap_or((1, 1));

    let path = uri_to_path(ws, uri)?;
    let name = Path::new(from_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("symbol")
        .to_string();

    Ok(Some(SymbolLocation {
        name,
        kind: "definition".into(),
        path,
        line,
        column,
    }))
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
    let ws_canon = ws.canonicalize().unwrap_or_else(|_| ws.to_path_buf());
    let path_canon = path.canonicalize().unwrap_or(path);
    if let Ok(rel) = path_canon.strip_prefix(&ws_canon) {
        return Ok(rel.to_string_lossy().replace('\\', "/"));
    }
    Ok(path_canon.to_string_lossy().replace('\\', "/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uri_roundtrip_inside_workspace() {
        let ws = std::env::temp_dir().join("reaper-solargraph-uri");
        let _ = std::fs::remove_dir_all(&ws);
        std::fs::create_dir_all(&ws).unwrap();
        let file = ws.join("app/models/user.rb");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, "class User\nend\n").unwrap();

        let ws = ws.canonicalize().unwrap();
        let uri = file_uri(&file).unwrap();
        let rel = uri_to_path(&ws, &uri).unwrap();
        assert_eq!(rel, "app/models/user.rb");
        let _ = std::fs::remove_dir_all(&ws);
    }
}
