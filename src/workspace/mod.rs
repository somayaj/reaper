pub mod conflict;
mod diagnostics;
mod exec;
pub mod exec_stream;
mod ruby_nav;
mod shell;
mod solargraph;
mod clangd;
mod jdtls;
mod lsp;
pub mod terminal;
mod build_tasks;
mod elide_pkl;
mod native_build_tasks;
mod package_manifest;
mod classpath;
pub use classpath::CompletionItem;
mod gradle;
mod maven;
mod maven_classpath_inflight;
mod inline_context;
mod language_compiler_context;
mod index_jobs;
mod java;
mod java_diagnostics;
pub use java_diagnostics::JavaDiagScope;
pub use diagnostics::FileDiagnosticsResult;
pub use java_javac_inflight::cancel_inflight_diagnostics;
mod java_javac_inflight;
mod java_index_patch;
#[cfg(test)]
mod java_save_javac_loop;
mod java_classpath;
mod java_psi;
mod java_sources;
mod java_ecosystem;
mod java_format;
mod java_synthetic_members;
mod languages;
pub mod ast;
mod project_jobs;
mod project_profile;
mod quick_fix;
mod coverage;
mod db_viewer;
pub(crate) mod db_ssh_tunnel;
mod run_project;
mod debug;
mod spring_props;
pub mod secret_scan;
mod symbols;
mod workspace_search;

use std::collections::HashMap;
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

/// True when this workspace should use jdtls (Maven/Gradle, plain `.java` trees, etc.).
pub fn workspace_uses_jdtls(ws: &Path, profile: &project_profile::ProjectProfile) -> bool {
    profile.languages.iter().any(|l| l == "java")
        || profile.indexers.iter().any(|i| i == "java")
        || classpath::is_java_indexable_workspace(ws)
}

pub fn jdtls_enabled() -> bool {
    jdtls::is_enabled()
}

pub fn jdtls_workspace_ready(ws: &Path) -> bool {
    jdtls::workspace_ready(ws)
}

/// Start jdtls when a Java workspace opens (blocks until initialized).
pub fn warm_jdtls_workspace(ws: &Path) -> Result<()> {
    jdtls::warm_workspace(ws)
}

