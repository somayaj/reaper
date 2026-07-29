//! Track long-running child processes (runs, terminals) and terminate them on IDE exit.

use std::collections::HashMap;
use std::process::{Child, Command, ExitStatus};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

#[cfg(unix)]
use std::os::unix::process::CommandExt;

use portable_pty::ChildKiller;

struct TrackedProcess {
    label: String,
    #[cfg(unix)]
    pgid: Option<i32>,
    killer: Option<Box<dyn ChildKiller + Send + Sync>>,
}

struct Registry {
    next_id: u64,
    processes: HashMap<u64, TrackedProcess>,
}

impl Registry {
    fn new() -> Self {
        Self {
            next_id: 1,
            processes: HashMap::new(),
        }
    }

    fn register(&mut self, label: String, pgid: Option<i32>, killer: Box<dyn ChildKiller + Send + Sync>) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.processes.insert(
            id,
            TrackedProcess {
                label,
                #[cfg(unix)]
                pgid,
                killer: Some(killer),
            },
        );
        id
    }

    fn unregister(&mut self, id: u64) {
        self.processes.remove(&id);
    }

    fn drain_all(&mut self) -> Vec<(u64, TrackedProcess)> {
        self.processes.drain().collect()
    }

    fn take(&mut self, id: u64) -> Option<TrackedProcess> {
        self.processes.remove(&id)
    }
}

fn registry() -> &'static Mutex<Registry> {
    static REGISTRY: OnceLock<Mutex<Registry>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(Registry::new()))
}

static SHUTDOWN_REQUESTED: AtomicBool = AtomicBool::new(false);
static SHUTDOWN_STARTED: AtomicBool = AtomicBool::new(false);
static ACTIVE_EXEC_ID: OnceLock<Mutex<Option<u64>>> = OnceLock::new();

const SHUTDOWN_EXIT_TIMEOUT: Duration = Duration::from_secs(2);

fn active_exec_id() -> &'static Mutex<Option<u64>> {
    ACTIVE_EXEC_ID.get_or_init(|| Mutex::new(None))
}

/// Signal background workers (indexing, classpath resolve) to stop promptly.
pub fn request_shutdown() {
    SHUTDOWN_REQUESTED.store(true, Ordering::SeqCst);
}

pub fn is_shutdown_requested() -> bool {
    SHUTDOWN_REQUESTED.load(Ordering::Relaxed)
}

/// Put spawned children in their own process group so we can kill the full tree on exit.
pub fn configure_command(cmd: &mut Command) {
    #[cfg(unix)]
    cmd.process_group(0);
    crate::platform::hide_console_window(cmd);
}

pub struct ProcessGuard {
    id: u64,
    clears_active_exec: bool,
}

impl Drop for ProcessGuard {
    fn drop(&mut self) {
        if self.clears_active_exec {
            if let Ok(mut active) = active_exec_id().lock() {
                if *active == Some(self.id) {
                    *active = None;
                }
            }
        }
        if let Ok(mut reg) = registry().lock() {
            reg.unregister(self.id);
        }
    }
}

pub fn guard_for_child(child: &mut Child, label: &str) -> ProcessGuard {
    guard_for_child_inner(child, label, false)
}

/// Track a UI exec-stream child (Gradle/Maven run) so the terminal can cancel it.
pub fn guard_for_exec_child(child: &mut Child, label: &str) -> ProcessGuard {
    guard_for_child_inner(child, label, true)
}

fn guard_for_child_inner(child: &mut Child, label: &str, track_exec: bool) -> ProcessGuard {
    let killer = child.clone_killer();
    #[cfg(unix)]
    let pgid = Some(child.id() as i32);
    #[cfg(not(unix))]
    let pgid = None;

    let id = registry()
        .lock()
        .expect("process registry lock")
        .register(label.to_string(), pgid, killer);
    if track_exec {
        if let Ok(mut active) = active_exec_id().lock() {
            *active = Some(id);
        }
    }
    ProcessGuard {
        id,
        clears_active_exec: track_exec,
    }
}

pub fn guard_for_pty(
    killer: Box<dyn ChildKiller + Send + Sync>,
    pid: Option<u32>,
    label: &str,
) -> ProcessGuard {
    #[cfg(unix)]
    let pgid = pid.map(|p| p as i32);
    #[cfg(not(unix))]
    let pgid = None;

    let id = registry()
        .lock()
        .expect("process registry lock")
        .register(label.to_string(), pgid, killer);
    ProcessGuard {
        id,
        clears_active_exec: false,
    }
}

