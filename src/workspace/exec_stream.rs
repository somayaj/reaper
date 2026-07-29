use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;

use anyhow::{Context, Result, bail};
use serde::Serialize;
use tokio::sync::mpsc as async_mpsc;

use crate::git::GitOutput;
use crate::jdk;
use crate::process_registry;
use crate::toolchain;

use super::exec::run_shell_command;
use super::shell;

#[derive(Debug, Clone, Serialize)]
pub struct ExecStreamEvent {
    pub t: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step: Option<String>,
}

fn emit(tx: &async_mpsc::Sender<ExecStreamEvent>, event: ExecStreamEvent) -> bool {
    tx.blocking_send(event).is_ok()
}

fn pump_reader<R: Read + Send + 'static>(
    mut reader: R,
    stream: &'static str,
    tx: mpsc::Sender<ExecStreamEvent>,
) {
    let mut buf = [0u8; 4096];
    loop {
        match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                let text = String::from_utf8_lossy(&buf[..n]).into_owned();
                if tx
                    .send(ExecStreamEvent {
                        t: stream.into(),
                        text: Some(text),
                        code: None,
                        step: None,
                    })
                    .is_err()
                {
                    break;
                }
            }
            Err(_) => break,
        }
    }
}

pub(crate) fn stream_process(cmd: &mut Command, tx: &async_mpsc::Sender<ExecStreamEvent>) -> Result<i32> {
    stream_process_inner(cmd, tx, false)
}

pub(crate) fn stream_process_user(cmd: &mut Command, tx: &async_mpsc::Sender<ExecStreamEvent>) -> Result<i32> {
    stream_process_inner(cmd, tx, true)
}

fn stream_process_inner(
    cmd: &mut Command,
    tx: &async_mpsc::Sender<ExecStreamEvent>,
    user_process: bool,
) -> Result<i32> {
    process_registry::configure_command(cmd);
    if user_process {
        crate::platform::hide_console_window_user(cmd);
    } else {
        crate::platform::hide_console_window(cmd);
    }
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    let label = cmd
        .get_program()
        .to_string_lossy()
        .into_owned();
    let mut child = cmd
        .spawn()
        .with_context(|| "failed to spawn process".to_string())?;
    let _guard = process_registry::guard_for_exec_child(&mut child, &label);

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let (line_tx, line_rx) = mpsc::channel::<ExecStreamEvent>();

    let mut pumps = Vec::new();
    if let Some(out) = stdout {
        let tx = line_tx.clone();
        pumps.push(thread::spawn(move || pump_reader(out, "stdout", tx)));
    }
    if let Some(err) = stderr {
        let tx = line_tx.clone();
        pumps.push(thread::spawn(move || pump_reader(err, "stderr", tx)));
    }
    drop(line_tx);

    let async_tx = tx.clone();
    let relay = thread::spawn(move || {
        while let Ok(event) = line_rx.recv() {
            if !emit(&async_tx, event) {
                break;
            }
        }
    });

    let status = process_registry::wait_on_child(&mut child).context("failed to wait on process")?;
    for pump in pumps {
        let _ = pump.join();
    }
    let _ = relay.join();

    Ok(status.code().unwrap_or(-1))
}

pub fn stream_shell(ws: &Path, cwd_rel: Option<&str>, command: &str, tx: async_mpsc::Sender<ExecStreamEvent>) -> Result<i32> {
    let command = command.trim();
    if command.is_empty() {
        bail!("command required");
    }
    let work_dir = shell::resolve_work_dir(ws, cwd_rel)?;
    let shell = crate::platform::login_shell();
    let mut cmd = crate::platform::command(&shell);
    crate::platform::configure_shell_script(&mut cmd, command);
    cmd.current_dir(work_dir);
    toolchain::apply_compiler_env(&mut cmd);
    let code = stream_process(&mut cmd, &tx)?;
    let _ = emit(&tx, ExecStreamEvent {
        t: "exit".into(),
        text: None,
        code: Some(code),
        step: None,
    });
    Ok(code)
}

pub fn stream_git(ws: &Path, args: &[&str], step: Option<&str>, tx: async_mpsc::Sender<ExecStreamEvent>) -> Result<i32> {
    let mut cmd = crate::platform::command("git");
    cmd.args(args).current_dir(ws);
    if let Some(step) = step {
        let _ = emit(&tx, ExecStreamEvent {
            t: "step".into(),
            text: Some(step.into()),
            code: None,
            step: Some(step.into()),
        });
    }
    let code = stream_process(&mut cmd, &tx)?;
    let _ = emit(&tx, ExecStreamEvent {
        t: "exit".into(),
        text: None,
        code: Some(code),
        step: step.map(str::to_string),
    });
    Ok(code)
}

