use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

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
/// Java DAP launch must stay well under the frontend's 540s `/debug/start` budget.
const JAVA_REQ_TIMEOUT: Duration = Duration::from_secs(30);
/// Gradle/Maven multi-module compiles can take several minutes on cold daemons.
const PREBUILD_TIMEOUT: Duration = Duration::from_secs(500);

struct DebugSession {
    ws_path: PathBuf,
    state: DebugState,
    breakpoints: Vec<DebugBreakpoint>,
    /// Separate mutex so DAP I/O (stackTrace/scopes) does not block step/continue.
    client: Option<Arc<Mutex<DapClient>>>,
    /// When the last DAP session ended — used so a second start still waits for jdtls
    /// even after `/stop` cleared status back to Idle.
    last_ended_at: Option<std::time::Instant>,
    /// True if the last session used the jdtls Java DAP (needs longer port cooldown).
    last_was_java: bool,
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
            last_ended_at: None,
            last_was_java: false,
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
    let mut normalized: Vec<DebugBreakpoint> = breakpoints
        .into_iter()
        .map(|bp| DebugBreakpoint {
            path: workspace::normalize_workspace_source_path(&bp.path),
            line: bp.line,
            condition: bp.condition,
        })
        .collect();
    // Collapse overlay + real-path duplicates (same file, same line).
    normalized.sort_by(|a, b| (&a.path, a.line).cmp(&(&b.path, b.line)));
    normalized.dedup_by(|a, b| a.path == b.path && a.line == b.line);

    let mut guard = session.lock().unwrap();
    guard.breakpoints = normalized.clone();
    guard.state.breakpoints = normalized;
    let bps = guard.breakpoints.clone();
    let client = guard.client.clone();
    drop(guard);
    if let Some(client) = client {
        let mut c = client.lock().unwrap();
        sync_breakpoints_to_adapter(&mut c, ws, &bps)?;
    }
    let guard = session.lock().unwrap();
    guard.publish_state();
    Ok(guard.state.clone())
}

