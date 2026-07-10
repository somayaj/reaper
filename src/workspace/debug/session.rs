use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use axum::extract::ws::{Message, WebSocket};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::sync::broadcast;

use super::adapters::{self, LaunchPlan};
use super::dap::{self, DapClient};
use super::types::{
    DebugBreakpoint, DebugEvent, DebugState, DebugStatus, DebugVariable, StackFrame,
};
use crate::process_registry;
use crate::workspace;

const REQ_TIMEOUT: Duration = Duration::from_secs(30);
const JAVA_REQ_TIMEOUT: Duration = Duration::from_secs(120);

struct DebugSession {
    ws_path: PathBuf,
    state: DebugState,
    breakpoints: Vec<DebugBreakpoint>,
    client: Option<DapClient>,
    _event_thread: Option<thread::JoinHandle<()>>,
    _process_guard: Option<process_registry::ProcessGuard>,
    broadcast: broadcast::Sender<DebugEvent>,
}

impl DebugSession {
    fn new(ws_path: PathBuf) -> Self {
        let (tx, _) = broadcast::channel(128);
        Self {
            ws_path,
            state: DebugState::default(),
            breakpoints: Vec::new(),
            client: None,
            _event_thread: None,
            _process_guard: None,
            broadcast: tx,
        }
    }

    fn emit(&self, event: DebugEvent) {
        let _ = self.broadcast.send(event);
    }

    fn set_state(&mut self, status: DebugStatus, message: Option<String>) {
        self.state.status = status;
        self.state.message = message;
        self.emit(DebugEvent::State {
            state: self.state.clone(),
        });
    }

    fn publish_state(&self) {
        self.emit(DebugEvent::State {
            state: self.state.clone(),
        });
    }
}

fn sessions() -> &'static Mutex<HashMap<String, Arc<Mutex<DebugSession>>>> {
    static SESSIONS: OnceLock<Mutex<HashMap<String, Arc<Mutex<DebugSession>>>>> = OnceLock::new();
    SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn session_key(ws: &Path) -> String {
    ws.to_string_lossy().to_string()
}

fn get_session(ws: &Path) -> Arc<Mutex<DebugSession>> {
    let key = session_key(ws);
    let mut map = sessions().lock().unwrap();
    map.entry(key)
        .or_insert_with(|| Arc::new(Mutex::new(DebugSession::new(ws.to_path_buf()))))
        .clone()
}

pub fn debug_state(ws: &Path) -> DebugState {
    let session = get_session(ws);
    let guard = session.lock().unwrap();
    guard.state.clone()
}

pub fn set_breakpoints(ws: &Path, breakpoints: Vec<DebugBreakpoint>) -> Result<DebugState> {
    let session = get_session(ws);
    let mut guard = session.lock().unwrap();
    guard.breakpoints = breakpoints.clone();
    guard.state.breakpoints = breakpoints;
    let bps = guard.breakpoints.clone();
    if let Some(client) = guard.client.as_mut() {
        sync_breakpoints_to_adapter(client, ws, &bps)?;
    }
    guard.publish_state();
    Ok(guard.state.clone())
}

