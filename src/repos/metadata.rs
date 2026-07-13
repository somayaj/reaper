use std::path::Path;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::auth;
use crate::config::Config;
use crate::settings::SettingsStore;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoMetadata {
    pub remote_url: Option<String>,
    pub remote_host: Option<String>,
    pub imported: bool,
    /// Original on-disk project folder for locally imported repos.
    #[serde(default)]
    pub local_path: Option<String>,
    /// PostgreSQL URL or SQLite path for SQL run / DB viewer.
    #[serde(default)]
    pub database_url: Option<String>,
    /// TLS client certificate settings for PostgreSQL (libpq) and MySQL/MariaDB.
    #[serde(default)]
    pub db_ssl: Option<DbSslSettings>,
    /// SSH bastion / jump-host tunnel for PostgreSQL and MySQL/MariaDB.
    #[serde(default)]
    pub db_ssh: Option<DbSshTunnelSettings>,
}

/// DB SSL options (DBeaver-style); absolute PEM paths for CA, client cert, and private key.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct DbSslSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssl_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssl_root_cert: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssl_cert: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssl_key: Option<String>,
}

impl DbSslSettings {
    pub fn is_empty(&self) -> bool {
        self.ssl_mode.as_deref().unwrap_or("").trim().is_empty()
            && self.ssl_root_cert.as_deref().unwrap_or("").trim().is_empty()
            && self.ssl_cert.as_deref().unwrap_or("").trim().is_empty()
            && self.ssl_key.as_deref().unwrap_or("").trim().is_empty()
    }

    pub fn normalized(self) -> Option<Self> {
        fn trim(value: Option<String>) -> Option<String> {
            value
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        }
        let out = Self {
            ssl_mode: trim(self.ssl_mode),
            ssl_root_cert: trim(self.ssl_root_cert),
            ssl_cert: trim(self.ssl_cert),
            ssl_key: trim(self.ssl_key),
        };
        if out.is_empty() {
            None
        } else {
            Some(out)
        }
    }
}

/// SSH local port-forward through a bastion (DBeaver-style).
///
/// When enabled, Reaper runs `ssh -N -L local:remote_host:remote_port user@bastion`
/// and rewrites the DB URL to `127.0.0.1:local_port`.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct DbSshTunnelSettings {
    #[serde(default)]
    pub enabled: bool,
    /// Bastion / jump host hostname or IP.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    /// Bastion SSH port (default 22).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    /// SSH username on the bastion.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    /// Absolute path to SSH private key (optional — ssh-agent / default keys also work).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity_file: Option<String>,
    /// DB host as reachable from the bastion (defaults to host from the database URL).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_host: Option<String>,
    /// DB port as reachable from the bastion (defaults to port from the database URL).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_port: Option<u16>,
    /// Local listen port for the forward (omit or 0 to auto-pick).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_port: Option<u16>,
}

impl DbSshTunnelSettings {
    pub fn is_enabled(&self) -> bool {
        self.enabled && self.host.as_deref().map(str::trim).is_some_and(|h| !h.is_empty())
    }

    pub fn normalized(self) -> Option<Self> {
        fn trim(value: Option<String>) -> Option<String> {
            value
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        }
        let out = Self {
            enabled: self.enabled,
            host: trim(self.host),
            port: self.port.filter(|&p| p > 0),
            user: trim(self.user),
            identity_file: trim(self.identity_file),
            remote_host: trim(self.remote_host),
            remote_port: self.remote_port.filter(|&p| p > 0),
            local_port: self.local_port.filter(|&p| p > 0),
        };
        if !out.enabled
            && out.host.is_none()
            && out.user.is_none()
            && out.identity_file.is_none()
            && out.remote_host.is_none()
            && out.port.is_none()
            && out.remote_port.is_none()
            && out.local_port.is_none()
        {
            None
        } else {
            Some(out)
        }
    }
}

impl Default for RepoMetadata {
    fn default() -> Self {
        Self {
            remote_url: None,
            remote_host: None,
            imported: false,
            local_path: None,
            database_url: None,
            db_ssl: None,
            db_ssh: None,
        }
    }
}

pub fn load(config: &Config, name: &str) -> Result<RepoMetadata> {
    let path = config.metadata_path(name);
    if !path.exists() {
        return Ok(RepoMetadata::default());
    }
    let raw = std::fs::read_to_string(&path)?;
    Ok(serde_json::from_str(&raw)?)
}

pub fn save(config: &Config, name: &str, metadata: &RepoMetadata) -> Result<()> {
    config.ensure_dirs()?;
    let path = config.metadata_path(name);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let raw = serde_json::to_string_pretty(metadata)?;
    std::fs::write(path, raw)?;
    Ok(())
}

pub fn delete(config: &Config, name: &str) -> Result<()> {
    let path = config.metadata_path(name);
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    Ok(())
}

pub fn remote_auth_ready(metadata: &RepoMetadata, settings: &SettingsStore) -> bool {
    remote_host(metadata).is_some_and(|host| settings.has_token_for_host(&host))
}

fn remote_host(metadata: &RepoMetadata) -> Option<String> {
    if let Some(host) = metadata.remote_host.as_ref().filter(|h| !h.is_empty()) {
        return Some(host.clone());
    }
    metadata
        .remote_url
        .as_ref()
        .and_then(|url| auth::host_from_url(url).ok())
}

pub fn set_local_path(config: &Config, name: &str, path: &Path) -> Result<RepoMetadata> {
    let mut metadata = load(config, name)?;
    metadata.local_path = Some(path.display().to_string());
    metadata.imported = true;
    save(config, name, &metadata)?;
    Ok(metadata)
}

pub fn set_remote(config: &Config, name: &str, clean_url: &str, host: &str) -> Result<RepoMetadata> {
    let mut metadata = load(config, name)?;
    metadata.remote_url = Some(clean_url.to_string());
    metadata.remote_host = Some(host.to_string());
    metadata.imported = true;
    save(config, name, &metadata)?;
    Ok(metadata)
}

pub fn set_database_url(
    config: &Config,
    name: &str,
    database_url: Option<String>,
) -> Result<RepoMetadata> {
    set_db_connection(config, name, database_url, None, None)
}

pub fn set_db_connection(
    config: &Config,
    name: &str,
    database_url: Option<String>,
    db_ssl: Option<DbSslSettings>,
    db_ssh: Option<DbSshTunnelSettings>,
) -> Result<RepoMetadata> {
    let mut metadata = load(config, name)?;
    metadata.database_url = database_url.filter(|s| !s.trim().is_empty());
    metadata.db_ssl = db_ssl.and_then(|ssl| ssl.normalized());
    metadata.db_ssh = db_ssh.and_then(|ssh| ssh.normalized());
    save(config, name, &metadata)?;
    Ok(metadata)
}

pub fn repo_db_ssl(config: &Config, name: &str) -> Option<DbSslSettings> {
    load(config, name).ok().and_then(|meta| meta.db_ssl)
}

pub fn repo_db_ssh(config: &Config, name: &str) -> Option<DbSshTunnelSettings> {
    load(config, name).ok().and_then(|meta| meta.db_ssh)
}

pub fn clear_remote(config: &Config, name: &str) -> Result<()> {
    let mut metadata = load(config, name)?;
    metadata.remote_url = None;
    metadata.remote_host = None;
    save(config, name, &metadata)
}