fn connect_java_dap(ws: &Path) -> Result<(DapClient, std::sync::mpsc::Receiver<Value>)> {
    // Each failed attempt requests a *new* jdtls debug port — reusing a dead port
    // is what makes the second Debug click fail after terminate.
    let mut last_err = None;
    for attempt in 0..4 {
        if attempt > 0 {
            thread::sleep(Duration::from_millis(350 * attempt as u64));
        }
        let port = match super::super::jdtls::start_java_debug_port(ws) {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!("java debug port attempt {} failed: {e:#}", attempt + 1);
                last_err = Some(e);
                continue;
            }
        };
        match dap::DapClient::connect_tcp("127.0.0.1", port) {
            Ok(c) => return Ok(c),
            Err(e) => {
                tracing::warn!("java DAP connect {port} attempt {} failed: {e:#}", attempt + 1);
                last_err = Some(e);
                // Brief pause then ask jdtls for a fresh port on the next loop.
                thread::sleep(Duration::from_millis(200));
            }
        }
    }
    Err(last_err.unwrap_or_else(|| anyhow::anyhow!("failed to start Java debug adapter")))
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
    // Include Terminated — natural exit still leaves jdtls DAP cleaning up.
    // Also honor last_ended_at: frontend /stop clears status to Idle before /start,
    // which used to skip the cooldown and break the second Debug click.
    let recent_end = guard
        .last_ended_at
        .map(|t| t.elapsed() < Duration::from_secs(3))
        .unwrap_or(false);
    let had_session =
        needs_restart_cooldown(&guard.state.status, guard.client.is_some()) || recent_end;
    let was_java = guard.last_was_java;
    // Extract client without locking it under the session mutex (avoids deadlock with
    // handle_stopped, which locks client then session).
    let old_client = take_client_for_shutdown(&mut guard);
    let breakpoints = guard.breakpoints.clone();
    drop(guard);
    shutdown_client(old_client);
    if had_session {
        // jdtls Java DAP needs longer than stdio adapters to free the listen port.
        let wait_ms = if was_java || recent_end { 1800 } else { 400 };
        tracing::debug!("debug restart cooldown {wait_ms}ms");
        thread::sleep(Duration::from_millis(wait_ms));
    }

    let ctx = workspace::run_context(ws, &rel_path, content, line.max(1), None, None, None)?;
    let bp_pairs: Vec<(String, u32)> = breakpoints
        .iter()
        .map(|b| (b.path.clone(), b.line))
        .collect();
    // Defer jdtls classpath resolve until after Maven/Gradle prebuild so classes exist
    // and we don't burn the start budget on buildWorkspace before compile.
    let mut plan = adapters::build_launch_plan(
        ws,
        &rel_path,
        &ctx,
        ctx.target.as_ref(),
        &bp_pairs,
        false,
    )?;

    {
        let mut guard = session.lock().unwrap();
        guard.state = DebugState {
            status: DebugStatus::Starting,
            language: Some(plan.language.clone()),
            adapter: Some(plan.adapter.label.clone()),
            breakpoints: breakpoints.clone(),
            ..DebugState::default()
        };
        guard.last_was_java = plan.use_jdtls_java;
        guard.publish_state();
    }

    // Prebuild + adapter spawn without holding the session lock.
    for cmd in &plan.pre_commands {
        let prebuild_cwd = plan.prebuild_cwd.as_deref().unwrap_or(ws);
        tracing::info!("debug start: prebuild in {}", prebuild_cwd.display());
        run_prebuild(prebuild_cwd, cmd)?;
    }

    if plan.use_jdtls_java {
        tracing::info!("debug start: resolving Java launch via jdtls");
        adapters::finalize_java_launch_plan(&mut plan, ws, &rel_path)?;
    }
    adapters::resolve_launch_program_after_prebuild(&mut plan, ws, &rel_path)?;

    tracing::info!(
        "debug start: connecting adapter (java={})",
        plan.use_jdtls_java
    );
    let (mut client, event_rx) = if plan.use_jdtls_java {
        connect_java_dap(ws)?
    } else {
        let adapter_cwd = plan.adapter.cwd.as_deref().unwrap_or(ws);
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

    initialize_adapter(&mut client, &plan)?;

    let broadcast = {
        let guard = session.lock().unwrap();
        guard.broadcast.clone()
    };
    let ws_path = ws.to_path_buf();
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
    let launch_args = plan.launch.clone();

    // Register client before launch so early `stopped` events are not dropped.
    {
        let mut guard = session.lock().unwrap();
        guard.client = Some(Arc::new(Mutex::new(client)));
        let event_thread = thread::spawn(move || {
            event_loop(event_rx, ws_path, broadcast);
        });
        guard._event_thread = Some(event_thread);
    }

    // Launch without holding the session lock across DAP waits.
    {
        let client = {
            let guard = session.lock().unwrap();
            guard.client.clone().context("debug client missing")?
        };
        let mut c = client.lock().unwrap();
        // CodeLLDB (and several other adapters) emit `initialized` after `launch`
        // and only answer `launch` once `configurationDone` is sent. Waiting on
        // launch first leaves the UI stuck on Starting….
        tracing::info!("debug start: sending launch");
        let (launch_seq, launch_rx) = c.begin_request("launch", launch_args)?;
        sync_breakpoints_to_adapter(&mut c, ws, &breakpoints)?;
        let done = c.request("configurationDone", json!({}), REQ_TIMEOUT)?;
        dap::response_success(&done)?;
        let launch_resp = c.await_response(launch_seq, launch_rx, "launch", launch_timeout)?;
        tracing::debug!("launch response: {}", launch_resp);
        dap::response_success(&launch_resp)?;
        tracing::info!("debug start: configurationDone ok");
        let process_guard = c
            .child
            .as_mut()
            .map(|child| process_registry::guard_for_child(child, "debug-adapter"));
        drop(c);
        if let Some(pg) = process_guard {
            let mut guard = session.lock().unwrap();
            guard._process_guard = Some(pg);
        }
    }

    // Wait for stopOnEntry / breakpoint — poll state only (event loop owns DAP fetches).
    if stop_on_entry || !breakpoints.is_empty() {
        for _ in 0..50 {
            {
                let guard = session.lock().unwrap();
                if guard.state.status == DebugStatus::Stopped {
                    return Ok(guard.state.clone());
                }
                if matches!(
                    guard.state.status,
                    DebugStatus::Terminated | DebugStatus::Idle
                ) {
                    return Ok(guard.state.clone());
                }
            }
            thread::sleep(Duration::from_millis(40));
        }
        // One late probe if the stopped event raced past us.
        let _ = try_sync_stopped_state(&session);
        let guard = session.lock().unwrap();
        if guard.state.status == DebugStatus::Stopped {
            return Ok(guard.state.clone());
        }
    }

    let mut guard = session.lock().unwrap();
    if guard.state.status != DebugStatus::Stopped {
        guard.set_state(DebugStatus::Running, None);
    }
    Ok(guard.state.clone())
}

