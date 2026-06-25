use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::auth::{self, host_from_url, normalize_remote_url};
use crate::config::Config;
use crate::git;
use crate::repos::{self, metadata, summarize_repo};
use crate::settings::SettingsStore;
use crate::workspace;

#[derive(Debug, Deserialize)]
pub struct ImportRepoRequest {
    pub name: String,
    pub remote_url: String,
}

#[derive(Debug, Deserialize)]
pub struct LinkRemoteRequest {
    pub remote_url: String,
}

pub fn import_repo(
    config: &Config,
    settings: &SettingsStore,
    req: ImportRepoRequest,
) -> Result<repos::RepoSummary> {
    if !Config::is_valid_repo_name(&req.name) {
        bail!("invalid repository name; use 'org/repo'");
    }
    let path = config.repo_path(&req.name);
    if path.exists() {
        bail!("repository already exists");
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let clean = normalize_remote_url(&req.remote_url)?;
    let host = host_from_url(&clean)?;
    let auth_url = auth::authenticated_url(&clean, settings)?;

    let clone = git::clone_bare(&auth_url, &path)?;
    if !clone.success() {
        bail!("import failed: {}", clone.stderr.trim());
    }

    git::set_remote_url(&path, "origin", &clean)?;
    metadata::set_remote(config, &req.name, &clean, &host)?;

    summarize_repo(config, settings, &req.name, &path)
}

pub fn link_remote(
    config: &Config,
    settings: &SettingsStore,
    name: &str,
    req: LinkRemoteRequest,
) -> Result<repos::RepoSummary> {
    if !config.repo_exists(name) {
        bail!("repository not found");
    }
    let clean = normalize_remote_url(&req.remote_url)?;
    let host = host_from_url(&clean)?;
    settings
        .token_for_host(&host)
        .ok_or_else(|| anyhow::anyhow!("no PAT configured for {host}"))?;

    metadata::set_remote(config, name, &clean, &host)?;

    if config.workspace_path(name).exists() {
        workspace::ensure_upstream_remote(&config.workspace_path(name), &clean)?;
    }

    summarize_repo(config, settings, name, &config.repo_path(name))
}

pub fn sync_from_remote(
    config: &Config,
    settings: &SettingsStore,
    name: &str,
) -> Result<git::GitOutput> {
    let meta = metadata::load(config, name)?;
    let clean = meta
        .remote_url
        .ok_or_else(|| anyhow::anyhow!("no remote linked"))?;
    let auth_url = auth::authenticated_url(&clean, settings)?;
    let bare = config.repo_path(name);

    let fetch = git::fetch_url_into_bare(&bare, &auth_url)?;
    if !fetch.success() {
        bail!("fetch failed: {}", fetch.stderr.trim());
    }

    let ws = workspace::ensure_workspace(config, name)?;
    workspace::sync_workspace(&ws)
}

pub fn push_to_remote(
    config: &Config,
    settings: &SettingsStore,
    name: &str,
) -> Result<git::GitOutput> {
    let meta = metadata::load(config, name)?;
    let clean = meta
        .remote_url
        .ok_or_else(|| anyhow::anyhow!("no remote linked"))?;
    let auth_url = auth::authenticated_url(&clean, settings)?;
    let ws = workspace::ensure_workspace(config, name)?;
    let branch = git::run_git(Some(&ws), &["branch", "--show-current"])?
        .stdout
        .trim()
        .to_string();
    if branch.is_empty() {
        bail!("could not determine current branch");
    }
    git::push_url(&ws, &auth_url, &branch)
}