/// Kick off jdtls warm in the background (no-op when disabled or already ready).
pub fn spawn_jdtls_warm(ws: &Path) {
    if !jdtls::is_enabled() {
        return;
    }
    let Ok(ws) = ws.canonicalize() else {
        return;
    };
    if jdtls::workspace_ready(&ws) {
        return;
    }
    std::thread::spawn(move || {
        if let Err(e) = warm_jdtls_workspace(&ws) {
            tracing::debug!("jdtls warm on workspace open: {e:#}");
        } else {
            tracing::info!("jdtls ready for {}", ws.display());
        }
    });
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

/// Fetch all remotes in the workspace clone so ahead/behind reflect upstream.
pub fn fetch_workspace_remotes(ws: &Path) -> Result<GitOutput> {
    let remotes = git::run_git(Some(ws), &["remote"])?;
    if !remotes.success() || remotes.stdout.trim().is_empty() {
        bail!("no remotes configured");
    }
    git::run_git(Some(ws), &["fetch", "--all", "--quiet", "--prune"])
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

pub(crate) fn should_skip_tree_name(name: &str, is_dir: bool) -> bool {
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

/// Skip compiled artifacts and dependency indexes in workspace search results.
pub(crate) fn should_skip_search_path(path: &str) -> bool {
    let p = path.replace('\\', "/");
    let lower = p.to_lowercase();
    if lower.ends_with(".class") {
        return true;
    }
    if lower.contains(".reaper/classpath-jar/") {
        return true;
    }
    for seg in [
        "/build/",
        "/target/",
        "/out/",
        "/bin/",
        "/.gradle/",
        "/classes/java/",
        "/classes/kotlin/",
    ] {
        if lower.contains(seg) {
            return true;
        }
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
        let rel = workspace_relative_path(ws, &path);

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
    Ok(())
}

/// Queue a coalesced background patch of the on-disk Java symbol index after a save.
pub fn patch_java_index_after_save(ws: &Path, rel_path: &str, content: &str) {
    java_index_patch::queue_java_index_patch_after_save(ws, rel_path, content);
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

fn java_top_level_type_line(content: &str, type_name: &str) -> Option<(u32, u32)> {
    for (idx, line_text) in content.lines().enumerate() {
        let line = (idx + 1) as u32;
        if java_ecosystem::java_class_simple_name_at_line(content, line).as_deref() != Some(type_name) {
            continue;
        }
        let col = column_of_word(line_text, type_name)?;
        return Some((line, col));
    }
    None
}

fn java_file_tree_symbol_edits(
    ws: &Path,
    from_rel: &str,
    to_rel: &str,
) -> Result<Vec<FileTextEdits>> {
    if !is_java_source_path(from_rel) || !is_java_source_path(to_rel) {
        return Ok(Vec::new());
    }
    let old_name = std::path::Path::new(from_rel)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let new_name = std::path::Path::new(to_rel)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    if old_name.is_empty() || new_name.is_empty() || old_name == new_name {
        return Ok(Vec::new());
    }
    if !is_valid_java_identifier(&old_name) || !is_valid_java_identifier(&new_name) {
        return Ok(Vec::new());
    }
    let from_abs = safe_join(ws, from_rel)?;
    if !from_abs.is_file() {
        return Ok(Vec::new());
    }
    let content = std::fs::read_to_string(&from_abs)?;
    let Some((line, column)) = java_top_level_type_line(&content, &old_name) else {
        return Ok(Vec::new());
    };
    if java_ecosystem::java_class_simple_name_at_line(&content, line).as_deref()
        != Some(old_name.as_str())
    {
        return Ok(Vec::new());
    }
    Ok(rename_word_fallback(
        ws, from_rel, line, column, &content, &new_name,
    ))
}

/// Symbol edits for renaming a Java file whose top-level type matches the file stem.
pub fn rename_path_symbol_plan(
    ws: &Path,
    from_rel: &str,
    to_rel: &str,
) -> Result<WorkspaceRenameResult> {
    let from_rel = from_rel.replace('\\', "/");
    let to_rel = to_rel.replace('\\', "/");
    let edits = java_file_tree_symbol_edits(ws, &from_rel, &to_rel)?;
    Ok(WorkspaceRenameResult {
        edits,
        path_rename: Some(PathRename {
            from: from_rel,
            to: to_rel,
        }),
    })
}

pub fn rename_path(ws: &Path, from_rel: &str, to_rel: &str) -> Result<()> {
    let from = safe_join(ws, from_rel)?;
    let to = safe_join(ws, to_rel)?;
    if !from.exists() {
        bail!("path not found");
    }
    if to.exists() {
        bail!("destination already exists");
    }
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::rename(&from, &to).with_context(|| {
        format!(
            "rename {} -> {}",
            from.display(),
            to.display()
        )
    })?;
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
        let mut cmd = if target.is_dir() {
            let mut c = Command::new("explorer");
            c.arg(&target);
            c
        } else {
            let mut c = Command::new("explorer");
            c.arg(format!("/select,{}", target.display()));
            c
        };
        crate::platform::hide_console_window(&mut cmd);
        let status = cmd.status().context("open in Explorer")?;
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
        return Ok(());
    }
}

const JAVA_DIAG_OVERLAY_PREFIX: &str = ".reaper/java-diagnostics/overlay/";
const JAVA_DIAG_OVERLAY_MARKER: &str = "/.reaper/java-diagnostics/overlay/";
const DIAG_OVERLAY_MARKER: &str = "/.reaper/diagnostics/overlay/";

/// Map diagnostics overlay copies back to workspace-relative source paths.
///
/// Handles both workspace-root overlays (`.reaper/java-diagnostics/overlay/…`) and
/// module-local ones (`services/foo/.reaper/java-diagnostics/overlay/…`).
pub fn normalize_workspace_source_path(rel_path: &str) -> String {
    let path = rel_path.replace('\\', "/");
    if let Some(rest) = path.strip_prefix(JAVA_DIAG_OVERLAY_PREFIX) {
        return rest.to_string();
    }
    if let Some(rest) = path.strip_prefix(".reaper/diagnostics/overlay/") {
        return rest.to_string();
    }
    for marker in [JAVA_DIAG_OVERLAY_MARKER, DIAG_OVERLAY_MARKER] {
        if let Some(idx) = path.find(marker) {
            let prefix = &path[..idx];
            let rest = &path[idx + marker.len()..];
            if prefix.is_empty() {
                return rest.to_string();
            }
            if rest.is_empty() {
                return prefix.to_string();
            }
            return format!("{prefix}/{rest}");
        }
    }
    // Any `{module}/.reaper/…/overlay/{rest}` → `{module}/{rest}`
    if let Some(reaper_idx) = path.find("/.reaper/") {
        if let Some(rel_overlay) = path[reaper_idx..].find("/overlay/") {
            let rest_start = reaper_idx + rel_overlay + "/overlay/".len();
            let prefix = &path[..reaper_idx];
            let rest = &path[rest_start..];
            if !rest.is_empty() {
                if prefix.is_empty() {
                    return rest.to_string();
                }
                return format!("{prefix}/{rest}");
            }
        }
    }
    path
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
    pub behind: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tracking: Option<String>,
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

    let (ahead, behind, tracking) = tracking_counts(ws, &branch, &out.stdout);

    Ok(WorkspaceStatus {
        clean: files.is_empty(),
        branch,
        files,
        stdout: out.stdout,
        merge,
        conflict_count,
        ahead,
        behind,
        tracking,
    })
}

fn tracking_counts(ws: &Path, branch: &str, status_stdout: &str) -> (usize, usize, Option<String>) {
    let (mut ahead, mut behind, tracking) = parse_status_branch_line(status_stdout)
        .unwrap_or((0, 0, None));
    let tracking = tracking.or_else(|| resolve_tracking_ref(ws, branch));
    let Some(upstream) = tracking.clone() else {
        return (0, 0, None);
    };
    if ahead == 0 && behind == 0 {
        ahead = count_rev_range(ws, &upstream, true);
        behind = count_rev_range(ws, &upstream, false);
    } else {
        if ahead == 0 {
            ahead = count_rev_range(ws, &upstream, true);
        }
        if behind == 0 {
            behind = count_rev_range(ws, &upstream, false);
        }
    }
    (ahead, behind, Some(upstream))
}

/// Parse `## branch...upstream [ahead N, behind M]` from porcelain status.
fn parse_status_branch_line(status_stdout: &str) -> Option<(usize, usize, Option<String>)> {
    let line = status_stdout.lines().find(|l| l.starts_with("## "))?;
    let rest = line.strip_prefix("## ")?.trim();
    if rest.starts_with("HEAD ") {
        return None;
    }
    let (branch_part, upstream_part) = rest.split_once("...")?;
    if branch_part.is_empty() {
        return None;
    }
    let mut upstream = upstream_part.to_string();
    let mut ahead = 0usize;
    let mut behind = 0usize;
    if let Some(bracket_start) = upstream.find(" [") {
        let bracket = upstream[bracket_start + 2..].strip_suffix(']')?.to_string();
        upstream.truncate(bracket_start);
        for part in bracket.split(',') {
            let part = part.trim();
            if let Some(n) = part.strip_prefix("ahead ") {
                ahead = n.trim().parse().unwrap_or(0);
            } else if let Some(n) = part.strip_prefix("behind ") {
                behind = n.trim().parse().unwrap_or(0);
            }
        }
    }
    upstream = upstream.split_whitespace().next()?.to_string();
    if upstream.is_empty() {
        return None;
    }
    Some((ahead, behind, Some(upstream)))
}

fn resolve_tracking_ref(ws: &Path, branch: &str) -> Option<String> {
    if let Ok(out) = git::run_git(
        Some(ws),
        &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"],
    ) {
        if out.success() {
            let label = out.stdout.trim();
            if !label.is_empty() {
                return Some(label.to_string());
            }
        }
    }
    let origin_branch = format!("origin/{branch}");
    if let Ok(verify) = git::run_git(Some(ws), &["rev-parse", "--verify", &origin_branch]) {
        if verify.success() {
            return Some(origin_branch);
        }
    }
    None
}

fn count_rev_range(ws: &Path, upstream: &str, ahead: bool) -> usize {
    let range = if ahead {
        format!("{upstream}..HEAD")
    } else {
        format!("HEAD..{upstream}")
    };
    if let Ok(out) = git::run_git(Some(ws), &["rev-list", "--count", &range]) {
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
    let branch = branch.trim();
    if branch.is_empty() {
        anyhow::bail!("branch name required");
    }
    // `switch` creates a local branch from origin/<name> when needed.
    let out = git::run_git(Some(ws), &["switch", branch])?;
    if out.success() {
        return Ok(out);
    }
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

pub fn build_tasks_tree(
    ws: &Path,
    rel_path: &str,
    compose_content: Option<&str>,
) -> Result<build_tasks::BuildTasksTree> {
    build_tasks::build_tasks_tree(ws, rel_path, compose_content)
}

pub fn package_manifest_view(ws: &Path, rel_path: &str) -> Result<package_manifest::PackageManifestView> {
    package_manifest::package_manifest_view(ws, rel_path)
}

pub fn run_context(
    ws: &Path,
    rel_path: &str,
    content: Option<&str>,
    line: u32,
    database_url: Option<&str>,
    db_ssl: Option<&metadata::DbSslSettings>,
    db_ssh: Option<&metadata::DbSshTunnelSettings>,
) -> Result<run_project::RunContext> {
    run_project::run_context(ws, rel_path, content, line, database_url, db_ssl, db_ssh)
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

pub fn coverage_for_file(ws: &Path, rel_path: &str) -> Result<coverage::FileCoverage> {
    coverage::coverage_for_file(ws, rel_path)
}

pub fn coverage_report_summary(
    ws: &Path,
    rel_path: &str,
) -> Result<coverage::CoverageReportSummary> {
    coverage::coverage_report_summary(ws, rel_path)
}

pub fn db_connection_view(
    ws: &Path,
    database_url: Option<&str>,
    ssl: Option<&metadata::DbSslSettings>,
    ssh: Option<&metadata::DbSshTunnelSettings>,
) -> db_viewer::DbConnectionView {
    db_viewer::connection_view(ws, database_url, ssl, ssh)
}

pub fn db_connection_view_for_repo(
    ws: &Path,
    meta: &metadata::RepoMetadata,
) -> db_viewer::DbConnectionView {
    db_viewer::connection_view_for_repo(ws, meta)
}

pub fn db_connection_view_for_repo_probed(
    ws: &Path,
    meta: &metadata::RepoMetadata,
    probe: bool,
) -> db_viewer::DbConnectionView {
    db_viewer::connection_view_for_repo_probed(ws, meta, probe)
}

pub fn attach_db_connection_list(
    view: db_viewer::DbConnectionView,
    meta: &metadata::RepoMetadata,
) -> db_viewer::DbConnectionView {
    db_viewer::attach_connection_list(view, meta)
}

pub fn merge_database_url_with_password(
    form_url: &str,
    password: Option<&str>,
    stored: Option<&str>,
) -> String {
    db_viewer::merge_database_url_with_password(form_url, password, stored)
}

pub fn effective_database_url(ws: &Path, stored: Option<&str>) -> Option<String> {
    db_viewer::effective_database_url(ws, stored)
}

pub fn db_schema(
    ws: &Path,
    database_url: Option<&str>,
    ssl: Option<&metadata::DbSslSettings>,
    ssh: Option<&metadata::DbSshTunnelSettings>,
) -> db_viewer::DbSchemaResponse {
    db_viewer::fetch_schema(ws, database_url, ssl, ssh)
}

pub fn db_query(
    ws: &Path,
    database_url: Option<&str>,
    ssl: Option<&metadata::DbSslSettings>,
    ssh: Option<&metadata::DbSshTunnelSettings>,
    sql: &str,
    limit: u32,
) -> db_viewer::DbQueryResult {
    db_viewer::run_query(ws, database_url, ssl, ssh, sql, limit)
}

pub use db_viewer::{
    DbConnectionDeleteRequest, DbConnectionRequest, DbConnectionSelectRequest, DbQueryRequest,
};

pub fn open_in_system(ws: &Path, rel_path: &str) -> Result<()> {
    let path = safe_join(ws, rel_path)?;
    if !path.exists() {
        bail!("path not found");
    }
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        let status = Command::new("open")
            .arg(&path)
            .status()
            .context("open in default application")?;
        if !status.success() {
            bail!("failed to open path");
        }
        return Ok(());
    }
    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        let mut cmd = Command::new("cmd");
        cmd.args(["/C", "start", "", &path.to_string_lossy()]);
        crate::platform::hide_console_window(&mut cmd);
        let status = cmd.status().context("open in default application")?;
        if !status.success() {
            bail!("failed to open path");
        }
        return Ok(());
    }
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        use std::process::Command;
        let status = Command::new("xdg-open")
            .arg(&path)
            .status()
            .context("open in default application")?;
        if !status.success() {
            bail!("failed to open path");
        }
        return Ok(());
    }
}

pub fn stream_workspace_run_task(
    ws: &Path,
    rel_path: &str,
    task: &str,
    coverage: bool,
    tx: tokio::sync::mpsc::Sender<exec_stream::ExecStreamEvent>,
) -> Result<i32> {
    run_project::stream_run_task(ws, rel_path, task, coverage, tx)
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
    let rel_path = normalize_workspace_source_path(rel_path);
    exec_stream::stream_java_main(ws, &rel_path, tx)
}

pub fn stream_workspace_sql_file(
    ws: &Path,
    rel_path: &str,
    content: Option<&str>,
    database_url: Option<&str>,
    db_ssl: Option<&metadata::DbSslSettings>,
    db_ssh: Option<&metadata::DbSshTunnelSettings>,
    tx: tokio::sync::mpsc::Sender<exec_stream::ExecStreamEvent>,
) -> Result<i32> {
    let rel_path = normalize_workspace_source_path(rel_path);
    let command =
        db_viewer::prepare_sql_run_command(ws, &rel_path, content, database_url, db_ssl, db_ssh)?;
    exec_stream::stream_shell(ws, None, &command, tx)
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

pub fn is_java_indexable_workspace(ws: &Path) -> bool {
    classpath::is_java_indexable_workspace(ws)
}

pub fn warm_java_index(ws: &Path) -> Result<classpath::WarmIndexStatus> {
    classpath::warm_index(ws)
}

pub fn warm_jdk_sources(ws: &Path) -> Result<bool> {
    classpath::warm_jdk_sources(ws)
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

pub use index_jobs::JavaIndexJobs;
pub use project_jobs::ProjectIndexJobs;
pub use quick_fix::{
    QuickFix, QuickFixDiagnostic, QuickFixEdit, filter_ai_import_fixes, merge_quick_fixes,
    suggest_local_quick_fixes,
};
pub use jdtls::JdtlsCodeAction;
pub use lsp::{
    FileTextEdits, PathRename, ReferenceLocation, RenameRange, SignatureHelp, WorkspaceRenameResult,
};
pub use debug::{
    DebugBreakpoint, DebugCapabilities, DebugState, continue_debug, debug_capabilities,
    debug_state, evaluate_hover, evaluate_watch, run_debug_websocket, set_breakpoints, start_debug,
    step_debug, stop_debug,
};

/// Ensure Homebrew and common developer tools are on PATH (GUI .app launches).
pub fn ensure_developer_path() {
    exec::ensure_developer_path();
}

/// Paths that use the custom Java index when jdtls is offline (Go to Class, Spring props).
pub fn definition_uses_java_index(from_path: &str) -> bool {
    classpath::is_java_like(from_path)
}

pub fn should_ensure_java_index_for_completions(from_path: &str) -> bool {
    definition_uses_java_index(from_path) || uses_spring_property_completions(from_path)
}

pub fn uses_spring_property_completions(from_path: &str) -> bool {
    spring_props::is_spring_config_file(from_path)
}

pub fn search_workspace(
    ws: &Path,
    query: &str,
    limit: usize,
) -> Result<Vec<workspace_search::WorkspaceSearchHit>> {
    workspace_search::search_workspace(ws, query, limit)
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
    fn path_rank(path: &str) -> u32 {
        let p = path.replace('\\', "/");
        if p.ends_with(".java") || p.ends_with(".kt") {
            return 0;
        }
        if p.contains("/src/") {
            return 10;
        }
        if should_skip_search_path(&p) {
            return 1000;
        }
        100
    }

    let mut best: std::collections::HashMap<String, (u32, symbols::ClassSearchHit)> =
        std::collections::HashMap::new();
    for (score, hit) in scored {
        if should_skip_search_path(&hit.path) {
            continue;
        }
        let key = if hit.qualified.contains('.') {
            hit.qualified.clone()
        } else {
            format!("{}:{}", hit.name, hit.path)
        };
        let rank = path_rank(&hit.path);
        let entry_score = score.saturating_sub(rank);
        match best.get(&key) {
            Some((existing_score, existing)) => {
                if entry_score > *existing_score
                    || (entry_score == *existing_score
                        && path_rank(&hit.path) < path_rank(&existing.path))
                {
                    best.insert(key, (entry_score, hit));
                }
            }
            None => {
                best.insert(key, (entry_score, hit));
            }
        }
    }
    let mut out: Vec<_> = best.into_values().collect();
    out.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.name.cmp(&b.1.name)));
    out.into_iter()
        .take(limit)
        .map(|(_, hit)| hit)
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

    if languages::is_c_like_path(from_path) {
        if let Some(info) = clangd::find_hover(ws, from_path, line, column, &content)? {
            return Ok(Some(info));
        }
    }

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

    if languages::is_c_like_path(from_path) {
        if let Some(info) = clangd::find_hover(ws, from_path, line, column, &content)? {
            return Ok(Some(info));
        }
    }

    if from_path.ends_with(".java") {
        if let Some(hit) = classpath::find_local_java_definition(from_path, line, column, &content) {
            if classpath::definition_path_is_openable(ws, &hit.path) {
                let def_content = read_file(ws, &hit.path).unwrap_or_else(|_| content.clone());
                return Ok(Some(symbols::hover_info_from_location(
                    &def_content, &hit,
                )));
            }
        }
        let mut allow_well_known = jdtls::use_java_navigation_fallback(ws);
        if jdtls::workspace_ready(ws) {
            if let Some(info) = jdtls::find_hover(ws, from_path, line, column, &content)? {
                return Ok(Some(info));
            }
            allow_well_known = true;
        }
        if let Some(hit) = classpath::find_external_definition_with_well_known(
            ws,
            from_path,
            line,
            column,
            &content,
            Some(allow_well_known),
        )? {
            if classpath::definition_path_is_openable(ws, &hit.path) {
                let def_content = read_file(ws, &hit.path).unwrap_or_else(|_| content.clone());
                return Ok(Some(symbols::hover_info_from_location(
                    &def_content, &hit,
                )));
            }
        }
        if !jdtls::workspace_ready(ws) {
            if let Some(info) = jdtls::find_hover(ws, from_path, line, column, &content)? {
                return Ok(Some(info));
            }
        }
    }

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
    // Java: same-file → classpath/index (Gradle siblings, imports) → jdtls (libraries) → symbol scan.
    if classpath::is_java_like(from_path) {
        if let Some(hit) = classpath::find_local_java_definition(from_path, line, column, &content) {
            if classpath::accept_java_definition(ws, &hit.path) {
                return Ok(Some(hit));
            }
        }
        if let Some(hit) = classpath::find_external_definition_with_well_known(
            ws,
            from_path,
            line,
            column,
            &content,
            Some(true),
        )? {
            if classpath::accept_java_definition(ws, &hit.path) {
                return Ok(Some(hit));
            }
        }
        if jdtls::workspace_ready(ws) {
            if let Some(hit) = jdtls::find_definition(ws, from_path, line, column, &content)? {
                if classpath::accept_java_definition(ws, &hit.path) {
                    return Ok(Some(hit));
                }
            }
        }
        if let Some(symbol) = symbols::word_at(&content, line, column)
            .filter(|s| !s.is_empty() && !symbols::is_keyword(s))
        {
            if let Some(hit) =
                symbols::find_definition_for_symbol(ws, from_path, &content, &symbol)?
            {
                if classpath::accept_java_definition(ws, &hit.path) {
                    return Ok(Some(hit));
                }
            }
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
    if languages::is_c_like_path(from_path) {
        if let Some(hit) = clangd::find_definition(ws, from_path, line, column, &content)? {
            return Ok(Some(hit));
        }
    }
    symbols::find_definition(ws, from_path, line, column, &content)
}

pub fn workspace_references(
    ws: &Path,
    from_path: &str,
    line: u32,
    column: u32,
    content: &str,
) -> Result<Vec<ReferenceLocation>> {
    if is_java_source_path(from_path) {
        let fallback_refs = find_word_references_fallback(ws, from_path, line, column, content);
        if !fallback_refs.is_empty() {
            return Ok(fallback_refs);
        }
        if jdtls::workspace_ready(ws) {
            return jdtls::find_references(ws, from_path, line, column, content);
        }
        return Ok(Vec::new());
    }
    if languages::is_c_like_path(from_path) {
        let fallback_refs = find_word_references_fallback(ws, from_path, line, column, content);
        if !fallback_refs.is_empty() {
            return Ok(fallback_refs);
        }
        return clangd::find_references(ws, from_path, line, column, content);
    }
    if ruby_nav::is_ruby_path(from_path) {
        let fallback_refs = find_word_references_fallback(ws, from_path, line, column, content);
        if !fallback_refs.is_empty() {
            return Ok(fallback_refs);
        }
        return solargraph::find_references(ws, from_path, line, column, content);
    }
    Ok(find_word_references_fallback(ws, from_path, line, column, content))
}

pub fn java_references(
    ws: &Path,
    from_path: &str,
    line: u32,
    column: u32,
    content: &str,
) -> Result<Vec<ReferenceLocation>> {
    workspace_references(ws, from_path, line, column, content)
}

fn merge_reference_locations(
    mut primary: Vec<ReferenceLocation>,
    secondary: Vec<ReferenceLocation>,
) -> Vec<ReferenceLocation> {
    for r in secondary {
        if !primary.iter().any(|e| reference_same(e, &r)) {
            primary.push(r);
        }
    }
    primary.sort_by(|a, b| a.path.cmp(&b.path).then(a.line.cmp(&b.line)));
    primary
}

fn reference_same(a: &ReferenceLocation, b: &ReferenceLocation) -> bool {
    a.path == b.path && a.line == b.line && a.column == b.column
}

fn find_word_references_fallback(
    ws: &Path,
    from_path: &str,
    line: u32,
    column: u32,
    content: &str,
) -> Vec<ReferenceLocation> {
    let Some(word) = symbols::word_at(content, line, column) else {
        return Vec::new();
    };
    if word.is_empty() || symbols::is_keyword(&word) {
        return Vec::new();
    }
    let mut out = Vec::new();
    let scan_root = reference_scan_root(ws, from_path);
    scan_word_references(ws, &scan_root, from_path, &word, &mut out);
    out.sort_by(|a, b| a.path.cmp(&b.path).then(a.line.cmp(&b.line)));
    out
}

fn reference_scan_root(ws: &Path, from_path: &str) -> PathBuf {
    classpath::java_navigation_scan_root(ws, from_path)
}

fn file_matches_reference_scope(from_path: &str, candidate: &str) -> bool {
    if languages::is_c_like_path(from_path) {
        return languages::is_c_like_path(candidate);
    }
    if classpath::is_java_like(from_path) {
        return classpath::is_java_like(candidate);
    }
    if ruby_nav::is_ruby_path(from_path) {
        return ruby_nav::is_ruby_path(candidate);
    }
    languages::language_for_path(from_path) == languages::language_for_path(candidate)
}

fn workspace_relative_path(ws: &Path, path: &Path) -> String {
    let ws_canon = ws.canonicalize().unwrap_or_else(|_| ws.to_path_buf());
    let path_canon = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    path_canon
        .strip_prefix(&ws_canon)
        .unwrap_or(&path_canon)
        .to_string_lossy()
        .replace('\\', "/")
}

fn scan_word_references(
    ws: &Path,
    dir: &Path,
    from_path: &str,
    word: &str,
    out: &mut Vec<ReferenceLocation>,
) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if path.is_dir() {
            if should_skip_tree_name(&name, true) {
                continue;
            }
            scan_word_references(ws, &path, from_path, word, out);
            continue;
        }
        let rel = workspace_relative_path(ws, &path);
        if should_skip_search_path(&rel) || !file_matches_reference_scope(from_path, &rel) {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        for (idx, line_text) in text.lines().enumerate() {
            if let Some(col) = column_of_word(line_text, word) {
                out.push(ReferenceLocation {
                    path: rel.clone(),
                    line: (idx + 1) as u32,
                    column: col,
                    end_line: (idx + 1) as u32,
                    end_column: col + word.len() as u32,
                });
            }
        }
    }
}

fn is_java_source_path(path: &str) -> bool {
    path.replace('\\', "/").to_lowercase().ends_with(".java")
}

fn prepare_rename_word_fallback(content: &str, line: u32, column: u32) -> Option<RenameRange> {
    let (start_line, start_col, end_line, end_col) = symbols::word_range_at(content, line, column)?;
    Some(RenameRange {
        line: start_line,
        column: start_col,
        end_line,
        end_column: end_col,
    })
}

fn rename_word_fallback(
    ws: &Path,
    from_path: &str,
    line: u32,
    column: u32,
    content: &str,
    new_name: &str,
) -> Vec<FileTextEdits> {
    let new_name = new_name.trim();
    if new_name.is_empty() {
        return Vec::new();
    }
    let refs = find_word_references_fallback(ws, from_path, line, column, content);
    if refs.is_empty() {
        return Vec::new();
    }
    let mut by_file: HashMap<String, Vec<QuickFixEdit>> = HashMap::new();
    for r in refs {
        by_file.entry(r.path).or_default().push(QuickFixEdit {
            start_line: r.line,
            start_column: r.column,
            end_line: r.end_line,
            end_column: r.end_column,
            text: new_name.to_string(),
        });
    }
    let mut out: Vec<FileTextEdits> = by_file
        .into_iter()
        .map(|(path, edits)| FileTextEdits { path, edits })
        .collect();
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

pub fn workspace_prepare_rename(
    ws: &Path,
    from_path: &str,
    line: u32,
    column: u32,
    content: &str,
) -> Result<Option<RenameRange>> {
    // Word fallback is instant; jdtls/clangd prepareRename can block on cold start.
    if let Some(range) = prepare_rename_word_fallback(content, line, column) {
        return Ok(Some(range));
    }
    if is_java_source_path(from_path) && jdtls::workspace_ready(ws) {
        if let Some(range) = jdtls::prepare_rename(ws, from_path, line, column, content)? {
            return Ok(Some(range));
        }
    }
    if languages::is_c_like_path(from_path) {
        if let Some(range) = clangd::prepare_rename(ws, from_path, line, column, content)? {
            return Ok(Some(range));
        }
    }
    Ok(None)
}

fn is_valid_java_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_alphabetic() && first != '_' {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// When renaming a Java top-level type whose name matches the file stem, also rename `.java`.
fn java_class_file_rename_candidate(
    from_path: &str,
    line: u32,
    column: u32,
    content: &str,
    new_name: &str,
) -> Option<PathRename> {
    if !is_java_source_path(from_path) {
        return None;
    }
    let new_name = new_name.trim();
    if new_name.is_empty() || !is_valid_java_identifier(new_name) {
        return None;
    }
    let old_name = symbols::word_at(content, line, column)?;
    let path = std::path::Path::new(from_path);
    let stem = path.file_stem()?.to_string_lossy();
    if stem != old_name {
        return None;
    }
    let class_on_line = java_ecosystem::java_class_simple_name_at_line(content, line)?;
    if class_on_line != old_name {
        return None;
    }
    let parent = path.parent().unwrap_or_else(|| std::path::Path::new(""));
    let new_rel = if parent.as_os_str().is_empty() {
        format!("{new_name}.java")
    } else {
        format!(
            "{}/{}.java",
            parent.to_string_lossy().replace('\\', "/"),
            new_name
        )
    };
    Some(PathRename {
        from: from_path.replace('\\', "/"),
        to: new_rel,
    })
}

pub fn workspace_rename(
    ws: &Path,
    from_path: &str,
    line: u32,
    column: u32,
    content: &str,
    new_name: &str,
) -> Result<WorkspaceRenameResult> {
    let path_rename = java_class_file_rename_candidate(from_path, line, column, content, new_name);
    // Prefer semantic jdtls rename when the language server is ready; fall back to
    // whole-project word replace only when jdtls is cold or returns nothing.
    if is_java_source_path(from_path) && jdtls::workspace_ready(ws) {
        match jdtls::rename_symbol(ws, from_path, line, column, content, new_name) {
            Ok(edits) if !edits.is_empty() => {
                return Ok(WorkspaceRenameResult {
                    edits,
                    path_rename,
                });
            }
            Ok(_) => {}
            Err(e) => {
                tracing::debug!("jdtls rename failed, falling back to text rename: {e:#}");
            }
        }
    }
    let fallback = rename_word_fallback(ws, from_path, line, column, content, new_name);
    if !fallback.is_empty() {
        return Ok(WorkspaceRenameResult {
            edits: fallback,
            path_rename,
        });
    }
    if languages::is_c_like_path(from_path) {
        let edits = clangd::rename_symbol(ws, from_path, line, column, content, new_name)?;
        if !edits.is_empty() {
            return Ok(WorkspaceRenameResult {
                edits,
                path_rename: None,
            });
        }
    }
    Ok(WorkspaceRenameResult {
        edits: Vec::new(),
        path_rename: None,
    })
}

fn column_of_word(line: &str, word: &str) -> Option<u32> {
    let mut i = 0;
    while let Some(idx) = line[i..].find(word) {
        let start = i + idx;
        let before_ok = start == 0 || !is_ident_char(line.as_bytes()[start - 1]);
        let end = start + word.len();
        let after_ok = end >= line.len() || !is_ident_char(line.as_bytes()[end]);
        if before_ok && after_ok {
            return Some((start + 1) as u32);
        }
        i = end;
    }
    None
}

fn is_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
}

pub fn java_prepare_rename(
    ws: &Path,
    from_path: &str,
    line: u32,
    column: u32,
    content: &str,
) -> Result<Option<RenameRange>> {
    workspace_prepare_rename(ws, from_path, line, column, content)
}

pub fn java_rename(
    ws: &Path,
    from_path: &str,
    line: u32,
    column: u32,
    content: &str,
    new_name: &str,
) -> Result<WorkspaceRenameResult> {
    workspace_rename(ws, from_path, line, column, content, new_name)
}

pub fn workspace_signature_help(
    ws: &Path,
    from_path: &str,
    line: u32,
    column: u32,
    content: &str,
) -> Result<Option<SignatureHelp>> {
    if classpath::is_java_like(from_path) {
        if let Some(help) = jdtls::signature_help(ws, from_path, line, column, content)? {
            return Ok(Some(help));
        }
    }
    if languages::is_c_like_path(from_path) {
        if let Some(help) = clangd::signature_help(ws, from_path, line, column, content)? {
            return Ok(Some(help));
        }
    }
    Ok(None)
}

pub fn java_signature_help(
    ws: &Path,
    from_path: &str,
    line: u32,
    column: u32,
    content: &str,
) -> Result<Option<SignatureHelp>> {
    workspace_signature_help(ws, from_path, line, column, content)
}

pub fn java_code_actions(
    ws: &Path,
    from_path: &str,
    line: u32,
    column: u32,
    content: &str,
    only: &[&str],
) -> Result<Vec<JdtlsCodeAction>> {
    java_code_actions_in_range(ws, from_path, line, column, content, only, None)
}

pub fn java_code_actions_in_range(
    ws: &Path,
    from_path: &str,
    line: u32,
    column: u32,
    content: &str,
    only: &[&str],
    selection: Option<(u32, u32, u32, u32)>,
) -> Result<Vec<JdtlsCodeAction>> {
    jdtls::code_actions_in_range(ws, from_path, line, column, content, only, selection)
}

pub fn jdtls_code_actions_as_quick_fixes(
    ws: &Path,
    from_path: &str,
    line: u32,
    column: u32,
    content: &str,
    only: &[&str],
) -> Result<Vec<QuickFix>> {
    let actions = java_code_actions(ws, from_path, line, column, content, only)?;
    Ok(actions
        .into_iter()
        .filter_map(|action| {
            let edits = action
                .edits
                .into_iter()
                .find(|f| f.path == from_path)
                .map(|f| f.edits)
                .filter(|e| !e.is_empty())?;
            Some(QuickFix {
                title: action.title,
                edits,
                provider: Some("jdtls".into()),
            })
        })
        .collect())
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

    let mut items = Vec::new();
    if from_path.ends_with(".java") && jdtls::workspace_ready(ws) {
        let jdtls_items = jdtls::find_completions(ws, from_path, line, column, &content)?;
        if !jdtls_items.is_empty() {
            let index_items = if classpath::is_java_like(from_path) {
                classpath::java_completions_for_jdtls_gap_fill(
                    ws, from_path, line, column, &content, prefix, overlays,
                )?
            } else {
                Vec::new()
            };
            items = merge_completion_items(jdtls_items, index_items, 80);
        }
    }
    if items.is_empty() && classpath::is_java_like(from_path) {
        items = classpath::java_completions(ws, from_path, line, column, &content, prefix, overlays)?;
    }

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

fn merge_completion_items(
    primary: Vec<classpath::CompletionItem>,
    fallback: Vec<classpath::CompletionItem>,
    limit: usize,
) -> Vec<classpath::CompletionItem> {
    use std::collections::HashSet;
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for item in primary {
        if seen.insert(item.label.clone()) {
            out.push(item);
        }
        if out.len() >= limit {
            return out;
        }
    }
    for item in fallback {
        if seen.insert(item.label.clone()) {
            out.push(item);
        }
        if out.len() >= limit {
            break;
        }
    }
    out
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
    scope: java_diagnostics::JavaDiagScope,
) -> Result<diagnostics::FileDiagnosticsResult> {
    diagnostics::diagnose_file(ws, rel_path, content, overlays, scope)
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

pub fn is_import_typing_line(path: &str, content: &str, line: u32, line_prefix: &str) -> bool {
    inline_context::is_import_typing_line(path, content, line, line_prefix)
}

#[cfg(test)]
mod path_tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn merge_completion_items_prefers_primary_then_fills_from_fallback() {
        let primary = vec![
            classpath::CompletionItem {
                label: "println".into(),
                kind: "method".into(),
                detail: None,
                insert: None,
                path: None,
                line: None,
                column: None,
                documentation: None,
            },
        ];
        let fallback = vec![
            classpath::CompletionItem {
                label: "print".into(),
                kind: "method".into(),
                detail: None,
                insert: None,
                path: None,
                line: None,
                column: None,
                documentation: None,
            },
            classpath::CompletionItem {
                label: "println".into(),
                kind: "method".into(),
                detail: None,
                insert: None,
                path: None,
                line: None,
                column: None,
                documentation: None,
            },
        ];
        let merged = merge_completion_items(primary, fallback, 80);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].label, "println");
        assert_eq!(merged[1].label, "print");
    }

    #[test]
    fn reference_scan_root_uses_gradle_wrapper_root_for_submodules() {
        let root = std::env::temp_dir().join(format!("reaper-ref-scan-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("gradle/wrapper")).unwrap();
        std::fs::write(root.join("settings.gradle"), "rootProject.name = 'root'\n").unwrap();
        std::fs::write(root.join("build.gradle"), "plugins { id 'java' }\n").unwrap();
        std::fs::write(root.join("gradlew"), "#!/bin/sh\nexit 0\n").unwrap();
        let module = root.join("api");
        std::fs::create_dir_all(module.join("src/main/java/com/example")).unwrap();
        std::fs::write(module.join("build.gradle"), "plugins { id 'java' }\n").unwrap();
        std::fs::write(
            module.join("src/main/java/com/example/Foo.java"),
            "package com.example; class Foo {}",
        )
        .unwrap();

        let from = "api/src/main/java/com/example/Foo.java";
        let scan = reference_scan_root(&root, from);
        assert_eq!(
            scan.canonicalize().unwrap_or(scan.clone()),
            root.canonicalize().unwrap_or(root.clone())
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[cfg(windows)]
    #[test]
    fn workspace_relative_path_handles_canonical_prefix_mismatch() {
        let root = std::env::temp_dir().join(format!("reaper-rel-path-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("src/main")).unwrap();
        let ws = root.canonicalize().unwrap();
        let child = root.join("src/main");
        let rel = workspace_relative_path(&ws, &child);
        assert_eq!(rel, "src/main");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn build_tree_level_returns_workspace_relative_paths() {
        let root = std::env::temp_dir().join(format!("reaper-tree-level-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("src/main/java/com/example")).unwrap();
        std::fs::write(
            root.join("src/main/java/com/example/App.java"),
            "class App {}",
        )
        .unwrap();
        std::fs::write(root.join("pom.xml"), "<project/>").unwrap();
        let ws = root.canonicalize().unwrap();
        let root_nodes = build_tree_level(&ws, None).unwrap();
        assert!(root_nodes.iter().any(|n| n.path == "src"));
        let src_nodes = build_tree_level(&ws, Some("src")).unwrap();
        assert_eq!(src_nodes.len(), 1);
        assert_eq!(src_nodes[0].path, "src/main");
        let java_nodes =
            build_tree_level(&ws, Some("src/main/java/com/example")).unwrap();
        assert_eq!(java_nodes.len(), 1);
        assert_eq!(java_nodes[0].path, "src/main/java/com/example/App.java");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[cfg(windows)]
    #[test]
    fn build_tree_level_never_returns_verbatim_or_absolute_paths() {
        let root = std::env::temp_dir().join(format!("reaper-tree-win-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("src/main")).unwrap();
        std::fs::write(root.join("src/main/App.java"), "class App {}").unwrap();
        let ws = root.canonicalize().unwrap();
        for nodes in [
            build_tree_level(&ws, None).unwrap(),
            build_tree_level(&ws, Some("src")).unwrap(),
            build_tree_level(&ws, Some("src/main")).unwrap(),
        ] {
            for node in nodes {
                assert!(
                    !node.path.starts_with("//?/"),
                    "tree path must be workspace-relative, got {}",
                    node.path
                );
                assert!(
                    !node.path.contains(":\\") && !node.path.starts_with('/'),
                    "tree path must not be absolute, got {}",
                    node.path
                );
            }
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn merge_reference_locations_dedupes_by_path_line_column() {
        let a = ReferenceLocation {
            path: "app/src/Main.java".into(),
            line: 10,
            column: 5,
            end_line: 10,
            end_column: 22,
        };
        let dup = a.clone();
        let other = ReferenceLocation {
            path: "common/src/Util.java".into(),
            line: 3,
            column: 12,
            end_line: 3,
            end_column: 29,
        };
        let merged = merge_reference_locations(vec![a], vec![dup, other.clone()]);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].path, "app/src/Main.java");
        assert_eq!(merged[1].path, other.path);
    }

    #[test]
    fn read_file_accepts_absolute_paths_outside_workspace() {
        let ws = std::env::temp_dir().join(format!("reaper-abs-read-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&ws);
        std::fs::create_dir_all(&ws).unwrap();
        let external = std::env::temp_dir().join(format!("reaper-ext-header-{}.h", std::process::id()));
        {
            let mut f = std::fs::File::create(&external).unwrap();
            writeln!(f, "int printf(const char*, ...);").unwrap();
        }
        let abs = external.to_string_lossy().into_owned();
        let content = read_file(&ws, &abs).expect("read absolute path");
        assert!(content.contains("printf"));
        let _ = std::fs::remove_file(&external);
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn parse_status_branch_line_reads_ahead_and_behind() {
        let stdout = "## main...origin/main [ahead 2, behind 3]\n M file.txt\n";
        let (ahead, behind, tracking) = parse_status_branch_line(stdout).unwrap();
        assert_eq!(ahead, 2);
        assert_eq!(behind, 3);
        assert_eq!(tracking.as_deref(), Some("origin/main"));
    }

    #[test]
    fn parse_status_branch_line_reads_upstream_only() {
        let stdout = "## feature...upstream/main\n";
        let (ahead, behind, tracking) = parse_status_branch_line(stdout).unwrap();
        assert_eq!(ahead, 0);
        assert_eq!(behind, 0);
        assert_eq!(tracking.as_deref(), Some("upstream/main"));
    }

    #[test]
    fn workspace_prepare_rename_java_returns_instant_word_range() {
        let root =
            std::env::temp_dir().join(format!("reaper-prepare-rename-java-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let content = "public class GatewayController {\n}\n";
        std::fs::write(root.join("GatewayController.java"), content).unwrap();
        let range = workspace_prepare_rename(&root, "GatewayController.java", 1, 14, content)
            .expect("prepare rename")
            .expect("word range");
        assert_eq!(range.line, 1);
        assert_eq!(range.column, 14);
        assert_eq!(range.end_line, 1);
        assert_eq!(range.end_column, 14 + "GatewayController".len() as u32);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn workspace_rename_java_uses_text_fallback_when_jdtls_cold() {
        let root =
            std::env::temp_dir().join(format!("reaper-rename-java-cold-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let gateway = "public class GatewayController {\n    int gateway;\n}\n";
        let service = "public class Service {\n    GatewayController controller;\n}\n";
        std::fs::write(root.join("GatewayController.java"), gateway).unwrap();
        std::fs::write(root.join("Service.java"), service).unwrap();
        assert!(
            !jdtls::workspace_ready(&root),
            "cold workspace must not have a ready jdtls session"
        );
        let result = workspace_rename(&root, "GatewayController.java", 1, 14, gateway, "ApiGateway")
            .expect("rename");
        let edits = result.edits;
        assert_eq!(edits.len(), 2);
        assert!(
            edits.iter().any(|e| e.path.ends_with("GatewayController.java")),
            "gateway file edits: {:?}",
            edits.iter().map(|e| &e.path).collect::<Vec<_>>(),
        );
        assert!(
            edits.iter().any(|e| e.path.ends_with("Service.java")),
            "service file edits: {:?}",
            edits.iter().map(|e| &e.path).collect::<Vec<_>>(),
        );
        let gateway_edits = edits
            .iter()
            .find(|e| e.path.ends_with("GatewayController.java"))
            .expect("gateway edits");
        assert!(
            gateway_edits
                .edits
                .iter()
                .any(|e| e.text == "ApiGateway"),
            "class name renamed"
        );
        let path_rename = result.path_rename.expect("java class rename should rename file");
        assert_eq!(path_rename.from, "GatewayController.java");
        assert_eq!(path_rename.to, "ApiGateway.java");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn workspace_rename_java_skips_file_rename_for_reference_in_other_file() {
        let root = std::env::temp_dir().join(format!("reaper-rename-java-ref-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let gateway = "public class GatewayController {\n}\n";
        let service = "public class Service {\n    GatewayController controller;\n}\n";
        std::fs::write(root.join("GatewayController.java"), gateway).unwrap();
        std::fs::write(root.join("Service.java"), service).unwrap();
        let result = workspace_rename(&root, "Service.java", 2, 5, service, "ApiGateway")
            .expect("rename");
        assert!(!result.edits.is_empty());
        assert!(result.path_rename.is_none(), "reference rename must not rename unrelated file");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn rename_path_symbol_plan_updates_references_for_java_file() {
        let root =
            std::env::temp_dir().join(format!("reaper-rename-path-sym-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let gateway = "public class GatewayController {\n}\n";
        let service = "public class Service {\n    GatewayController controller;\n}\n";
        std::fs::write(root.join("GatewayController.java"), gateway).unwrap();
        std::fs::write(root.join("Service.java"), service).unwrap();
        let plan = rename_path_symbol_plan(&root, "GatewayController.java", "ApiGateway.java")
            .expect("plan");
        assert_eq!(plan.edits.len(), 2);
        assert!(
            plan.edits.iter().any(|e| e.path.ends_with("Service.java")),
            "service references should be updated"
        );
        assert_eq!(
            plan.path_rename.as_ref().map(|p| p.to.as_str()),
            Some("ApiGateway.java")
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn workspace_references_java_uses_text_fallback_when_jdtls_cold() {
        let root =
            std::env::temp_dir().join(format!("reaper-refs-java-cold-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let gateway = "public class GatewayController {\n}\n";
        let service = "public class Service {\n    GatewayController controller;\n}\n";
        std::fs::write(root.join("GatewayController.java"), gateway).unwrap();
        std::fs::write(root.join("Service.java"), service).unwrap();
        let refs = workspace_references(&root, "GatewayController.java", 1, 14, gateway)
            .expect("references");
        assert_eq!(refs.len(), 2);
        assert!(refs.iter().any(|r| r.path.ends_with("Service.java")));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn rename_path_moves_file_within_workspace() {
        let root = std::env::temp_dir().join(format!("reaper-rename-path-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("GatewayController.java"), "class GatewayController {}\n").unwrap();
        rename_path(&root, "GatewayController.java", "ApiGateway.java").expect("rename");
        assert!(!root.join("GatewayController.java").exists());
        assert!(root.join("ApiGateway.java").is_file());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn prepare_rename_word_fallback_on_identifier() {
        let content = "fn main() {\n    let count = 1;\n}\n";
        let range = prepare_rename_word_fallback(content, 2, 9).expect("range");
        assert_eq!(range.line, 2);
        assert_eq!(range.column, 9);
        assert_eq!(range.end_column, 14);
    }

    #[test]
    fn normalize_workspace_source_path_strips_root_and_module_overlays() {
        assert_eq!(
            normalize_workspace_source_path(
                ".reaper/java-diagnostics/overlay/services/api/src/main/java/App.java"
            ),
            "services/api/src/main/java/App.java"
        );
        assert_eq!(
            normalize_workspace_source_path(
                "services/api/.reaper/java-diagnostics/overlay/src/main/java/App.java"
            ),
            "services/api/src/main/java/App.java"
        );
        assert_eq!(
            normalize_workspace_source_path("services/api/src/main/java/App.java"),
            "services/api/src/main/java/App.java"
        );
    }

    #[test]
    fn rename_word_fallback_replaces_all_occurrences_in_scope() {
        let root = std::env::temp_dir().join(format!("reaper-rename-fb-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("src/a.py"),
            "def foo():\n    foo = 1\n    return foo\n",
        )
        .unwrap();
        std::fs::write(root.join("src/b.py"), "foo = 2\n").unwrap();
        let edits = rename_word_fallback(
            &root,
            "src/a.py",
            1,
            5,
            "def foo():\n    foo = 1\n    return foo\n",
            "bar",
        );
        assert!(!edits.is_empty());
        let a = edits.iter().find(|e| e.path == "src/a.py").expect("a.py edits");
        assert_eq!(a.edits.len(), 3);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Manual repro against a local multi-module Gradle checkout (run with `--ignored`).
    #[test]
    #[ignore]
    fn spring_gradle_gateway_navigates_to_common_api_response() {
        let ws = std::path::Path::new("/Users/sunny/reaper/workspaces/Spring-gradle-complicated");
        if !ws.join("settings.gradle").is_file() {
            return;
        }
        let path = "services/gateway-service/src/main/java/com/example/gateway/web/GatewayController.java";
        let content = std::fs::read_to_string(ws.join(path)).expect("gateway source");
        for (line, col, label) in [(3, 33, "import"), (21, 12, "return-type")] {
            let hit = find_definition_with_content(ws, path, line, col, Some(&content))
                .expect("definition lookup")
                .unwrap_or_else(|| panic!("no definition for {label} at {line}:{col}"));
            let norm = hit.path.replace('\\', "/");
            assert!(
                norm.contains("libs/common") && norm.contains("ApiResponse.java"),
                "{label}: expected common ApiResponse source, got {:?}",
                hit
            );
        }
    }
}
