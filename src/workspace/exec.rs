use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{Context, Result};

use crate::git::GitOutput;

pub fn run_command(cwd: &Path, program: &str, args: &[&str]) -> Result<GitOutput> {
    let output = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .with_context(|| format!("failed to run {program}"))?;

    Ok(GitOutput {
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        exit_code: output.status.code().unwrap_or(-1),
    })
}
