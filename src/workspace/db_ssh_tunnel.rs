//! SSH bastion / jump-host local port forwards for the Database viewer.
//!
//! Spawns `ssh -N -L local:remote_host:remote_port` and keeps the process alive
//! for the workspace so schema/query/sql-run can reuse the same tunnel.

use std::collections::HashMap;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};

use crate::process_registry::{self, ProcessGuard};
use crate::repos::metadata::DbSshTunnelSettings;

#[derive(Debug, Clone)]
pub struct TunnelEndpoint {
    pub local_port: u16,
    pub remote_host: String,
    pub remote_port: u16,
}

struct TunnelSession {
    /// Fingerprint of settings + DB target so we restart when config changes.
    fingerprint: String,
    local_port: u16,
    child: Child,
    _guard: ProcessGuard,
}

fn sessions() -> &'static Mutex<HashMap<String, TunnelSession>> {
    static SESSIONS: OnceLock<Mutex<HashMap<String, TunnelSession>>> = OnceLock::new();
    SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn workspace_key(ws: &Path) -> String {
    ws.canonicalize()
        .unwrap_or_else(|_| ws.to_path_buf())
        .to_string_lossy()
        .into_owned()
}

pub fn stop_tunnel(ws: &Path) {
    let key = workspace_key(ws);
    if let Ok(mut map) = sessions().lock() {
        if let Some(mut session) = map.remove(&key) {
            let _ = session.child.kill();
            let _ = session.child.wait();
        }
    }
}

/// Ensure an SSH local forward is running; returns the local bind port.
pub fn ensure_tunnel(
    ws: &Path,
    database_url: &str,
    ssh: &DbSshTunnelSettings,
) -> Result<TunnelEndpoint> {
    if !ssh.is_enabled() {
        bail!("SSH tunnel is not enabled");
    }
    let bastion = ssh
        .host
        .as_deref()
        .map(str::trim)
        .filter(|h| !h.is_empty())
        .context("SSH bastion host is required")?;
    let user = ssh
        .user
        .as_deref()
        .map(str::trim)
        .filter(|u| !u.is_empty())
        .context("SSH bastion user is required")?;

    if let Some(key) = ssh.identity_file.as_deref().filter(|p| !p.is_empty()) {
        let path = PathBuf::from(key);
        if !path.is_file() {
            bail!("SSH identity file not found: {key}");
        }
    }

    let (db_host, db_port) = parse_db_host_port(database_url)?;
    let remote_host = ssh
        .remote_host
        .as_deref()
        .map(str::trim)
        .filter(|h| !h.is_empty())
        .unwrap_or(db_host.as_str())
        .to_string();
    let remote_port = ssh.remote_port.unwrap_or(db_port);
    let bastion_port = ssh.port.unwrap_or(22);
    let preferred_local = ssh.local_port.filter(|&p| p > 0);

    let fingerprint = format!(
        "{user}@{bastion}:{bastion_port}->{remote_host}:{remote_port}|id={}|local={:?}",
        ssh.identity_file.as_deref().unwrap_or(""),
        preferred_local
    );

    let key = workspace_key(ws);
    {
        let mut map = sessions().lock().expect("ssh tunnel sessions lock");
        if let Some(session) = map.get_mut(&key) {
            if session.fingerprint == fingerprint {
                match session.child.try_wait() {
                    Ok(None) if port_is_open(session.local_port) => {
                        return Ok(TunnelEndpoint {
                            local_port: session.local_port,
                            remote_host,
                            remote_port,
                        });
                    }
                    _ => {
                        let _ = session.child.kill();
                        let _ = session.child.wait();
                        map.remove(&key);
                    }
                }
            } else {
                if let Some(mut old) = map.remove(&key) {
                    let _ = old.child.kill();
                    let _ = old.child.wait();
                }
            }
        }
    }

    let local_port = match preferred_local {
        Some(port) => port,
        None => pick_free_local_port()?,
    };

    let mut child = spawn_ssh_tunnel(
        user,
        bastion,
        bastion_port,
        local_port,
        &remote_host,
        remote_port,
        ssh.identity_file.as_deref(),
    )?;
    let guard = process_registry::guard_for_child(&mut child, "db-ssh-tunnel");

    wait_for_tunnel(&mut child, local_port).map_err(|e| {
        let _ = child.kill();
        let _ = child.wait();
        e
    })?;

    let endpoint = TunnelEndpoint {
        local_port,
        remote_host: remote_host.clone(),
        remote_port,
    };

    let mut map = sessions().lock().expect("ssh tunnel sessions lock");
    map.insert(
        key,
        TunnelSession {
            fingerprint,
            local_port,
            child,
            _guard: guard,
        },
    );
    Ok(endpoint)
}

/// Rewrite a postgres/mysql URL so clients connect through the local forward.
pub fn rewrite_url_through_tunnel(url: &str, local_port: u16) -> Result<String> {
    let mut parsed = url::Url::parse(url).with_context(|| format!("invalid database URL: {url}"))?;
    parsed
        .set_host(Some("127.0.0.1"))
        .context("failed to set tunnel host")?;
    parsed
        .set_port(Some(local_port))
        .map_err(|_| anyhow::anyhow!("failed to set tunnel port"))?;
    Ok(parsed.to_string())
}