#[cfg(unix)]
fn signal_process_group(pgid: i32, sig: i32) {
    if pgid <= 0 {
        return;
    }
    unsafe {
        libc::kill(-pgid, sig);
    }
}

fn terminate_entry(entry: &TrackedProcess) {
    tracing::debug!("Stopping tracked process: {}", entry.label);
    #[cfg(unix)]
    if let Some(pgid) = entry.pgid {
        signal_process_group(pgid, libc::SIGTERM);
        unsafe {
            libc::kill(pgid, libc::SIGTERM);
        }
    }
    if let Some(mut killer) = entry.killer.as_ref().map(|k| k.clone_killer()) {
        let _ = killer.kill();
    }
}

fn force_kill_entry(entry: &TrackedProcess) {
    #[cfg(unix)]
    if let Some(pgid) = entry.pgid {
        signal_process_group(pgid, libc::SIGKILL);
        unsafe {
            libc::kill(pgid, libc::SIGKILL);
        }
    }
    if let Some(mut killer) = entry.killer.as_ref().map(|k| k.clone_killer()) {
        let _ = killer.kill();
    }
}

/// Terminate all tracked child processes (runs, terminal shells, etc.).
pub fn shutdown_all() {
    request_shutdown();

    let entries = registry()
        .lock()
        .expect("process registry lock")
        .drain_all();

    if entries.is_empty() {
        return;
    }

    tracing::info!("Stopping {} tracked process(es) on exit", entries.len());
    for (_, entry) in &entries {
        terminate_entry(entry);
    }

    std::thread::sleep(Duration::from_millis(300));

    for (_, entry) in entries {
        force_kill_entry(&entry);
    }
}

/// Cancel the active UI exec command (Gradle/Maven run from the terminal panel).
pub fn cancel_active_exec() -> bool {
    let id = match active_exec_id().lock() {
        Ok(mut active) => active.take(),
        Err(_) => return false,
    };
    let Some(id) = id else {
        return false;
    };

    let entry = registry()
        .lock()
        .ok()
        .and_then(|mut reg| reg.take(id));

    let Some(entry) = entry else {
        return false;
    };

    tracing::info!("Cancelled terminal command: {}", entry.label);
    terminate_entry(&entry);
    std::thread::sleep(Duration::from_millis(100));
    force_kill_entry(&entry);
    true
}

/// First Ctrl+C: stop children and begin graceful server shutdown. Spawns a hard exit timer.
pub fn initiate_shutdown() {
    if SHUTDOWN_STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    tracing::info!("Shutting down Reaper…");
    shutdown_all();
    std::thread::spawn(|| {
        std::thread::sleep(SHUTDOWN_EXIT_TIMEOUT);
        if is_shutdown_requested() {
            tracing::info!("Shutdown timeout — exiting");
            std::process::exit(0);
        }
    });
}

/// Wait until [`initiate_shutdown`] has been called (for axum graceful shutdown).
pub async fn wait_for_shutdown_signal() {
    while !is_shutdown_requested() {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Handle Ctrl+C: initiate shutdown, then force exit on a second interrupt or timeout.
pub async fn shutdown_watchdog() {
    if tokio::signal::ctrl_c().await.is_err() {
        return;
    }
    initiate_shutdown();
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            tracing::warn!("Second interrupt — forcing exit");
            std::process::exit(130);
        }
        _ = tokio::time::sleep(SHUTDOWN_EXIT_TIMEOUT) => {
            tracing::info!("Shutdown timeout — exiting");
            std::process::exit(0);
        }
    }
}

/// Poll until the child exits, or kill it when shutdown is requested.
pub fn wait_on_child(child: &mut Child) -> std::io::Result<ExitStatus> {
    loop {
        if is_shutdown_requested() {
            let _ = child.kill();
            return child.wait();
        }
        match child.try_wait()? {
            Some(status) => return Ok(status),
            None => std::thread::sleep(Duration::from_millis(50)),
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    #[cfg(windows)]
    fn configure_command_uses_hidden_console_flags() {
        assert_eq!(
            crate::platform::windows_console_creation_flags(),
            0x0800_0000 | 0x0000_0008
        );
    }
}
