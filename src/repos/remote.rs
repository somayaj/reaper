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

#[derive(Debug, Deserialize)]
pub struct PublishRequest {
    #[serde(default)]
    pub remote_url: Option<String>,
    #[serde(default)]
    pub host: Option<String>,
    #[serde(default)]
    pub create: bool,
    #[serde(default)]
    pub private: bool,
    #[serde(default)]
    pub github_repo: Option<String>,
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

#[derive(Debug, Deserialize)]
pub struct ImportLocalRepoRequest {
    pub local_path: String,
    #[serde(default)]
    pub name: Option<String>,
}

pub fn import_local_repo(
    config: &Config,
    settings: &SettingsStore,
    req: ImportLocalRepoRequest,
) -> Result<repos::RepoSummary> {
    let local_path = req.local_path.trim();
    if local_path.is_empty() {
        bail!("local path is required");
    }
    let src = std::path::Path::new(local_path);
    if !src.exists() {
        bail!("path does not exist: {}", local_path);
    }

    let name = match req.name.filter(|n| !n.trim().is_empty()) {
        Some(n) => n.trim().to_string(),
        None => derive_repo_name_from_local_path(src)?,
    };
    if !Config::is_valid_repo_name(&name) {
        bail!("invalid repository name; use 'repo' or 'org/repo'");
    }

    let path = config.repo_path(&name);
    if path.exists() {
        if settings.is_repo_hidden(&name) {
            return super::restore_repo(config, settings, &name);
        }
        bail!("repository already exists");
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let clone = git::clone_bare_local(src, &path)?;
    if !clone.success() {
        bail!("import failed: {}", clone.stderr.trim());
    }

    let src_canon = src
        .canonicalize()
        .with_context(|| format!("resolve source path {}", src.display()))?;
    metadata::set_local_path(config, &name, &src_canon)?;
    let _ = settings.push_recent_git_local_path(&src_canon.display().to_string());

    if let Some(origin) = git::remote_url(&src_canon, "origin") {
        if let Ok(clean) = normalize_remote_url(&origin) {
            if let Ok(host) = host_from_url(&clean) {
                git::set_remote_url(&path, "origin", &clean)?;
                metadata::set_remote(config, &name, &clean, &host)?;
                let _ = settings.push_recent_git_remote(&clean);
                return summarize_repo(config, settings, &name, &path);
            }
        }
    }

    summarize_repo(config, settings, &name, &path)
}

fn derive_repo_name_from_local_path(path: &std::path::Path) -> Result<String> {
    let canonical = path
        .canonicalize()
        .with_context(|| format!("resolve path {}", path.display()))?;
    let basename = canonical
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("repo");
    let base = basename.strip_suffix(".git").unwrap_or(basename);
    let sanitized: String = base
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let sanitized = sanitized.trim_matches(|c| c == '-' || c == '.');
    if sanitized.is_empty() || !Config::is_valid_repo_name(sanitized) {
        bail!("could not derive a valid repo name from path; set name explicitly");
    }
    Ok(sanitized.to_string())
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
        if settings.is_repo_hidden(&name) {
            return super::restore_repo(config, settings, &name);
        }
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
    let _ = settings.push_recent_git_remote(&clean);

    summarize_repo(config, settings, &name, &path)
}

fn parse_github_target(input: &str) -> Result<(String, String)> {
    parse_host_repo_target("github.com", input)
}

fn parse_host_repo_target(default_host: &str, input: &str) -> Result<(String, String)> {
    let trimmed = input.trim();
    if trimmed.contains("://") {
        let clean = normalize_remote_url(trimmed)?;
        let host = host_from_url(&clean)?;
        if host != default_host && !host.ends_with(&format!(".{default_host}")) {
            bail!("expected {default_host} URL, got {host}");
        }
        let url = Url::parse(&clean).context("invalid remote URL")?;
        let path = url.path().trim_start_matches('/').trim_end_matches(".git");
        let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
        if parts.len() < 2 {
            bail!("use owner/repo or a full HTTPS URL");
        }
        let owner = parts[0].to_string();
        let repo = parts[parts.len() - 1].to_string();
        return Ok((owner, repo));
    }
    let parts: Vec<&str> = trimmed.split('/').filter(|p| !p.is_empty()).collect();
    if parts.len() != 2 {
        bail!("use owner/repo or a full HTTPS URL");
    }
    Ok((parts[0].to_string(), parts[1].to_string()))
}

fn resolve_publish_target(req: &PublishRequest) -> Result<(String, String)> {
    if let Some(legacy) = req.github_repo.as_deref().filter(|s| !s.trim().is_empty()) {
        let (owner, repo) = parse_github_target(legacy)?;
        return Ok(("github.com".into(), format!("https://github.com/{owner}/{repo}.git")));
    }
    let host = req
        .host
        .as_deref()
        .map(str::trim)
        .filter(|h| !h.is_empty())
        .unwrap_or("github.com");
    let raw = req
        .remote_url
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!("remote URL is required"))?;
    if raw.contains("://") {
        let clean = normalize_remote_url(raw)?;
        let resolved_host = host_from_url(&clean)?;
        return Ok((resolved_host, clean));
    }
    let (owner, repo) = parse_host_repo_target(host, raw)?;
    Ok((host.to_string(), format!("https://{host}/{owner}/{repo}.git")))
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

async fn ensure_gitlab_repo(token: &str, namespace: &str, repo: &str, private: bool) -> Result<bool> {
    let client = reqwest::Client::new();
    let encoded = format!("{namespace}%2F{repo}");
    let existing = client
        .get(format!("https://gitlab.com/api/v4/projects/{encoded}"))
        .header("PRIVATE-TOKEN", token)
        .send()
        .await
        .context("gitlab api request failed")?;
    if existing.status().is_success() {
        return Ok(false);
    }
    let create = client
        .post("https://gitlab.com/api/v4/projects")
        .header("PRIVATE-TOKEN", token)
        .json(&serde_json::json!({
            "name": repo,
            "path": repo,
            "namespace_path": namespace,
            "visibility": if private { "private" } else { "public" },
        }))
        .send()
        .await
        .context("gitlab create repo failed")?;
    let status = create.status();
    if status.is_success() {
        return Ok(true);
    }
    let body = create.text().await.unwrap_or_default();
    if body.contains("has already been taken") {
        return Ok(false);
    }
    bail!("gitlab create failed ({}): {body}", status);
}

async fn ensure_bitbucket_repo(
    token: &str,
    workspace: &str,
    repo: &str,
    private: bool,
) -> Result<bool> {
    let client = reqwest::Client::new();
    let auth = format!("Bearer {token}");
    let existing = client
        .get(format!(
            "https://api.bitbucket.org/2.0/repositories/{workspace}/{repo}"
        ))
        .header("Authorization", &auth)
        .send()
        .await
        .context("bitbucket api request failed")?;
    if existing.status().is_success() {
        return Ok(false);
    }
    let create = client
        .post(format!(
            "https://api.bitbucket.org/2.0/repositories/{workspace}/{repo}"
        ))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "scm": "git",
            "is_private": private,
        }))
        .send()
        .await
        .context("bitbucket create repo failed")?;
    let status = create.status();
    if status.is_success() {
        return Ok(true);
    }
    let body = create.text().await.unwrap_or_default();
    if body.contains("already exists") {
        return Ok(false);
    }
    bail!("bitbucket create failed ({}): {body}", status);
}

