use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct GitOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

impl GitOutput {
    pub fn success(&self) -> bool {
        self.exit_code == 0
    }
}

pub fn run_git(cwd: Option<&Path>, args: &[&str]) -> Result<GitOutput> {
    let mut cmd = Command::new("git");
    cmd.args(args);
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    let output = cmd
        .output()
        .with_context(|| format!("failed to spawn git {}", args.join(" ")))?;

    Ok(GitOutput {
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        exit_code: output.status.code().unwrap_or(-1),
    })
}

pub fn init_bare_repo(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let out = run_git(None, &["init", "--bare", path.to_str().unwrap()])?;
    if !out.success() {
        bail!("git init failed: {}", out.stderr.trim());
    }
    Ok(())
}

pub fn clone_bare(auth_url: &str, dest: &Path) -> Result<GitOutput> {
    if dest.exists() {
        bail!("destination already exists");
    }
    run_git(
        None,
        &[
            "clone",
            "--bare",
            auth_url,
            dest.to_str().context("invalid dest path")?,
        ],
    )
}

pub fn clone_bare_local(src: &Path, dest: &Path) -> Result<GitOutput> {
    if dest.exists() {
        bail!("destination already exists");
    }
    let src = src
        .canonicalize()
        .with_context(|| format!("resolve source path {}", src.display()))?;
    if !is_git_repo(&src) {
        bail!("not a git repository: {}", src.display());
    }
    run_git(
        None,
        &[
            "clone",
            "--bare",
            src.to_str().context("invalid source path")?,
            dest.to_str().context("invalid dest path")?,
        ],
    )
}

pub fn is_git_repo(path: &Path) -> bool {
    run_git(Some(path), &["rev-parse", "--git-dir"])
        .map(|o| o.success())
        .unwrap_or(false)
}

pub fn remote_url(repo: &Path, name: &str) -> Option<String> {
    let out = run_git(Some(repo), &["remote", "get-url", name]).ok();
    out.filter(|o| o.success())
        .map(|o| o.stdout.trim().to_string())
        .filter(|u| !u.is_empty())
}

pub fn set_remote_url(repo: &Path, name: &str, url: &str) -> Result<()> {
    let existing = run_git(Some(repo), &["remote", "get-url", name])?;
    if existing.success() {
        let out = run_git(Some(repo), &["remote", "set-url", name, url])?;
        if !out.success() {
            bail!("{}", out.stderr.trim());
        }
    } else {
        let out = run_git(Some(repo), &["remote", "add", name, url])?;
        if !out.success() {
            bail!("{}", out.stderr.trim());
        }
    }
    Ok(())
}

pub fn fetch_url_into_bare(bare: &Path, auth_url: &str) -> Result<GitOutput> {
    run_git(
        Some(bare),
        &[
            "fetch",
            auth_url,
            "+refs/heads/*:refs/heads/*",
            "+refs/tags/*:refs/tags/*",
        ],
    )
}

pub fn push_url(ws: &Path, auth_url: &str, branch: &str) -> Result<GitOutput> {
    let refspec = format!("HEAD:refs/heads/{branch}");
    run_git(Some(ws), &["push", auth_url, &refspec])
}

pub fn seed_bare_repo_with_readme(bare_path: &Path, readme: &str) -> Result<()> {
    let bare_abs = bare_path
        .canonicalize()
        .with_context(|| format!("resolve bare repo path {}", bare_path.display()))?;
    let temp = tempfile_dir(&bare_abs)?;
    let _guard = TempDirGuard(temp.clone());

    run_git(Some(&temp), &["init"])?;
    std::fs::write(temp.join("README.md"), readme)?;
    run_git(Some(&temp), &["add", "README.md"])?;
    run_git(
        Some(&temp),
        &[
            "commit",
            "-m",
            "Initial commit",
            "--author",
            "Reaper <reaper@localhost>",
        ],
    )?;
    run_git(
        Some(&temp),
        &["branch", "-M", "main"],
    )?;
    run_git(
        Some(&temp),
        &[
            "remote",
            "add",
            "origin",
            bare_abs.to_str().context("invalid bare path")?,
        ],
    )?;
    let push = run_git(Some(&temp), &["push", "-u", "origin", "main"])?;
    if !push.success() {
        bail!("failed to seed repo: {}", push.stderr.trim());
    }
    let _ = run_git(
        Some(&bare_abs),
        &["symbolic-ref", "HEAD", "refs/heads/main"],
    );
    Ok(())
}

