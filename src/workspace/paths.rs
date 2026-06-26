use std::path::PathBuf;

use anyhow::{Result, bail};

use crate::config::Config;
use crate::repos::metadata;
use crate::settings::SettingsStore;

/// Expand `~` and require an absolute path.
pub fn normalize_workspace_path(input: &str) -> Result<PathBuf> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        bail!("workspace path required");
    }

    let expanded = if trimmed == "~" {
        home_dir()?
    } else if let Some(rest) = trimmed.strip_prefix("~/") {
        home_dir()?.join(rest)
    } else if let Some(rest) = trimmed.strip_prefix('~') {
        if rest.is_empty() || rest.starts_with('/') || rest.starts_with('\\') {
            home_dir()?.join(rest.trim_start_matches(['/', '\\']))
        } else {
            bail!("workspace path must be absolute (or use ~/…)");
        }
    } else {
        PathBuf::from(trimmed)
    };

    if !expanded.is_absolute() {
        bail!("workspace path must be absolute (or use ~/…)");
    }

    Ok(expanded)
}

fn home_dir() -> Result<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| anyhow::anyhow!("HOME is not set"))
}

/// Resolve the checkout directory for a repository.
///
/// Priority: per-repo metadata override → global workspaces root → default under data dir.
pub fn resolve_workspace_path(
    config: &Config,
    settings: &SettingsStore,
    name: &str,
) -> Result<PathBuf> {
    let meta = metadata::load(config, name)?;
    if let Some(custom) = meta.workspace_path.as_deref().filter(|p| !p.is_empty()) {
        return normalize_workspace_path(custom);
    }

    if let Some(root) = settings.workspaces_root() {
        return Ok(PathBuf::from(root).join(name));
    }

    Ok(config.workspace_path(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_tilde_path() {
        if std::env::var_os("HOME").is_some() {
            let p = normalize_workspace_path("~/Projects/reaper").unwrap();
            assert!(p.is_absolute());
            assert!(p.ends_with("Projects/reaper"));
        }
    }

    #[test]
    fn reject_relative_path() {
        assert!(normalize_workspace_path("relative/path").is_err());
    }
}