pub fn stream_sync(ws: &Path, tx: async_mpsc::Sender<ExecStreamEvent>) -> Result<i32> {
    stream_git(ws, &["pull", "--ff-only"], Some("pull"), tx)
}

pub fn stream_push(ws: &Path, auth_url: &str, branch: &str, tx: async_mpsc::Sender<ExecStreamEvent>) -> Result<i32> {
    let refspec = format!("HEAD:refs/heads/{branch}");
    stream_git(ws, &["push", auth_url, &refspec], Some("push"), tx)
}

pub fn stream_commit_and_push(
    ws: &Path,
    message: &str,
    paths: Option<&[String]>,
    tx: async_mpsc::Sender<ExecStreamEvent>,
) -> Result<i32> {
    if message.trim().is_empty() {
        bail!("commit message required");
    }

    let add_args: Vec<String> = match paths {
        Some(paths) if !paths.is_empty() => paths.iter().cloned().collect(),
        _ => vec!["-A".into()],
    };

    if add_args.len() == 1 && add_args[0] == "-A" {
        let code = stream_git(ws, &["add", "-A"], Some("add"), tx.clone())?;
        if code != 0 {
            return Ok(code);
        }
    } else {
        for path in &add_args {
            let code = stream_git(ws, &["add", path], Some("add"), tx.clone())?;
            if code != 0 {
                return Ok(code);
            }
        }
    }

    let code = stream_git(
        ws,
        &["commit", "-m", message],
        Some("commit"),
        tx.clone(),
    )?;
    if code != 0 {
        return Ok(code);
    }

    stream_git(ws, &["push"], Some("push"), tx)
}

pub fn stream_gradle(ws: &Path, rel_path: &str, task: &str, tx: async_mpsc::Sender<ExecStreamEvent>) -> Result<i32> {
    use super::gradle::{parse_gradle_task, resolve_gradle_command, find_gradle_root};

    let task = task.trim();
    if task.is_empty() {
        bail!("gradle task required");
    }
    let parts = parse_gradle_task(task)?;
    let root = find_gradle_root(ws, rel_path)?
        .ok_or_else(|| anyhow::anyhow!("not inside a Gradle project"))?;
    let cmd = resolve_gradle_command(&root)?;
    let mut args = cmd.project_args.clone();
    args.push("--no-daemon".into());
    args.push("--no-configuration-cache".into());
    args.push("--console=plain".into());
    args.extend(parts);
    stream_gradle_command(&cmd, &args, tx)
}

pub fn stream_gradle_command(
    cmd: &super::gradle::GradleCommand,
    args: &[String],
    tx: async_mpsc::Sender<ExecStreamEvent>,
) -> Result<i32> {
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();

    let label = format!("$ {} {}", cmd.program.display(), arg_refs.join(" "));
    let _ = emit(&tx, ExecStreamEvent {
        t: "stdout".into(),
        text: Some(format!("{label}\n")),
        code: None,
        step: Some("gradle".into()),
    });

    let mut command = crate::platform::command_path(&cmd.program);
    command.args(&arg_refs).current_dir(&cmd.cwd);
    if let Ok(home) = super::gradle::gradle_java_home_for_project(&cmd.cwd) {
        jdk::apply_java_home(&mut command, &home);
    }
    let code = stream_process(&mut command, &tx)?;
    let _ = emit(&tx, ExecStreamEvent {
        t: "exit".into(),
        text: None,
        code: Some(code),
        step: Some("gradle".into()),
    });
    Ok(code)
}

fn emit_javac_compiler_output(
    tx: &async_mpsc::Sender<ExecStreamEvent>,
    output: &std::process::Output,
    step: &str,
) {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let mut combined = String::new();
    if !stderr.is_empty() {
        combined.push_str(&stderr);
        if !stderr.ends_with('\n') {
            combined.push('\n');
        }
    }
    if !stdout.is_empty() {
        combined.push_str(&stdout);
    }
    if combined.trim().is_empty() && !output.status.success() {
        combined = format!(
            "javac failed with exit code {}\n",
            output.status.code().unwrap_or(-1)
        );
    }
    if !combined.is_empty() {
        let _ = emit(
            tx,
            ExecStreamEvent {
                t: "stdout".into(),
                text: Some(combined),
                code: None,
                step: Some(step.into()),
            },
        );
    }
}

