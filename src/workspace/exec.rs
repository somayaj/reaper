use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};

use crate::git::GitOutput;
use crate::jdk;
use crate::toolchain;

pub fn run_command(cwd: &Path, program: &str, args: &[&str]) -> Result<GitOutput> {
    run_command_with_env(cwd, program, args, &[] as &[(&str, &str)])
}

pub fn run_command_with_env(
    cwd: &Path,
    program: &str,
    args: &[&str],
    env: &[(&str, &str)],
) -> Result<GitOutput> {
    let mut cmd = crate::platform::command(program);
    cmd.args(args)
        .current_dir(cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (key, value) in env {
        cmd.env(key, value);
    }

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
    let mut cmd = crate::platform::command(program);
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
    let shell = crate::platform::login_shell();
    let mut cmd = crate::platform::command(&shell);
    crate::platform::configure_shell_script(&mut cmd, command);
    cmd.current_dir(cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    toolchain::apply_compiler_env(&mut cmd);

    let output = cmd
        .output()
        .with_context(|| format!("failed to run shell command: {command}"))?;

    Ok(GitOutput {
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        exit_code: output.status.code().unwrap_or(-1),
    })
}

fn shell_quote(arg: &str) -> String {
    format!("'{}'", arg.replace('\'', "'\"'\"'"))
}

/// Run a program with args via login shell so Homebrew and user PATH work from the .app.
pub fn run_shell_argv(cwd: &Path, program: &str, args: &[&str]) -> Result<GitOutput> {
    let mut command = shell_quote(program);
    for arg in args {
        command.push(' ');
        command.push_str(&shell_quote(arg));
    }
    run_shell_command(cwd, &command)
}

/// Prepend common developer directories so GUI-launched Reaper finds brew tools.
pub fn ensure_developer_path() {
    let mut prepend: Vec<PathBuf> = Vec::new();
    for dir in [
        "/opt/homebrew/bin",
        "/opt/homebrew/sbin",
        "/usr/local/bin",
        "/usr/local/sbin",
    ] {
        let path = PathBuf::from(dir);
        if path.is_dir() {
            prepend.push(path);
        }
    }

    let current = std::env::var_os("PATH").unwrap_or_default();
    let mut merged: Vec<PathBuf> = prepend;
    for entry in std::env::split_paths(&current) {
        if !merged.contains(&entry) {
            merged.push(entry);
        }
    }
    if let Ok(joined) = std::env::join_paths(&merged) {
        std::env::set_var("PATH", joined);
    }
}

/// Run a formatter/linter via login shell (Homebrew PATH in GUI apps).
pub fn try_shell_stdin_command(cwd: &Path, program: &str, args: &[&str], content: &str) -> Result<String> {
    use std::io::Write;

    let mut command = shell_quote(program);
    for arg in args {
        command.push(' ');
        command.push_str(&shell_quote(arg));
    }
    let shell = crate::platform::login_shell();
    let mut child = crate::platform::command(&shell);
    crate::platform::configure_shell_script(&mut child, &command);
    let mut child = child
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("failed to run shell formatter: {command}"))?;

    if let Some(stdin) = child.stdin.as_mut() {
        stdin.write_all(content.as_bytes())?;
    }

    let out = child.wait_with_output()?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        bail!("{program} failed: {err}");
    }

    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Run a formatter/linter that reads stdin and writes stdout.
pub fn try_stdin_command(cwd: &Path, program: &str, args: &[&str], content: &str) -> Result<String> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let mut child = crate::platform::command(program);
    let mut child = child
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("failed to run {program}"))?;

    if let Some(stdin) = child.stdin.as_mut() {
        stdin.write_all(content.as_bytes())?;
    }

    let out = child.wait_with_output()?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        bail!("{program} failed: {err}");
    }

    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Like `try_stdin_command`, resolving the program from Settings → Compiler.
pub fn try_tool_stdin(cwd: &Path, tool_id: &str, args: &[&str], content: &str) -> Result<String> {
    let program = toolchain::resolve_program(tool_id)
        .with_context(|| format!("{tool_id} not found — set it in Settings → Compiler"))?;
    try_stdin_command(cwd, program.to_string_lossy().as_ref(), args, content)
}
