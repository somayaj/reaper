use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use anyhow::{Context, Result, bail};
use serde::Serialize;

use crate::git::GitOutput;
use crate::jdk;

use super::{read_file, safe_join};
use super::exec::run_java_command;

#[derive(Debug, Clone, Serialize)]
pub struct JavaMainInfo {
    pub runnable: bool,
    pub class_name: String,
    pub package: Option<String>,
    pub qualified_name: String,
}

pub fn java_main_info(ws: &Path, rel_path: &str) -> Result<JavaMainInfo> {
    let file_path = safe_join(ws, rel_path)?;
    if !file_path.is_file() {
        bail!("not a file");
    }
    if !rel_path.ends_with(".java") {
        return Ok(JavaMainInfo {
            runnable: false,
            class_name: String::new(),
            package: None,
            qualified_name: String::new(),
        });
    }

    let source = read_file(ws, rel_path)?;
    match parse_java_main(&source, &file_path) {
        Ok(info) => Ok(info),
        Err(_) => Ok(JavaMainInfo {
            runnable: false,
            class_name: String::new(),
            package: None,
            qualified_name: String::new(),
        }),
    }
}

pub fn run_java_main(ws: &Path, rel_path: &str) -> Result<GitOutput> {
    let file_path = safe_join(ws, rel_path)?;
    if !file_path.is_file() {
        bail!("not a file");
    }
    if !rel_path.ends_with(".java") {
        bail!("not a Java file");
    }

    let source = read_file(ws, rel_path)?;
    let info = parse_java_main(&source, &file_path)?;

    // safe_join canonicalizes paths; use the validated relative path for javac
    let rel = rel_path.replace('\\', "/");

    let mut compile_log = String::new();
    compile_log.push_str(&format!("$ javac -d .reaper/java-out {rel}\n"));

    let compile = plain_javac_output(ws, &rel, false)?;
    let compile = GitOutput {
        stdout: String::from_utf8_lossy(&compile.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&compile.stderr).into_owned(),
        exit_code: compile.status.code().unwrap_or(-1),
    };
    compile_log.push_str(&compile.stdout);
    compile_log.push_str(&compile.stderr);

    if !compile.success() {
        return Ok(GitOutput {
            stdout: compile_log,
            stderr: String::new(),
            exit_code: compile.exit_code,
        });
    }

    let mut run_log = compile_log;
    run_log.push_str(&format!(
        "\n$ java -cp .reaper/java-out {}\n",
        info.qualified_name
    ));

    let run = run_java_command(
        ws,
        "java",
        &["-cp", ".reaper/java-out", &info.qualified_name],
    )?;
    run_log.push_str(&run.stdout);
    if !run.stderr.is_empty() {
        run_log.push_str(&run.stderr);
    }

    Ok(GitOutput {
        stdout: run_log,
        stderr: String::new(),
        exit_code: run.exit_code,
    })
}

pub fn parse_java_main(source: &str, file_path: &Path) -> Result<JavaMainInfo> {
    if !has_static_main(source) {
        bail!("no public static void main method found");
    }

    let package = find_package(source);
    let stem = file_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Main")
        .to_string();

    let class_name = find_public_class(source).unwrap_or_else(|| find_class(source).unwrap_or(stem));

    let qualified_name = match &package {
        Some(pkg) => format!("{pkg}.{class_name}"),
        None => class_name.clone(),
    };

    Ok(JavaMainInfo {
        runnable: true,
        class_name,
        package,
        qualified_name,
    })
}

/// True when the source likely launches a desktop window (Swing/AWT/JavaFX).
pub fn is_gui_java_source(source: &str) -> bool {
    source.contains("javax.swing")
        || source.contains("java.awt")
        || source.contains("javafx.")
        || source.contains("org.eclipse.swt")
}

/// Compile a single `.java` file into `.reaper/java-out` using the configured JDK.
pub fn plain_javac_output(cwd: &Path, rel_path: &str, debug: bool) -> Result<Output> {
    let home = jdk::effective_java_home()?;
    std::fs::create_dir_all(cwd.join(".reaper/java-out"))?;
    let rel = rel_path.replace('\\', "/");
    let mut cmd = crate::platform::command_user_process(jdk::javac_bin(&home));
    cmd.current_dir(cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut args = vec![
        "-d".to_string(),
        ".reaper/java-out".to_string(),
        "-encoding".to_string(),
        "UTF-8".to_string(),
    ];
    if debug {
        args.push("-g".to_string());
    }
    args.push(rel);
    cmd.args(&args);
    jdk::apply_java_env(&mut cmd);
    cmd.output()
        .with_context(|| format!("failed to run javac in {}", cwd.display()))
}

fn java_user_command(cwd: &Path, args: &[&str]) -> Result<Command> {
    let home = jdk::effective_java_home()?;
    let mut cmd = crate::platform::command_user_process(jdk::java_bin(&home));
    cmd.current_dir(cwd)
        .args(args);
    jdk::apply_java_env(&mut cmd);
    Ok(cmd)
}

/// Run a main class from `.reaper/java-out` using the configured JDK.
pub fn plain_java_run_command(cwd: &Path, main_class: &str) -> Result<Command> {
    java_user_command(cwd, &["-cp", ".reaper/java-out", main_class])
}

fn has_static_main(source: &str) -> bool {
    let normalized: String = source
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    normalized.contains("staticvoidmain(")
}

fn find_package(source: &str) -> Option<String> {
    for line in source.lines() {
        let trimmed = line.split("//").next().unwrap_or(line).trim();
        if let Some(rest) = trimmed.strip_prefix("package ") {
            if let Some(pkg) = rest.strip_suffix(';') {
                let pkg = pkg.trim();
                if !pkg.is_empty() && pkg.chars().all(|c| c.is_ascii_alphanumeric() || c == '.') {
                    return Some(pkg.to_string());
                }
            }
        }
    }
    None
}

fn find_public_class(source: &str) -> Option<String> {
    find_class_after_keyword(source, "public class ")
}

fn find_class(source: &str) -> Option<String> {
    find_class_after_keyword(source, "class ")
}

fn find_class_after_keyword(source: &str, keyword: &str) -> Option<String> {
    let bytes = source.as_bytes();
    let key = keyword.as_bytes();
    let mut i = 0;
    while i + key.len() <= bytes.len() {
        if &bytes[i..i + key.len()] == key {
            let before_ok = i == 0 || !bytes[i - 1].is_ascii_alphanumeric() && bytes[i - 1] != b'_';
            if before_ok {
                let mut j = i + key.len();
                while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                    j += 1;
                }
                let start = j;
                while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
                    j += 1;
                }
                if j > start {
                    return Some(source[start..j].to_string());
                }
            }
        }
        i += 1;
    }
    None
}

