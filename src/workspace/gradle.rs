use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

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
    pub is_spring_boot: bool,
    pub has_junit: bool,
    pub has_spring_test: bool,
    pub has_jacoco: bool,
    pub has_lombok: bool,
    pub has_slf4j: bool,
}

pub fn gradle_project_info(ws: &Path, rel_path: &str) -> Result<GradleProjectInfo> {
    let rel_path = super::normalize_workspace_source_path(rel_path);
    let empty = GradleProjectInfo {
        is_gradle: false,
        project_root: String::new(),
        has_wrapper: false,
        default_task: String::new(),
        application_main: None,
        tasks: Vec::new(),
        is_spring_boot: false,
        has_junit: false,
        has_spring_test: false,
        has_jacoco: false,
        has_lombok: false,
        has_slf4j: false,
    };

    let _ = safe_join(ws, &rel_path)?;
    let Some(root) = find_gradle_root(ws, &rel_path)? else {
        return Ok(empty);
    };

    let project_root = rel_path_for(ws, &root)?;
    let has_wrapper = root.join("gradlew").exists() || root.join("gradlew.bat").exists();
    let build_content = read_build_file(&root).unwrap_or_default();
    let application_main = find_application_main(&build_content);
    let is_spring_boot = is_spring_boot_project_for_file(ws, &rel_path).unwrap_or(false)
        || is_spring_boot_project(&root);
    let has_application = has_application_plugin(&build_content);
    let default_task = if is_spring_boot {
        "bootRun".to_string()
    } else if has_application {
        "run".to_string()
    } else {
        "build".to_string()
    };

    let markers = super::java_ecosystem::scan_gradle_project(&root);

    let mut tasks = vec!["build".to_string(), "test".to_string(), "clean".to_string()];
    if is_spring_boot {
        tasks.insert(0, "bootRun".to_string());
    } else if has_application {
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
        is_spring_boot,
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
        } else if (raw[i].contains('/') || raw[i].contains('\\'))
            && !raw[i].starts_with("--")
            && !raw[i].starts_with("-D")
        {
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

/// Run `compileTestJava` for the resolved Gradle project (used for test-source diagnostics).
pub fn run_gradle_compile_test_java(project_root: &Path) -> Result<GitOutput> {
    let cmd = resolve_gradle_command(project_root)?;
    let mut arg_owned = cmd.project_args.clone();
    arg_owned.extend([
        "--no-daemon".into(),
        "--parallel".into(),
        "--console=plain".into(),
        "-q".into(),
        "compileTestJava".into(),
    ]);
    let arg_refs: Vec<&str> = arg_owned.iter().map(String::as_str).collect();
    run_gradle_with_command(&cmd, &arg_refs)
}

pub fn run_gradle_with_command(cmd: &GradleCommand, args: &[&str]) -> Result<GitOutput> {
    use std::process::{Command, Stdio};

    if crate::process_registry::is_shutdown_requested() {
        bail!("Reaper is shutting down");
    }

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
    crate::process_registry::configure_command(&mut process);
    if let Ok(home) = gradle_java_home_for_project(&cmd.cwd) {
        crate::jdk::apply_java_home(&mut process, &home);
    }

    let label = format!("gradle {}", args.join(" "));
    let mut child = process
        .spawn()
        .with_context(|| format!("failed to run {}", cmd.program.display()))?;
    let _guard = crate::process_registry::guard_for_child(&mut child, &label);
    let output = child
        .wait_with_output()
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

    let wrapper_root = find_gradle_wrapper_root(&project_root);

    #[cfg(windows)]
    if wrapper_root.join("gradlew.bat").is_file() {
        return Ok(gradle_wrapper_command(&wrapper_root, &project_root, "gradlew.bat"));
    }

    #[cfg(not(windows))]
    if wrapper_root.join("gradlew").is_file() {
        ensure_gradlew_executable(&wrapper_root)?;
        return Ok(gradle_wrapper_command(
            &wrapper_root,
            &project_root,
            "./gradlew",
        ));
    }

    if let Some(gradle) = crate::toolchain::resolve_program("gradle") {
        return Ok(GradleCommand {
            program: gradle,
            cwd: project_root.clone(),
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

/// Directory containing `gradlew` — walk up from a nested module if needed.
pub fn find_gradle_wrapper_root(project_root: &Path) -> PathBuf {
    let mut dir = project_root.to_path_buf();
    loop {
        if dir.join("gradlew").is_file() || dir.join("gradlew.bat").is_file() {
            return dir;
        }
        let Some(parent) = dir.parent() else {
            break;
        };
        dir = parent.to_path_buf();
    }
    project_root.to_path_buf()
}

/// True when `dir` is the Gradle settings / multi-module root.
pub fn is_gradle_settings_root(dir: &Path) -> bool {
    dir.join("settings.gradle").is_file() || dir.join("settings.gradle.kts").is_file()
}

/// Closest Gradle settings / multi-module root above `project_root` (walks parents; gradlew not required).
pub fn find_gradle_settings_repo_root(project_root: &Path) -> PathBuf {
    let mut dir = project_root.to_path_buf();
    loop {
        if is_gradle_settings_root(&dir) {
            return dir;
        }
        let Some(parent) = dir.parent() else {
            break;
        };
        dir = parent.to_path_buf();
    }
    find_gradle_wrapper_root(project_root)
}

/// Wrapper root for a source file in a multi-module Gradle build (sibling modules share one index).
pub fn find_gradle_repo_root(ws: &Path, rel_path: &str) -> Result<Option<PathBuf>> {
    let Some(module) = find_gradle_root(ws, rel_path)? else {
        return Ok(None);
    };
    Ok(Some(find_gradle_settings_repo_root(&module)))
}

fn gradle_wrapper_command(
    wrapper_root: &Path,
    project_root: &Path,
    program: &str,
) -> GradleCommand {
    let mut project_args = Vec::new();
    if wrapper_root != project_root {
        project_args.push("-p".into());
        project_args.push(project_root.to_string_lossy().into_owned());
    }
    GradleCommand {
        program: PathBuf::from(program),
        cwd: wrapper_root.to_path_buf(),
        project_args,
    }
}

/// JVM for running this project's Gradle wrapper (respects wrapper vs Java compatibility).
pub fn gradle_java_home_for_project(project_root: &Path) -> Result<PathBuf> {
    let wrapper_root = find_gradle_wrapper_root(project_root);
    let max_major = max_java_version_for_project(&wrapper_root);
    crate::jdk::gradle_java_home_with_max(max_major)
}

/// Highest Java major version Gradle can run with for this project's wrapper.
pub fn max_java_version_for_project(project_root: &Path) -> u32 {
    wrapper_gradle_version(project_root)
        .map(|(major, minor)| max_java_for_gradle(major, minor))
        .unwrap_or(19)
}

fn wrapper_gradle_version(project_root: &Path) -> Option<(u32, u32)> {
    let path = project_root.join("gradle/wrapper/gradle-wrapper.properties");
    let text = std::fs::read_to_string(path).ok()?;
    for line in text.lines() {
        let url = line
            .strip_prefix("distributionUrl=")
            .map(str::trim)
            .unwrap_or(line.trim());
        let after = url.split("gradle-").nth(1)?;
        let mut parts = after.split(|c: char| !c.is_ascii_digit());
        let major = parts.next()?.parse().ok()?;
        let minor = parts.next().unwrap_or("0").parse().ok()?;
        return Some((major, minor));
    }
    None
}

fn max_java_for_gradle(major: u32, minor: u32) -> u32 {
    match major {
        0..=6 => 11,
        7 => 19,
        8 if minor >= 14 => 24,
        8 if minor >= 10 => 23,
        8 if minor >= 5 => 21,
        8 => 20,
        _ => 25,
    }
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
    let rel_path = super::normalize_workspace_source_path(rel_path);
    let file_path = safe_join(ws, &rel_path)?;
    let ws_canon = ws
        .canonicalize()
        .with_context(|| format!("resolve workspace {}", ws.display()))?;

    let mut dir = if file_path.is_file() {
        file_path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| file_path.clone())
    } else {
        file_path
            .parent()
            .filter(|p| p.exists())
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| ws.to_path_buf())
    };

    loop {
        let dir_canon = match dir.canonicalize() {
            Ok(c) => c,
            Err(_) => break,
        };
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

pub fn is_gradle_project_dir(dir: &Path) -> bool {
    dir.join("settings.gradle").is_file()
        || dir.join("settings.gradle.kts").is_file()
        || dir.join("build.gradle").is_file()
        || dir.join("build.gradle.kts").is_file()
}

const GRADLE_ROOTS_CACHE_TTL: Duration = Duration::from_secs(60);

static GRADLE_ROOTS_CACHE: LazyLock<Mutex<HashMap<PathBuf, (Instant, Vec<PathBuf>)>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Discover Gradle project roots under a workspace (supports nested layouts like `repo-1/`).
pub fn find_all_gradle_roots(ws: &Path) -> Result<Vec<PathBuf>> {
    let key = ws.canonicalize().unwrap_or_else(|_| ws.to_path_buf());
    if let Ok(guard) = GRADLE_ROOTS_CACHE.lock() {
        if let Some((at, roots)) = guard.get(&key) {
            if at.elapsed() < GRADLE_ROOTS_CACHE_TTL {
                return Ok(roots.clone());
            }
        }
    }

    let roots = compute_all_gradle_roots(ws)?;
    if let Ok(mut guard) = GRADLE_ROOTS_CACHE.lock() {
        guard.insert(key, (Instant::now(), roots.clone()));
    }
    Ok(roots)
}

pub fn invalidate_gradle_roots_cache(ws: &Path) {
    let key = ws.canonicalize().unwrap_or_else(|_| ws.to_path_buf());
    if let Ok(mut guard) = GRADLE_ROOTS_CACHE.lock() {
        guard.remove(&key);
    }
}

fn compute_all_gradle_roots(ws: &Path) -> Result<Vec<PathBuf>> {
    let ws_canon = ws
        .canonicalize()
        .with_context(|| format!("resolve workspace {}", ws.display()))?;
    let mut found = Vec::new();
    collect_gradle_roots(&ws_canon, ws, 0, 8, &mut found)?;
    found.sort_by(|a, b| a.display().to_string().cmp(&b.display().to_string()));
    found.dedup();
    Ok(found)
}

fn should_skip_gradle_scan_dir(name: &std::ffi::OsStr) -> bool {
    matches!(
        name.to_str(),
        Some(
            ".git" | ".reaper" | "node_modules" | "build" | "target" | ".gradle" | "out"
                | "dist" | "bin" | ".idea" | ".vscode" | "coverage" | "tmp"
        )
    )
}

fn collect_gradle_roots(
    ws_canon: &Path,
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
    if !dir_canon.starts_with(ws_canon) {
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
        if should_skip_gradle_scan_dir(&entry.file_name()) {
            continue;
        }
        collect_gradle_roots(ws_canon, &path, depth + 1, max_depth, out)?;
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
        .map(|content| build_file_is_spring_boot(&content))
        .unwrap_or(false)
}

/// True when the Gradle module containing `rel_path` (or an ancestor module) applies Spring Boot.
pub fn is_spring_boot_project_for_file(ws: &Path, rel_path: &str) -> Result<bool> {
    let Some(root) = find_gradle_root(ws, rel_path)? else {
        return Ok(false);
    };
    if is_spring_boot_project(&root) {
        return Ok(true);
    }
    if let Some(module) = find_gradle_module_for_source_file(ws, rel_path, &root)? {
        let module_dir = root.join(&module);
        if is_spring_boot_project(&module_dir) {
            return Ok(true);
        }
    }
    Ok(false)
}

pub fn build_file_is_spring_boot(content: &str) -> bool {
    let compact: String = content.chars().filter(|c| !c.is_whitespace()).collect();
    compact.contains("org.springframework.boot")
        || (compact.contains("spring-boot") && compact.contains("bootRun"))
        || compact.contains("id(\"org.springframework.boot\")")
        || compact.contains("id('org.springframework.boot')")
        || compact.contains("alias(libs.plugins.spring.boot)")
}

/// Gradle task to run Spring Boot for the module containing `rel_path`, optionally pinning main class.
pub fn gradle_boot_run_task(
    ws: &Path,
    rel_path: &str,
    main_class: Option<&str>,
) -> Result<String> {
    let root = find_gradle_root(ws, rel_path)?
        .ok_or_else(|| anyhow::anyhow!("not inside a Gradle project"))?;
    let repo_root = find_gradle_wrapper_root(&root);
    let module = find_gradle_module_for_source_file(ws, rel_path, &repo_root)?;
    let cmd = resolve_gradle_command(&root)?;
    let base = if cmd.project_args.iter().any(|a| a == "-p") {
        "bootRun".to_string()
    } else if let Some(m) = module.filter(|m| !m.is_empty()) {
        format!(":{}:bootRun", m.replace('/', ":"))
    } else {
        "bootRun".to_string()
    };
    if let Some(mc) = main_class.filter(|s| !s.is_empty()) {
        Ok(format!("{base} -Dspring-boot.run.main-class={mc}"))
    } else {
        Ok(base)
    }
}

/// Nearest Gradle subproject directory between `rel_path` and `gradle_root` (e.g. `app` for `app/src/main/java/Foo.java`).
pub fn find_gradle_module_for_source_file(
    ws: &Path,
    rel_path: &str,
    gradle_root: &Path,
) -> Result<Option<String>> {
    let file_path = safe_join(ws, rel_path)?;
    let Some(mut dir) = file_path.parent().map(|p| p.to_path_buf()) else {
        return Ok(None);
    };
    let root = gradle_root
        .canonicalize()
        .with_context(|| format!("resolve gradle root {}", gradle_root.display()))?;

    loop {
        let dir_canon = match dir.canonicalize() {
            Ok(c) => c,
            Err(_) => break,
        };
        if !dir_canon.starts_with(&root) {
            break;
        }
        if dir_canon != root
            && (dir.join("build.gradle").is_file() || dir.join("build.gradle.kts").is_file())
        {
            let rel = dir_canon
                .strip_prefix(&root)
                .unwrap_or(&dir_canon)
                .to_string_lossy()
                .replace('\\', "/");
            let rel = rel.trim_start_matches('/').to_string();
            return Ok(if rel.is_empty() { None } else { Some(rel) });
        }
        if dir_canon == root {
            return Ok(None);
        }
        dir = match dir.parent() {
            Some(p) => p.to_path_buf(),
            None => break,
        };
    }
    Ok(None)
}

pub fn read_build_file_content(root: &Path) -> Option<String> {
    read_build_file(root)
}

pub(crate) fn read_build_file(root: &Path) -> Option<String> {
    for name in ["build.gradle", "build.gradle.kts"] {
        let path = root.join(name);
        if path.is_file() {
            return std::fs::read_to_string(path).ok();
        }
    }
    None
}

/// Gradle `project(':…')` paths declared in a module build file (e.g. `:libs:common`).
pub(crate) fn parse_gradle_project_dependency_paths(content: &str) -> Vec<String> {
    let mut out = Vec::new();
    let needle = "project(";
    let mut search_from = 0usize;
    while let Some(idx) = content[search_from..].find(needle) {
        let start = search_from + idx + needle.len();
        let rest = content[start..].trim_start();
        let Some(quote) = rest.chars().next() else {
            search_from = start;
            continue;
        };
        if quote != '\'' && quote != '"' {
            search_from = start;
            continue;
        }
        let path_start = quote.len_utf8();
        let Some(end_rel) = rest[path_start..].find(quote) else {
            break;
        };
        let path = rest[path_start..path_start + end_rel].trim().to_string();
        if !path.is_empty() && !out.iter().any(|p| p == &path) {
            out.push(path);
        }
        search_from = start + path_start + end_rel + quote.len_utf8();
    }
    out
}

fn gradle_project_path_to_relative_dir(project_path: &str) -> String {
    project_path
        .trim()
        .trim_start_matches(':')
        .replace(':', "/")
}

/// Transitive Gradle `project()` dependency module directories under the wrapper root.
pub fn gradle_project_dependency_dirs(project_root: &Path) -> Vec<PathBuf> {
    let wrapper_root = find_gradle_wrapper_root(project_root);
    let module_root = project_root
        .canonicalize()
        .unwrap_or_else(|_| project_root.to_path_buf());
    let mut visited = std::collections::HashSet::new();
    let mut out = Vec::new();
    collect_gradle_project_dependency_dirs_inner(
        &module_root,
        &wrapper_root,
        &mut visited,
        &mut out,
    );
    out
}

fn collect_gradle_project_dependency_dirs_inner(
    module_root: &Path,
    wrapper_root: &Path,
    visited: &mut std::collections::HashSet<PathBuf>,
    out: &mut Vec<PathBuf>,
) {
    let Some(content) = read_build_file(module_root) else {
        return;
    };
    for project_path in parse_gradle_project_dependency_paths(&content) {
        let rel = gradle_project_path_to_relative_dir(&project_path);
        let sibling = if rel.is_empty() {
            wrapper_root.to_path_buf()
        } else {
            wrapper_root.join(rel)
        };
        let sibling = sibling.canonicalize().unwrap_or(sibling);
        if sibling == module_root {
            continue;
        }
        if !is_gradle_project_dir(&sibling) {
            continue;
        }
        if visited.insert(sibling.clone()) {
            out.push(sibling.clone());
            collect_gradle_project_dependency_dirs_inner(&sibling, wrapper_root, visited, out);
        }
    }
}

pub(crate) fn has_application_plugin(content: &str) -> bool {
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
    fn resolve_gradlew_from_nested_module_without_wrapper() {
        let root = std::env::temp_dir().join(format!("reaper-gradle-wrap-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("gradle/wrapper")).unwrap();
        std::fs::write(root.join("settings.gradle"), "rootProject.name = 'root'\n").unwrap();
        std::fs::write(root.join("build.gradle"), "plugins { id 'java' }\n").unwrap();
        std::fs::write(
            root.join("gradle/wrapper/gradle-wrapper.properties"),
            "distributionUrl=https\\://services.gradle.org/distributions/gradle-8.5-bin.zip\n",
        )
        .unwrap();
        std::fs::write(root.join("gradlew"), "#!/bin/sh\nexit 0\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(root.join("gradlew")).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(root.join("gradlew"), perms).unwrap();
        }
        let module = root.join("api");
        std::fs::create_dir_all(&module).unwrap();
        std::fs::write(module.join("build.gradle"), "plugins { id 'java' }\n").unwrap();

        let cmd = resolve_gradle_command(&module).expect("resolve gradle command");
        assert_eq!(cmd.program, PathBuf::from("./gradlew"));
        assert!(cmd.cwd.join("gradlew").is_file());
        assert_eq!(cmd.project_args.len(), 2);
        assert_eq!(cmd.project_args[0], "-p");
        assert!(PathBuf::from(&cmd.project_args[1]).ends_with("api"));
        let _ = std::fs::remove_dir_all(&root);
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

    #[test]
    fn find_gradle_root_with_missing_file_uses_parent() {
        let ws = std::env::temp_dir().join("reaper-gradle-missing-file");
        let _ = std::fs::remove_dir_all(&ws);
        std::fs::create_dir_all(&ws).unwrap();
        let root = find_gradle_root(&ws, "NotYetSaved.java")
            .expect("find gradle root should not error for unsaved paths");
        assert!(root.is_none());
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn max_java_for_gradle_8_14() {
        assert_eq!(max_java_for_gradle(8, 14), 24);
        assert_eq!(max_java_for_gradle(8, 5), 21);
    }

    #[test]
    fn parse_gradle_project_dependency_paths_finds_colon_paths() {
        let text = r#"
dependencies {
    implementation project(':libs:common')
    api project(":libs:core:core-web")
}
"#;
        let paths = parse_gradle_project_dependency_paths(text);
        assert_eq!(paths, vec![":libs:common", ":libs:core:core-web"]);
    }

    #[test]
    fn gradle_project_dependency_dirs_includes_transitive_siblings() {
        let root = std::env::temp_dir().join(format!(
            "reaper-gradle-project-deps-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("libs/common/src/main/java")).unwrap();
        std::fs::create_dir_all(root.join("libs/core/core-web/src/main/java")).unwrap();
        std::fs::create_dir_all(root.join("services/gateway/src/main/java")).unwrap();
        std::fs::write(root.join("settings.gradle"), "rootProject.name = 'demo'\n").unwrap();
        std::fs::write(root.join("gradlew"), "#!/bin/sh\n").unwrap();
        std::fs::write(root.join("build.gradle"), "subprojects { apply plugin: 'java' }\n").unwrap();
        std::fs::write(root.join("libs/common/build.gradle"), "plugins { id 'java-library' }\n").unwrap();
        std::fs::write(
            root.join("libs/core/core-web/build.gradle"),
            "plugins { id 'java-library' }\n",
        )
        .unwrap();
        std::fs::write(
            root.join("services/gateway/build.gradle"),
            r#"plugins { id 'java' }
dependencies {
    implementation project(':libs:common')
    implementation project(':libs:core:core-web')
}
"#,
        )
        .unwrap();
        std::fs::write(
            root.join("libs/common/build.gradle"),
            r#"plugins { id 'java-library' }
dependencies {
    implementation project(':libs:core:core-web')
}
"#,
        )
        .unwrap();

        let gateway = root.join("services/gateway");
        let root = root.canonicalize().unwrap_or(root);
        let siblings = gradle_project_dependency_dirs(&gateway);
        let names: Vec<String> = siblings
            .iter()
            .map(|p| {
                p.strip_prefix(&root)
                    .unwrap_or(p)
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect();
        assert!(
            names.iter().any(|n| n == "libs/common"),
            "expected libs/common in {names:?}"
        );
        assert!(
            names.iter().any(|n| n == "libs/core/core-web"),
            "expected libs/core/core-web in {names:?}"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn spring_boot_module_and_boot_run_task() {
        let root = std::env::temp_dir().join(format!("reaper-spring-boot-run-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("gateway/src/main/java/com/example")).unwrap();
        std::fs::write(root.join("settings.gradle"), "rootProject.name = 'demo'\n").unwrap();
        std::fs::write(root.join("build.gradle"), "plugins { id 'java' }\n").unwrap();
        std::fs::write(
            root.join("gateway/build.gradle.kts"),
            "plugins { id(\"org.springframework.boot\") }\n",
        )
        .unwrap();
        std::fs::write(
            root.join("gateway/src/main/java/com/example/GatewayApplication.java"),
            "package com.example;\npublic class GatewayApplication {}\n",
        )
        .unwrap();

        let rel = "gateway/src/main/java/com/example/GatewayApplication.java";
        assert!(is_spring_boot_project_for_file(&root, rel).unwrap());
        let module = find_gradle_module_for_source_file(&root, rel, &root)
            .expect("module lookup")
            .expect("gateway module");
        assert_eq!(module, "gateway");
        let task = gradle_boot_run_task(&root, rel, Some("com.example.GatewayApplication"))
            .expect("bootRun task");
        assert_eq!(
            task,
            "bootRun -Dspring-boot.run.main-class=com.example.GatewayApplication"
        );

        let parts = parse_gradle_task(&task).expect("parse bootRun task");
        assert!(parts
            .iter()
            .any(|p| p.starts_with("-Dspring-boot.run.main-class=com.example")));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn spring_boot_detection_normalizes_overlay_path() {
        let root = std::env::temp_dir().join(format!("reaper-spring-overlay-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("gateway/src/main/java/com/example")).unwrap();
        std::fs::write(root.join("settings.gradle"), "rootProject.name = 'demo'\n").unwrap();
        std::fs::write(root.join("build.gradle"), "plugins { id 'java' }\n").unwrap();
        std::fs::write(
            root.join("gateway/build.gradle.kts"),
            "plugins { id(\"org.springframework.boot\") }\n",
        )
        .unwrap();
        std::fs::write(
            root.join("gateway/src/main/java/com/example/GatewayApplication.java"),
            "@SpringBootApplication\npackage com.example;\npublic class GatewayApplication {}\n",
        )
        .unwrap();

        let rel = ".reaper/java-diagnostics/overlay/gateway/src/main/java/com/example/GatewayApplication.java";
        assert!(is_spring_boot_project_for_file(&root, rel).unwrap());
        let task = gradle_boot_run_task(&root, rel, Some("com.example.GatewayApplication"))
            .expect("bootRun task");
        assert!(task.contains("bootRun"));
        assert!(task.contains("com.example.GatewayApplication"));

        let ctx = super::super::run_project::run_context(
            &root,
            rel,
            Some("@SpringBootApplication\npackage com.example;\npublic class GatewayApplication { public static void main(String[] args) {} }\n"),
            1,
            None,
            None,
            None,
        )
        .expect("run context");
        assert!(ctx.project.has_project);
        let target = ctx.target.expect("run target");
        assert_eq!(target.mode, "spring-boot");
        assert!(target.runnable);
        assert!(target.task.as_deref().unwrap_or("").contains("bootRun"));

        let _ = std::fs::remove_dir_all(&root);
    }
}
