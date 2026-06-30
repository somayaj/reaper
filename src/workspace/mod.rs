pub mod conflict;
mod diagnostics;
mod exec;
pub mod exec_stream;
mod ruby_nav;
mod shell;
mod solargraph;
pub mod terminal;
mod classpath;
pub use classpath::CompletionItem;
mod gradle;
mod maven;
mod inline_context;
mod language_compiler_context;
mod index_jobs;
mod java;
mod java_diagnostics;
mod java_sources;
mod java_ecosystem;
mod java_format;
mod languages;
mod project_jobs;
mod project_profile;
mod quick_fix;
mod run_project;
mod spring_props;
mod symbols;

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Serialize;

use crate::config::Config;
use crate::git::{self, GitOutput};
use crate::repos::metadata;

pub fn is_git_checkout(path: &Path) -> bool {
    path.is_dir() && path.join(".git").exists()
}

/// Resolved project folder: local import path when set, otherwise the managed workspace clone.
pub fn project_folder(config: &Config, name: &str) -> Option<PathBuf> {
    if let Ok(meta) = metadata::load(config, name) {
        if let Some(ref local) = meta.local_path {
            let path = PathBuf::from(local);
            if is_git_checkout(&path) {
                return path.canonicalize().ok().or(Some(path));
            }
        }
    }
    let ws = config.workspace_path(name);
    if is_git_checkout(&ws) {
        return ws.canonicalize().ok().or(Some(ws));
    }
    None
}

pub fn ensure_workspace(config: &Config, name: &str) -> Result<PathBuf> {
    if !config.repo_exists(name) {
        bail!("repository not found");
    }
    config.ensure_dirs()?;

    if let Ok(meta) = metadata::load(config, name) {
        if let Some(ref local) = meta.local_path {
            let path = PathBuf::from(local);
            if is_git_checkout(&path) {
                let ws = path
                    .canonicalize()
                    .with_context(|| format!("resolve project folder {}", path.display()))?;
                let _ = ensure_reaper_gitignore(&ws);
                return Ok(ws);
            }
            bail!(
                "project folder no longer exists or is not a git checkout: {}",
                local
            );
        }
    }

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
    let _ = ensure_reaper_gitignore(&ws);
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
    /// `main`, `test`, or `generated` for Maven/Gradle source roots.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_kind: Option<String>,
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

fn should_skip_tree_name(name: &str, is_dir: bool) -> bool {
    if name == ".git" {
        return true;
    }
    if !is_dir && name.ends_with(".class") {
        return true;
    }
    if is_dir {
        return matches!(
            name,
            "node_modules" | "target" | "build" | ".gradle" | "dist" | "out" | "bin"
                | ".idea" | ".vscode" | "vendor" | "tmp" | "log" | "storage" | ".reaper"
        );
    }
    false
}

fn collect_children(ws: &Path, dir: &Path, nodes: &mut Vec<FileNode>, recursive: bool) -> Result<()> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .with_context(|| format!("read dir {}", dir.display()))?
        .filter_map(|e| e.ok())
        .collect();
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = entry.path();
        if should_skip_tree_name(&name, path.is_dir()) {
            continue;
        }
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
                path: rel.clone(),
                node_type: "dir".into(),
                children,
                has_children,
                source_kind: java_sources::source_root_kind(&rel).map(str::to_string),
            });
        } else {
            nodes.push(FileNode {
                name,
                path: rel.clone(),
                node_type: "file".into(),
                children: None,
                has_children: false,
                source_kind: java_sources::source_kind_for_path(&rel).map(str::to_string),
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
                !should_skip_tree_name(&name, entry.path().is_dir())
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
    std::fs::write(&path, content).with_context(|| format!("write {}", path.display()))?;
    if rel_path.ends_with(".java") {
        let _ = classpath::patch_java_index_file(ws, rel_path, content);
    }
    Ok(())
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

pub fn create_dir(ws: &Path, rel_path: &str) -> Result<()> {
    let path = safe_join(ws, rel_path)?;
    if path.exists() {
        bail!("path already exists");
    }
    std::fs::create_dir_all(&path).with_context(|| format!("mkdir {}", path.display()))
}

pub fn reveal_in_system(ws: &Path, rel_path: &str) -> Result<()> {
    let ws_canon = ws
        .canonicalize()
        .with_context(|| format!("resolve workspace {}", ws.display()))?;
    let target = if rel_path.is_empty() {
        ws_canon
    } else {
        let path = safe_join(ws, rel_path)?;
        if !path.exists() {
            bail!("path not found");
        }
        path
    };

    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        let status = if target.is_dir() {
            Command::new("open").arg(&target).status()
        } else {
            Command::new("open").arg("-R").arg(&target).status()
        }
        .context("open in Finder")?;
        if !status.success() {
            bail!("failed to reveal in Finder");
        }
        return Ok(());
    }

    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        let status = if target.is_dir() {
            Command::new("explorer").arg(&target).status()
        } else {
            Command::new("explorer")
                .arg(format!("/select,{}", target.display()))
                .status()
        }
        .context("open in Explorer")?;
        if !status.success() {
            bail!("failed to reveal in Explorer");
        }
        return Ok(());
    }

    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        use std::process::Command;
        let open_target = if target.is_dir() {
            target
        } else {
            target
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or(ws_canon)
        };
        let status = Command::new("xdg-open")
            .arg(&open_target)
            .status()
            .context("open in file manager")?;
        if !status.success() {
            bail!("failed to reveal in file manager");
        }
    }

    Ok(())
}