pub fn start_debug(
    ws: &Path,
    rel_path: &str,
    content: Option<&str>,
    line: u32,
) -> Result<DebugState> {
    let rel_path = workspace::normalize_workspace_source_path(rel_path);
    let session = get_session(ws);
    let mut guard = session.lock().unwrap();
    stop_session_inner(&mut guard)?;

    let ctx = workspace::run_context(ws, &rel_path, content, line.max(1), None, None)?;
    let bp_pairs: Vec<(String, u32)> = guard
        .breakpoints
        .iter()
        .map(|b| (b.path.clone(), b.line))
        .collect();
    let plan = adapters::build_launch_plan(
        ws,
        &rel_path,
        &ctx,
        ctx.target.as_ref(),
        &bp_pairs,
        true,
    )?;

    guard.state = DebugState {
        status: DebugStatus::Starting,
        language: Some(plan.language.clone()),
        adapter: Some(plan.adapter.label.clone()),
        breakpoints: guard.breakpoints.clone(),
        ..DebugState::default()
    };
    guard.publish_state();

    for cmd in &plan.pre_commands {
        let prebuild_cwd = plan.prebuild_cwd.as_deref().unwrap_or(ws);
        run_prebuild(prebuild_cwd, cmd)?;
    }

    let (mut client, event_rx) = if plan.use_jdtls_java {
        let port = super::super::jdtls::start_java_debug_port(ws)?;
        dap::DapClient::connect_tcp("127.0.0.1", port)?
    } else {
        let adapter_cwd = plan
            .adapter
            .cwd
            .as_deref()
            .unwrap_or(ws);
        let adapter_env: Vec<(String, String)> = plan
            .adapter
            .env
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        DapClient::spawn(
            &plan.adapter.command,
            &plan.adapter.args,
            adapter_cwd,
            &adapter_env,
        )?
    };
    let broadcast = guard.broadcast.clone();
    let ws_path = ws.to_path_buf();
    let breakpoints = guard.breakpoints.clone();

    initialize_adapter(&mut client, &plan)?;

    // Register client before launch so early `stopped` events (stopOnEntry) are not dropped.
    guard.client = Some(client);
    let event_thread = thread::spawn(move || {
        event_loop(event_rx, ws_path, broadcast);
    });
    guard._event_thread = Some(event_thread);

    let launch_timeout = if plan.use_jdtls_java {
        JAVA_REQ_TIMEOUT
    } else {
        REQ_TIMEOUT
    };
    let stop_on_entry = plan
        .launch
        .get("stopOnEntry")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let client = guard.client.as_mut().context("debug client missing")?;
    tracing::debug!("sending launch request with config: {}", plan.launch);
    let launch_resp = client.request("launch", plan.launch.clone(), launch_timeout)?;
    tracing::debug!("launch response: {}", launch_resp);
    dap::response_success(&launch_resp)?;
    sync_breakpoints_to_adapter(client, ws, &breakpoints)?;
    let done = client.request("configurationDone", json!({}), REQ_TIMEOUT)?;
    dap::response_success(&done)?;

    if let Some(child) = guard
        .client
        .as_mut()
        .and_then(|c| c.child.as_mut())
    {
        guard._process_guard =
            Some(process_registry::guard_for_child(child, "debug-adapter"));
    }

    if stop_on_entry {
        try_sync_stopped_state(&mut guard)?;
    }
    if guard.state.status != DebugStatus::Stopped {
        guard.set_state(DebugStatus::Running, None);
    }
    Ok(guard.state.clone())
}

pub fn continue_debug(ws: &Path) -> Result<DebugState> {
    let session = get_session(ws);
    let mut guard = session.lock().unwrap();
    let thread_id = guard.state.thread_id.unwrap_or(1);
    let client = guard.client.as_mut().context("no active debug session")?;
    client.request(
        "continue",
        json!({ "threadId": thread_id }),
        REQ_TIMEOUT,
    )?;
    guard.state.status = DebugStatus::Running;
    guard.state.frames.clear();
    guard.state.variables.clear();
    guard.publish_state();
    Ok(guard.state.clone())
}

pub fn step_debug(ws: &Path, kind: &str) -> Result<DebugState> {
    let session = get_session(ws);
    let mut guard = session.lock().unwrap();
    let thread_id = guard.state.thread_id.unwrap_or(1);
    let command = match kind {
        "in" => "stepIn",
        "out" => "stepOut",
        _ => "next",
    };
    let client = guard.client.as_mut().context("no active debug session")?;
    client.request(
        command,
        json!({ "threadId": thread_id }),
        REQ_TIMEOUT,
    )?;
    guard.state.status = DebugStatus::Running;
    guard.publish_state();
    Ok(guard.state.clone())
}

pub fn evaluate_watch(ws: &Path, expression: &str, frame_id: Option<i64>) -> Result<String> {
    let session = get_session(ws);
    let mut guard = session.lock().unwrap();
    let frame = frame_id.or_else(|| guard.state.frames.first().map(|f| f.id));
    let client = guard.client.as_mut().context("no active debug session")?;
    let resp = client.request(
        "evaluate",
        json!({
            "expression": expression,
            "frameId": frame,
            "context": "watch",
        }),
        REQ_TIMEOUT,
    )?;
    let body = dap::response_body(&resp)?;
    Ok(body
        .get("result")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string())
}

pub fn stop_debug(ws: &Path) -> Result<DebugState> {
    let session = get_session(ws);
    let mut guard = session.lock().unwrap();
    stop_session_inner(&mut guard)?;
    guard.set_state(DebugStatus::Idle, None);
    Ok(guard.state.clone())
}

pub fn debug_capabilities(ws: &Path, rel_path: &str, line: u32) -> Result<super::types::DebugCapabilities> {
    adapters::debug_capabilities(ws, rel_path, line)
}

