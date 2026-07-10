use std::io::{Read, Write};
use std::net::TcpStream;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

static SEQ: AtomicI64 = AtomicI64::new(1);

fn next_seq() -> i64 {
    SEQ.fetch_add(1, Ordering::SeqCst)
}

enum DapWriter {
    Stdin(ChildStdin),
    Tcp(TcpStream),
}

impl Write for DapWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            DapWriter::Stdin(s) => s.write(buf),
            DapWriter::Tcp(s) => s.write(buf),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            DapWriter::Stdin(s) => s.flush(),
            DapWriter::Tcp(s) => s.flush(),
        }
    }
}

pub struct DapClient {
    pub child: Option<Child>,
    writer: DapWriter,
    pending: Arc<Mutex<std::collections::HashMap<i64, Sender<Value>>>>,
    _reader: thread::JoinHandle<()>,
}

impl DapClient {
    pub fn spawn(
        command: &str,
        args: &[String],
        cwd: &std::path::Path,
        env: &[(String, String)],
    ) -> Result<(Self, Receiver<Value>)> {
        let mut cmd = Command::new(command);
        cmd.args(args)
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        for (key, value) in env {
            cmd.env(key, value);
        }
        let mut child = cmd
            .spawn()
            .with_context(|| format!("spawn debug adapter: {command}"))?;
        let stdin = child.stdin.take().context("adapter stdin")?;
        let stdout = child.stdout.take().context("adapter stdout")?;
        let pending: Arc<Mutex<std::collections::HashMap<i64, Sender<Value>>>> =
            Arc::new(Mutex::new(std::collections::HashMap::new()));
        let (event_tx, event_rx) = mpsc::channel();
        let pending_reader = pending.clone();
        let reader = thread::spawn(move || {
            if let Err(e) = read_loop(stdout, pending_reader, event_tx) {
                tracing::warn!("dap reader ended: {e:#}");
            }
        });
        Ok((
            Self {
                child: Some(child),
                writer: DapWriter::Stdin(stdin),
                pending,
                _reader: reader,
            },
            event_rx,
        ))
    }

    /// Connect to a Java debug adapter started by jdtls (`vscode.java.startDebugSession`).
    pub fn connect_tcp(host: &str, port: u16) -> Result<(Self, Receiver<Value>)> {
        tracing::debug!("connecting to DAP at {host}:{port}");
        let stream =
            TcpStream::connect((host, port)).with_context(|| format!("connect DAP {host}:{port}"))?;
        tracing::debug!("connected to DAP at {host}:{port}");
        let reader = stream
            .try_clone()
            .context("clone DAP tcp stream for reader")?;
        let pending: Arc<Mutex<std::collections::HashMap<i64, Sender<Value>>>> =
            Arc::new(Mutex::new(std::collections::HashMap::new()));
        let (event_tx, event_rx) = mpsc::channel();
        let pending_reader = pending.clone();
        let reader_handle = thread::spawn(move || {
            if let Err(e) = read_loop(reader, pending_reader, event_tx) {
                tracing::warn!("dap tcp reader ended: {e:#}");
            }
        });
        Ok((
            Self {
                child: None,
                writer: DapWriter::Tcp(stream),
                pending,
                _reader: reader_handle,
            },
            event_rx,
        ))
    }

    pub fn request(&mut self, command: &str, arguments: Value, timeout: Duration) -> Result<Value> {
        let seq = next_seq();
        let (tx, rx) = mpsc::channel();
        self.pending.lock().unwrap().insert(seq, tx);
        let msg = json!({
            "seq": seq,
            "type": "request",
            "command": command,
            "arguments": arguments,
        });
        write_message(&mut self.writer, &msg)?;
        match rx.recv_timeout(timeout) {
            Ok(v) => Ok(v),
            Err(_) => bail!("dap request timed out: {command}"),
        }
    }

    pub fn write_raw(&mut self, msg: &Value) -> Result<()> {
        write_message(&mut self.writer, msg)
    }

    pub fn kill(&mut self) {
        if let Some(child) = self.child.as_mut() {
            let _ = child.kill();
        }
    }
}

impl Drop for DapClient {
    fn drop(&mut self) {
        if let Some(child) = self.child.as_mut() {
            let _ = child.kill();
        }
    }
}

fn read_loop(
    mut stdout: impl Read,
    pending: Arc<Mutex<std::collections::HashMap<i64, Sender<Value>>>>,
    event_tx: Sender<Value>,
) -> Result<()> {
    loop {
        let msg = read_message(&mut stdout)?;
        let kind = msg.get("type").and_then(|v| v.as_str()).unwrap_or("");
        match kind {
            "response" => {
                if let Some(req_seq) = msg.get("request_seq").and_then(|v| v.as_i64()) {
                    let tx = pending.lock().unwrap().remove(&req_seq);
                    if let Some(tx) = tx {
                        let _ = tx.send(msg);
                    }
                }
            }
            "event" => {
                let _ = event_tx.send(msg);
            }
            _ => {}
        }
    }
}

pub fn write_message(w: &mut impl Write, msg: &Value) -> Result<()> {
    let json = serde_json::to_string(msg)?;
    write!(w, "Content-Length: {}\r\n\r\n{}", json.len(), json)?;
    w.flush()?;
    Ok(())
}

pub fn read_message(r: &mut impl Read) -> Result<Value> {
    let mut header = String::new();
    let mut buf = [0u8; 1];
    loop {
        header.clear();
        loop {
            match r.read_exact(&mut buf) {
                Ok(_) => {
                    header.push(buf[0] as char);
                    if header.ends_with("\r\n\r\n") {
                        break;
                    }
                    if header.len() > 8192 {
                        bail!("invalid DAP header");
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    bail!("DAP connection closed unexpectedly (server disconnected before sending header)");
                }
                Err(e) => {
                    bail!("DAP read error: {e}");
                }
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

pub fn response_body(resp: &Value) -> Result<&Value> {
    if resp.get("success").and_then(|v| v.as_bool()) == Some(false) {
        let msg = resp
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("dap request failed");
        bail!("{msg}");
    }
    resp.get("body").with_context(|| {
        format!("missing response body in DAP response: {}", resp)
    })
}

/// Check that a DAP response succeeded (used for responses that don't have a body)
pub fn response_success(resp: &Value) -> Result<()> {
    if resp.get("success").and_then(|v| v.as_bool()) == Some(false) {
        let msg = resp
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("dap request failed");
        bail!("{msg}");
    }
    Ok(())
}
