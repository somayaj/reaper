pub mod conflict;
mod diagnostics;
mod exec;
mod ruby_nav;
mod shell;
mod solargraph;
mod classpath;
mod gradle;
mod index_jobs;
mod java;
mod java_diagnostics;
mod spring_props;
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<FileNode>>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub has_children: bool,
}

pub fn build_tree(ws: &Path) -> Result<Vec<FileNode>> {
    let mut nodes = Vec::new();
    collect_children(ws, ws, &mut nodes, true)?;
    Ok(nodes)
}

/// One directory level for lazy tree loading in the explorer.
pub fn build_tree_level(ws: &Path, rel_dir: Option<&str>) -> Result<Vec<FileNode>> {
    let dir = match rel_dir.filter(|s| !s.is_empty()) {
        None => ws.to_path_buf(),
        Some(p) => safe_join(ws, p)?,
    };
    if !dir.is_dir() {
        bail!("not a directory");
    }
    let mut nodes = Vec::new();
    collect_children(ws, &dir, &mut nodes, false)?;
    Ok(nodes)
}

fn should_skip_entry(name: &str) -> bool {
    name == ".git"
}

fn collect_children(ws: &Path, dir: &Path, nodes: &mut Vec<FileNode>, recursive: bool) -> Result<()> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .with_context(|| format!("read dir {}", dir.display()))?
        .filter_map(|e| e.ok())
        .collect();
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let name = entry.file_name().to_string_lossy().into_owned();
        if should_skip_entry(&name) {
            continue;
        }
        let path = entry.path();
        let rel = path
            .strip_prefix(ws)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");

        if path.is_dir() {
            let has_children = dir_has_listable_children(ws, &path);
            let children = if recursive {
                let mut nested = Vec::new();
                collect_children(ws, &path, &mut nested, true)?;
                Some(nested)
            } else {
                None
            };
            nodes.push(FileNode {
                name,
                path: rel,
                node_type: "dir".into(),
                children,
                has_children,
            });
        } else {
            nodes.push(FileNode {
                name,
                path: rel,
                node_type: "file".into(),
                children: None,
                has_children: false,
            });
        }
    }
    Ok(())
}

fn dir_has_listable_children(_ws: &Path, dir: &Path) -> bool {
    std::fs::read_dir(dir)
        .ok()
        .map(|read_dir| {
            read_dir.filter_map(|e| e.ok()).any(|entry| {
                let name = entry.file_name().to_string_lossy().into_owned();
                !should_skip_entry(&name)
            })
        })
        .unwrap_or(false)
}

pub fn read_file(ws: &Path, rel_path: &str) -> Result<String> {
    let path = if rel_path.starts_with('/') {
        PathBuf::from(rel_path)
    } else {
        safe_join(ws, rel_path)?
    };
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
    pub merge: conflict::MergeState,
    pub conflict_count: usize,
}

pub fn workspace_status(ws: &Path) -> Result<WorkspaceStatus> {
    let branch_out = git::run_git(Some(ws), &["branch", "--show-current"])?;
    let branch = branch_out.stdout.trim().to_string();

    let out = git::run_git(Some(ws), &["status", "--porcelain", "-b"])?;
    let merge = conflict::merge_state(ws);
    let mut files = Vec::new();
    let mut conflict_count = 0usize;
    for line in out.stdout.lines() {
        if line.starts_with("##") {
            continue;
        }
        if line.len() < 3 {
            continue;
        }
        let code = &line[..2];
        let path = line[3..].trim().to_string();
        if conflict::is_unmerged(code) {
            conflict_count += 1;
            files.push(StatusFile {
                path,
                status: "conflict".into(),
                staged: false,
            });
            continue;
        }
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
        merge,
        conflict_count,
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
    let mut args = vec!["diff", "--no-color", "-U3"];
    if staged {
        args.push("--cached");
    }
    if let Some(p) = path {
        args.push(p);
    }
    let out = git::run_git(Some(ws), &args)?;
    Ok(out.stdout)
}

pub fn diff_for_commit(ws: &Path) -> Result<String> {
    let out = git::run_git(Some(ws), &["diff", "HEAD", "--no-color", "-U2"])?;
    if !out.stdout.trim().is_empty() {
        return Ok(out.stdout);
    }
    let status = git::run_git(Some(ws), &["status", "--porcelain", "-u"])?;
    Ok(format!(
        "(new or untracked files — no tracked diff)\n\n{}",
        status.stdout
    ))
}

pub fn commit_diff(ws: &Path, hash: &str) -> Result<String> {
    if hash.len() < 7 || hash.len() > 40 || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
        bail!("invalid commit hash");
    }
    let out = git::run_git(
        Some(ws),
        &["show", hash, "--pretty=format:", "-p", "-U3", "--no-color"],
    )?;
    if !out.success() {
        bail!("{}", out.stderr.trim());
    }
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

pub fn run_workspace_shell(ws: &Path, cwd_rel: Option<&str>, command: &str) -> Result<GitOutput> {
    shell::run_shell(ws, cwd_rel, command)
}

pub fn change_workspace_directory(
    ws: &Path,
    cwd_rel: Option<&str>,
    target: &str,
) -> Result<String> {
    shell::change_directory(ws, cwd_rel, target)
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

pub fn is_gradle_workspace(ws: &Path) -> bool {
    classpath::is_gradle_workspace(ws)
}

pub fn warm_java_index(ws: &Path) -> Result<classpath::WarmIndexStatus> {
    classpath::warm_index(ws)
}

pub fn peek_java_index(ws: &Path) -> Result<classpath::WarmIndexStatus> {
    classpath::peek_index_status(ws)
}

pub use index_jobs::{JavaIndexJobs, JavaIndexStatus};

/// Go-to-definition from these paths uses the Gradle/Java index; Ruby and other languages do not.
pub fn definition_uses_java_index(from_path: &str) -> bool {
    classpath::is_java_like(from_path)
}

pub fn search_classes(ws: &Path, query: &str, limit: usize) -> Result<Vec<symbols::ClassSearchHit>> {
    let limit = limit.clamp(1, 100);
    let skip_java = classpath::is_gradle_workspace(ws);
    let mut scored: Vec<(u32, symbols::ClassSearchHit)> = Vec::new();

    for hit in classpath::search_indexed_classes(ws, query, limit.saturating_mul(3))? {
        let base = symbols::class_name_match_score(query, &hit.name, &hit.qualified).unwrap_or(0);
        scored.push((base + indexed_hit_score(&hit), hit));
    }
    for hit in symbols::search_workspace_classes(ws, query, limit.saturating_mul(3), skip_java)? {
        let base = symbols::class_name_match_score(query, &hit.name, &hit.qualified).unwrap_or(0);
        scored.push((base + workspace_hit_score(&hit), hit));
    }

    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.name.cmp(&b.1.name)));
    Ok(dedupe_search_classes(scored, limit))
}

