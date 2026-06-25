mod exec;
mod gradle;
mod java;
mod symbols;

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Serialize;

use crate::config::Config;
use crate::git::{self, GitOutput};

pub fn ensure_workspace(config: &Config, name: &str) -> Result<PathBuf> {
    if !config.repo_exists(name) {
        bail!("repository not found");
    }
    config.ensure_dirs()?;
    let bare = config.repo_path(name);
    let ws = config.workspace_path(name);

    if !ws.exists() {
        let out = git::run_git(
            None,
            &[
                "clone",
                bare.to_str().context("invalid bare path")?,
                ws.to_str().context("invalid workspace path")?,
            ],
        )?;
        if !out.success() {
            bail!("clone failed: {}", out.stderr.trim());
        }
    }
    Ok(ws)
}

pub fn sync_workspace(ws: &Path) -> Result<GitOutput> {
    git::run_git(Some(ws), &["pull", "--ff-only"])
}

#[derive(Debug, Serialize)]
pub struct FileNode {
    pub name: String,
    pub path: String,
    #[serde(rename = "type")]
    pub node_type: String,
    pub children: Option<Vec<FileNode>>,
}

pub fn build_tree(ws: &Path) -> Result<Vec<FileNode>> {
    let mut nodes = Vec::new();
    collect_children(ws, ws, &mut nodes)?;
    Ok(nodes)
}

fn collect_children(ws: &Path, dir: &Path, nodes: &mut Vec<FileNode>) -> Result<()> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .with_context(|| format!("read dir {}", dir.display()))?
        .filter_map(|e| e.ok())
        .collect();
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name == ".git" {
            continue;
        }
        let path = entry.path();
        let rel = path
            .strip_prefix(ws)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");

        if path.is_dir() {
            let mut children = Vec::new();
            collect_children(ws, &path, &mut children)?;
            nodes.push(FileNode {
                name,
                path: rel,
                node_type: "dir".into(),
                children: Some(children),
            });
        } else {
            nodes.push(FileNode {
                name,
                path: rel,
                node_type: "file".into(),
                children: None,
            });
        }
    }
    Ok(())
}

pub fn read_file(ws: &Path, rel_path: &str) -> Result<String> {
    let path = safe_join(ws, rel_path)?;
    if !path.is_file() {
        bail!("not a file");
    }
    std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))
}

pub fn write_file(ws: &Path, rel_path: &str, content: &str) -> Result<()> {
    let path = safe_join(ws, rel_path)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, content).with_context(|| format!("write {}", path.display()))
}

pub fn create_file(ws: &Path, rel_path: &str, content: &str) -> Result<()> {
    let path = safe_join(ws, rel_path)?;
    if path.exists() {
        bail!("file already exists");
    }
    write_file(ws, rel_path, content)
}

pub fn delete_path(ws: &Path, rel_path: &str) -> Result<()> {
    let path = safe_join(ws, rel_path)?;
    if path.is_dir() {
        std::fs::remove_dir_all(&path)?;
    } else if path.is_file() {
        std::fs::remove_file(&path)?;
    } else {
        bail!("path not found");
    }
    Ok(())
}

fn safe_join(base: &Path, rel: &str) -> Result<PathBuf> {
    if rel.contains("..") || rel.starts_with('/') {
        bail!("invalid path");
    }
    let canonical_base = base
        .canonicalize()
        .with_context(|| format!("resolve workspace {}", base.display()))?;

    let mut resolved = canonical_base.clone();
    for component in Path::new(rel).components() {
        match component {
            std::path::Component::Normal(c) => resolved.push(c),
            std::path::Component::CurDir => {}
            _ => bail!("invalid path"),
        }
    }

    if !resolved.starts_with(&canonical_base) {
        bail!("path escapes workspace");
    }
    Ok(resolved)
}

#[derive(Debug, Serialize)]
pub struct StatusFile {
    pub path: String,
    pub status: String,
    pub staged: bool,
}

#[derive(Debug, Serialize)]
pub struct WorkspaceStatus {
    pub branch: String,
    pub clean: bool,
    pub files: Vec<StatusFile>,
    pub stdout: String,
}

