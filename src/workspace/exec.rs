use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{Context, Result};

use crate::git::GitOutput;
use crate::jdk;
use crate::toolchain;

pub fn run_command(cwd: &Path, program: &str, args: &[&str]) -> Result<GitOutput> {
    let mut cmd = Command::new(program);
    cmd.args(args)
        .current_dir(cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let output = cmd
        .output()
        .with_context(|| format!("failed to run {program}"))?;

    Ok(GitOutput {
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        exit_code: output.status.code().unwrap_or(-1),
    })
}

/// Run a configured compiler (Settings → Compiler), falling back to PATH.
pub fn run_tool_command(cwd: &Path, tool_id: &str, args: &[&str]) -> Result<GitOutput> {
    let program = toolchain::resolve_program_or(tool_id)?;
    run_command(cwd, program.to_string_lossy().as_ref(), args)
}

/// Run java/javac with the configured JDK (Settings → Toolchains), not system default.
pub fn run_java_command(cwd: &Path, program: &str, args: &[&str]) -> Result<GitOutput> {
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
