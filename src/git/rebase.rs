use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use super::{run_git, CommitInfo, GitOutput};

#[derive(Debug, Clone, Deserialize)]
pub struct RebaseStep {
    pub hash: String,
    pub action: String,
    #[serde(default)]
    pub subject: Option<String>,
}

pub fn log_rebase_range(ws: &Path, onto: &str, limit: usize) -> Result<Vec<CommitInfo>> {
    let onto = onto.trim();
    if onto.is_empty() {
        bail!("onto ref is required");
    }
    let limit = limit.to_string();
    let range = format!("{onto}..HEAD");
    let out = run_git(
        Some(ws),
        &[
            "log",
            &format!("-{limit}"),
            "--reverse",
            &format!("--format=%H%x1f%an%x1f%ai%x1f%s"),
            &range,
        ],
    )?;
    if !out.success() {
        bail!("{}", out.stderr.trim());
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

pub fn start_interactive_rebase(ws: &Path, onto: &str, steps: &[RebaseStep]) -> Result<GitOutput> {
    let onto = onto.trim();
    if onto.is_empty() {
        bail!("onto ref is required");
    }
    if steps.is_empty() {
        bail!("rebase plan is empty");
    }
    let mut todo = String::new();
    for step in steps {
        let action = step.action.trim().to_ascii_lowercase();
        if !matches!(
            action.as_str(),
            "pick" | "squash" | "fixup" | "drop" | "reword" | "edit"
        ) {
            bail!("invalid rebase action: {}", step.action);
        }
        if action == "drop" {
            todo.push_str(&format!("drop {}\n", step.hash.trim()));
            continue;
        }
        let subject = step.subject.as_deref().unwrap_or("").trim();
        if subject.is_empty() {
            todo.push_str(&format!("{action} {}\n", step.hash.trim()));
        } else {
            todo.push_str(&format!("{action} {} {subject}\n", step.hash.trim()));
        }
    }

    let dir = std::env::temp_dir().join(format!("reaper-rebase-{}", std::process::id()));
    std::fs::create_dir_all(&dir).context("create rebase temp dir")?;
    let todo_file = dir.join("todo");
    std::fs::write(&todo_file, todo).context("write rebase todo")?;
    let script = dir.join("editor.sh");
    std::fs::write(
        &script,
        format!("#!/bin/sh\ncp \"{}\" \"$1\"\n", todo_file.display()),
    )
    .context("write rebase editor script")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755))?;
    }

    let script_str = script
        .to_str()
        .context("rebase editor script path is not valid UTF-8")?;

    let mut git = std::process::Command::new("git");
    git.current_dir(ws)
        .env("GIT_SEQUENCE_EDITOR", script_str)
        .args(["rebase", "-i", onto]);
    crate::platform::hide_console_window(&mut git);
    let result = git
        .output()
        .map(|o| GitOutput {
            stdout: String::from_utf8_lossy(&o.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&o.stderr).into_owned(),
            exit_code: o.status.code().unwrap_or(-1),
        })
        .context("git rebase -i failed")?;
    let _ = std::fs::remove_dir_all(&dir);
    Ok(result)
}

pub fn cherry_pick(ws: &Path, hash: &str) -> Result<GitOutput> {
    let hash = hash.trim();
    if hash.is_empty() {
        bail!("commit hash is required");
    }
    run_git(Some(ws), &["cherry-pick", hash])
}

pub fn abort_rebase(ws: &Path) -> Result<GitOutput> {
    run_git(Some(ws), &["rebase", "--abort"])
}

pub fn abort_cherry_pick(ws: &Path) -> Result<GitOutput> {
    run_git(Some(ws), &["cherry-pick", "--abort"])
}
