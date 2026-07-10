use std::io::{Read, Write};
use std::net::{Shutdown, TcpStream};
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
        // Short read timeout so shutdown can unblock the reader, but TimedOut must be
        // retried — a paused debug session can sit idle for minutes with no DAP traffic.
        let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));
        let _ = stream.set_write_timeout(Some(Duration::from_secs(10)));
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
        let (seq, rx) = self.begin_request(command, arguments)?;
        self.await_response(seq, rx, command, timeout)
    }

    /// Send a request and return `(seq, response_rx)` without waiting.
    ///
    /// Needed for CodeLLDB (and other DAP adapters) where `launch` only completes
    /// after `configurationDone` — waiting on `launch` first deadlocks start.
    pub fn begin_request(
        &mut self,
        command: &str,
        arguments: Value,
    ) -> Result<(i64, Receiver<Value>)> {
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
        Ok((seq, rx))
    }

    pub fn await_response(
        &self,
        seq: i64,
        rx: Receiver<Value>,
        command: &str,
        timeout: Duration,
    ) -> Result<Value> {
        match rx.recv_timeout(timeout) {
            Ok(v) => Ok(v),
            Err(_) => {
                self.pending.lock().unwrap().remove(&seq);
                bail!("dap request timed out: {command}")
            }
        }
    }

    /// Send a request without waiting for a response (used during teardown).
    pub fn send_fire_and_forget(&mut self, command: &str, arguments: Value) -> Result<()> {
        let seq = next_seq();
        let msg = json!({
            "seq": seq,
            "type": "request",
            "command": command,
            "arguments": arguments,
        });
        write_message(&mut self.writer, &msg)
    }

    pub fn write_raw(&mut self, msg: &Value) -> Result<()> {
        write_message(&mut self.writer, msg)
    }

    /// Force-close the DAP transport so reader threads exit and ports can be reused.
    pub fn shutdown(&mut self) {
        match &mut self.writer {
            DapWriter::Tcp(stream) => {
                let _ = stream.shutdown(Shutdown::Both);
            }
            DapWriter::Stdin(_) => {}
        }
        if let Some(child) = self.child.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
        // Unblock any waiters.
        let pending = std::mem::take(&mut *self.pending.lock().unwrap());
        drop(pending);
    }

    pub fn kill(&mut self) {
        self.shutdown();
    }
}

impl Drop for DapClient {
    fn drop(&mut self) {
        self.shutdown();
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

fn read_exact_retry(r: &mut impl Read, buf: &mut [u8]) -> Result<()> {
    let mut filled = 0;
    while filled < buf.len() {
        match r.read(&mut buf[filled..]) {
            Ok(0) => {
                bail!("DAP connection closed unexpectedly (server disconnected)");
            }
            Ok(n) => filled += n,
            // Idle paused sessions produce TimedOut/WouldBlock — keep waiting.
            Err(e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                continue;
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => bail!("DAP read error: {e}"),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{self, Cursor};

    /// Reader that returns TimedOut a few times, then serves real bytes.
    /// Models a paused Java DAP TCP socket with a short read timeout.
    struct IdleThenData {
        idle_left: usize,
        inner: Cursor<Vec<u8>>,
    }

    impl Read for IdleThenData {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            if self.idle_left > 0 {
                self.idle_left -= 1;
                return Err(io::Error::new(io::ErrorKind::TimedOut, "idle"));
            }
            self.inner.read(buf)
        }
    }

    #[test]
    fn response_success_accepts_bodyless_ok() {
        let resp = json!({
            "type": "response",
            "command": "launch",
            "success": true,
            "request_seq": 2,
            "seq": 4,
        });
        assert!(response_success(&resp).is_ok());
    }

    #[test]
    fn response_success_rejects_failure() {
        let resp = json!({
            "type": "response",
            "command": "next",
            "success": false,
            "message": "not stopped",
        });
        let err = response_success(&resp).unwrap_err().to_string();
        assert!(err.contains("not stopped"));
    }

    #[test]
    fn response_body_errors_clearly_when_missing() {
        let resp = json!({
            "type": "response",
            "command": "launch",
            "success": true,
        });
        let err = response_body(&resp).unwrap_err().to_string();
        assert!(err.contains("missing response body"));
    }

    #[test]
    fn read_message_survives_idle_timeouts() {
        let payload = br#"{"type":"event","event":"stopped","body":{"reason":"breakpoint","threadId":1}}"#;
        let mut framed = format!("Content-Length: {}\r\n\r\n", payload.len()).into_bytes();
        framed.extend_from_slice(payload);
        let mut reader = IdleThenData {
            idle_left: 3,
            inner: Cursor::new(framed),
        };
        let msg = read_message(&mut reader).expect("should read after idle timeouts");
        assert_eq!(msg.get("event").and_then(|v| v.as_str()), Some("stopped"));
    }

    #[test]
    fn read_exact_retry_keeps_waiting_on_would_block() {
        struct BlockThenByte {
            blocked: bool,
        }
        impl Read for BlockThenByte {
            fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
                if !self.blocked {
                    self.blocked = true;
                    return Err(io::Error::new(io::ErrorKind::WouldBlock, "nb"));
                }
                buf[0] = b'Z';
                Ok(1)
            }
        }
        let mut r = BlockThenByte { blocked: false };
        let mut out = [0u8; 1];
        read_exact_retry(&mut r, &mut out).unwrap();
        assert_eq!(out[0], b'Z');
    }
}

pub fn read_message(r: &mut impl Read) -> Result<Value> {
    let mut header = String::new();
    let mut buf = [0u8; 1];
    loop {
        header.clear();
        loop {
            read_exact_retry(r, &mut buf)?;
            header.push(buf[0] as char);
            if header.ends_with("\r\n\r\n") {
                break;
            }
            if header.len() > 8192 {
                bail!("invalid DAP header");
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
        read_exact_retry(r, &mut body)?;
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
