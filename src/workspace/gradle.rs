use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Serialize;

use crate::git::GitOutput;

use super::safe_join;

#[derive(Debug, Clone)]
pub struct GradleCommand {
    pub program: PathBuf,
    pub cwd: PathBuf,
    pub project_args: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GradleProjectInfo {
    pub is_gradle: bool,
    pub project_root: String,
    pub has_wrapper: bool,
    pub default_task: String,
    pub application_main: Option<String>,
    pub tasks: Vec<String>,
    pub has_junit: bool,
    pub has_spring_test: bool,
    pub has_jacoco: bool,
    pub has_lombok: bool,
    pub has_slf4j: bool,
}

pub fn gradle_project_info(ws: &Path, rel_path: &str) -> Result<GradleProjectInfo> {
    let empty = GradleProjectInfo {
        is_gradle: false,
        project_root: String::new(),
        has_wrapper: false,
        default_task: String::new(),
        application_main: None,
        tasks: Vec::new(),
        has_junit: false,
        has_spring_test: false,
        has_jacoco: false,
        has_lombok: false,
        has_slf4j: false,
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

    let markers = super::java_ecosystem::scan_gradle_project(&root);

    let mut tasks = vec!["build".to_string(), "test".to_string(), "clean".to_string()];
    if has_application {
        tasks.insert(0, "run".to_string());
    }
    if markers.jacoco {
        tasks.push("jacocoTestReport".to_string());
    }

    Ok(GradleProjectInfo {
        is_gradle: true,
        project_root,
        has_wrapper,
        default_task,
        application_main,
        tasks,
        has_junit: markers.junit,
        has_spring_test: markers.spring_test,
        has_jacoco: markers.jacoco,
        has_lombok: markers.lombok,
        has_slf4j: markers.slf4j,
    })
}

/// Split a Gradle task string into argv, tolerating UI copy/paste noise and `/` in `--tests` filters.
pub fn parse_gradle_task(task: &str) -> Result<Vec<String>> {
    let task = task.trim();
    if task.is_empty() {
        bail!("gradle task required");
    }

    let mut raw: Vec<String> = Vec::new();
    for word in task.split_whitespace() {
        if word.starts_with('(') {
            // Terminal UI suffix like (module/src/test/java/Foo.java)
            break;
        }
        raw.push(word.to_string());
    }

    let mut args = Vec::new();
    let mut i = 0;
    while i < raw.len() {
        if raw[i] == "--tests" {
            args.push("--tests".into());
            i += 1;
            let pattern = raw.get(i).ok_or_else(|| anyhow::anyhow!("--tests requires a pattern"))?;
            args.push(normalize_test_pattern(pattern));
            i += 1;
        } else if let Some(rest) = raw[i].strip_prefix("--tests=") {
            args.push(format!("--tests={}", normalize_test_pattern(rest)));
            i += 1;
        } else if raw[i].contains('/') || raw[i].contains('\\') {
            bail!("invalid gradle task");
        } else {
            args.push(raw[i].clone());
            i += 1;
        }
    }
    Ok(args)
}

fn normalize_test_pattern(pattern: &str) -> String {
    pattern.trim().replace('/', ".")
}

pub fn gradle_test_task_name(project_root: &str) -> String {
    let root = project_root.trim().replace('\\', "/");
    let root = root.strip_prefix("./").unwrap_or(&root);
    if root.is_empty() || root == "." {
        return "test".into();
    }
    let segments: Vec<&str> = root.split('/').filter(|s| !s.is_empty()).collect();
    if segments.is_empty() {
        "test".into()
    } else {
        format!(":{}:test", segments.join(":"))
    }
}

pub fn run_gradle(ws: &Path, rel_path: &str, task: &str) -> Result<GitOutput> {
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
    args.push("--console=plain".into());
    args.extend(parts);
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();

    let mut log = String::new();
    log.push_str(&format!(
        "$ {} {}\n",
        cmd.program.display(),
        arg_refs.join(" ")
    ));

    let out = run_gradle_with_command(&cmd, &arg_refs)?;
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

pub fn run_gradle_with_command(cmd: &GradleCommand, args: &[&str]) -> Result<GitOutput> {
    use std::process::{Command, Stdio};

    let mut process = Command::new(
        cmd.program
            .to_str()
            .with_context(|| format!("gradle program path is not valid UTF-8: {}", cmd.program.display()))?,
    );
    process
        .args(args)
        .current_dir(&cmd.cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    crate::jdk::apply_gradle_java_env(&mut process);

    let output = process
        .output()
        .with_context(|| format!("failed to run {}", cmd.program.display()))?;

    Ok(GitOutput {
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        exit_code: output.status.code().unwrap_or(-1),
    })
}

pub fn resolve_gradle_command(project_root: &Path) -> Result<GradleCommand> {
    let project_root = project_root
        .canonicalize()
        .with_context(|| format!("resolve gradle project root {}", project_root.display()))?;

    #[cfg(windows)]
    if project_root.join("gradlew.bat").is_file() {
        return Ok(GradleCommand {
            program: PathBuf::from("gradlew.bat"),
            cwd: project_root,
            project_args: Vec::new(),
        });
    }

    #[cfg(not(windows))]
    if project_root.join("gradlew").is_file() {
        ensure_gradlew_executable(&project_root)?;
        return Ok(GradleCommand {
            program: PathBuf::from("./gradlew"),
            cwd: project_root,
            project_args: Vec::new(),
        });
    }

    if let Some(bundled) = bundled_gradlew() {
        return Ok(GradleCommand {
            program: bundled.clone(),
            cwd: bundled
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| bundled.clone()),
            project_args: vec![
                "-p".into(),
                project_root.to_string_lossy().into_owned(),
            ],
        });
    }

    if let Some(gradle) = find_cached_gradle_distribution() {
        return Ok(GradleCommand {
            program: gradle,
            cwd: project_root.clone(),
            project_args: Vec::new(),
        });
    }

    if let Ok(path) = which_gradle_on_path() {
        return Ok(GradleCommand {
            program: path,
            cwd: project_root,
            project_args: Vec::new(),
        });
    }

    bail!(
        "Gradle not found for {}. Add a Gradle wrapper (gradlew) to the project or install Gradle.",
        project_root.display()
    )
}

fn bundled_gradlew() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("REAPER_GRADLE_HOME") {
        let gradlew = PathBuf::from(&dir).join("gradlew");
        if gradlew.is_file() {
            return Some(gradlew);
        }
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(mac_os) = exe.parent() {
            let bundled = mac_os.join("../Resources/gradlew");
            if bundled.is_file() {
                return Some(bundled.canonicalize().unwrap_or(bundled));
            }
        }
    }

    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("gradlew");
    if manifest.is_file() {
        return Some(manifest);
    }

    None
}

fn find_cached_gradle_distribution() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    let dists = PathBuf::from(home).join(".gradle/wrapper/dists");
    let mut candidates = Vec::new();
    collect_gradle_bins(&dists, &mut candidates);
    candidates.sort();
    candidates.pop()
}

fn collect_gradle_bins(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let bin = path.join("gradle").join("bin").join("gradle");
            #[cfg(windows)]
            let bin = path.join("gradle").join("bin").join("gradle.bat");
            if bin.is_file() {
                out.push(bin);
            }
            collect_gradle_bins(&path, out);
        }
    }
}