fn dap_step_command(kind: &str) -> &'static str {
    match kind {
        "in" => "stepIn",
        "out" => "stepOut",
        _ => "next",
    }
}

pub fn continue_debug(ws: &Path) -> Result<DebugState> {
    let session = get_session(ws);
    let (client, thread_id) = {
        let guard = session.lock().unwrap();
        (
            guard.client.clone().context("no active debug session")?,
            guard.state.thread_id.unwrap_or(1),
        )
    };
    {
        let mut c = client.lock().unwrap();
        c.send_fire_and_forget("continue", json!({ "threadId": thread_id }))?;
    }
    let mut guard = session.lock().unwrap();
    guard.state.status = DebugStatus::Running;
    guard.state.frames.clear();
    guard.state.variables.clear();
    let state = guard.state.clone();
    guard.publish_state();
    Ok(state)
}

pub fn step_debug(ws: &Path, kind: &str) -> Result<DebugState> {
    let session = get_session(ws);
    let command = dap_step_command(kind);
    let (client, thread_id) = {
        let guard = session.lock().unwrap();
        (
            guard.client.clone().context("no active debug session")?,
            guard.state.thread_id.unwrap_or(1),
        )
    };
    tracing::debug!("step_debug: sending {} for thread {}", command, thread_id);
    {
        let mut c = client.lock().unwrap();
        c.send_fire_and_forget(
            command,
            json!({
                "threadId": thread_id,
                "granularity": "line",
            }),
        )?;
    }
    tracing::debug!("step_debug: {} sent", command);
    let mut guard = session.lock().unwrap();
    guard.state.status = DebugStatus::Running;
    guard.state.frames.clear();
    guard.state.variables.clear();
    let state = guard.state.clone();
    guard.publish_state();
    Ok(state)
}

pub fn evaluate_watch(ws: &Path, expression: &str, frame_id: Option<i64>) -> Result<String> {
    evaluate_expression(ws, expression, frame_id, "watch")
}

pub fn evaluate_hover(ws: &Path, expression: &str, frame_id: Option<i64>) -> Result<String> {
    evaluate_expression(ws, expression, frame_id, "hover")
}

fn evaluate_expression(
    ws: &Path,
    expression: &str,
    frame_id: Option<i64>,
    context: &str,
) -> Result<String> {
    let session = get_session(ws);
    let (client, frame, status, language) = {
        let guard = session.lock().unwrap();
        (
            guard.client.clone().context("no active debug session")?,
            frame_id.or_else(|| guard.state.frames.first().map(|f| f.id)),
            guard.state.status.clone(),
            guard.state.language.clone(),
        )
    };
    if status != DebugStatus::Stopped {
        bail!("debugger is not paused");
    }
    let mut args = json!({
        "expression": expression,
        "context": context,
    });
    if let Some(fid) = frame {
        args["frameId"] = json!(fid);
    }
    let mut c = client.lock().unwrap();
    let resp = c.request("evaluate", args, REQ_TIMEOUT)?;
    if resp.get("success").and_then(|v| v.as_bool()) == Some(false) {
        let msg = resp
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("evaluate failed");
        bail!("{msg}");
    }
    let body = resp.get("body").cloned().unwrap_or(json!({}));
    let raw = body
        .get("result")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    Ok(pretty_java_debug_value(
        &mut c,
        expression,
        &raw,
        frame,
        language.as_deref(),
    ))
}

/// Java DAP often returns `String[0]@8` for arrays — expand to `[]` / `Arrays.toString`.
fn pretty_java_debug_value(
    client: &mut DapClient,
    expression: &str,
    raw: &str,
    frame_id: Option<i64>,
    language: Option<&str>,
) -> String {
    let lang = language.unwrap_or("").to_ascii_lowercase();
    let jvm = lang.contains("java")
        || lang.contains("spring")
        || lang.contains("kotlin")
        || lang.is_empty();
    if !jvm {
        return raw.to_string();
    }
    let Some(len) = parse_java_array_ref_len(raw) else {
        return raw.to_string();
    };
    if len == 0 {
        return "[]".into();
    }
    // Cap expansion — huge arrays stay as the compact ref.
    if len > 64 {
        return format!("{raw} (len={len})");
    }
    let expr = expression.trim();
    if expr.is_empty() || expr.contains(';') {
        return raw.to_string();
    }
    let mut args = json!({
        "expression": format!("java.util.Arrays.toString({expr})"),
        "context": "watch",
    });
    if let Some(fid) = frame_id {
        args["frameId"] = json!(fid);
    }
    match client.request("evaluate", args, REQ_TIMEOUT) {
        Ok(resp) if resp.get("success").and_then(|v| v.as_bool()) != Some(false) => resp
            .get("body")
            .and_then(|b| b.get("result"))
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| raw.to_string()),
        _ => raw.to_string(),
    }
}

