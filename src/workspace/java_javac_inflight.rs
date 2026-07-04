//! Single-flight, cancellable javac runs for Java diagnostics.
//!
//! Per file: supersede stale compiles for the same path.
//! Per workspace: at most one javac process at a time (avoids pile-ups across open tabs).

use std::collections::HashMap;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use anyhow::{Context, Result};

use crate::jdk;
use crate::process_registry;

struct Slot {
    generation: AtomicU64,
    pid: Mutex<Option<u32>>,
    fingerprint: Mutex<Option<u64>>,
    last_output: Mutex<Option<(u64, CancellableOutput)>>,
}

static SLOTS: OnceLock<Mutex<HashMap<String, Arc<Slot>>>> = OnceLock::new();
static RUN_LOCKS: OnceLock<Mutex<HashMap<String, Arc<Mutex<()>>>>> = OnceLock::new();
static WS_LOCKS: OnceLock<Mutex<HashMap<String, Arc<Mutex<()>>>>> = OnceLock::new();

fn slots() -> &'static Mutex<HashMap<String, Arc<Slot>>> {
    SLOTS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn run_locks() -> &'static Mutex<HashMap<String, Arc<Mutex<()>>>> {
    RUN_LOCKS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn ws_locks() -> &'static Mutex<HashMap<String, Arc<Mutex<()>>>> {
    WS_LOCKS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn diag_key(ws: &Path, rel_path: &str) -> String {
    let ws = ws.canonicalize().unwrap_or_else(|_| ws.to_path_buf());
    format!("{}::{}", ws.display(), rel_path)
}

fn workspace_key(ws: &Path) -> String {
    ws.canonicalize()
        .unwrap_or_else(|_| ws.to_path_buf())
        .display()
        .to_string()
}

fn workspace_lock_for(ws_key: &str) -> Arc<Mutex<()>> {
    let mut map = ws_locks().lock().expect("java javac inflight workspace locks lock");
    map.entry(ws_key.to_string())
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone()
}

fn slot_for(key: &str) -> Arc<Slot> {
    let mut map = slots().lock().expect("java javac inflight slots lock");
    map.entry(key.to_string())
        .or_insert_with(|| {
            Arc::new(Slot {
                generation: AtomicU64::new(0),
                pid: Mutex::new(None),
                fingerprint: Mutex::new(None),
                last_output: Mutex::new(None),
            })
        })
        .clone()
}

fn run_lock_for(key: &str) -> Arc<Mutex<()>> {
    let mut map = run_locks().lock().expect("java javac inflight run locks lock");
    map.entry(key.to_string())
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone()
}

#[cfg(unix)]
fn kill_pid(pid: u32) {
    unsafe {
        libc::kill(pid as i32, libc::SIGTERM);
    }
}

#[cfg(not(unix))]
fn kill_pid(pid: u32) {
    let _ = pid;
}

fn kill_previous(slot: &Slot) {
    if let Some(pid) = *slot.pid.lock().expect("java javac inflight pid lock") {
        kill_pid(pid);
    }
}

#[derive(Clone)]
pub struct CancellableOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub cancelled: bool,
}

pub fn peek_cached(
    ws: &Path,
    rel_path: &str,
    content_fingerprint: u64,
) -> Option<CancellableOutput> {
    let slot = slot_for(&diag_key(ws, rel_path));
    let guard = slot.last_output.lock().ok()?;
    let (fp, out) = guard.as_ref()?;
    if *fp == content_fingerprint && !out.cancelled {
        Some(out.clone())
    } else {
        None
    }
}

/// Run java/javac for diagnostics, cancelling any stale compile for the same workspace file.
pub fn run_cancellable_java_command(
    ws: &Path,
    rel_path: &str,
    program: &str,
    args: &[&str],
    content_fingerprint: u64,
) -> Result<CancellableOutput> {
    let key = diag_key(ws, rel_path);
    let slot = slot_for(&key);

    if let Some((cached_fp, cached)) = slot.last_output.lock().expect("java javac last output lock").clone()
    {
        if cached_fp == content_fingerprint && !cached.cancelled {
            return Ok(cached);
        }
    }

    let supersede = {
        let mut fp = slot
            .fingerprint
            .lock()
            .expect("java javac inflight fingerprint lock");
        let bump = fp.map_or(true, |prev| prev != content_fingerprint);
        if bump {
            *fp = Some(content_fingerprint);
        }
        bump
    };

    let my_gen = if supersede {
        kill_previous(&slot);
        slot.generation.fetch_add(1, Ordering::SeqCst) + 1
    } else {
        slot.generation.load(Ordering::SeqCst)
    };

    let run_lock = run_lock_for(&key);
    let _run_guard = run_lock
        .lock()
        .expect("java javac inflight run lock");

    if let Some((cached_fp, cached)) = slot
        .last_output
        .lock()
        .expect("java javac last output lock")
        .clone()
    {
        if cached_fp == content_fingerprint && !cached.cancelled {
            return Ok(cached);
        }
    }

    if slot.generation.load(Ordering::SeqCst) != my_gen {
        return Ok(cancelled_output());
    }

    kill_previous(&slot);

    let ws_key = workspace_key(ws);
    let ws_lock = workspace_lock_for(&ws_key);
    let _ws_guard = ws_lock
        .lock()
        .expect("java javac inflight workspace lock");

    if slot.generation.load(Ordering::SeqCst) != my_gen {
        return Ok(cancelled_output());
    }

    kill_previous(&slot);

    let mut cmd = Command::new(program);
    cmd.args(args)
        .current_dir(ws)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    jdk::apply_java_env(&mut cmd);
    process_registry::configure_command(&mut cmd);

    let child = cmd
        .spawn()
        .with_context(|| format!("failed to run {program}"))?;
    let child_pid = child.id();
    *slot.pid.lock().expect("java javac inflight pid lock") = Some(child_pid);

    let output = child
        .wait_with_output()
        .with_context(|| format!("failed to wait on {program}"))?;

    {
        let mut pid_guard = slot.pid.lock().expect("java javac inflight pid lock");
        if *pid_guard == Some(child_pid) {
            *pid_guard = None;
        }
    }

    if slot.generation.load(Ordering::SeqCst) != my_gen {
        return Ok(cancelled_output());
    }

    let output = CancellableOutput {
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        exit_code: output.status.code().unwrap_or(-1),
        cancelled: false,
    };
    *slot
        .last_output
        .lock()
        .expect("java javac last output lock") = Some((content_fingerprint, output.clone()));

    Ok(output)
}

fn cancelled_output() -> CancellableOutput {
    CancellableOutput {
        stdout: String::new(),
        stderr: String::new(),
        exit_code: -1,
        cancelled: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diag_key_is_stable_for_same_path() {
        let dir = std::env::temp_dir().join(format!("reaper-javac-key-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let a = diag_key(&dir, "src/App.java");
        let b = diag_key(&dir, "src/App.java");
        assert_eq!(a, b);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn diag_key_differs_by_file() {
        let dir = std::env::temp_dir().join(format!("reaper-javac-key2-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let a = diag_key(&dir, "src/App.java");
        let b = diag_key(&dir, "src/Main.java");
        assert_ne!(a, b);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn workspace_key_is_stable() {
        let dir = std::env::temp_dir().join(format!("reaper-javac-ws-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let a = workspace_key(&dir);
        let b = workspace_key(&dir);
        assert_eq!(a, b);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