pub fn stream_java_main(ws: &Path, rel_path: &str, tx: async_mpsc::Sender<ExecStreamEvent>) -> Result<i32> {
    use super::java::parse_java_main;
    use super::run_project;
    use super::{normalize_workspace_source_path, read_file, safe_join};

    let rel_path = normalize_workspace_source_path(rel_path);
    let file_path = safe_join(ws, &rel_path)?;
    if !file_path.is_file() {
        bail!("not a file");
    }
    if !rel_path.ends_with(".java") {
        bail!("not a Java file");
    }

    let source = read_file(ws, &rel_path)?;
    if let Some(code) = run_project::try_stream_spring_boot_main(ws, &rel_path, &source, tx.clone())? {
        return Ok(code);
    }
    // Maven/Gradle mains: compile + run with the resolved dependency classpath (mvnw/gradlew).
    if let Some(code) =
        run_project::try_stream_build_tool_java_main(ws, &rel_path, &source, tx.clone())?
    {
        return Ok(code);
    }

    let info = parse_java_main(&source, &file_path)?;
    let rel = rel_path.replace('\\', "/");

    let _ = emit(&tx, ExecStreamEvent {
        t: "stdout".into(),
        text: Some(format!("$ javac -d .reaper/java-out {rel}\n")),
        code: None,
        step: Some("javac".into()),
    });

    let compile = super::java::plain_javac_output(ws, &rel, false)?;
    emit_javac_compiler_output(&tx, &compile, "javac");
    if !compile.status.success() {
        let compile_code = compile.status.code().unwrap_or(-1);
        let _ = emit(&tx, ExecStreamEvent {
            t: "exit".into(),
            text: None,
            code: Some(compile_code),
            step: Some("javac".into()),
        });
        return Ok(compile_code);
    }

    let _ = emit(&tx, ExecStreamEvent {
        t: "stdout".into(),
        text: Some(format!("\n$ java -cp .reaper/java-out {}\n", info.qualified_name)),
        code: None,
        step: Some("java".into()),
    });

    let mut java = super::java::plain_java_run_command(ws, &info.qualified_name)?;
    let run_code = stream_process_user(&mut java, &tx)?;
    let _ = emit(&tx, ExecStreamEvent {
        t: "exit".into(),
        text: None,
        code: Some(run_code),
        step: Some("java".into()),
    });
    Ok(run_code)
}

pub fn stream_maven(ws: &Path, rel_path: &str, goal: &str, tx: async_mpsc::Sender<ExecStreamEvent>) -> Result<i32> {
    use super::maven::{find_maven_root, parse_maven_goal, resolve_maven_command};

    let parts = parse_maven_goal(goal)?;
    let root = find_maven_root(ws, rel_path)?
        .ok_or_else(|| anyhow::anyhow!("not inside a Maven project"))?;
    let cmd = resolve_maven_command(&root);
    let mut args = cmd.project_args.clone();
    // `-am` includes packaging=pom parents; `spring-boot:run` then fails on the reactor
    // ("Unable to find a suitable main class" on enterprise-platform). Keep `-pl` only.
    if is_maven_app_run_goal(&parts) {
        args.retain(|a| a != "-am");
    }
    // Do not pass `-q`: it hides compiler-plugin/javac diagnostics on failure.
    args.push("--batch-mode".to_string());
    args.extend(parts);
    stream_maven_command(&cmd, &args, tx)
}

fn is_maven_app_run_goal(parts: &[String]) -> bool {
    parts.iter().any(|p| {
        let p = p.as_str();
        p == "spring-boot:run"
            || p.ends_with(":spring-boot:run")
            || p == "exec:java"
            || p.ends_with(":exec:java")
    })
}

pub fn stream_maven_command(
    cmd: &super::maven::MavenCommand,
    args: &[String],
    tx: async_mpsc::Sender<ExecStreamEvent>,
) -> Result<i32> {
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();

    let label = format!("$ {} {}", cmd.program.display(), arg_refs.join(" "));
    let _ = emit(&tx, ExecStreamEvent {
        t: "stdout".into(),
        text: Some(format!("{label}\n")),
        code: None,
        step: Some("maven".into()),
    });

    let mut command = crate::platform::command_path(&cmd.program);
    command.args(&arg_refs).current_dir(&cmd.cwd);
    jdk::apply_java_env(&mut command);
    let code = stream_process(&mut command, &tx)?;
    let _ = emit(&tx, ExecStreamEvent {
        t: "exit".into(),
        text: None,
        code: Some(code),
        step: Some("maven".into()),
    });
    Ok(code)
}

/// Fallback for tests — buffered shell (unchanged behavior).
#[allow(dead_code)]
pub fn run_shell_buffered(ws: &Path, cwd_rel: Option<&str>, command: &str) -> Result<GitOutput> {
    let command = command.trim();
    if command.is_empty() {
        bail!("command required");
    }
    let work_dir = shell::resolve_work_dir(ws, cwd_rel)?;
    run_shell_command(&work_dir, command)
}