fn parse_java_array_ref_len(value: &str) -> Option<usize> {
    // e.g. String[0]@8  or  java.lang.String[3]@1a2b
    let trimmed = value.trim();
    let bracket = trimmed.rfind('[')?;
    let close = trimmed[bracket..].find(']')? + bracket;
    let at = trimmed.find('@')?;
    if at < close {
        return None;
    }
    trimmed[bracket + 1..close].parse().ok()
}

fn format_java_variable_value(value: &str) -> String {
    match parse_java_array_ref_len(value) {
        Some(0) => "[]".into(),
        Some(len) if len > 64 => format!("{value} (len={len})"),
        _ => value.to_string(),
    }
}

pub fn stop_debug(ws: &Path) -> Result<DebugState> {
    let session = get_session(ws);
    let client = {
        let mut guard = session.lock().unwrap();
        // Take client out without locking it while holding the session mutex
        // (handle_stopped may hold the client lock and then need the session lock).
        let client = guard.client.take();
        if client.is_some()
            || matches!(
                guard.state.status,
                DebugStatus::Running
                    | DebugStatus::Stopped
                    | DebugStatus::Starting
                    | DebugStatus::Terminated
            )
        {
            guard.last_ended_at = Some(std::time::Instant::now());
        }
        guard._event_thread = None;
        guard._process_guard = None;
        guard.state.status = DebugStatus::Idle;
        guard.state.message = None;
        guard.state.thread_id = None;
        guard.state.stop_reason = None;
        guard.state.language = None;
        guard.state.adapter = None;
        guard.state.frames.clear();
        guard.state.variables.clear();
        client
    };
    shutdown_client(client);
    let mut guard = session.lock().unwrap();
    guard.set_state(DebugStatus::Idle, None);
    Ok(guard.state.clone())
}

