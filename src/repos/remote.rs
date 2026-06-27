use anyhow::{Context, Result, bail};
use serde::Deserialize;
use url::Url;

use crate::auth::{self, host_from_url, normalize_remote_url, derive_repo_name_from_url};
use crate::config::Config;
use crate::git;
use crate::repos::{self, metadata, summarize_repo};
use crate::settings::SettingsStore;
use crate::workspace;

#[derive(Debug, Deserialize)]
pub struct ImportRepoRequest {
    pub remote_url: String,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PublishToGitHubRequest {
    pub github_repo: String,
    #[serde(default)]
    pub create: bool,
    #[serde(default)]
    pub private: bool,
}

#[derive(Debug, serde::Serialize)]
pub struct PublishResult {
    pub remote_url: String,
    pub created: bool,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
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
    let name = match req.name.filter(|n| !n.trim().is_empty()) {
        Some(n) => n.trim().to_string(),
        None => derive_repo_name_from_url(&req.remote_url)?,
    };
    if !Config::is_valid_repo_name(&name) {
        bail!("invalid repository name; use 'repo' or 'org/repo'");
    }
    let path = config.repo_path(&name);
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
    metadata::set_remote(config, &name, &clean, &host)?;

    summarize_repo(config, settings, &name, &path)
}

fn parse_github_target(input: &str) -> Result<(String, String)> {
    let trimmed = input.trim();
    if trimmed.contains("github.com") {
        let clean = normalize_remote_url(trimmed)?;
        let host = host_from_url(&clean)?;
        if host != "github.com" {
            bail!("only github.com URLs are supported for publish");
        }
        let url = Url::parse(&clean).context("invalid github URL")?;
        let path = url.path().trim_start_matches('/').trim_end_matches(".git");
        let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
        if parts.len() != 2 {
            bail!("use owner/repo or a github.com URL");
        }
        return Ok((parts[0].to_string(), parts[1].to_string()));
    }
    let parts: Vec<&str> = trimmed.split('/').filter(|p| !p.is_empty()).collect();
    if parts.len() != 2 {
        bail!("use owner/repo or a github.com URL");
    }
    Ok((parts[0].to_string(), parts[1].to_string()))
}

async fn ensure_github_repo(
    token: &str,
    owner: &str,
    name: &str,
    private: bool,
) -> Result<bool> {
    let client = reqwest::Client::new();
    let auth_header = format!("Bearer {token}");
    let api = |path: &str| format!("https://api.github.com{path}");

    let existing = client
        .get(api(&format!("/repos/{owner}/{name}")))
        .header("Authorization", &auth_header)
        .header("User-Agent", "reaper")
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .context("github api request failed")?;
    if existing.status().is_success() {
        return Ok(false);
    }

    let user_resp = client
        .get(api("/user"))
        .header("Authorization", &auth_header)
        .header("User-Agent", "reaper")
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .context("github user lookup failed")?;
    if !user_resp.status().is_success() {
        bail!(
            "github authentication failed: {}",
            user_resp.text().await.unwrap_or_default()
        );
    }
    let login = user_resp
        .json::<serde_json::Value>()
        .await
        .ok()
        .and_then(|v| v.get("login").and_then(|l| l.as_str().map(str::to_string)))
        .unwrap_or_default();

    let create_url = if owner == login {
        api("/user/repos")
    } else {
        api(&format!("/orgs/{owner}/repos"))
    };

    let create = client
        .post(&create_url)
        .header("Authorization", &auth_header)
        .header("User-Agent", "reaper")
        .header("Accept", "application/vnd.github+json")
        .json(&serde_json::json!({
            "name": name,
            "private": private,
        }))
        .send()
        .await
        .context("github create repo failed")?;

    let status = create.status();
    if status.is_success() {
        return Ok(true);
    }
    let body = create.text().await.unwrap_or_default();
    if status.as_u16() == 422 && body.contains("already exists") {
        return Ok(false);
    }
    bail!("github create failed ({}): {body}", status);
}

pub async fn publish_to_github(
    config: &Config,
    settings: &SettingsStore,
    name: &str,
    req: PublishToGitHubRequest,
) -> Result<PublishResult> {
    if !config.repo_exists(name) {
        bail!("repository not found");
    }

    let (owner, repo) = parse_github_target(&req.github_repo)?;
    let clean = format!("https://github.com/{owner}/{repo}.git");
    let token = settings
        .token_for_host("github.com")
        .ok_or_else(|| anyhow::anyhow!("no PAT configured for github.com"))?;

    let created = if req.create {
        ensure_github_repo(&token, &owner, &repo, req.private).await?
    } else {
        false
    };

    link_remote(
        config,
        settings,
        name,
        LinkRemoteRequest {
            remote_url: clean.clone(),
        },
    )?;

    let out = push_to_remote(config, settings, name)?;

    Ok(PublishResult {
        remote_url: clean,
        created,
        stdout: out.stdout,
        stderr: out.stderr,
        exit_code: out.exit_code,
    })
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

#[derive(Debug, serde::Serialize)]
pub struct PushPreviewCommit {
    pub hash: String,
    pub subject: String,
}

#[derive(Debug, serde::Serialize)]
pub struct PushPreview {
    pub branch: String,
    pub remote: String,
    pub remote_url: Option<String>,
    pub upstream: Option<String>,
    pub ahead: usize,
    pub commits: Vec<PushPreviewCommit>,
    pub files: Vec<String>,
    pub can_push: bool,
    pub note: Option<String>,
}

pub fn push_preview(config: &Config, settings: &SettingsStore, name: &str) -> Result<PushPreview> {
    let _ = settings;
    let meta = metadata::load(config, name)?;
    let remote_url = meta.remote_url.clone();
    if remote_url.is_none() {
        return Ok(PushPreview {
            branch: String::new(),
            remote: "origin".into(),
            remote_url: None,
            upstream: None,
            ahead: 0,
            commits: vec![],
            files: vec![],
            can_push: false,
            note: Some("No remote linked — publish or link a remote first".into()),
        });
    }

    let ws = workspace::ensure_workspace(config, name)?;
    let branch = git::run_git(Some(&ws), &["branch", "--show-current"])?
        .stdout
        .trim()
        .to_string();
    if branch.is_empty() {
        bail!("could not determine current branch");
    }

    let upstream = upstream_label(&ws);
    let range = unpushed_commit_range(&ws, &branch)?;
    let (commits, files, note) = match range.as_deref() {
        Some(r) => {
            let commits = commits_in_range(&ws, r)?;
            let files = files_in_range(&ws, r)?;
            let note = if commits.is_empty() {
                Some("Already up to date with remote".into())
            } else {
                None
            };
            (commits, files, note)
        }
        None => {
            let commits = commits_on_branch(&ws, &branch, 50)?;
            let files = files_on_branch(&ws, &branch)?;
            let note = Some(format!(
                "First push — branch '{branch}' will be published to origin"
            ));
            (commits, files, note)
        }
    };

    let ahead = commits.len();
    Ok(PushPreview {
        branch,
        remote: "origin".into(),
        remote_url,
        upstream,
        ahead,
        can_push: ahead > 0,
        commits,
        files,
        note,
    })
}

fn upstream_label(ws: &std::path::Path) -> Option<String> {
    let out = git::run_git(
        Some(ws),
        &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"],
    )
    .ok()?;
    if !out.success() {
        return None;
    }
    let label = out.stdout.trim();
    if label.is_empty() {
        None
    } else {
        Some(label.to_string())
    }
}

fn unpushed_commit_range(ws: &std::path::Path, branch: &str) -> Result<Option<String>> {
    if let Some(up) = upstream_label(ws) {
        return Ok(Some(format!("{up}..HEAD")));
    }
    let origin_branch = format!("origin/{branch}");
    if let Ok(verify) = git::run_git(Some(ws), &["rev-parse", "--verify", &origin_branch]) {
        if verify.success() {
            return Ok(Some(format!("{origin_branch}..HEAD")));
        }
    }
    Ok(None)
}

fn commits_in_range(ws: &std::path::Path, range: &str) -> Result<Vec<PushPreviewCommit>> {
    let out = git::run_git(
        Some(ws),
        &["log", range, "--format=%H%x1f%s", "-n", "100"],
    )?;
    Ok(parse_commit_lines(&out.stdout))
}

fn commits_on_branch(ws: &std::path::Path, branch: &str, limit: usize) -> Result<Vec<PushPreviewCommit>> {
    let limit = limit.to_string();
    let out = git::run_git(
        Some(ws),
        &["log", branch, &format!("-{limit}"), "--format=%H%x1f%s"],
    )?;
    Ok(parse_commit_lines(&out.stdout))
}

fn parse_commit_lines(stdout: &str) -> Vec<PushPreviewCommit> {
    stdout
        .lines()
        .filter_map(|line| {
            let mut parts = line.split('\x1f');
            Some(PushPreviewCommit {
                hash: parts.next()?.to_string(),
                subject: parts.next().unwrap_or("").to_string(),
            })
        })
        .collect()
}

fn files_in_range(ws: &std::path::Path, range: &str) -> Result<Vec<String>> {
    let out = git::run_git(Some(ws), &["diff", "--name-only", range])?;
    Ok(unique_sorted_paths(&out.stdout))
}

fn files_on_branch(ws: &std::path::Path, branch: &str) -> Result<Vec<String>> {
    let out = git::run_git(
        Some(ws),
        &["log", branch, "--name-only", "--format=", "-n", "100"],
    )?;
    Ok(unique_sorted_paths(&out.stdout))
}

fn unique_sorted_paths(stdout: &str) -> Vec<String> {
    let mut paths: Vec<String> = stdout
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect();
    paths.sort_unstable();
    paths.dedup();
    paths
}