fn indexed_hit_score(hit: &symbols::ClassSearchHit) -> u32 {
    let path = hit.path.replace('\\', "/");
    if path.contains(".reaper/") || path.contains("/org/springframework/") {
        0
    } else if path.contains("/src/") || path.starts_with("app/") {
        300
    } else {
        50
    }
}

fn workspace_hit_score(hit: &symbols::ClassSearchHit) -> u32 {
    if hit.path.starts_with("app/") || hit.path.contains("/src/") {
        400
    } else {
        150
    }
}

fn dedupe_search_classes(
    scored: Vec<(u32, symbols::ClassSearchHit)>,
    limit: usize,
) -> Vec<symbols::ClassSearchHit> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for (score, hit) in scored {
        let key = format!("{}:{}:{}", hit.name, hit.path, hit.line);
        if seen.insert(key) {
            out.push((score, hit));
        }
    }
    out.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.name.cmp(&b.1.name)));
    out.into_iter().take(limit).map(|(_, hit)| hit).collect()
}

pub fn find_definition(
    ws: &Path,
    from_path: &str,
    line: u32,
    column: u32,
) -> Result<Option<symbols::SymbolLocation>> {
    let content = read_file(ws, from_path)?;
    // Java/Kotlin: indexed imports + JDK/dependency sources beat workspace-wide text search
    // (otherwise "String" in a .java file can jump to jquery.js).
    if classpath::is_java_like(from_path) {
        if let Some(hit) = classpath::find_external_definition(ws, from_path, line, column, &content)? {
            return Ok(Some(hit));
        }
    }
    if ruby_nav::is_ruby_path(from_path) {
        if let Some(hit) = solargraph::find_definition(ws, from_path, line, column, &content)? {
            return Ok(Some(hit));
        }
        if let Some(hit) = ruby_nav::find_definition(ws, line, column, &content)? {
            return Ok(Some(hit));
        }
    }
    symbols::find_definition(ws, from_path, line, column, &content)
}

pub fn java_completions(
    ws: &Path,
    from_path: &str,
    line: u32,
    column: u32,
    prefix: &str,
) -> Result<Vec<classpath::CompletionItem>> {
    let content = read_file(ws, from_path)?;
    if spring_props::is_spring_config_file(from_path) {
        return spring_props::completions(ws, from_path, line, column, &content, prefix);
    }
    classpath::java_completions(ws, from_path, line, column, &content, prefix)
}

pub fn format_file(ws: &Path, rel_path: &str, content: &str) -> Result<String> {
    let _path = safe_join(ws, rel_path)?;
    symbols::format_content(rel_path, content)
}

pub fn file_diagnostics(
    ws: &Path,
    rel_path: &str,
    content: &str,
) -> Result<Vec<diagnostics::Diagnostic>> {
    diagnostics::check_file(ws, rel_path, content)
}

pub fn conflict_stages(ws: &Path, rel_path: &str) -> Result<conflict::ConflictStages> {
    conflict::conflict_stages(ws, rel_path)
}

pub fn mark_conflict_resolved(ws: &Path, rel_path: &str) -> Result<GitOutput> {
    conflict::mark_conflict_resolved(ws, rel_path)
}

pub fn ensure_upstream_remote(ws: &Path, clean_url: &str) -> Result<()> {
    git::set_remote_url(ws, "upstream", clean_url)
}