fn which_gradle_on_path() -> Result<PathBuf> {
    let output = std::process::Command::new("which")
        .arg("gradle")
        .output()
        .context("failed to run which gradle")?;
    if !output.status.success() {
        bail!("gradle not on PATH");
    }
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if path.is_empty() {
        bail!("gradle not on PATH");
    }
    Ok(PathBuf::from(path))
}

pub fn find_gradle_root(ws: &Path, rel_path: &str) -> Result<Option<PathBuf>> {
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

        if is_gradle_project_dir(&dir) {
            return Ok(Some(dir));
        }

        let Some(parent) = dir.parent() else {
            break;
        };
        dir = parent.to_path_buf();
    }

    if rel_path == "." || rel_path.is_empty() {
        return find_first_gradle_root(ws);
    }

    Ok(None)
}

fn is_gradle_project_dir(dir: &Path) -> bool {
    dir.join("settings.gradle").is_file()
        || dir.join("settings.gradle.kts").is_file()
        || dir.join("build.gradle").is_file()
        || dir.join("build.gradle.kts").is_file()
}

/// Discover Gradle project roots under a workspace (supports nested layouts like `repo-1/`).
pub fn find_all_gradle_roots(ws: &Path) -> Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    collect_gradle_roots(ws, ws, 0, 8, &mut found)?;
    found.sort_by(|a, b| a.display().to_string().cmp(&b.display().to_string()));
    found.dedup();
    Ok(found)
}

