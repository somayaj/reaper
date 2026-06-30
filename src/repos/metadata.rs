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
}

impl Default for RepoMetadata {
    fn default() -> Self {
        Self {
            remote_url: None,
            remote_host: None,
            imported: false,
            local_path: None,
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

pub fn clear_remote(config: &Config, name: &str) -> Result<()> {
    let mut metadata = load(config, name)?;
    metadata.remote_url = None;
    metadata.remote_host = None;
    save(config, name, &metadata)
}
