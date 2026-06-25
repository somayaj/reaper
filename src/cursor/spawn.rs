use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::OnceLock;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

use super::{CursorBridge, bridge_url};

static BRIDGE_CHILD: OnceLock<Mutex<Option<Child>>> = OnceLock::new();
static BRIDGE_ERROR: OnceLock<Mutex<Option<String>>> = OnceLock::new();

fn bridge_child_slot() -> &'static Mutex<Option<Child>> {
    BRIDGE_CHILD.get_or_init(|| Mutex::new(None))
}

fn bridge_error_slot() -> &'static Mutex<Option<String>> {
    BRIDGE_ERROR.get_or_init(|| Mutex::new(None))
}

pub async fn last_bridge_error() -> Option<String> {
    bridge_error_slot().lock().await.clone()
}

async fn set_bridge_error(msg: Option<String>) {
    *bridge_error_slot().lock().await = msg;
}

pub fn bridge_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("REAPER_CURSOR_BRIDGE_DIR") {
        return PathBuf::from(dir);
    }

    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("cursor-bridge");
    if manifest.join("server.mjs").is_file() {
        return manifest;
    }

    if let Ok(cwd) = std::env::current_dir() {
        let local = cwd.join("cursor-bridge");
        if local.join("server.mjs").is_file() {
            return local;
        }
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            for candidate in [
                dir.join("../../cursor-bridge"),
                dir.join("../cursor-bridge"),
                dir.join("cursor-bridge"),
            ] {
                if candidate.join("server.mjs").is_file() {
                    return candidate
                        .canonicalize()
                        .unwrap_or(candidate);
                }
            }
        }
    }

    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("cursor-bridge")
}

fn find_node() -> Result<PathBuf> {
    if let Ok(path) = std::env::var("REAPER_NODE") {
        let p = PathBuf::from(path);
        if p.is_file() {
            return Ok(p);
        }
    }

    for path in [
        PathBuf::from("/opt/homebrew/bin/node"),
        PathBuf::from("/usr/local/bin/node"),
        PathBuf::from("/Applications/Cursor.app/Contents/Resources/app/resources/helpers/node"),
    ] {
        if path.is_file() {
            return Ok(path);
        }
    }

    if let Ok(path) = which_node() {
        return Ok(path);
    }

    bail!(
        "Node.js not found. Install with `brew install node`, or set REAPER_NODE to Cursor's bundled node"
    );
}

fn which_node() -> Result<PathBuf> {
    let output = std::process::Command::new("sh")
        .arg("-lc")
        .arg("command -v node")
        .output()
        .context("failed to locate node")?;
    if !output.status.success() {
        bail!("node not on PATH");
    }
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if path.is_empty() {
        bail!("node not on PATH");
    }
    Ok(PathBuf::from(path))
}

async fn ensure_node_modules(node: &Path, dir: &Path) -> Result<()> {
    if dir.join("node_modules/@cursor/sdk/package.json").is_file() {
        return Ok(());
    }

    tracing::info!("Installing cursor-bridge dependencies (first run, may take a minute)…");

    if Command::new("npm")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .map(|s| s.success())
        .unwrap_or(false)
    {
        let status = Command::new("npm")
            .arg("install")
            .current_dir(dir)
            .status()
            .await
            .context("npm install failed to start")?;
        if status.success() && dir.join("node_modules/@cursor/sdk/package.json").is_file() {
            return Ok(());
        }
        tracing::warn!("npm install failed or incomplete; falling back to install-deps.mjs");
    }

    let install_script = dir.join("install-deps.mjs");
    if !install_script.is_file() {
        bail!("cursor-bridge/install-deps.mjs missing");
    }

    let status = Command::new(node)
        .arg("install-deps.mjs")
        .current_dir(dir)
        .status()
        .await
        .context("install-deps.mjs failed to start")?;

    if !status.success() {
        bail!("install-deps.mjs failed in cursor-bridge");
    }

    if !dir.join("node_modules/@cursor/sdk/package.json").is_file() {
        bail!("@cursor/sdk missing after install");
    }

    Ok(())
}

pub async fn reclaim_bridge_port() {
    if bridge_child_slot().lock().await.is_some() {
        return;
    }

    let bridge = CursorBridge::new();
    if !bridge.health().await {
        return;
    }

    let port = std::env::var("REAPER_CURSOR_BRIDGE_PORT").unwrap_or_else(|_| "8091".into());
    tracing::info!("Stopping orphaned Cursor bridge on port {port}…");
    let script = format!(
        "lsof -ti tcp:{port} 2>/dev/null | xargs kill -9 2>/dev/null || true"
    );
    let _ = Command::new("sh")
        .arg("-lc")
        .arg(script)
        .status()
        .await;
    tokio::time::sleep(Duration::from_millis(300)).await;
}

pub async fn ensure_bridge_running() -> Result<()> {
    set_bridge_error(None).await;

    let bridge = CursorBridge::new();
    if bridge.health().await {
        tracing::debug!("Cursor bridge already running at {}", bridge_url());
        return Ok(());
    }

    if std::env::var("REAPER_CURSOR_BRIDGE_DISABLE").is_ok() {
        tracing::info!("Cursor bridge auto-start disabled (REAPER_CURSOR_BRIDGE_DISABLE set)");
        return Ok(());
    }

    let dir = bridge_dir();
    if !dir.join("server.mjs").exists() {
        bail!(
            "cursor-bridge not found at {}; set REAPER_CURSOR_BRIDGE_DIR",
            dir.display()
        );
    }

    let node = find_node()?;
    tracing::info!("Using Node.js at {}", node.display());

    ensure_node_modules(&node, &dir).await?;

    tracing::info!("Starting Cursor bridge at {}…", dir.display());
    let mut child = Command::new(&node)
        .arg("server.mjs")
        .current_dir(&dir)
        .kill_on_drop(true)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("failed to start cursor bridge with {}", node.display()))?;

    if let Some(stderr) = child.stderr.take() {
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                tracing::warn!("cursor-bridge: {line}");
                set_bridge_error(Some(line)).await;
            }
        });
    }

    {
        let mut slot = bridge_child_slot().lock().await;
        if let Some(mut old) = slot.take() {
            let _ = old.kill().await;
        }
        *slot = Some(child);
    }

    for attempt in 1..=60 {
        tokio::time::sleep(Duration::from_millis(250)).await;
        if bridge.health().await {
            set_bridge_error(None).await;
            tracing::info!("Cursor bridge ready at {}", bridge_url());
            return Ok(());
        }
        if attempt % 8 == 0 {
            tracing::debug!("waiting for Cursor bridge ({attempt}/60)…");
        }
    }

    stop_bridge().await;
    let err = last_bridge_error()
        .await
        .unwrap_or_else(|| "bridge did not respond on port 8091".into());
    bail!("Cursor bridge failed to start: {err}");
}

pub async fn stop_bridge() {
    let mut slot = bridge_child_slot().lock().await;
    if let Some(mut child) = slot.take() {
        let _ = child.kill().await;
        tracing::info!("Cursor bridge stopped");
    }
}