pub fn workspace_status(ws: &Path) -> Result<WorkspaceStatus> {
    let branch_out = git::run_git(Some(ws), &["branch", "--show-current"])?;
    let branch = branch_out.stdout.trim().to_string();

    let out = git::run_git(Some(ws), &["status", "--porcelain", "-b"])?;
    let mut files = Vec::new();
    for line in out.stdout.lines() {
        if line.starts_with("##") {
            continue;
        }
        if line.len() < 3 {
            continue;
        }
        let code = &line[..2];
        let path = line[3..].trim().to_string();
        let staged = code.chars().nth(0).is_some_and(|c| c != ' ' && c != '?');
        let unstaged = code.chars().nth(1).is_some_and(|c| c != ' ');
        if staged {
            files.push(StatusFile {
                path: path.clone(),
                status: status_label(code.chars().next().unwrap_or(' ')),
                staged: true,
            });
        }
        if unstaged {
            files.push(StatusFile {
                path,
                status: status_label(code.chars().nth(1).unwrap_or(' ')),
                staged: false,
            });
        }
    }

    Ok(WorkspaceStatus {
        clean: files.is_empty(),
        branch,
        files,
        stdout: out.stdout,
    })
}

fn status_label(c: char) -> String {
    match c {
        'M' => "modified".into(),
        'A' => "added".into(),
        'D' => "deleted".into(),
        'R' => "renamed".into(),
        '?' => "untracked".into(),
        _ => "changed".into(),
    }
}

pub fn workspace_diff(ws: &Path, path: Option<&str>, staged: bool) -> Result<String> {
    let mut args = vec!["diff"];
    if staged {
        args.push("--cached");
    }
    if let Some(p) = path {
        args.push(p);
    }
    let out = git::run_git(Some(ws), &args)?;
    Ok(out.stdout)
}

pub fn commit_and_push(ws: &Path, message: &str, paths: Option<&[String]>) -> Result<GitOutput> {
    if message.trim().is_empty() {
        bail!("commit message required");
    }
    match paths {
        Some(paths) if !paths.is_empty() => {
            for p in paths {
                let add = git::run_git(Some(ws), &["add", p])?;
                if !add.success() {
                    bail!("git add failed: {}", add.stderr.trim());
                }
            }
        }
        _ => {
            let add = git::run_git(Some(ws), &["add", "-A"])?;
            if !add.success() {
                bail!("git add failed: {}", add.stderr.trim());
            }
        }
    }
    let commit = git::run_git(Some(ws), &["commit", "-m", message])?;
    if !commit.success() {
        return Ok(commit);
    }
    git::run_git(Some(ws), &["push"])
}

pub fn checkout_branch(ws: &Path, branch: &str) -> Result<GitOutput> {
    git::run_git(Some(ws), &["checkout", branch])
}

pub fn run_workspace_git(ws: &Path, args: &[String]) -> Result<GitOutput> {
    git::run_workspace_command(ws, args)
}

pub fn java_main_info(ws: &Path, rel_path: &str) -> Result<java::JavaMainInfo> {
    java::java_main_info(ws, rel_path)
}

pub fn run_java_main(ws: &Path, rel_path: &str) -> Result<GitOutput> {
    java::run_java_main(ws, rel_path)
}

pub fn gradle_project_info(ws: &Path, rel_path: &str) -> Result<gradle::GradleProjectInfo> {
    gradle::gradle_project_info(ws, rel_path)
}

pub fn run_gradle(ws: &Path, rel_path: &str, task: &str) -> Result<GitOutput> {
    gradle::run_gradle(ws, rel_path, task)
}

pub fn find_definition(
    ws: &Path,
    from_path: &str,
    line: u32,
    column: u32,
) -> Result<Option<symbols::SymbolLocation>> {
    let content = read_file(ws, from_path)?;
    symbols::find_definition(ws, from_path, line, column, &content)
}

pub fn format_file(ws: &Path, rel_path: &str, content: &str) -> Result<String> {
    let _path = safe_join(ws, rel_path)?;
    symbols::format_content(rel_path, content)
}

pub fn ensure_upstream_remote(ws: &Path, clean_url: &str) -> Result<()> {
    git::set_remote_url(ws, "upstream", clean_url)
}