pub fn safe_join(base: &Path, rel: &str) -> Result<PathBuf> {
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
    pub ahead: usize,
}

pub fn workspace_status(ws: &Path) -> Result<WorkspaceStatus> {
    let branch_out = git::run_git(Some(ws), &["branch", "--show-current"])?;
    let branch = branch_out.stdout.trim().to_string();

    let out = git::run_git(Some(ws), &["status", "--porcelain", "-b", "-uall"])?;
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
        let path = parse_porcelain_path(&line[3..]);
        if path.ends_with('/') {
            continue;
        }
        if is_reaper_internal_path(&path) {
            continue;
        }
        if is_ignored_status_path(&path) {
            continue;
        }
        if is_workspace_directory(ws, &path) {
            continue;
        }
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
        ahead: unpushed_commit_count(ws),
    })
}

fn unpushed_commit_count(ws: &Path) -> usize {
    if let Ok(out) = git::run_git(Some(ws), &["rev-list", "--count", "@{u}..HEAD"]) {
        if out.success() {
            return out.stdout.trim().parse().unwrap_or(0);
        }
    }
    0
}

/// Reaper writes indexes, build scratch, and diagnostics under `.reaper/` — not user changes.
fn is_reaper_internal_path(path: &str) -> bool {
    path == ".reaper" || path.starts_with(".reaper/")
}

/// Maven/Gradle build output and wrapper scripts — not user source edits.
fn is_ignored_status_path(path: &str) -> bool {
    let path = path.trim_end_matches('/');
    if path == "target" || path.starts_with("target/") {
        return true;
    }
    if path.split('/').any(|seg| seg == "target") {
        return true;
    }
    let name = path.rsplit('/').next().unwrap_or(path);
    matches!(name, "mvnw" | "mvnw.cmd" | "gradlew" | "gradlew.bat")
}

fn is_workspace_directory(ws: &Path, rel: &str) -> bool {
    ws.join(rel).is_dir()
}

const DEFAULT_GITIGNORE_LINES: &[&str] = &[".reaper/", "target/"];

fn gitignore_has_entry(content: &str, entry: &str) -> bool {
    let key = entry.trim_end_matches('/');
    content.lines().any(|line| {
        let t = line.trim();
        match key {
            ".reaper" => matches!(t, ".reaper" | ".reaper/" | "/.reaper/" | "**/.reaper/"),
            "target" => matches!(t, "target" | "target/" | "/target/" | "**/target/"),
            _ => t == entry || t == key || t == format!("/{key}/"),
        }
    })
}

