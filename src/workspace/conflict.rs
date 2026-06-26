use std::path::Path;

use anyhow::Result;
use serde::Serialize;

use super::safe_join;
use crate::git::{self, GitOutput};

#[derive(Debug, Clone, Serialize)]
pub struct MergeState {
    pub active: bool,
    pub kind: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConflictStages {
    pub path: String,
    pub base: Option<String>,
    pub ours: Option<String>,
    pub theirs: Option<String>,
}

pub fn is_unmerged(code: &str) -> bool {
    matches!(code, "DD" | "AU" | "UD" | "UA" | "DU" | "AA" | "UU")
}

pub fn merge_state(ws: &Path) -> MergeState {
    let git = ws.join(".git");
    let kind = if git.join("MERGE_HEAD").exists() {
        Some("merge".into())
    } else if git.join("rebase-merge").exists() || git.join("rebase-apply").exists() {
        Some("rebase".into())
    } else if git.join("CHERRY_PICK_HEAD").exists() {
        Some("cherry-pick".into())
    } else {
        None
    };
    MergeState {
        active: kind.is_some(),
        kind,
    }
}

pub fn conflict_stages(ws: &Path, rel_path: &str) -> Result<ConflictStages> {
    let _ = safe_join(ws, rel_path)?;
    Ok(ConflictStages {
        path: rel_path.to_string(),
        base: show_stage(ws, 1, rel_path)?,
        ours: show_stage(ws, 2, rel_path)?,
        theirs: show_stage(ws, 3, rel_path)?,
    })
}

fn show_stage(ws: &Path, stage: u8, path: &str) -> Result<Option<String>> {
    let spec = format!(":{stage}:{path}");
    let out = git::run_git(Some(ws), &["show", &spec])?;
    if out.success() {
        Ok(Some(out.stdout))
    } else {
        Ok(None)
    }
}

pub fn mark_conflict_resolved(ws: &Path, rel_path: &str) -> Result<GitOutput> {
    let _ = safe_join(ws, rel_path)?;
    git::run_git(Some(ws), &["add", rel_path])
}
