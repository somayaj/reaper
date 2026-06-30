//! Interactive PTY shell for the in-app terminal (real bash session).

use std::io::{Read, Write};
use std::path::Path;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};
use axum::extract::ws::{Message, WebSocket};
use futures_util::{
    sink::SinkExt,
    stream::StreamExt,
};
use portable_pty::{CommandBuilder, MasterPty, PtySize, native_pty_system};
use serde::Deserialize;
use tokio::sync::mpsc as async_mpsc;

use super::shell;

#[derive(Clone)]
struct PtyControl {
    input_tx: Sender<Vec<u8>>,
    resize_tx: Sender<(u16, u16)>,
}

impl PtyControl {
    fn write(&self, data: &[u8]) {
        if data.is_empty() {
            return;
        }
        let _ = self.input_tx.send(data.to_vec());
    }

    fn resize(&self, cols: u16, rows: u16) {
        if cols == 0 || rows == 0 {
            return;
        }
        let _ = self.resize_tx.send((cols, rows));
    }
}

struct PtySession {
    control: PtyControl,
    _writer_thread: thread::JoinHandle<()>,
    _reader_thread: thread::JoinHandle<()>,
    _child_thread: thread::JoinHandle<()>,
}

#[derive(Debug, Deserialize)]
struct TerminalResize {
    #[serde(rename = "type")]
    kind: String,
    cols: u16,
    rows: u16,
}

pub fn spawn_pty_session(
    cwd: &Path,
    cols: u16,
    rows: u16,
) -> Result<(PtySession, PtyControl, Receiver<Vec<u8>>)> {
    let pty_system = native_pty_system();
    let size = PtySize {
        rows: rows.max(2),
        cols: cols.max(2),
        pixel_width: 0,
        pixel_height: 0,
    };
    let pair = pty_system.openpty(size).context("openpty")?;

    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
    let mut cmd = CommandBuilder::new(&shell);
    cmd.arg("-l");
    cmd.cwd(cwd);
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");

    let mut child = pair.slave.spawn_command(cmd).context("spawn shell")?;

    let reader = pair.master.try_clone_reader().context("pty reader")?;
    let writer = pair.master.take_writer().context("pty writer")?;
    let master: Arc<Mutex<Box<dyn MasterPty + Send>>> = Arc::new(Mutex::new(pair.master));

    let (out_tx, out_rx) = mpsc::channel();
    let (input_tx, input_rx) = mpsc::channel::<Vec<u8>>();
    let (resize_tx, resize_rx) = mpsc::channel::<(u16, u16)>();

    let control = PtyControl {
        input_tx,
        resize_tx,
    };

    let reader_thread = thread::spawn(move || {
        let mut reader = reader;
        let mut buf = [0u8; 8192];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if out_tx.send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    let master_writer = Arc::clone(&master);
    let writer_thread = thread::spawn(move || {
        let mut writer = writer;
        loop {
            while let Ok((cols, rows)) = resize_rx.try_recv() {
                if let Ok(m) = master_writer.lock() {
                    let _ = m.resize(PtySize {
                        rows: rows.max(2),
                        cols: cols.max(2),
                        pixel_width: 0,
                        pixel_height: 0,
                    });
                }
            }

            match input_rx.recv_timeout(Duration::from_millis(40)) {
                Ok(data) => {
                    if writer.write_all(&data).is_err() {
                        break;
                    }
                    let _ = writer.flush();
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
    });

    let child_thread = thread::spawn(move || {
        let _ = child.wait();
    });

    Ok((
        PtySession {
            control: control.clone(),
            _writer_thread: writer_thread,
            _reader_thread: reader_thread,
            _child_thread: child_thread,
        },
        control,
        out_rx,
    ))
}

pub async fn run_terminal_websocket(socket: WebSocket, ws: &Path, cwd_rel: Option<&str>) -> Result<()> {
    let work_dir = shell::resolve_work_dir(ws, cwd_rel)?;
    let (_session, control, out_rx) = spawn_pty_session(&work_dir, 120, 32)?;

    let (async_out_tx, mut async_out_rx) = async_mpsc::channel::<Vec<u8>>(64);
    let bridge = thread::spawn(move || {
        while let Ok(chunk) = out_rx.recv() {
            if async_out_tx.blocking_send(chunk).is_err() {
                break;
            }
        }
    });

    let (mut ws_tx, mut ws_rx) = socket.split();
    let control_recv = control.clone();

    let send_task = tokio::spawn(async move {
        while let Some(chunk) = async_out_rx.recv().await {
            if ws_tx.send(Message::Binary(chunk.into())).await.is_err() {
                break;
            }
        }
    });

    let recv_task = tokio::spawn(async move {
        while let Some(msg) = ws_rx.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    if let Ok(ctrl) = serde_json::from_str::<TerminalResize>(&text) {
                        if ctrl.kind == "resize" {
                            control_recv.resize(ctrl.cols, ctrl.rows);
                            continue;
                        }
                    }
                    control_recv.write(text.as_bytes());
                }
                Ok(Message::Binary(data)) => control_recv.write(&data),
                Ok(Message::Close(_)) => break,
                Ok(Message::Ping(_)) | Ok(Message::Pong(_)) => {}
                Err(_) => break,
            }
        }
    });

    let _ = tokio::join!(send_task, recv_task);
    let _ = bridge.join();
    Ok(())
}
