use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::OnceLock;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

use crate::config::running_in_app_bundle;
use crate::port;

use super::{CursorBridge, bridge_url, load_saved_bridge_url, save_bridge_port, set_bridge_url};

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
    bridge_source_dir()
}

fn bridge_deps_ready(dir: &Path) -> bool {
    dir.join("node_modules/@cursor/sdk/package.json").is_file()
        && dir.join("node_modules/@connectrpc/connect/package.json").is_file()
}

fn runtime_bridge_dir() -> Option<PathBuf> {
    Some(crate::config::Config::resolve_data_dir().join("cursor-bridge"))
}

fn sync_bridge_to_runtime(source: &Path, dest: &Path) -> Result<()> {
    let version = env!("CARGO_PKG_VERSION");
    let marker = dest.join(".bridge-version");
    let ready = bridge_deps_ready(dest);
    let same_version = fs::read_to_string(&marker)
        .map(|v| v.trim() == version)
        .unwrap_or(false);

    if ready && same_version && dest.join("server.mjs").is_file() {
        return Ok(());
    }

    tracing::info!(
        "Setting up Cursor bridge in {}…",
        dest.display()
    );

    if dest.exists() {
        fs::remove_dir_all(dest).context("failed to clear old Cursor bridge runtime")?;
    }
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }

    #[cfg(target_os = "macos")]
    let status = std::process::Command::new("ditto")
        .arg(source)
        .arg(dest)
        .status()
        .context("failed to copy cursor-bridge into Reaper data directory")?;

    #[cfg(not(target_os = "macos"))]
    let status = std::process::Command::new("cp")
        .arg("-R")
        .arg(source)
        .arg(dest)
        .status()
        .context("failed to copy cursor-bridge into Reaper data directory")?;

    if !status.success() {
        bail!("failed to copy cursor-bridge into Reaper data directory");
    }
    fs::write(&marker, version)?;
    Ok(())
}

async fn prepare_bridge_dir() -> Result<PathBuf> {
    let source = bridge_source_dir();
    if !source.join("server.mjs").is_file() {
        bail!(
            "cursor-bridge not found at {}; set REAPER_CURSOR_BRIDGE_DIR",
            source.display()
        );
    }

    if running_in_app_bundle() {
        let dest = runtime_bridge_dir()
            .ok_or_else(|| anyhow::anyhow!("HOME not set; cannot install Cursor bridge"))?;
        sync_bridge_to_runtime(&source, &dest)?;
        return Ok(dest);
    }

    Ok(source)
}

fn bridge_source_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("REAPER_CURSOR_BRIDGE_DIR") {
        return PathBuf::from(dir);
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            for candidate in [
                dir.join("../Resources/cursor-bridge"),
                dir.join("../../cursor-bridge"),
                dir.join("../cursor-bridge"),
                dir.join("cursor-bridge"),
            ] {
                if candidate.join("server.mjs").is_file() {
                    return candidate.canonicalize().unwrap_or(candidate);
                }
            }
        }
    }

    if let Ok(cwd) = std::env::current_dir() {
        let local = cwd.join("cursor-bridge");
        if local.join("server.mjs").is_file() {
            return local;
        }
    }

    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("cursor-bridge");
    if manifest.join("server.mjs").is_file() {
        return manifest;
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

    if let Ok(path) = find_node_via_login_path() {
        return Ok(path);
    }

    if let Ok(path) = which_node() {
        return Ok(path);
    }

    bail!(
        "Node.js not found. Install with `brew install node`, or set REAPER_NODE to Cursor's bundled node"
    );
}

fn find_node_via_login_path() -> Result<PathBuf> {
    let output = std::process::Command::new("sh")
        .arg("-lc")
        .arg("/usr/libexec/path_helper -s >/dev/null 2>&1; command -v node")
        .output()
        .context("failed to locate node via login PATH")?;
    if !output.status.success() {
        bail!("node not on login PATH");
    }
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if path.is_empty() || !PathBuf::from(&path).is_file() {
        bail!("node not on login PATH");
    }
    Ok(PathBuf::from(path))
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
    if bridge_deps_ready(dir) {
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
        if status.success() && bridge_deps_ready(dir) {
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

    if !bridge_deps_ready(dir) {
        bail!("cursor-bridge dependencies incomplete after install");
    }

    Ok(())
}

pub async fn reclaim_bridge_port() {
    if bridge_child_slot().lock().await.is_some() {
        return;
    }

    let Some(url) = load_saved_bridge_url() else {
        return;
    };
    set_bridge_url(url.clone());
    let bridge = CursorBridge::new();
    if !bridge.health().await {
        return;
    }

    let port = url
        .trim_start_matches("http://")
        .split('/')
        .next()
        .and_then(|host_port| host_port.rsplit(':').next())
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(8091);
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

    if let Some(url) = load_saved_bridge_url() {
        set_bridge_url(url);
    }

    let bridge = CursorBridge::new();
    if bridge.health().await {
        let owned = bridge_child_slot().lock().await.is_some();
        if owned {
            tracing::debug!("Cursor bridge already running at {}", bridge_url());
            return Ok(());
        }
        tracing::warn!(
            "Replacing orphan Cursor bridge at {} (not managed by this Reaper process)",
            bridge_url()
        );
        if let Some(url) = load_saved_bridge_url() {
            if let Some(port) = url
                .trim_start_matches("http://")
                .split('/')
                .next()
                .and_then(|host_port| host_port.rsplit(':').next())
                .and_then(|p| p.parse::<u16>().ok())
            {
                let script = format!(
                    "lsof -ti tcp:{port} 2>/dev/null | xargs kill -9 2>/dev/null || true"
                );
                let _ = Command::new("sh").arg("-lc").arg(script).status().await;
                tokio::time::sleep(Duration::from_millis(300)).await;
            }
        }
    }

    if std::env::var("REAPER_CURSOR_BRIDGE_DISABLE").is_ok() {
        tracing::info!("Cursor bridge auto-start disabled (REAPER_CURSOR_BRIDGE_DISABLE set)");
        return Ok(());
    }

    let dir = prepare_bridge_dir().await?;
    let node = find_node()?;
    tracing::info!("Using Node.js at {}", node.display());

    ensure_node_modules(&node, &dir).await?;

    let host = std::env::var("REAPER_HOST").unwrap_or_else(|_| "127.0.0.1".into());
    let bridge_port = std::env::var("REAPER_CURSOR_BRIDGE_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .filter(|p| *p != port::AUTO_PORT)
        .unwrap_or_else(|| {
            port::pick_ephemeral_port(&host).unwrap_or_else(|_| port::random_port_candidate())
        });
    let bridge_port = if port::is_avoided_port(bridge_port) {
        port::pick_ephemeral_port(&host).unwrap_or(bridge_port)
    } else {
        bridge_port
    };
    let url = format!("http://{host}:{bridge_port}");
    set_bridge_url(url.clone());
    save_bridge_port(bridge_port);
    tracing::info!("Starting Cursor bridge on {url}…");

    let mut child = Command::new(&node)
        .arg("server.mjs")
        .current_dir(&dir)
        .env("REAPER_CURSOR_BRIDGE_PORT", bridge_port.to_string())
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
        .unwrap_or_else(|| format!("bridge did not respond at {}", bridge_url()));
    bail!("Cursor bridge failed to start: {err}");
}

pub async fn stop_bridge() {
    let mut slot = bridge_child_slot().lock().await;
    if let Some(mut child) = slot.take() {
        let _ = child.kill().await;
        tracing::info!("Cursor bridge stopped");
    }
}
