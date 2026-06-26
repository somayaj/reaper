use std::path::PathBuf;

use anyhow::{Context, Result};

#[cfg(target_os = "macos")]
pub fn pick_directory(prompt: &str) -> Result<Option<PathBuf>> {
    let escaped = prompt.replace('\\', "\\\\").replace('"', "\\\"");
    let script = format!(
        "POSIX path of (choose folder with prompt \"{escaped}\")"
    );
    let output = std::process::Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .output()
        .context("osascript folder picker failed")?;

    if output.status.code() == Some(1) {
        // User cancelled the dialog.
        return Ok(None);
    }
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("folder picker failed: {}", stderr.trim());
    }

    let path = String::from_utf8(output.stdout)
        .context("invalid folder picker output")?
        .trim()
        .trim_end_matches('/')
        .to_string();
    if path.is_empty() {
        return Ok(None);
    }
    Ok(Some(PathBuf::from(path)))
}

#[cfg(not(target_os = "macos"))]
pub fn pick_directory(_prompt: &str) -> Result<Option<PathBuf>> {
    Ok(None)
}