pub async fn run_debug_websocket(socket: WebSocket, ws: &Path) -> Result<()> {
    let session = get_session(ws);
    let mut rx = {
        let guard = session.lock().unwrap();
        guard.broadcast.subscribe()
    };
    let (mut sink, mut stream) = socket.split();
    let init = {
        let guard = session.lock().unwrap();
        DebugEvent::State {
            state: guard.state.clone(),
        }
    };
    sink.send(Message::Text(serde_json::to_string(&init)?.into()))
        .await?;

    let send_task = tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(ev) => {
                    let text = match serde_json::to_string(&ev) {
                        Ok(t) => t,
                        Err(_) => continue,
                    };
                    if sink.send(Message::Text(text.into())).await.is_err() {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    while let Some(msg) = stream.next().await {
        match msg {
            Ok(Message::Close(_)) | Err(_) => break,
            _ => {}
        }
    }
    send_task.abort();
    Ok(())
}

fn stop_session_inner(guard: &mut DebugSession) -> Result<()> {
    if let Some(mut client) = guard.client.take() {
        let _ = client.request("disconnect", json!({ "terminateDebuggee": true }), Duration::from_secs(5));
        client.kill();
    }
    guard._event_thread = None;
    guard._process_guard = None;
    guard.state.thread_id = None;
    guard.state.stop_reason = None;
    guard.state.frames.clear();
    guard.state.variables.clear();
    Ok(())
}

fn run_prebuild(cwd: &Path, command: &str) -> Result<()> {
    use std::process::Command;
    let status = Command::new("bash")
        .arg("-lc")
        .arg(command)
        .current_dir(cwd)
        .status()
        .with_context(|| format!("prebuild in {}: {command}", cwd.display()))?;
    if !status.success() {
        bail!("prebuild failed: {command}");
    }
    Ok(())
}

fn initialize_adapter(client: &mut DapClient, plan: &LaunchPlan) -> Result<()> {
    let adapter_id = plan
        .launch
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("reaper");
    let init = client.request(
        "initialize",
        json!({
            "clientID": "reaper",
            "clientName": "Reaper",
            "adapterID": adapter_id,
            "pathFormat": "path",
            "linesStartAt1": true,
            "columnsStartAt1": true,
            "supportsVariableType": true,
            "supportsVariablePaging": false,
            "supportsRunInTerminalRequest": false,
        }),
        REQ_TIMEOUT,
    )?;
    dap::response_body(&init)?;
    Ok(())
}

fn sync_breakpoints_to_adapter(
    client: &mut DapClient,
    ws: &Path,
    breakpoints: &[DebugBreakpoint],
) -> Result<()> {
    let mut by_path: HashMap<String, Vec<&DebugBreakpoint>> = HashMap::new();
    for bp in breakpoints {
        by_path.entry(bp.path.clone()).or_default().push(bp);
    }
    if by_path.is_empty() {
        return Ok(());
    }
    for (path, bps) in by_path {
        let abs = ws.join(&path);
        let source = json!({ "path": abs.display().to_string() });
        let lines: Vec<Value> = bps
            .iter()
            .map(|b| {
                let mut v = json!({ "line": b.line });
                if let Some(cond) = &b.condition {
                    v["condition"] = json!(cond);
                }
                v
            })
            .collect();
        let resp = client.request(
            "setBreakpoints",
            json!({
                "source": source,
                "breakpoints": lines,
            }),
            REQ_TIMEOUT,
        )?;
        dap::response_body(&resp)?;
    }
    Ok(())
}

fn event_loop(
    event_rx: std::sync::mpsc::Receiver<Value>,
    ws_path: PathBuf,
    broadcast: broadcast::Sender<DebugEvent>,
) {
    while let Ok(msg) = event_rx.recv() {
        let event_name = msg.get("event").and_then(|v| v.as_str()).unwrap_or("");
        let body = msg.get("body").cloned().unwrap_or(json!({}));
        match event_name {
            "output" => {
                let category = body
                    .get("category")
                    .and_then(|v| v.as_str())
                    .unwrap_or("stdout")
                    .to_string();
                let text = body
                    .get("output")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let _ = broadcast.send(DebugEvent::Output { category, text });
            }
            "stopped" => {
                handle_stopped(&ws_path, &body, &broadcast);
            }
            "terminated" => {
                if let Ok(mut guard) = get_session(&ws_path).lock() {
                    guard.client = None;
                    guard.state.status = DebugStatus::Terminated;
                    guard.state.frames.clear();
                    guard.state.variables.clear();
                    let _ = broadcast.send(DebugEvent::State {
                        state: guard.state.clone(),
                    });
                }
            }
            _ => {}
        }
    }
}

fn try_sync_stopped_state(guard: &mut DebugSession) -> Result<()> {
    if guard.state.status == DebugStatus::Stopped {
        return Ok(());
    }
    let Some(client) = guard.client.as_mut() else {
        return Ok(());
    };
    let threads_resp = match client.request("threads", json!({}), REQ_TIMEOUT) {
        Ok(r) => r,
        Err(e) => {
            tracing::debug!("debug threads probe failed: {e:#}");
            return Ok(());
        }
    };
    let body = match dap::response_body(&threads_resp) {
        Ok(b) => b,
        Err(e) => {
            tracing::debug!("debug threads response invalid: {e:#}");
            return Ok(());
        }
    };
    let Some(threads) = body.get("threads").and_then(|v| v.as_array()) else {
        return Ok(());
    };
    for th in threads {
        let thread_id = th.get("id").and_then(|v| v.as_i64()).unwrap_or(1);
        let Ok(frames) = fetch_stack(client, thread_id) else {
            continue;
        };
        if frames.is_empty() {
            continue;
        }
        let variables = frames
            .first()
            .map(|f| f.id)
            .and_then(|frame_id| fetch_scopes(client, frame_id).ok())
            .unwrap_or_default();
        guard.state.status = DebugStatus::Stopped;
        guard.state.thread_id = Some(thread_id);
        guard.state.stop_reason = Some("entry".into());
        guard.state.frames = frames;
        guard.state.variables = variables;
        guard.publish_state();
        return Ok(());
    }
    Ok(())
}

fn handle_stopped(ws: &Path, body: &Value, broadcast: &broadcast::Sender<DebugEvent>) {
    let reason = body
        .get("reason")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();
    let thread_id = body.get("threadId").and_then(|v| v.as_i64()).unwrap_or(1);

    let session = get_session(ws);
    let snapshot = {
        let mut guard = session.lock().unwrap();
        let Some(client) = guard.client.as_mut() else {
            return;
        };
        let frames = fetch_stack(client, thread_id).unwrap_or_default();
        let variables = frames
            .first()
            .map(|f| f.id)
            .and_then(|frame_id| fetch_scopes(client, frame_id).ok())
            .unwrap_or_default();
        guard.state.status = DebugStatus::Stopped;
        guard.state.thread_id = Some(thread_id);
        guard.state.stop_reason = Some(reason);
        guard.state.frames = frames;
        guard.state.variables = variables;
        guard.state.clone()
    };
    let _ = broadcast.send(DebugEvent::State { state: snapshot });
}

fn fetch_stack(client: &mut DapClient, thread_id: i64) -> Result<Vec<StackFrame>> {
    let resp = client.request(
        "stackTrace",
        json!({
            "threadId": thread_id,
            "startFrame": 0,
            "levels": 50,
        }),
        REQ_TIMEOUT,
    )?;
    let body = dap::response_body(&resp)?;
    let frames = body
        .get("stackFrames")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    Ok(frames
        .iter()
        .filter_map(parse_stack_frame)
        .collect())
}

fn fetch_scopes(client: &mut DapClient, frame_id: i64) -> Result<Vec<DebugVariable>> {
    let resp = client.request(
        "scopes",
        json!({ "frameId": frame_id }),
        REQ_TIMEOUT,
    )?;
    let body = dap::response_body(&resp)?;
    let scopes = body
        .get("scopes")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let mut out = Vec::new();
    for scope in scopes {
        let name = scope
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("scope")
            .to_string();
        let variables_ref = scope.get("variablesReference").and_then(|v| v.as_i64()).unwrap_or(0);
        if variables_ref > 0 {
            if let Ok(vars) = fetch_variables(client, variables_ref) {
                for mut v in vars {
                    if v.name.is_empty() {
                        v.name = name.clone();
                    }
                    out.push(v);
                }
            }
        } else {
            out.push(DebugVariable {
                name,
                value: String::new(),
                type_name: None,
                variables_reference: 0,
            });
        }
    }
    Ok(out)
}

fn fetch_variables(client: &mut DapClient, variables_ref: i64) -> Result<Vec<DebugVariable>> {
    let resp = client.request(
        "variables",
        json!({ "variablesReference": variables_ref }),
        REQ_TIMEOUT,
    )?;
    let body = dap::response_body(&resp)?;
    let vars = body
        .get("variables")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    Ok(vars
        .iter()
        .filter_map(|v| {
            Some(DebugVariable {
                name: v.get("name")?.as_str()?.to_string(),
                value: v.get("value")?.as_str()?.to_string(),
                type_name: v
                    .get("type")
                    .and_then(|t| t.as_str())
                    .map(str::to_string),
                variables_reference: v
                    .get("variablesReference")
                    .and_then(|r| r.as_i64())
                    .unwrap_or(0),
            })
        })
        .collect())
}

fn parse_stack_frame(v: &Value) -> Option<StackFrame> {
    let id = v.get("id")?.as_i64()?;
    let name = v.get("name")?.as_str()?.to_string();
    let source = v.get("source")?;
    let path = source
        .get("path")
        .and_then(|p| p.as_str())
        .or_else(|| source.get("name").and_then(|p| p.as_str()))
        .map(str::to_string);
    let line = v.get("line").and_then(|l| l.as_u64()).map(|l| l as u32);
    let column = v.get("column").and_then(|c| c.as_u64()).map(|c| c as u32);
    Some(StackFrame {
        id,
        name,
        path,
        line,
        column,
    })
}
