//! Native OS integrations (folder picker, etc.).

/// Opens a native folder chooser. Returns `None` if the user cancels.
pub fn pick_folder(prompt: &str) -> anyhow::Result<Option<String>> {
    #[cfg(target_os = "macos")]
    {
        pick_folder_macos(prompt)
    }
    #[cfg(target_os = "windows")]
    {
        pick_folder_windows(prompt)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = prompt;
        anyhow::bail!("folder picker is only available on macOS and Windows")
    }
}

#[cfg(target_os = "macos")]
fn pick_folder_macos(prompt: &str) -> anyhow::Result<Option<String>> {
    let escaped = prompt.replace('\\', "\\\\").replace('"', "\\\"");
    let script = format!("POSIX path of (choose folder with prompt \"{escaped}\")");
    let out = std::process::Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .output()?;

    if !out.status.success() {
        // User cancelled or dialog failed — treat cancel as no selection.
        return Ok(None);
    }

    let path = String::from_utf8(out.stdout)?
        .trim()
        .trim_end_matches('/')
        .to_string();
    if path.is_empty() {
        Ok(None)
    } else {
        Ok(Some(path))
    }
}

#[cfg(target_os = "windows")]
fn pick_folder_windows(prompt: &str) -> anyhow::Result<Option<String>> {
    let path = rfd::FileDialog::new()
        .set_title(prompt)
        .pick_folder();
    Ok(path.map(|p| p.to_string_lossy().replace('\\', "/")))
}
