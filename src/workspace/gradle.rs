use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Serialize;

use crate::git::GitOutput;

use super::exec::run_command;
use super::safe_join;

#[derive(Debug, Clone, Serialize)]
pub struct GradleProjectInfo {
    pub is_gradle: bool,
    pub project_root: String,
    pub has_wrapper: bool,
    pub default_task: String,
    pub application_main: Option<String>,
    pub tasks: Vec<String>,
}

pub fn gradle_project_info(ws: &Path, rel_path: &str) -> Result<GradleProjectInfo> {
    let empty = GradleProjectInfo {
        is_gradle: false,
        project_root: String::new(),
        has_wrapper: false,
        default_task: String::new(),
        application_main: None,
        tasks: Vec::new(),
    };

    let _ = safe_join(ws, rel_path)?;
    let Some(root) = find_gradle_root(ws, rel_path)? else {
        return Ok(empty);
    };

    let project_root = rel_path_for(ws, &root)?;
    let has_wrapper = root.join("gradlew").exists() || root.join("gradlew.bat").exists();
    let build_content = read_build_file(&root).unwrap_or_default();
    let application_main = find_application_main(&build_content);
    let has_application = has_application_plugin(&build_content);
    let default_task = if has_application {
        "run".to_string()
    } else {
        "build".to_string()
    };

    let mut tasks = vec!["build".to_string(), "test".to_string(), "clean".to_string()];
    if has_application {
        tasks.insert(0, "run".to_string());
    }

    Ok(GradleProjectInfo {
        is_gradle: true,
        project_root,
        has_wrapper,
        default_task,
        application_main,
        tasks,
    })
}

pub fn run_gradle(ws: &Path, rel_path: &str, task: &str) -> Result<GitOutput> {
    let task = task.trim();
    if task.is_empty() {
        bail!("gradle task required");
    }
    if task.contains(char::is_whitespace) || task.contains('/') || task.contains('\\') {
        bail!("invalid gradle task");
    }

    let root = find_gradle_root(ws, rel_path)?
        .ok_or_else(|| anyhow::anyhow!("not inside a Gradle project"))?;

    ensure_gradlew_executable(&root)?;

    let (program, extra_args) = gradle_program(&root);
    let mut args: Vec<&str> = extra_args;
    args.extend(["--no-daemon", "--console=plain", task]);

    let mut log = String::new();
    log.push_str(&format!("$ cd {} && {} {}\n", rel_path_for(ws, &root)?, program, args.join(" ")));

    let out = run_command(&root, program, &args)?;
    log.push_str(&out.stdout);
    if !out.stderr.is_empty() {
        if !log.ends_with('\n') {
            log.push('\n');
        }
        log.push_str(&out.stderr);
    }

    Ok(GitOutput {
        stdout: log,
        stderr: String::new(),
        exit_code: out.exit_code,
    })
}

fn find_gradle_root(ws: &Path, rel_path: &str) -> Result<Option<PathBuf>> {
    let file_path = safe_join(ws, rel_path)?;
    let ws_canon = ws
        .canonicalize()
        .with_context(|| format!("resolve workspace {}", ws.display()))?;

    let mut dir = if file_path.is_file() {
        file_path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| file_path.clone())
    } else {
        file_path.clone()
    };

    loop {
        let dir_canon = dir
            .canonicalize()
            .with_context(|| format!("resolve {}", dir.display()))?;
        if !dir_canon.starts_with(&ws_canon) {
            break;
        }

        let has_settings = dir.join("settings.gradle").is_file()
            || dir.join("settings.gradle.kts").is_file();
        let has_build = dir.join("build.gradle").is_file() || dir.join("build.gradle.kts").is_file();

        if has_settings || has_build {
            return Ok(Some(dir));
        }

        let Some(parent) = dir.parent() else {
            break;
        };
        dir = parent.to_path_buf();
    }

    Ok(None)
}

fn rel_path_for(ws: &Path, path: &Path) -> Result<String> {
    let ws_canon = ws.canonicalize()?;
    let path_canon = path.canonicalize()?;
    Ok(path_canon
        .strip_prefix(&ws_canon)
        .with_context(|| "path outside workspace")?
        .to_string_lossy()
        .replace('\\', "/"))
}

fn read_build_file(root: &Path) -> Option<String> {
    for name in ["build.gradle", "build.gradle.kts"] {
        let path = root.join(name);
        if path.is_file() {
            return std::fs::read_to_string(path).ok();
        }
    }
    None
}

fn has_application_plugin(content: &str) -> bool {
    let normalized: String = content
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    normalized.contains("id'application'")
        || normalized.contains("id\"application\"")
        || normalized.contains("id(\"application\")")
        || normalized.contains("id('application')")
}

fn find_application_main(content: &str) -> Option<String> {
    for line in content.lines() {
        let trimmed = line.split("//").next().unwrap_or(line).trim();
        if let Some(rest) = trimmed.strip_prefix("mainClass") {
            let rest = rest.trim_start_matches(['=', ' ']);
            if let Some(class) = parse_quoted_value(rest) {
                return Some(class);
            }
        }
        if trimmed.contains("mainClass") {
            if let Some(idx) = trimmed.find("mainClass") {
                let rest = &trimmed[idx + "mainClass".len()..];
                let rest = rest.trim_start_matches(['=', ' ', '.', 'g', 'e', 't', '(', ')']);
                if let Some(class) = parse_quoted_value(rest) {
                    return Some(class);
                }
            }
        }
    }
    None
}

fn parse_quoted_value(s: &str) -> Option<String> {
    let s = s.trim();
    if let Some(rest) = s.strip_prefix('\'') {
        return rest.split('\'').next().map(str::to_string);
    }
    if let Some(rest) = s.strip_prefix('"') {
        return rest.split('"').next().map(str::to_string);
    }
    None
}

fn gradle_program(root: &Path) -> (&'static str, Vec<&'static str>) {
    #[cfg(windows)]
    {
        if root.join("gradlew.bat").is_file() {
            return ("gradlew.bat", Vec::new());
        }
    }
    #[cfg(not(windows))]
    {
        if root.join("gradlew").is_file() {
            return ("./gradlew", Vec::new());
        }
    }
    ("gradle", Vec::new())
}

fn ensure_gradlew_executable(root: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let gradlew = root.join("gradlew");
        if gradlew.is_file() {
            let meta = std::fs::metadata(&gradlew)?;
            let mut perms = meta.permissions();
            if perms.mode() & 0o111 == 0 {
                perms.set_mode(perms.mode() | 0o755);
                std::fs::set_permissions(&gradlew, perms)?;
            }
        }
    }
    Ok(())
}