fn tempfile_dir(near: &Path) -> Result<std::path::PathBuf> {
    let parent = near.parent().unwrap_or_else(|| Path::new("."));
    let dir = parent.join(format!(
        ".seed-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

struct TempDirGuard(std::path::PathBuf);

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

const LEGACY_BRANCH: &str = "master";

fn is_legacy_branch(name: &str) -> bool {
    name == LEGACY_BRANCH
}

pub fn list_branches(repo: &Path) -> Result<Vec<String>> {
    let out = run_git(Some(repo), &["branch", "--format=%(refname:short)"])?;
    if !out.success() {
        bail!("{}", out.stderr.trim());
    }
    let mut branches: Vec<String> = out
        .stdout
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !is_legacy_branch(l))
        .map(str::to_string)
        .collect();

    // Include remote-only branches (e.g. origin/feature not checked out locally).
    if let Ok(remote_out) = run_git(
        Some(repo),
        &["branch", "-r", "--format=%(refname:short)"],
    ) {
        if remote_out.success() {
            for line in remote_out.stdout.lines() {
                let line = line.trim();
                if line.is_empty() || line.ends_with("/HEAD") || is_legacy_branch(line) {
                    continue;
                }
                let name = line.strip_prefix("origin/").unwrap_or(line).to_string();
                if !branches.iter().any(|b| b == &name) {
                    branches.push(name);
                }
            }
        }
    }

    branches.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
    Ok(branches)
}

fn remote_default_branch(repo: &Path) -> Option<String> {
    let out = run_git(
        Some(repo),
        &["symbolic-ref", "--short", "refs/remotes/origin/HEAD"],
    )
    .ok()?;
    if out.success() {
        let name = out.stdout.trim().to_string();
        if !name.is_empty() && !is_legacy_branch(&name) {
            return Some(name);
        }
    }
    let out = run_git(Some(repo), &["remote", "show", "origin"]).ok()?;
    if !out.success() {
        return None;
    }
    for line in out.stdout.lines() {
        if let Some(rest) = line.trim().strip_prefix("HEAD branch: ") {
            let name = rest.trim();
            if !name.is_empty() && !is_legacy_branch(name) {
                return Some(name.to_string());
            }
        }
    }
    None
}

pub fn default_branch(repo: &Path) -> Result<String> {
    if let Some(name) = remote_default_branch(repo) {
        return Ok(name);
    }
    let out = run_git(Some(repo), &["symbolic-ref", "--short", "HEAD"])?;
    if out.success() {
        let name = out.stdout.trim();
        if !name.is_empty() && !is_legacy_branch(name) {
            return Ok(name.to_string());
        }
    }
    let branches = list_branches(repo)?;
    if branches.iter().any(|b| b == "main") {
        return Ok("main".into());
    }
    branches
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("no default branch"))
}

#[derive(Debug, Serialize)]
pub struct CommitInfo {
    pub hash: String,
    pub author: String,
    pub date: String,
    pub subject: String,
}

pub fn log(repo: &Path, limit: usize) -> Result<Vec<CommitInfo>> {
    let limit = limit.to_string();
    let out = run_git(
        Some(repo),
        &[
            "log",
            &format!("-{limit}"),
            "--format=%H%x1f%an%x1f%ai%x1f%s",
        ],
    )?;
    if !out.success() {
        return Ok(vec![]);
    }
    Ok(out
        .stdout
        .lines()
        .filter_map(|line| {
            let mut parts = line.split('\x1f');
            Some(CommitInfo {
                hash: parts.next()?.to_string(),
                author: parts.next()?.to_string(),
                date: parts.next()?.to_string(),
                subject: parts.next()?.to_string(),
            })
        })
        .collect())
}

#[derive(Debug, Serialize)]
pub struct TreeEntry {
    pub mode: String,
    pub kind: String,
    pub hash: String,
    pub path: String,
}

pub fn ls_tree(repo: &Path, rev: &str, path: Option<&str>) -> Result<Vec<TreeEntry>> {
    let mut args = vec!["ls-tree", rev];
    if let Some(p) = path {
        args.push(p);
    }
    let out = run_git(Some(repo), &args)?;
    if !out.success() {
        bail!("{}", out.stderr.trim());
    }
    Ok(out
        .stdout
        .lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let mode = parts.next()?.to_string();
            let kind = parts.next()?.to_string();
            let hash = parts.next()?.to_string();
            let path = parts.collect::<Vec<_>>().join(" ");
            Some(TreeEntry {
                mode,
                kind,
                hash,
                path,
            })
        })
        .collect())
}