fn collect_gradle_roots(
    ws: &Path,
    dir: &Path,
    depth: usize,
    max_depth: usize,
    out: &mut Vec<PathBuf>,
) -> Result<()> {
    if depth > max_depth {
        return Ok(());
    }
    let dir_canon = dir
        .canonicalize()
        .with_context(|| format!("resolve {}", dir.display()))?;
    let ws_canon = ws
        .canonicalize()
        .with_context(|| format!("resolve workspace {}", ws.display()))?;
    if !dir_canon.starts_with(&ws_canon) {
        return Ok(());
    }

    if is_gradle_project_dir(dir) {
        out.push(dir_canon);
        return Ok(());
    }

    let Ok(entries) = std::fs::read_dir(dir) else {
        return Ok(());
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = entry.file_name();
        if name == ".git" || name == ".reaper" || name == "node_modules" || name == "build" {
            continue;
        }
        collect_gradle_roots(ws, &path, depth + 1, max_depth, out)?;
    }
    Ok(())
}

fn find_first_gradle_root(ws: &Path) -> Result<Option<PathBuf>> {
    Ok(find_all_gradle_roots(ws)?.into_iter().next())
}

pub fn rel_path_for(ws: &Path, path: &Path) -> Result<String> {
    let ws_canon = ws.canonicalize()?;
    let path_canon = path.canonicalize()?;
    Ok(path_canon
        .strip_prefix(&ws_canon)
        .with_context(|| "path outside workspace")?
        .to_string_lossy()
        .replace('\\', "/"))
}

pub fn is_spring_boot_project(root: &Path) -> bool {
    read_build_file(root)
        .map(|content| content.contains("org.springframework.boot"))
        .unwrap_or(false)
}

pub fn read_build_file_content(root: &Path) -> Option<String> {
    read_build_file(root)
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

pub fn ensure_gradlew_executable(root: &Path) -> Result<()> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_gradlew_exists_in_repo() {
        let gradlew = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("gradlew");
        assert!(gradlew.is_file(), "bundled gradlew should exist at repo root");
    }

    #[test]
    fn resolve_gradlew_from_relative_project_root() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("data/workspaces/somayaj/repo-1");
        if !root.join("gradlew").is_file() {
            return;
        }
        let rel = PathBuf::from("./data/workspaces/somayaj/repo-1");
        let cmd = resolve_gradle_command(&rel).expect("resolve gradle command");
        assert_eq!(cmd.program, PathBuf::from("./gradlew"));
        assert!(cmd.cwd.join("gradlew").is_file());
    }

    #[test]
    fn parse_gradle_task_ignores_ui_path_suffix() {
        let args = parse_gradle_task(
            "test --tests org.springframework.boot.test.web.FooTests (spring-boot-project/spring-boot-test/src/test/java/org/springframework/boot/test/web/FooTests.java)",
        )
        .expect("parse task");
        assert_eq!(
            args,
            vec![
                "test".to_string(),
                "--tests".to_string(),
                "org.springframework.boot.test.web.FooTests".to_string(),
            ]
        );
    }

    #[test]
    fn parse_gradle_task_normalizes_slash_test_filters() {
        let args = parse_gradle_task("test --tests org/springframework/boot/FooTests")
            .expect("parse task");
        assert_eq!(args[2], "org.springframework.boot.FooTests");
    }

    #[test]
    fn gradle_test_task_name_for_nested_module() {
        assert_eq!(
            gradle_test_task_name("spring-boot-project/spring-boot-test"),
            ":spring-boot-project:spring-boot-test:test"
        );
        assert_eq!(gradle_test_task_name("."), "test");
    }
}
