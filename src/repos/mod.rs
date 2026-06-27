pub mod metadata;
mod remote;

pub use remote::{
    ImportLocalRepoRequest, ImportRepoRequest, LinkRemoteRequest, PublishResult,
    PublishToGitHubRequest, PushPreview, import_local_repo, import_repo, link_remote,
    publish_to_github, push_preview, push_to_remote, sync_from_remote,
};

use std::path::Path;

use anyhow::{Result, bail};
use serde::Serialize;

use crate::config::{self, Config};
use crate::git;
use crate::settings::SettingsStore;

#[derive(Debug, Serialize)]
pub struct RepoSummary {
    pub name: String,
    pub description: Option<String>,
    pub clone_url: String,
    pub default_branch: Option<String>,
    pub branch_count: usize,
    pub commit_count: usize,
    pub remote_url: Option<String>,
    pub remote_host: Option<String>,
    pub remote_configured: bool,
    pub imported: bool,
}

#[derive(Debug, Serialize)]
pub struct RepoDetail {
    #[serde(flatten)]
    pub summary: RepoSummary,
    pub branches: Vec<String>,
    pub recent_commits: Vec<git::CommitInfo>,
}

pub fn list_repos(config: &Config, settings: &SettingsStore) -> Result<Vec<RepoSummary>> {
    config.ensure_dirs()?;
    let discovered = config::discover_repos(&config.repos_dir)?;
    discovered
        .into_iter()
        .map(|(name, path)| summarize_repo(config, settings, &name, &path))
        .collect()
}

pub fn summarize_repo(
    config: &Config,
    settings: &SettingsStore,
    name: &str,
    path: &Path,
) -> Result<RepoSummary> {
    let branches = git::list_branches(path).unwrap_or_default();
    let default_branch = git::default_branch(path).ok();
    let commits = git::log(path, 1_000).unwrap_or_default();
    let meta = metadata::load(config, name).unwrap_or_default();

    Ok(RepoSummary {
        name: name.to_string(),
        description: git::repo_description(path),
        clone_url: config.clone_url(name),
        default_branch,
        branch_count: branches.len(),
        commit_count: commits.len(),
        remote_url: meta.remote_url.clone(),
        remote_host: meta.remote_host.clone(),
        remote_configured: metadata::remote_auth_ready(&meta, settings),
        imported: meta.imported,
    })
}

pub fn get_repo(config: &Config, settings: &SettingsStore, name: &str) -> Result<RepoDetail> {
    let path = config.repo_path(name);
    if !config.repo_exists(name) {
        bail!("repository not found");
    }
    let summary = summarize_repo(config, settings, name, &path)?;
    let branches = git::list_branches(&path).unwrap_or_default();
    let recent_commits = git::log(&path, 20).unwrap_or_default();
    Ok(RepoDetail {
        summary,
        branches,
        recent_commits,
    })
}

#[derive(Debug, serde::Deserialize)]
pub struct CreateRepoRequest {
    pub name: String,
    pub description: Option<String>,
    pub init_with_readme: Option<bool>,
    pub readme: Option<String>,
    pub remote_url: Option<String>,
}

pub fn create_repo(
    config: &Config,
    settings: &SettingsStore,
    req: CreateRepoRequest,
) -> Result<RepoSummary> {
    if !Config::is_valid_repo_name(&req.name) {
        bail!("invalid repository name; use 'repo' or 'org/repo'");
    }
    let path = config.repo_path(&req.name);
    if path.exists() {
        bail!("repository already exists");
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    git::init_bare_repo(&path)?;

    if let Some(desc) = &req.description {
        git::set_repo_description(&path, desc)?;
    }

    if req.init_with_readme.unwrap_or(true) {
        let readme = req.readme.unwrap_or_else(|| {
            format!(
                "# {}\n\nManaged by [Reaper](http://{}:{}).\n",
                req.name, config.host, config.port
            )
        });
        git::seed_bare_repo_with_readme(&path, &readme)?;
    }

    if let Some(url) = req.remote_url.filter(|u| !u.is_empty()) {
        link_remote(config, settings, &req.name, LinkRemoteRequest { remote_url: url })?;
    }

    summarize_repo(config, settings, &req.name, &path)
}

pub fn delete_repo(config: &Config, name: &str) -> Result<()> {
    let path = config.repo_path(name);
    if !config.repo_exists(name) {
        bail!("repository not found");
    }
    std::fs::remove_dir_all(path)?;
    let _ = metadata::delete(config, name);
    Ok(())
}