pub fn show_file(repo: &Path, rev: &str, path: &str) -> Result<String> {
    let spec = format!("{rev}:{path}");
    let out = run_git(Some(repo), &["show", &spec])?;
    if !out.success() {
        bail!("{}", out.stderr.trim());
    }
    Ok(out.stdout)
}

const READONLY_SUBCOMMANDS: &[&str] = &[
    "status", "log", "branch", "show", "diff", "tag", "remote", "rev-parse", "describe",
    "shortlog", "blame", "ls-tree", "cat-file", "rev-list", "name-rev", "for-each-ref",
];

const WORKSPACE_SUBCOMMANDS: &[&str] = &[
    "status", "log", "branch", "show", "diff", "tag", "remote", "rev-parse", "describe",
    "shortlog", "blame", "ls-tree", "cat-file", "rev-list", "name-rev", "for-each-ref",
    "add", "commit", "checkout", "pull", "push", "fetch", "merge", "rebase", "cherry-pick",
    "stash", "reset", "switch", "restore", "clean", "mv", "rm",
];

fn validate_git_args(args: &[String]) -> Result<()> {
    for arg in args {
        if arg.contains("--upload-pack") || arg.contains("--receive-pack") {
            bail!("unsafe argument rejected");
        }
    }
    Ok(())
}

pub fn run_allowed_command(repo: &Path, args: &[String]) -> Result<GitOutput> {
    if args.is_empty() {
        bail!("no git command provided");
    }
    let sub = args[0].as_str();
    if !READONLY_SUBCOMMANDS.contains(&sub) {
        bail!(
            "command '{sub}' is not allowed; permitted: {}",
            READONLY_SUBCOMMANDS.join(", ")
        );
    }
    validate_git_args(&args[1..])?;
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    run_git(Some(repo), &refs)
}

pub fn run_workspace_command(ws: &Path, args: &[String]) -> Result<GitOutput> {
    if args.is_empty() {
        bail!("no git command provided");
    }
    let sub = args[0].as_str();
    if !WORKSPACE_SUBCOMMANDS.contains(&sub) {
        bail!(
            "command '{sub}' is not allowed; permitted: {}",
            WORKSPACE_SUBCOMMANDS.join(", ")
        );
    }
    validate_git_args(args)?;
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    run_git(Some(ws), &refs)
}

pub fn repo_description(repo: &Path) -> Option<String> {
    let path = repo.join("description");
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && s != "Unnamed repository; edit this file 'description' to name the repository.")
}

pub fn set_repo_description(repo: &Path, description: &str) -> Result<()> {
    std::fs::write(repo.join("description"), format!("{description}\n"))?;
    Ok(())
}
