use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{Context, Result};

use crate::git::GitOutput;
use crate::jdk;

pub fn run_command(cwd: &Path, program: &str, args: &[&str]) -> Result<GitOutput> {
    let mut cmd = Command::new(program);
    cmd.args(args)
        .current_dir(cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    jdk::apply_java_env(&mut cmd);

    let output = cmd
        .output()
        .with_context(|| format!("failed to run {program}"))?;

    Ok(GitOutput {
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        exit_code: output.status.code().unwrap_or(-1),
    })
}

/// Run a command through bash login shell (PATH, nvm, brew, etc. in GUI apps).
pub fn run_shell_command(cwd: &Path, command: &str) -> Result<GitOutput> {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".into());
    run_command(cwd, &shell, &["-lc", command])
}