pub async fn publish_to_remote(
    config: &Config,
    settings: &SettingsStore,
    name: &str,
    req: PublishRequest,
) -> Result<PublishResult> {
    if !config.repo_exists(name) {
        bail!("repository not found");
    }

    let (host, clean) = resolve_publish_target(&req)?;
    let token = settings
        .token_for_host(&host)
        .ok_or_else(|| anyhow::anyhow!("no PAT configured for {host}"))?;

    let created = if req.create {
        match host.as_str() {
            "github.com" => {
                let slug = req
                    .remote_url
                    .as_deref()
                    .or(req.github_repo.as_deref())
                    .context("remote URL is required")?;
                let (owner, repo) = parse_github_target(slug)?;
                ensure_github_repo(&token, &owner, &repo, req.private).await?
            }
            h if h.contains("gitlab") => {
                let slug = req
                    .remote_url
                    .as_deref()
                    .context("remote URL is required")?;
                let (namespace, repo) = if slug.contains("://") {
                    let url = Url::parse(&clean).context("invalid gitlab URL")?;
                    let path = url
                        .path()
                        .trim_start_matches('/')
                        .trim_end_matches(".git");
                    let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
                    if parts.len() < 2 {
                        bail!("use group/repo or a full GitLab HTTPS URL");
                    }
                    (
                        parts[parts.len() - 2].to_string(),
                        parts[parts.len() - 1].to_string(),
                    )
                } else {
                    parse_host_repo_target("gitlab.com", slug)?
                };
                ensure_gitlab_repo(&token, &namespace, &repo, req.private).await?
            }
            h if h.contains("bitbucket") => {
                let slug = req
                    .remote_url
                    .as_deref()
                    .context("remote URL is required")?;
                let (workspace, repo) = parse_host_repo_target("bitbucket.org", slug)?;
                ensure_bitbucket_repo(&token, &workspace, &repo, req.private).await?
            }
            other => bail!(
                "automatic repo creation is not supported for {other}; create the repo manually and uncheck Create"
            ),
        }
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

pub async fn publish_to_github(
    config: &Config,
    settings: &SettingsStore,
    name: &str,
    req: PublishToGitHubRequest,
) -> Result<PublishResult> {
    publish_to_remote(
        config,
        settings,
        name,
        PublishRequest {
            remote_url: Some(req.github_repo.clone()),
            host: Some("github.com".into()),
            create: req.create,
            private: req.private,
            github_repo: Some(req.github_repo),
        },
    )
    .await
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

    if let Ok(ws) = workspace::ensure_workspace(config, name) {
        workspace::ensure_upstream_remote(&ws, &clean)?;
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
    push_workspace(config, settings, name)
}

fn bare_branch_rev(bare: &std::path::Path, branch: &str) -> Option<String> {
    let refname = format!("refs/heads/{branch}");
    let out = git::run_git(Some(bare), &["rev-parse", &refname]).ok()?;
    if !out.success() {
        return None;
    }
    let rev = out.stdout.trim();
    if rev.is_empty() {
        None
    } else {
        Some(rev.to_string())
    }
}

fn rollback_bare_host(
    config: &Config,
    name: &str,
    branch: &str,
    rev: &str,
) -> Result<()> {
    let bare = config
        .repo_path(name)
        .canonicalize()
        .with_context(|| format!("resolve bare repo {}", name))?;
    git::run_git(
        Some(&bare),
        &["update-ref", &format!("refs/heads/{branch}"), rev],
    )?;
    if let Ok(ws) = workspace::ensure_workspace(config, name) {
        let _ = sync_remote_tracking_ref(&ws, "origin", branch);
    }
    Ok(())
}

/// After undoing a workspace commit, sync the Reaper bare host to the current workspace HEAD.
pub fn sync_bare_from_workspace(config: &Config, name: &str) -> Result<git::GitOutput> {
    let ws = workspace::ensure_workspace(config, name)?;
    let branch = current_branch(&ws)?;
    let bare = config
        .repo_path(name)
        .canonicalize()
        .with_context(|| format!("resolve bare repo {}", name))?;
    let bare_url = bare
        .to_str()
        .context("invalid bare repo path")?
        .to_string();
    let out = git::push_url(&ws, &bare_url, &branch)?;
    if out.success() {
        let _ = sync_remote_tracking_ref(&ws, "origin", &branch);
    }
    Ok(out)
}

fn push_workspace(
    config: &Config,
    settings: &SettingsStore,
    name: &str,
) -> Result<git::GitOutput> {
    if !config.repo_exists(name) {
        bail!("repository not found");
    }

    let ws = workspace::ensure_workspace(config, name)?;
    let branch = current_branch(&ws)?;
    let meta = metadata::load(config, name).unwrap_or_default();

    let mut steps: Vec<(&str, git::GitOutput)> = Vec::new();

    let bare = config
        .repo_path(name)
        .canonicalize()
        .with_context(|| format!("resolve bare repo {}", name))?;
    let bare_url = bare
        .to_str()
        .context("invalid bare repo path")?
        .to_string();
    let pre_bare_rev = bare_branch_rev(&bare, &branch);
    let local = git::push_url(&ws, &bare_url, &branch)?;
    sync_remote_tracking_ref(&ws, "origin", &branch)?;
    steps.push(("local", local));

    if let Some(clean) = meta.remote_url.filter(|u| !u.trim().is_empty()) {
        let auth_url = auth::authenticated_url(&clean, settings)?;
        if auth_url != bare_url {
            let remote = git::push_url(&ws, &auth_url, &branch)?;
            if !remote.success() {
                if let Some(rev) = pre_bare_rev.as_deref() {
                    let _ = rollback_bare_host(config, name, &branch, rev);
                }
            } else {
                sync_remote_tracking_ref(&ws, "upstream", &branch)?;
            }
            steps.push(("remote", remote));
        }
    }

    Ok(merge_git_outputs(steps))
}

fn current_branch(ws: &std::path::Path) -> Result<String> {
    let branch = git::run_git(Some(ws), &["branch", "--show-current"])?
        .stdout
        .trim()
        .to_string();
    if branch.is_empty() {
        bail!("could not determine current branch");
    }
    Ok(branch)
}

fn sync_remote_tracking_ref(ws: &std::path::Path, remote: &str, branch: &str) -> Result<()> {
    let head = git::run_git(Some(ws), &["rev-parse", "HEAD"])?
        .stdout
        .trim()
        .to_string();
    if head.is_empty() {
        return Ok(());
    }
    let tracking = format!("refs/remotes/{remote}/{branch}");
    let _ = git::run_git(Some(ws), &["update-ref", &tracking, &head]);
    Ok(())
}

fn merge_git_outputs(steps: Vec<(&str, git::GitOutput)>) -> git::GitOutput {
    let mut stdout = String::new();
    let mut stderr = String::new();
    let mut exit_code = 0;
    for (label, out) in steps {
        append_labeled_output(&mut stdout, label, &out.stdout);
        append_labeled_output(&mut stderr, label, &out.stderr);
        if !out.success() {
            exit_code = out.exit_code;
        }
    }
    git::GitOutput {
        stdout,
        stderr,
        exit_code,
    }
}

fn append_labeled_output(dest: &mut String, label: &str, chunk: &str) {
    let chunk = chunk.trim();
    if chunk.is_empty() {
        return;
    }
    if !dest.is_empty() {
        dest.push('\n');
    }
    dest.push_str(&format!("[{label}]\n{chunk}"));
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
    pub secret_warnings: Vec<workspace::secret_scan::SecretFinding>,
}

pub fn push_preview(config: &Config, settings: &SettingsStore, name: &str) -> Result<PushPreview> {
    let _ = settings;
    if !config.repo_exists(name) {
        bail!("repository not found");
    }

    let meta = metadata::load(config, name).unwrap_or_default();
    let remote_url = meta
        .remote_url
        .clone()
        .or_else(|| Some(config.clone_url(name)));

    let ws = workspace::ensure_workspace(config, name)?;
    let branch = current_branch(&ws)?;

    let upstream = upstream_label(&ws);
    let prefer_upstream = meta.remote_url.is_some();
    let range = unpushed_commit_range(&ws, &branch, prefer_upstream)?;
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
    let secret_warnings =
        workspace::secret_scan::scan_push_files(&ws, &files, range.as_deref(), &branch);
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
        secret_warnings,
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

fn unpushed_commit_range(
    ws: &std::path::Path,
    branch: &str,
    prefer_upstream: bool,
) -> Result<Option<String>> {
    if prefer_upstream {
        let upstream_branch = format!("upstream/{branch}");
        if let Ok(verify) = git::run_git(Some(ws), &["rev-parse", "--verify", &upstream_branch]) {
            if verify.success() {
                return Ok(Some(format!("{upstream_branch}..HEAD")));
            }
        }
    }
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
