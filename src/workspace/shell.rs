use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::git::GitOutput;

use super::exec::run_shell_command;

/// Run an arbitrary shell command in the workspace (bash -lc).
pub fn run_shell(ws: &Path, cwd_rel: Option<&str>, command: &str) -> Result<GitOutput> {
    let command = command.trim();
    if command.is_empty() {
        bail!("command required");
    }
    let work_dir = resolve_work_dir(ws, cwd_rel)?;
    run_shell_command(&work_dir, command)
}

/// Resolve `cd <target>` within the workspace; returns new cwd relative to workspace root.
pub fn change_directory(ws: &Path, cwd_rel: Option<&str>, target: &str) -> Result<String> {
    let ws_canon = ws
        .canonicalize()
        .with_context(|| format!("resolve workspace {}", ws.display()))?;

    let mut resolved = match cwd_rel.filter(|s| !s.is_empty()) {
        Some(rel) => resolve_work_dir(ws, Some(rel))?,
        None => ws_canon.clone(),
    };

    let target = target.trim();
    if target.is_empty() || target == "~" {
        return rel_from_workspace(&ws_canon, &ws_canon);
    }
    if target.starts_with('/') {
        bail!("absolute paths outside workspace are not supported");
    }

    for component in Path::new(target).components() {
        match component {
            Component::Normal(c) => resolved.push(c),
            Component::CurDir => {}
            Component::ParentDir => {
                resolved = resolved
                    .parent()
                    .filter(|p| p.starts_with(&ws_canon))
                    .unwrap_or(&ws_canon)
                    .to_path_buf();
            }
            _ => bail!("invalid path"),
        }
    }

    if !resolved.is_dir() {
        bail!("no such directory: {target}");
    }

    let resolved = resolved
        .canonicalize()
        .with_context(|| format!("resolve {}", resolved.display()))?;
    if !resolved.starts_with(&ws_canon) {
        bail!("path escapes workspace");
    }

    rel_from_workspace(&ws_canon, &resolved)
}

fn resolve_work_dir(ws: &Path, cwd_rel: Option<&str>) -> Result<PathBuf> {
    match cwd_rel.filter(|s| !s.is_empty()) {
        Some(rel) => {
            let dir = ws.join(rel);
            if !dir.is_dir() {
                bail!("no such directory: {rel}");
            }
            dir.canonicalize()
                .with_context(|| format!("resolve directory {}", dir.display()))
        }
        None => ws.canonicalize()
            .with_context(|| format!("resolve workspace {}", ws.display())),
    }
}

fn rel_from_workspace(ws_canon: &Path, path: &Path) -> Result<String> {
    if path == ws_canon {
        return Ok(String::new());
    }
    let rel = path
        .strip_prefix(ws_canon)
        .with_context(|| "path outside workspace")?;
    Ok(rel.to_string_lossy().replace('\\', "/"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn cd_into_subdir_and_back() {
        let ws = std::env::temp_dir().join("reaper-shell-test");
        let _ = fs::remove_dir_all(&ws);
        fs::create_dir_all(ws.join("src/main")).unwrap();

        let sub = change_directory(&ws, None, "src/main").unwrap();
        assert_eq!(sub, "src/main");

        let up = change_directory(&ws, Some(&sub), "..").unwrap();
        assert_eq!(up, "src");

        let root = change_directory(&ws, Some(&up), "..").unwrap();
        assert!(root.is_empty());

        let _ = fs::remove_dir_all(&ws);
    }
}