/// Append Reaper/build defaults to `.gitignore` when missing.
pub fn ensure_reaper_gitignore(ws: &Path) -> Result<()> {
    if !is_git_checkout(ws) {
        return Ok(());
    }
    let ignore_path = ws.join(".gitignore");
    let content = if ignore_path.exists() {
        std::fs::read_to_string(&ignore_path)?
    } else {
        String::new()
    };
    let missing: Vec<&str> = DEFAULT_GITIGNORE_LINES
        .iter()
        .copied()
        .filter(|line| !gitignore_has_entry(&content, line))
        .collect();
    if missing.is_empty() {
        return Ok(());
    }
    let sep = if content.is_empty() || content.ends_with('\n') {
        ""
    } else {
        "\n"
    };
    let block = missing.join("\n");
    if content.is_empty() {
        std::fs::write(&ignore_path, format!("{block}\n"))?;
    } else {
        std::fs::write(&ignore_path, format!("{content}{sep}{block}\n"))?;
    }
    Ok(())
}

fn parse_porcelain_path(raw: &str) -> String {
    let s = raw.trim();
    let path = if let Some((_old, new)) = s.split_once(" -> ") {
        new.trim()
    } else {
        s
    };
    unquote_git_path(path)
}

fn unquote_git_path(s: &str) -> String {
    if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
        s[1..s.len() - 1]
            .replace("\\n", "\n")
            .replace("\\t", "\t")
            .replace("\\\"", "\"")
            .replace("\\\\", "\\")
    } else {
        s.to_string()
    }
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
    if let Some(rel) = path {
        return workspace_diff_path(ws, rel, staged);
    }
    let mut args = vec!["diff", "--no-color", "-U3"];
    if staged {
        args.push("--cached");
    }
    let out = git::run_git(Some(ws), &args)?;
    Ok(out.stdout)
}

fn workspace_diff_path(ws: &Path, rel: &str, staged: bool) -> Result<String> {
    let mut args = vec!["diff", "--no-color", "-U3"];
    if staged {
        args.push("--cached");
    }
    args.push("--");
    args.push(rel);
    let out = git::run_git(Some(ws), &args)?;
    if !out.stdout.trim().is_empty() {
        return Ok(out.stdout);
    }
    if staged {
        return Ok(String::new());
    }
    let full = ws.join(rel);
    if full.is_file() {
        return diff_against_empty(ws, rel);
    }
    // Deleted tracked file — diff vs last commit.
    let head = git::run_git(
        Some(ws),
        &["diff", "--no-color", "-U3", "HEAD", "--", rel],
    )?;
    Ok(head.stdout)
}