pub fn debug_capabilities(
    ws: &Path,
    rel_path: &str,
    line: u32,
    content: Option<&str>,
) -> Result<super::types::DebugCapabilities> {
    adapters::debug_capabilities(ws, rel_path, line, content)
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

fn take_client_for_shutdown(
    guard: &mut DebugSession,
) -> Option<Arc<Mutex<DapClient>>> {
    let client = guard.client.take();
    if client.is_some()
        || matches!(
            guard.state.status,
            DebugStatus::Running
                | DebugStatus::Stopped
                | DebugStatus::Starting
                | DebugStatus::Terminated
        )
    {
        guard.last_ended_at = Some(std::time::Instant::now());
    }
    guard._event_thread = None;
    guard._process_guard = None;
    guard.state.status = DebugStatus::Idle;
    guard.state.message = None;
    guard.state.thread_id = None;
    guard.state.stop_reason = None;
    guard.state.language = None;
    guard.state.adapter = None;
    guard.state.frames.clear();
    guard.state.variables.clear();
    client
}

fn shutdown_client(client: Option<Arc<Mutex<DapClient>>>) {
    let Some(client) = client else {
        return;
    };
    // Never block forever: handle_stopped may hold this lock for stackTrace/scopes.
    let locked = client.try_lock();
    match locked {
        Ok(mut c) => {
            let _ = c.send_fire_and_forget(
                "disconnect",
                json!({ "terminateDebuggee": true }),
            );
            thread::sleep(Duration::from_millis(50));
            c.shutdown();
        }
        Err(_) => {
            // Another thread holds the DAP lock (usually handle_stopped). Spawn a
            // short-lived closer so we don't deadlock the session mutex / HTTP handlers.
            let client2 = client.clone();
            thread::spawn(move || {
                if let Ok(mut c) = client2.lock() {
                    let _ = c.send_fire_and_forget(
                        "disconnect",
                        json!({ "terminateDebuggee": true }),
                    );
                    c.shutdown();
                }
            });
            // Give the closer a moment; don't wait on the session path.
            thread::sleep(Duration::from_millis(100));
        }
    }
}

fn stop_session_inner(guard: &mut DebugSession) -> Result<()> {
    // Used from the event loop (terminated). Take client out; shutdown without
    // holding any other locks the event loop might need.
    let client = take_client_for_shutdown(guard);
    shutdown_client(client);
    Ok(())
}

fn run_prebuild(cwd: &Path, command: &str) -> Result<()> {
    use std::process::{Command, Stdio};
    let mut child = Command::new("bash")
        .arg("-lc")
        .arg(command)
        .current_dir(cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("prebuild in {}: {command}", cwd.display()))?;
    let started = Instant::now();
    loop {
        match child
            .try_wait()
            .with_context(|| format!("prebuild wait in {}: {command}", cwd.display()))?
        {
            Some(status) => {
                let mut stdout = String::new();
                let mut stderr = String::new();
                if let Some(mut out) = child.stdout.take() {
                    use std::io::Read;
                    let _ = out.read_to_string(&mut stdout);
                }
                if let Some(mut err) = child.stderr.take() {
                    use std::io::Read;
                    let _ = err.read_to_string(&mut stderr);
                }
                if !status.success() {
                    let detail = [stderr.trim(), stdout.trim()]
                        .into_iter()
                        .find(|s| !s.is_empty())
                        .unwrap_or("(no compiler output)");
                    let clipped: String = detail.chars().take(2000).collect();
                    bail!("prebuild failed: {command}\n{clipped}");
                }
                return Ok(());
            }
            None => {
                if started.elapsed() >= PREBUILD_TIMEOUT {
                    let _ = child.kill();
                    let _ = child.wait();
                    bail!(
                        "prebuild timed out after {}s: {command}",
                        PREBUILD_TIMEOUT.as_secs()
                    );
                }
                thread::sleep(Duration::from_millis(100));
            }
        }
    }
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
                    // Full teardown so a second Debug click can open a fresh Java DAP port.
                    let _ = stop_session_inner(&mut guard);
                    guard.state.status = DebugStatus::Terminated;
                    guard.state.message = Some("Debug session ended".into());
                    let _ = broadcast.send(DebugEvent::State {
                        state: guard.state.clone(),
                    });
                }
            }
            _ => {}
        }
    }
}