pub fn build_ssh_args(
    user: &str,
    bastion: &str,
    bastion_port: u16,
    local_port: u16,
    remote_host: &str,
    remote_port: u16,
    identity_file: Option<&str>,
) -> Vec<String> {
    let mut args = vec![
        "-N".into(),
        "-L".into(),
        format!("{local_port}:{remote_host}:{remote_port}"),
        "-p".into(),
        bastion_port.to_string(),
        "-o".into(),
        "BatchMode=yes".into(),
        "-o".into(),
        "ExitOnForwardFailure=yes".into(),
        "-o".into(),
        "StrictHostKeyChecking=accept-new".into(),
        "-o".into(),
        "ServerAliveInterval=30".into(),
        "-o".into(),
        "ServerAliveCountMax=3".into(),
    ];
    if let Some(key) = identity_file.filter(|p| !p.is_empty()) {
        args.push("-i".into());
        args.push(key.to_string());
        args.push("-o".into());
        args.push("IdentitiesOnly=yes".into());
    }
    args.push(format!("{user}@{bastion}"));
    args
}

pub fn parse_db_host_port(url: &str) -> Result<(String, u16)> {
    let parsed = url::Url::parse(url).with_context(|| format!("invalid database URL: {url}"))?;
    let host = parsed
        .host_str()
        .filter(|h| !h.is_empty())
        .unwrap_or("localhost")
        .to_string();
    let scheme = parsed.scheme().to_ascii_lowercase();
    let default_port = match scheme.as_str() {
        "mysql" | "mysql2" | "mariadb" => 3306,
        _ => 5432,
    };
    let port = parsed.port().unwrap_or(default_port);
    Ok((host, port))
}

fn spawn_ssh_tunnel(
    user: &str,
    bastion: &str,
    bastion_port: u16,
    local_port: u16,
    remote_host: &str,
    remote_port: u16,
    identity_file: Option<&str>,
) -> Result<Child> {
    let program = crate::toolchain::resolve_program("ssh")
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| "ssh".into());
    let args = build_ssh_args(
        user,
        bastion,
        bastion_port,
        local_port,
        remote_host,
        remote_port,
        identity_file,
    );
    let mut cmd = Command::new(&program);
    cmd.args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    process_registry::configure_command(&mut cmd);
    cmd.spawn()
        .with_context(|| format!("failed to start SSH tunnel (`{program}`). Install OpenSSH client."))
}

fn wait_for_tunnel(child: &mut Child, local_port: u16) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(12);
    while Instant::now() < deadline {
        if let Some(status) = child.try_wait()? {
            let mut err = String::new();
            if let Some(mut stderr) = child.stderr.take() {
                let _ = std::io::Read::read_to_string(&mut stderr, &mut err);
            }
            let detail = err.trim();
            if detail.is_empty() {
                bail!("SSH tunnel exited early (status {status})");
            }
            bail!("SSH tunnel failed: {detail}");
        }
        if port_is_open(local_port) {
            // Drop stderr pipe so it cannot fill and block ssh.
            let _ = child.stderr.take();
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let _ = child.kill();
    bail!("SSH tunnel timed out waiting for local port {local_port}");
}

fn port_is_open(port: u16) -> bool {
    TcpStream::connect_timeout(
        &SocketAddr::from(([127, 0, 0, 1], port)),
        Duration::from_millis(150),
    )
    .is_ok()
}

fn pick_free_local_port() -> Result<u16> {
    let listener = TcpListener::bind("127.0.0.1:0").context("could not allocate local tunnel port")?;
    let port = listener.local_addr()?.port();
    drop(listener);
    Ok(port)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_ssh_args_with_identity() {
        let args = build_ssh_args(
            "deploy",
            "bastion.example",
            2222,
            15432,
            "db.internal",
            5432,
            Some("/Users/me/.ssh/id_ed25519"),
        );
        assert!(args.contains(&"-N".into()));
        assert!(args.contains(&"-L".into()));
        assert!(args.contains(&"15432:db.internal:5432".into()));
        assert!(args.contains(&"-p".into()));
        assert!(args.contains(&"2222".into()));
        assert!(args.contains(&"-i".into()));
        assert!(args.contains(&"/Users/me/.ssh/id_ed25519".into()));
        assert!(args.contains(&"IdentitiesOnly=yes".into()));
        assert!(args.contains(&"deploy@bastion.example".into()));
        assert!(args.contains(&"ExitOnForwardFailure=yes".into()));
    }

    #[test]
    fn rewrites_url_to_localhost_forward() {
        let out = rewrite_url_through_tunnel(
            "postgresql://app:secret@db.prod.internal:5432/orders",
            15432,
        )
        .unwrap();
        assert!(out.contains("127.0.0.1:15432"));
        assert!(out.contains("orders"));
        assert!(out.contains("app"));
        assert!(!out.contains("db.prod.internal"));
    }

    #[test]
    fn parses_mysql_default_port() {
        let (host, port) = parse_db_host_port("mysql://root@db.example/app").unwrap();
        assert_eq!(host, "db.example");
        assert_eq!(port, 3306);
    }

    #[test]
    fn ssh_settings_enabled_requires_host() {
        let disabled = DbSshTunnelSettings {
            enabled: true,
            host: None,
            ..Default::default()
        };
        assert!(!disabled.is_enabled());
        let enabled = DbSshTunnelSettings {
            enabled: true,
            host: Some("bastion".into()),
            user: Some("ubuntu".into()),
            ..Default::default()
        };
        assert!(enabled.is_enabled());
    }
}