/// Untracked files have no index entry — compare against /dev/null.
fn diff_against_empty(ws: &Path, rel: &str) -> Result<String> {
    let null = if cfg!(windows) { "NUL" } else { "/dev/null" };
    let out = git::run_git(
        Some(ws),
        &["diff", "--no-color", "-U3", "--no-index", "--", null, rel],
    )?;
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

pub fn commit_changes(
    ws: &Path,
    message: &str,
    paths: Option<&[String]>,
    push: bool,
) -> Result<GitOutput> {
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
    if push {
        git::run_git(Some(ws), &["push"])
    } else {
        Ok(commit)
    }
}

pub fn commit_and_push(ws: &Path, message: &str, paths: Option<&[String]>) -> Result<GitOutput> {
    commit_changes(ws, message, paths, true)
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

pub fn stream_workspace_shell(
    ws: &Path,
    cwd_rel: Option<&str>,
    command: &str,
    tx: tokio::sync::mpsc::Sender<exec_stream::ExecStreamEvent>,
) -> Result<i32> {
    exec_stream::stream_shell(ws, cwd_rel, command, tx)
}

pub use exec_stream::ExecStreamEvent;

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

pub fn run_project_info(ws: &Path, rel_path: &str) -> Result<run_project::RunProjectInfo> {
    run_project::run_project_info(ws, rel_path)
}

pub fn run_context(
    ws: &Path,
    rel_path: &str,
    content: Option<&str>,
    line: u32,
) -> Result<run_project::RunContext> {
    run_project::run_context(ws, rel_path, content, line)
}

pub use run_project::{AiRunTargetHint, JavaRunTarget, RunContext, RunProjectInfo};

pub fn apply_ai_run_target(target: &mut JavaRunTarget, hint: &AiRunTargetHint) {
    run_project::apply_ai_run_target(target, hint);
}

pub fn needs_ai_run_classification(target: &JavaRunTarget, content: &str) -> bool {
    run_project::needs_ai_run_classification(target, content)
}

pub fn maven_project_info(ws: &Path, rel_path: &str) -> Result<maven::MavenProjectInfo> {
    maven::maven_project_info(ws, rel_path)
}

pub fn run_gradle(ws: &Path, rel_path: &str, task: &str) -> Result<GitOutput> {
    gradle::run_gradle(ws, rel_path, task)
}

pub fn stream_workspace_gradle(
    ws: &Path,
    rel_path: &str,
    task: &str,
    tx: tokio::sync::mpsc::Sender<exec_stream::ExecStreamEvent>,
) -> Result<i32> {
    exec_stream::stream_gradle(ws, rel_path, task, tx)
}

pub fn stream_workspace_run_task(
    ws: &Path,
    rel_path: &str,
    task: &str,
    tx: tokio::sync::mpsc::Sender<exec_stream::ExecStreamEvent>,
) -> Result<i32> {
    run_project::stream_run_task(ws, rel_path, task, tx)
}

pub fn stream_workspace_maven(
    ws: &Path,
    rel_path: &str,
    goal: &str,
    tx: tokio::sync::mpsc::Sender<exec_stream::ExecStreamEvent>,
) -> Result<i32> {
    exec_stream::stream_maven(ws, rel_path, goal, tx)
}

pub fn stream_workspace_java_main(
    ws: &Path,
    rel_path: &str,
    tx: tokio::sync::mpsc::Sender<exec_stream::ExecStreamEvent>,
) -> Result<i32> {
    exec_stream::stream_java_main(ws, rel_path, tx)
}

pub fn java_file_context(
    ws: &Path,
    rel_path: &str,
    content: &str,
    line: u32,
) -> Result<java_ecosystem::JavaFileContext> {
    java_ecosystem::detect_java_file_context(ws, rel_path, content, line)
}

pub fn java_test_methods(
    ws: &Path,
    rel_path: &str,
    content: &str,
) -> Result<Vec<java_ecosystem::TestMethodMarker>> {
    let _ = safe_join(ws, rel_path)?;
    Ok(java_ecosystem::list_test_methods(rel_path, content))
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

pub fn java_index_needs_refresh(ws: &Path) -> bool {
    classpath::java_index_needs_refresh(ws)
}

pub fn detect_project_profile(ws: &Path) -> Result<project_profile::ProjectProfile> {
    project_profile::detect(ws)
}

pub use index_jobs::{JavaIndexJobs, JavaIndexStatus};
pub use project_jobs::{ProjectIndexJobs, ProjectIndexStatus};
pub use project_profile::ProjectProfile;
pub use quick_fix::{QuickFix, QuickFixDiagnostic, QuickFixEdit, suggest_local_quick_fixes};

/// Ensure Homebrew and common developer tools are on PATH (GUI .app launches).
pub fn ensure_developer_path() {
    exec::ensure_developer_path();
}

/// Go-to-definition from these paths uses the Gradle/Java index; Ruby and other languages do not.
pub fn definition_uses_java_index(from_path: &str) -> bool {
    classpath::is_java_like(from_path)
}

pub fn uses_spring_property_completions(from_path: &str) -> bool {
    spring_props::is_spring_config_file(from_path)
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
    if path.contains(".reaper/java-sources/jdk/") || hit.qualified.starts_with("java.") || hit.qualified.starts_with("jdk.") {
        0
    } else if path.contains("/org/springframework/") || hit.qualified.starts_with("org.springframework.") {
        120
    } else if path.contains("/src/") || path.starts_with("app/") {
        300
    } else if path.contains(".reaper/") {
        30
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
    out.into_iter()
        .take(limit)
        .map(|(_, hit)| hit)
        .filter(|hit| !hit.path.to_lowercase().ends_with(".class"))
        .collect()
}

pub fn find_definition(
    ws: &Path,
    from_path: &str,
    line: u32,
    column: u32,
) -> Result<Option<symbols::SymbolLocation>> {
    find_definition_with_content(ws, from_path, line, column, None)
}

pub fn find_symbol_hover_with_content(
    ws: &Path,
    from_path: &str,
    line: u32,
    column: u32,
    symbol: &str,
    content: Option<&str>,
) -> Result<Option<symbols::HoverInfo>> {
    let symbol = symbol.trim().trim_end_matches("()");
    if symbol.is_empty() {
        return Ok(None);
    }
    let content = match content {
        Some(c) => c.to_string(),
        None => read_file(ws, from_path)?,
    };

    if classpath::is_java_like(from_path) {
        let items = java_completions(ws, from_path, line, column, "", Some(&content), &[])?;
        if let Some(item) = items.into_iter().find(|i| i.label == symbol) {
            let mut info = hover_info_from_completion_item(&item);
            if info.documentation.is_none() {
                if let (Some(path), Some(line_no)) = (info.path.clone(), info.line) {
                    if let Ok(def_content) = read_file(ws, &path) {
                        let hit = symbols::SymbolLocation {
                            name: symbol.to_string(),
                            kind: info.kind.clone(),
                            path,
                            line: line_no,
                            column: 1,
                        };
                        info = symbols::hover_info_from_location(&def_content, &hit);
                    }
                }
            }
            return Ok(Some(info));
        }
    }

    if let Some(hit) = symbols::find_definition_for_symbol(ws, from_path, &content, symbol)? {
        let def_content = read_file(ws, &hit.path).unwrap_or(content);
        return Ok(Some(symbols::hover_info_from_location(&def_content, &hit)));
    }

    Ok(None)
}

pub fn find_member_hover_with_content(
    ws: &Path,
    from_path: &str,
    line: u32,
    column: u32,
    member: &str,
    content: Option<&str>,
) -> Result<Option<symbols::HoverInfo>> {
    let member = member.trim().trim_end_matches("()");
    if member.is_empty() {
        return Ok(None);
    }
    let content = match content {
        Some(c) => c.to_string(),
        None => read_file(ws, from_path)?,
    };
    if !classpath::is_java_like(from_path) {
        return Ok(None);
    }

    if let Some(col) = symbols::java_hover_column_for_member(&content, line, column, member) {
        if let Some(info) = find_hover_with_content(ws, from_path, line, col, Some(&content))? {
            return Ok(Some(info));
        }
    }

    let items = java_completions(ws, from_path, line, column, "", Some(&content), &[])?;
    let Some(item) = items.into_iter().find(|i| i.label == member) else {
        return Ok(None);
    };
    let mut info = hover_info_from_completion_item(&item);
    if info.documentation.is_none() {
        if let (Some(path), Some(line_no)) = (info.path.clone(), info.line) {
            if let Ok(def_content) = read_file(ws, &path) {
                let hit = symbols::SymbolLocation {
                    name: member.to_string(),
                    kind: info.kind.clone(),
                    path,
                    line: line_no,
                    column: 1,
                };
                info = symbols::hover_info_from_location(&def_content, &hit);
            }
        }
    }
    Ok(Some(info))
}

fn hover_info_from_completion_item(item: &classpath::CompletionItem) -> symbols::HoverInfo {
    symbols::HoverInfo {
        name: item.label.clone(),
        kind: item.kind.clone(),
        signature: item.detail.clone(),
        documentation: item.documentation.clone(),
        path: item.path.clone(),
        line: item.line,
    }
}

pub fn find_hover_with_content(
    ws: &Path,
    from_path: &str,
    line: u32,
    column: u32,
    content: Option<&str>,
) -> Result<Option<symbols::HoverInfo>> {
    let content = match content {
        Some(c) => c.to_string(),
        None => read_file(ws, from_path)?,
    };
    let hit = find_definition_with_content(ws, from_path, line, column, Some(&content))?;
    let Some(hit) = hit else {
        return Ok(None);
    };
    let def_content = read_file(ws, &hit.path).unwrap_or(content);
    Ok(Some(symbols::hover_info_from_location(&def_content, &hit)))
}

pub fn find_definition_with_content(
    ws: &Path,
    from_path: &str,
    line: u32,
    column: u32,
    content: Option<&str>,
) -> Result<Option<symbols::SymbolLocation>> {
    let content = match content {
        Some(c) => c.to_string(),
        None => read_file(ws, from_path)?,
    };
    // Java/Kotlin: indexed imports + JDK/dependency sources beat workspace-wide text search
    // (otherwise "String" in a .java file can jump to jquery.js).
    if classpath::is_java_like(from_path) {
        if let Some(hit) =
            classpath::find_external_definition(ws, from_path, line, column, &content)?
        {
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
    content: Option<&str>,
    overlays: &[(String, String)],
) -> Result<Vec<classpath::CompletionItem>> {
    let content = match content {
        Some(c) => c.to_string(),
        None => read_file(ws, from_path)?,
    };
    if spring_props::is_spring_config_file(from_path) {
        return spring_props::completions(ws, from_path, line, column, &content, prefix);
    }

    let mut items = if classpath::is_java_like(from_path) {
        classpath::java_completions(ws, from_path, line, column, &content, prefix, overlays)?
    } else {
        Vec::new()
    };

    let sym_items = symbols::completions(ws, from_path, &content, prefix, line, column)?;
    if items.is_empty() {
        return Ok(sym_items);
    }

    use std::collections::HashSet;
    let mut seen: HashSet<String> = items.iter().map(|i| i.label.clone()).collect();
    for item in sym_items {
        if seen.insert(item.label.clone()) {
            items.push(item);
        }
        if items.len() >= 80 {
            break;
        }
    }
    Ok(items)
}

pub fn format_file(ws: &Path, rel_path: &str, content: &str) -> Result<String> {
    let _path = safe_join(ws, rel_path)?;
    symbols::format_content(ws, rel_path, content)
}

pub fn file_diagnostics(
    ws: &Path,
    rel_path: &str,
    content: &str,
    overlays: &[(String, String)],
) -> Result<Vec<diagnostics::Diagnostic>> {
    diagnostics::check_file(ws, rel_path, content, overlays)
}

pub fn java_language_level(ws: &Path, rel_path: &str) -> u32 {
    java_diagnostics::java_language_level(ws, rel_path)
}

pub fn language_compiler_context(ws: &Path, rel_path: &str) -> language_compiler_context::LanguageCompilerContext {
    language_compiler_context::detect(ws, rel_path)
}

pub fn compiler_tool_ids_for_path(path: &str) -> Vec<&'static str> {
    languages::compiler_tool_ids_for_path(path)
}

pub fn language_for_path(path: &str) -> Option<&'static str> {
    languages::language_for_path(path)
}

pub fn file_extensions_for_tool(tool_id: &str) -> &'static [&'static str] {
    languages::file_extensions_for_tool(tool_id)
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

pub fn build_inline_completion_context(
    ws: &Path,
    path: &str,
    line: u32,
    column: u32,
    content: &str,
    line_prefix: &str,
) -> String {
    inline_context::build_inline_completion_context(ws, path, line, column, content, line_prefix)
}

pub fn inline_completion_fallback(
    ws: &Path,
    path: &str,
    line: u32,
    column: u32,
    content: &str,
    line_prefix: &str,
) -> Option<String> {
    inline_context::inline_completion_fallback(ws, path, line, column, content, line_prefix)
}

pub fn should_prefer_ai_statement_inline(
    path: &str,
    line_prefix: &str,
    content: &str,
    line: u32,
) -> bool {
    inline_context::should_prefer_ai_statement_inline(path, line_prefix, content, line)
}