fn try_sync_stopped_state(session: &Arc<Mutex<DebugSession>>) -> Result<()> {
    let client = {
        let guard = session.lock().unwrap();
        if guard.state.status == DebugStatus::Stopped {
            return Ok(());
        }
        match guard.client.clone() {
            Some(c) => c,
            None => return Ok(()),
        }
    };
    let mut c = client.lock().unwrap();
    let threads_resp = match c.request("threads", json!({}), Duration::from_secs(3)) {
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
        let Ok(frames) = fetch_stack(&mut c, thread_id) else {
            continue;
        };
        if frames.is_empty() {
            continue;
        }
        let variables = frames
            .first()
            .map(|f| f.id)
            .and_then(|frame_id| fetch_scopes(&mut c, frame_id).ok())
            .unwrap_or_default();
        drop(c);
        let mut guard = session.lock().unwrap();
        if matches!(
            guard.state.status,
            DebugStatus::Stopped | DebugStatus::Terminated | DebugStatus::Idle
        ) {
            return Ok(());
        }
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
    let client = {
        let mut guard = session.lock().unwrap();
        // Publish a lightweight stopped state immediately so the UI enables step buttons
        // without waiting for stackTrace/scopes (which can take seconds on Java).
        guard.state.status = DebugStatus::Stopped;
        guard.state.thread_id = Some(thread_id);
        guard.state.stop_reason = Some(reason.clone());
        let quick = guard.state.clone();
        let _ = broadcast.send(DebugEvent::State { state: quick });
        guard.client.clone()
    };
    let Some(client) = client else {
        return;
    };

    let (frames, variables) = {
        let mut c = client.lock().unwrap();
        let frames = fetch_stack(&mut c, thread_id).unwrap_or_default();
        let variables = frames
            .first()
            .map(|f| f.id)
            .and_then(|frame_id| fetch_scopes(&mut c, frame_id).ok())
            .unwrap_or_default();
        (frames, variables)
    };

    let snapshot = {
        let mut guard = session.lock().unwrap();
        // Ignore stale stopped updates if the user already stepped again.
        if guard.state.status != DebugStatus::Stopped
            || guard.state.thread_id != Some(thread_id)
        {
            return;
        }
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
    let mut scopes = body
        .get("scopes")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    // Prefer Local / Arguments so the Variables panel shows useful values first.
    scopes.sort_by_key(|scope| {
        let name = scope
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if name.contains("local") {
            0
        } else if name.contains("arg") {
            1
        } else {
            2
        }
    });
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
                value: format_java_variable_value(v.get("value")?.as_str()?),
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

fn needs_restart_cooldown(status: &DebugStatus, has_client: bool) -> bool {
    has_client
        || matches!(
            status,
            DebugStatus::Running
                | DebugStatus::Stopped
                | DebugStatus::Starting
                | DebugStatus::Terminated
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dap_step_command_maps_kinds() {
        assert_eq!(dap_step_command("in"), "stepIn");
        assert_eq!(dap_step_command("out"), "stepOut");
        assert_eq!(dap_step_command("over"), "next");
        assert_eq!(dap_step_command(""), "next");
    }

    #[test]
    fn restart_cooldown_includes_terminated() {
        assert!(needs_restart_cooldown(&DebugStatus::Terminated, false));
        assert!(needs_restart_cooldown(&DebugStatus::Stopped, false));
        assert!(needs_restart_cooldown(&DebugStatus::Idle, true));
        assert!(!needs_restart_cooldown(&DebugStatus::Idle, false));
    }

    #[test]
    fn parse_stack_frame_reads_path_and_line() {
        let v = json!({
            "id": 42,
            "name": "main",
            "line": 12,
            "column": 3,
            "source": { "path": "/tmp/Hello.java", "name": "Hello.java" }
        });
        let frame = parse_stack_frame(&v).expect("frame");
        assert_eq!(frame.id, 42);
        assert_eq!(frame.name, "main");
        assert_eq!(frame.path.as_deref(), Some("/tmp/Hello.java"));
        assert_eq!(frame.line, Some(12));
        assert_eq!(frame.column, Some(3));
    }

    #[test]
    fn parse_stack_frame_falls_back_to_source_name() {
        let v = json!({
            "id": 1,
            "name": "foo",
            "line": 1,
            "source": { "name": "Foo.java" }
        });
        let frame = parse_stack_frame(&v).expect("frame");
        assert_eq!(frame.path.as_deref(), Some("Foo.java"));
    }

    #[test]
    fn stop_session_inner_clears_client_and_frames() {
        let mut session = DebugSession::new(PathBuf::from("/tmp/ws"));
        session.state.status = DebugStatus::Stopped;
        session.state.thread_id = Some(1);
        session.state.frames = vec![StackFrame {
            id: 1,
            name: "main".into(),
            path: Some("A.java".into()),
            line: Some(1),
            column: Some(1),
        }];
        session.state.variables = vec![DebugVariable {
            name: "x".into(),
            value: "1".into(),
            type_name: None,
            variables_reference: 0,
        }];
        // No live client — still must reset to Idle for a clean second start.
        stop_session_inner(&mut session).unwrap();
        assert_eq!(session.state.status, DebugStatus::Idle);
        assert!(session.client.is_none());
        assert!(session.state.frames.is_empty());
        assert!(session.state.variables.is_empty());
        assert!(session.state.thread_id.is_none());
        assert!(session.last_ended_at.is_some());
    }

    #[test]
    fn recent_end_forces_restart_even_when_idle() {
        let mut session = DebugSession::new(PathBuf::from("/tmp/ws"));
        session.state.status = DebugStatus::Stopped;
        session.last_was_java = true;
        stop_session_inner(&mut session).unwrap();
        assert_eq!(session.state.status, DebugStatus::Idle);
        let recent = session
            .last_ended_at
            .map(|t| t.elapsed() < Duration::from_secs(3))
            .unwrap_or(false);
        assert!(recent);
        assert!(
            needs_restart_cooldown(&session.state.status, session.client.is_some())
                || recent
                || session.last_was_java
        );
    }

    #[test]
    fn parse_java_array_ref_len_reads_empty_and_sized() {
        assert_eq!(parse_java_array_ref_len("String[0]@8"), Some(0));
        assert_eq!(parse_java_array_ref_len("java.lang.String[3]@1a2b"), Some(3));
        assert_eq!(parse_java_array_ref_len("\"hello\""), None);
        assert_eq!(format_java_variable_value("String[0]@8"), "[]");
    }
}
