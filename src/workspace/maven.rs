use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use serde::Serialize;

use crate::jdk;

use super::gradle;
use super::safe_join;

#[derive(Debug, Clone, Serialize)]
pub struct MavenProjectInfo {
    pub is_maven: bool,
    pub project_root: String,
    pub has_wrapper: bool,
    pub is_spring_boot: bool,
    pub default_goal: String,
    pub goals: Vec<String>,
    pub application_main: Option<String>,
    pub has_junit: bool,
    pub has_spring_test: bool,
    pub has_jacoco: bool,
    pub has_lombok: bool,
    pub has_slf4j: bool,
}

pub fn is_maven_project_root(dir: &Path) -> bool {
    dir.join("pom.xml").is_file() && !gradle::is_gradle_project_dir(dir)
}

#[derive(Debug, Clone)]
pub struct MavenCommand {
    pub program: PathBuf,
    pub cwd: PathBuf,
    /// Extra args when invoking from a wrapper/reactor root for a nested module (`-pl` / `-am`).
    pub project_args: Vec<String>,
}

/// Directory containing `mvnw` — walk up from a nested module if needed.
pub fn find_maven_wrapper_root(project_root: &Path) -> PathBuf {
    let mut dir = project_root.to_path_buf();
    loop {
        if dir.join("mvnw").is_file() || dir.join("mvnw.cmd").is_file() {
            return dir;
        }
        let Some(parent) = dir.parent() else {
            break;
        };
        dir = parent.to_path_buf();
    }
    project_root.to_path_buf()
}

fn system_maven_program() -> PathBuf {
    crate::toolchain::resolve_program("maven").unwrap_or_else(|| PathBuf::from("mvn"))
}

fn maven_pl_args(wrapper_or_reactor: &Path, project_root: &Path) -> Vec<String> {
    let project_canon = project_root
        .canonicalize()
        .unwrap_or_else(|_| project_root.to_path_buf());
    let base_canon = wrapper_or_reactor
        .canonicalize()
        .unwrap_or_else(|_| wrapper_or_reactor.to_path_buf());
    if project_canon == base_canon {
        return Vec::new();
    }
    if let Some(pl) = module_pl_selector(&project_canon, &base_canon) {
        return vec!["-pl".into(), pl, "-am".into()];
    }
    if let Some(ctx) = maven_reactor_context(&project_canon) {
        if !ctx.module_pl.is_empty()
            && ctx.reactor_root.canonicalize().ok().as_ref() == Some(&base_canon)
        {
            return vec!["-pl".into(), ctx.module_pl, "-am".into()];
        }
    }
    Vec::new()
}

fn maven_wrapper_command(wrapper_root: &Path, project_root: &Path) -> MavenCommand {
    let _ = ensure_mvnw_executable(wrapper_root);
    let program = if cfg!(windows) && wrapper_root.join("mvnw.cmd").is_file() {
        PathBuf::from("./mvnw.cmd")
    } else {
        PathBuf::from("./mvnw")
    };
    MavenCommand {
        program,
        cwd: wrapper_root.to_path_buf(),
        project_args: maven_pl_args(wrapper_root, project_root),
    }
}

/// Prefer `./mvnw` (walking up to the reactor if needed), then Settings → Compiler, then `mvn` on PATH.
/// Nested modules get `-pl <module> -am` when the command runs from a parent wrapper/reactor.
pub fn resolve_maven_command(project_root: &Path) -> MavenCommand {
    let wrapper_root = find_maven_wrapper_root(project_root);
    if wrapper_root.join("mvnw").is_file() || wrapper_root.join("mvnw.cmd").is_file() {
        return maven_wrapper_command(&wrapper_root, project_root);
    }

    // No wrapper: still run multi-module builds from the reactor with `-pl` so dependencies compile.
    if let Some(ctx) = maven_reactor_context(project_root) {
        if !ctx.module_pl.is_empty() {
            return MavenCommand {
                program: system_maven_program(),
                cwd: ctx.reactor_root,
                project_args: vec!["-pl".into(), ctx.module_pl, "-am".into()],
            };
        }
    }

    MavenCommand {
        program: system_maven_program(),
        cwd: project_root.to_path_buf(),
        project_args: Vec::new(),
    }
}

/// Ensure `mvnw` is executable (git checkouts sometimes drop the bit).
pub fn ensure_mvnw_executable(root: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mvnw = root.join("mvnw");
        if mvnw.is_file() {
            let meta = std::fs::metadata(&mvnw)?;
            let mut perms = meta.permissions();
            let mode = perms.mode();
            if mode & 0o111 == 0 {
                perms.set_mode(mode | 0o755);
                std::fs::set_permissions(&mvnw, perms)?;
            }
        }
    }
    let _ = root;
    Ok(())
}

pub fn run_maven(project_root: &Path, args: &[&str]) -> Result<std::process::Output> {
    if crate::process_registry::is_shutdown_requested() {
        bail!("Reaper is shutting down");
    }
    let cmd = resolve_maven_command(project_root);
    let mut process = crate::platform::command_path(&cmd.program);
    process
        .current_dir(&cmd.cwd)
        .args(&cmd.project_args)
        .args(args);
    crate::process_registry::configure_command(&mut process);
    jdk::apply_java_env(&mut process);
    let label = format!("mvn {}", args.join(" "));
    let mut child = process
        .spawn()
        .with_context(|| format!("spawn {}", cmd.program.display()))?;
    let _guard = crate::process_registry::guard_for_child(&mut child, &label);
    child
        .wait_with_output()
        .with_context(|| format!("wait for {}", cmd.program.display()))
}

const MAVEN_ROOTS_CACHE_TTL: Duration = Duration::from_secs(60);

static MAVEN_ROOTS_CACHE: LazyLock<Mutex<HashMap<PathBuf, (Instant, Vec<PathBuf>)>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Discover Maven module roots (skips Gradle projects and reactor parents without sources).
pub fn find_all_maven_roots(ws: &Path) -> Result<Vec<PathBuf>> {
    let key = ws.canonicalize().unwrap_or_else(|_| ws.to_path_buf());
    if let Ok(guard) = MAVEN_ROOTS_CACHE.lock() {
        if let Some((at, roots)) = guard.get(&key) {
            if at.elapsed() < MAVEN_ROOTS_CACHE_TTL {
                return Ok(roots.clone());
            }
        }
    }

    let roots = compute_all_maven_roots(ws)?;
    if let Ok(mut guard) = MAVEN_ROOTS_CACHE.lock() {
        guard.insert(key, (Instant::now(), roots.clone()));
    }
    Ok(roots)
}

pub fn invalidate_maven_roots_cache(ws: &Path) {
    let key = ws.canonicalize().unwrap_or_else(|_| ws.to_path_buf());
    if let Ok(mut guard) = MAVEN_ROOTS_CACHE.lock() {
        guard.remove(&key);
    }
}

fn compute_all_maven_roots(ws: &Path) -> Result<Vec<PathBuf>> {
    let ws_canon = ws
        .canonicalize()
        .with_context(|| format!("resolve workspace {}", ws.display()))?;
    let mut found = Vec::new();
    collect_maven_roots(&ws_canon, ws, 0, 8, &mut found)?;
    found.sort_by(|a, b| a.display().to_string().cmp(&b.display().to_string()));
    found.dedup();
    Ok(found)
}

fn should_skip_maven_scan_dir(name: &std::ffi::OsStr) -> bool {
    matches!(
        name.to_str(),
        Some(
            ".git" | ".reaper" | "node_modules" | "build" | "target" | ".gradle" | "out"
                | "dist" | "bin" | ".idea" | ".vscode" | "coverage" | "tmp"
        )
    )
}

fn collect_maven_roots(
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

    if gradle::is_gradle_project_dir(dir) {
        return Ok(());
    }

    if dir.join("pom.xml").is_file() {
        let pom = read_pom(dir)?;
        if pom.is_reactor() {
            for module in &pom.modules {
                let module_dir = dir.join(module);
                if module_dir.is_dir() {
                    collect_maven_roots(ws_canon, &module_dir, depth + 1, max_depth, out)?;
                }
            }
            return Ok(());
        }
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
        if should_skip_maven_scan_dir(&entry.file_name()) {
            continue;
        }
        collect_maven_roots(ws_canon, &path, depth + 1, max_depth, out)?;
    }
    Ok(())
}

/// Walk up from a file path to the containing Maven module (pom.xml, not Gradle).
pub fn find_maven_root(ws: &Path, rel_path: &str) -> Result<Option<PathBuf>> {
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

        if is_maven_project_root(&dir) {
            return Ok(Some(dir));
        }

        let Some(parent) = dir.parent() else {
            break;
        };
        dir = parent.to_path_buf();
    }

    if rel_path == "." || rel_path.is_empty() {
        return find_all_maven_roots(ws).map(|roots| roots.into_iter().next());
    }

    Ok(None)
}

pub fn maven_project_info(ws: &Path, rel_path: &str) -> Result<MavenProjectInfo> {
    let empty = MavenProjectInfo {
        is_maven: false,
        project_root: String::new(),
        has_wrapper: false,
        is_spring_boot: false,
        default_goal: String::new(),
        goals: Vec::new(),
        application_main: None,
        has_junit: false,
        has_spring_test: false,
        has_jacoco: false,
        has_lombok: false,
        has_slf4j: false,
    };

    let _ = safe_join(ws, rel_path)?;
    let Some(root) = find_maven_root(ws, rel_path)? else {
        return Ok(empty);
    };

    let project_root = gradle::rel_path_for(ws, &root)?;
    let is_spring_boot = is_spring_boot_project(&root);
    let wrapper_root = find_maven_wrapper_root(&root);
    let has_wrapper =
        wrapper_root.join("mvnw").is_file() || wrapper_root.join("mvnw.cmd").is_file();
    let default_goal = if is_spring_boot {
        "spring-boot:run".to_string()
    } else {
        "package".to_string()
    };

    let mut goals = vec![
        "compile".to_string(),
        "test".to_string(),
        "package".to_string(),
        "clean".to_string(),
    ];
    if is_spring_boot {
        goals.insert(0, "spring-boot:run".to_string());
    }

    let pom_raw = std::fs::read_to_string(root.join("pom.xml")).unwrap_or_default();
    let markers = super::java_ecosystem::scan_maven_pom(&pom_raw);

    Ok(MavenProjectInfo {
        is_maven: true,
        project_root,
        has_wrapper,
        is_spring_boot,
        default_goal,
        goals,
        application_main: None,
        has_junit: markers.junit,
        has_spring_test: markers.spring_test,
        has_jacoco: markers.jacoco,
        has_lombok: markers.lombok,
        has_slf4j: markers.slf4j,
    })
}

/// Split a Maven goal string (`spring-boot:run`, `test`, `clean package`) into argv tokens.
pub fn parse_maven_goal(goal: &str) -> Result<Vec<String>> {
    let goal = goal.trim();
    if goal.is_empty() {
        bail!("maven goal required");
    }
    Ok(goal
        .split_whitespace()
        .map(|token| normalize_maven_goal_token(token))
        .collect())
}

/// Convert Gradle-style `-Dtest=fqcn.method` to Surefire `-Dtest=fqcn#method`.
fn normalize_maven_goal_token(token: &str) -> String {
    let Some(value) = token.strip_prefix("-Dtest=") else {
        return token.to_string();
    };
    format!("-Dtest={}", surefire_test_pattern(value))
}

/// Gradle `--tests` uses `fqcn.method`; Maven Surefire uses `fqcn#method`.
pub fn surefire_test_pattern(filter: &str) -> String {
    let filter = filter.trim();
    if let Some(last_dot) = filter.rfind('.') {
        let method = &filter[last_dot + 1..];
        if !method.is_empty()
            && method
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_lowercase())
        {
            return format!("{}#{}", &filter[..last_dot], method);
        }
    }
    filter.to_string()
}

pub fn is_spring_boot_project(root: &Path) -> bool {
    read_pom(root)
        .map(|pom| pom.looks_like_spring_boot())
        .unwrap_or(false)
}

pub fn classpath_stamp_parts(root: &Path) -> Result<Vec<String>> {
    let mut parts = Vec::new();
    stamp_pom_chain(root, &mut parts)?;
    Ok(parts)
}

fn stamp_pom_chain(root: &Path, parts: &mut Vec<String>) -> Result<()> {
    let pom_path = root.join("pom.xml");
    if pom_path.is_file() {
        let meta = std::fs::metadata(&pom_path)?;
        parts.push(format!(
            "pom:{}:{}",
            meta.len(),
            meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
    }
    if let Ok(pom) = read_pom(root) {
        if let Some((group, artifact, version)) = pom.parent_coords() {
            if let Some(parent_dir) = resolve_pom_directory(&group, &artifact, &version) {
                if parent_dir != root {
                    stamp_pom_chain(&parent_dir, parts)?;
                }
            }
        }
    }
    Ok(())
}

pub fn collect_dependency_coordinates(maven_root: &Path) -> Vec<(String, String, String)> {
    let mut coords = Vec::new();
    let mut seen = HashSet::new();
    collect_coordinates_from_pom(maven_root, &mut coords, &mut seen, 0);
    coords
}

/// Resolve Maven dependency JARs from the local `~/.m2` cache, walking transitive deps.
pub fn collect_transitive_jar_paths(maven_root: &Path, include_test_scope: bool) -> Vec<PathBuf> {
    let roots = collect_dependency_coordinates(maven_root);
    collect_transitive_jars(&roots, include_test_scope, find_m2_jar, read_m2_pom_text)
}

/// Walk transitive Maven POM dependencies, resolving JARs and POMs via callbacks.
pub fn collect_transitive_jars<FJ, FP>(
    roots: &[(String, String, String)],
    include_test_scope: bool,
    find_jar: FJ,
    read_pom: FP,
) -> Vec<PathBuf>
where
    FJ: Fn(&str, &str, &str) -> Option<PathBuf>,
    FP: Fn(&str, &str, &str) -> Option<String>,
{
    use std::collections::VecDeque;

    const MAX_DEPTH: usize = 10;

    let mut queue: VecDeque<(String, String, String, usize)> = roots
        .iter()
        .map(|(g, a, v)| (g.clone(), a.clone(), v.clone(), 0))
        .collect();
    let mut seen = HashSet::new();
    let mut jars = Vec::new();

    while let Some((group, artifact, version, depth)) = queue.pop_front() {
        if depth > MAX_DEPTH {
            continue;
        }
        let key = format!("{group}:{artifact}:{version}");
        if !seen.insert(key) {
            continue;
        }

        if let Some(jar) = find_jar(&group, &artifact, &version) {
            jars.push(jar);
        }

        let Some(raw) = read_pom(&group, &artifact, &version) else {
            continue;
        };
        enqueue_transitive_deps(&raw, &mut queue, depth + 1, include_test_scope);
    }

    jars
}

pub fn read_m2_pom_text(group: &str, artifact: &str, version: &str) -> Option<String> {
    let pom_path = m2_home()
        .join(group.replace('.', "/"))
        .join(artifact)
        .join(version)
        .join(format!("{artifact}-{version}.pom"));
    std::fs::read_to_string(pom_path).ok()
}

fn read_m2_pom_model(group: &str, artifact: &str, version: &str) -> Option<PomModel> {
    read_m2_pom_text(group, artifact, version).map(|raw| parse_pom(&raw))
}

fn enqueue_transitive_deps(
    pom_raw: &str,
    queue: &mut std::collections::VecDeque<(String, String, String, usize)>,
    depth: usize,
    include_test_scope: bool,
) {
    let pom = parse_pom(pom_raw);
    let management = dependency_management_for_resolved_pom(&pom);

    for dep in &pom.dependencies {
        if dep.optional {
            continue;
        }
        let scope = dep.scope.as_deref().unwrap_or("compile");
        if scope == "test" && !include_test_scope {
            continue;
        }
        if matches!(scope, "provided" | "system") {
            continue;
        }

        let group = resolve_property(&dep.group_id, &pom.properties);
        let artifact = resolve_property(&dep.artifact_id, &pom.properties);
        let version = dep
            .version
            .as_deref()
            .and_then(|v| resolve_version(Some(v), &pom))
            .or_else(|| {
                management
                    .get(&format!("{group}:{artifact}"))
                    .map(|(v, _)| v.clone())
            });
        if let Some(version) = version {
            queue.push_back((group, artifact, version, depth));
        }
    }
}

fn collect_coordinates_from_pom(
    root: &Path,
    out: &mut Vec<(String, String, String)>,
    seen: &mut HashSet<String>,
    depth: usize,
) {
    if depth > 6 {
        return;
    }
    let pom = match read_pom(root) {
        Ok(p) => p,
        Err(_) => return,
    };

    let management = effective_dependency_management(root, &pom, 0);

    for dep in &pom.dependencies {
        let group = resolve_property(&dep.group_id, &pom.properties);
        let artifact = resolve_property(&dep.artifact_id, &pom.properties);
        let version = dep
            .version
            .as_deref()
            .and_then(|v| resolve_version(Some(v), &pom))
            .or_else(|| {
                management
                    .get(&format!("{group}:{artifact}"))
                    .map(|(v, _)| v.clone())
            });
        if let Some(version) = version {
            let key = format!("{group}:{artifact}:{version}");
            if seen.insert(key) {
                out.push((group, artifact, version));
            }
        }
    }
}

/// Effective versions from parent chain + imported BOMs (Spring Boot starter-parent, etc.).
fn effective_dependency_management(
    root: &Path,
    pom: &PomModel,
    depth: usize,
) -> HashMap<String, (String, String)> {
    if depth > 8 {
        return HashMap::new();
    }

    let mut management = HashMap::new();
    // Prefer workspace `relativePath` parents, then ~/.m2 coordinates.
    if let Some(parent_dir) = parent_pom_dir(root, pom) {
        if let Ok(parent_pom) = read_pom(&parent_dir) {
            management = effective_dependency_management(&parent_dir, &parent_pom, depth + 1);
        } else if let Some((group, artifact, version)) = pom.parent_coords() {
            // ~/.m2 layout uses `$artifact-$version.pom`, not `pom.xml`.
            if let Some(parent_pom) = read_m2_pom_model(&group, &artifact, &version) {
                management = effective_dependency_management(&parent_dir, &parent_pom, depth + 1);
            }
        }
    }
    merge_pom_dependency_management(pom, &mut management);
    management
}

/// Managed `group:artifact` → version entries from a Maven BOM POM (imports included when in ~/.m2).
pub fn bom_managed_versions(group: &str, artifact: &str, version: &str) -> HashMap<String, String> {
    read_m2_pom_text(group, artifact, version)
        .map(|raw| bom_managed_versions_from_pom(&raw))
        .unwrap_or_default()
}

pub fn bom_managed_versions_from_pom(raw: &str) -> HashMap<String, String> {
    let pom = parse_pom(raw);
    dependency_management_for_resolved_pom(&pom)
        .into_iter()
        .map(|(ga, (ver, _))| (ga, ver))
        .collect()
}

fn dependency_management_for_resolved_pom(pom: &PomModel) -> HashMap<String, (String, String)> {
    if let (Some(g), Some(a), Some(v)) = (&pom.group_id, &pom.artifact_id, &pom.version) {
        if let Some(dir) = resolve_pom_directory(g, a, v) {
            return effective_dependency_management(&dir, pom, 0);
        }
    }
    let mut management = HashMap::new();
    merge_pom_dependency_management(pom, &mut management);
    management
}

fn merge_pom_dependency_management(
    pom: &PomModel,
    management: &mut HashMap<String, (String, String)>,
) {
    merge_pom_dependency_management_depth(pom, management, 0);
}

/// Expand `dependencyManagement` entries, recursively following `scope=import` BOMs
/// (e.g. `spring-cloud-dependencies` → `spring-cloud-netflix-dependencies` → starters).
fn merge_pom_dependency_management_depth(
    pom: &PomModel,
    management: &mut HashMap<String, (String, String)>,
    depth: usize,
) {
    if depth > 12 {
        return;
    }
    for dep in &pom.dependency_management {
        if dep.optional {
            continue;
        }
        let group = resolve_property(&dep.group_id, &pom.properties);
        let artifact = resolve_property(&dep.artifact_id, &pom.properties);
        let scope = dep.scope.as_deref().unwrap_or("compile");
        let version = dep
            .version
            .as_deref()
            .and_then(|v| resolve_version(Some(v), pom));

        if scope == "import" {
            if let Some(version) = version {
                if let Some(bom_pom) = read_m2_pom_model(&group, &artifact, &version) {
                    merge_pom_dependency_management_depth(&bom_pom, management, depth + 1);
                }
            }
            continue;
        }

        if let Some(version) = version {
            management.insert(format!("{group}:{artifact}"), (version, artifact));
        }
    }
}

pub fn m2_home() -> PathBuf {
    std::env::var("MAVEN_HOME")
        .map(|h| PathBuf::from(h).join("repository"))
        .or_else(|_| std::env::var("M2_HOME").map(|h| PathBuf::from(h).join("repository")))
        .unwrap_or_else(|_| {
            std::env::var("HOME")
                .map(|h| PathBuf::from(h).join(".m2").join("repository"))
                .unwrap_or_else(|_| PathBuf::from(".m2/repository"))
        })
}

pub fn find_m2_jar(group: &str, artifact: &str, version: &str) -> Option<PathBuf> {
    let version_dir = m2_home()
        .join(group.replace('.', "/"))
        .join(artifact)
        .join(version);
    if !version_dir.is_dir() {
        return None;
    }
    let expected = version_dir.join(format!("{artifact}-{version}.jar"));
    if expected.is_file() {
        return Some(expected);
    }
    for entry in std::fs::read_dir(&version_dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("jar") {
            continue;
        }
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name.contains("-sources") || name.contains("-javadoc") {
            continue;
        }
        if name.starts_with(&format!("{artifact}-")) {
            return Some(path);
        }
    }
    None
}

pub fn find_m2_sources_jar(group: &str, artifact: &str, version: &str) -> Option<PathBuf> {
    let version_dir = m2_home()
        .join(group.replace('.', "/"))
        .join(artifact)
        .join(version);
    let expected = version_dir.join(format!("{artifact}-{version}-sources.jar"));
    if expected.is_file() {
        return Some(expected);
    }
    for entry in std::fs::read_dir(&version_dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.ends_with("-sources.jar"))
        {
            return Some(path);
        }
    }
    None
}

fn resolve_pom_directory(group: &str, artifact: &str, version: &str) -> Option<PathBuf> {
    let pom_path = m2_home()
        .join(group.replace('.', "/"))
        .join(artifact)
        .join(version)
        .join(format!("{artifact}-{version}.pom"));
    if pom_path.is_file() {
        return pom_path.parent().map(|p| p.to_path_buf());
    }
    None
}

#[derive(Debug, Clone)]
struct PomDependency {
    group_id: String,
    artifact_id: String,
    version: Option<String>,
    scope: Option<String>,
    optional: bool,
}

#[derive(Debug, Clone)]
struct PomModel {
    group_id: Option<String>,
    artifact_id: Option<String>,
    version: Option<String>,
    packaging: Option<String>,
    parent: Option<(String, String, String)>,
    properties: HashMap<String, String>,
    dependencies: Vec<PomDependency>,
    dependency_management: Vec<PomDependency>,
    modules: Vec<String>,
    raw: String,
}

impl PomModel {
    fn is_reactor(&self) -> bool {
        self.packaging.as_deref() == Some("pom") && !self.modules.is_empty()
    }

    fn parent_coords(&self) -> Option<(String, String, String)> {
        self.parent.clone()
    }

    fn looks_like_spring_boot(&self) -> bool {
        let compact = self.raw.replace(char::is_whitespace, "");
        compact.contains("org.springframework.boot")
            || self
                .parent
                .as_ref()
                .is_some_and(|(g, _, _)| g.starts_with("org.springframework.boot"))
    }
}

/// Lightweight POM view for build-task trees (module folder names match the project explorer).
#[derive(Debug, Clone)]
pub struct PomTreeInfo {
    pub modules: Vec<String>,
    pub raw: String,
    pub is_reactor: bool,
    pub is_spring_boot: bool,
}

pub fn pom_tree_info(root: &Path) -> Result<PomTreeInfo> {
    let pom = read_pom(root)?;
    Ok(PomTreeInfo {
        modules: pom.modules.clone(),
        raw: pom.raw.clone(),
        is_reactor: pom.is_reactor(),
        is_spring_boot: pom.looks_like_spring_boot(),
    })
}

fn read_pom(root: &Path) -> Result<PomModel> {
    let path = root.join("pom.xml");
    let raw = std::fs::read_to_string(&path)
        .with_context(|| format!("read {}", path.display()))?;
    Ok(parse_pom(&raw))
}

fn parse_pom(raw: &str) -> PomModel {
    let mut properties = parse_properties(raw);
    let parent = parse_parent(raw, &properties);
    let group_id =
        tag_value(raw, "groupId").or_else(|| parent.as_ref().map(|(g, _, _)| g.clone()));
    let artifact_id = tag_value(raw, "artifactId");
    let version = tag_value(raw, "version")
        .or_else(|| parent.as_ref().map(|(_, _, v)| v.clone()));
    // Maven interpolates ${project.version} / ${project.groupId} in BOMs (Spring Cloud).
    if let Some(ref g) = group_id {
        properties
            .entry("project.groupId".into())
            .or_insert_with(|| g.clone());
    }
    if let Some(ref a) = artifact_id {
        properties
            .entry("project.artifactId".into())
            .or_insert_with(|| a.clone());
    }
    if let Some(ref v) = version {
        properties
            .entry("project.version".into())
            .or_insert_with(|| v.clone());
    }
    if let Some((_, _, ref pv)) = parent {
        properties
            .entry("project.parent.version".into())
            .or_insert_with(|| pv.clone());
    }

    let dependency_management =
        parse_dependencies_in_section(raw, "dependencyManagement", &properties);
    // Top-level <dependencies>, not the nested list inside <dependencyManagement>.
    let dependencies = parse_dependencies_in_section(
        &strip_tag_block(raw, "dependencyManagement"),
        "dependencies",
        &properties,
    );
    let modules = parse_modules(raw);
    PomModel {
        group_id,
        artifact_id,
        version,
        packaging: tag_value(raw, "packaging"),
        parent,
        properties,
        dependencies,
        dependency_management,
        modules,
        raw: raw.to_string(),
    }
}

/// Remove the first `<tag>...</tag>` block so nested sections are not mistaken for top-level ones.
fn strip_tag_block(raw: &str, tag: &str) -> String {
    let Some(block) = extract_tag_block(raw, tag) else {
        return raw.to_string();
    };
    // extract_tag_block returns inner content; remove the full element including tags.
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let Some(start) = raw.find(&open) else {
        return raw.to_string();
    };
    let Some(rel_end) = raw[start..].find(&close) else {
        return raw.to_string();
    };
    let end = start + rel_end + close.len();
    let mut out = String::with_capacity(raw.len() - (end - start));
    out.push_str(&raw[..start]);
    out.push_str(&raw[end..]);
    let _ = block;
    out
}

fn parse_parent(raw: &str, properties: &HashMap<String, String>) -> Option<(String, String, String)> {
    let section = extract_tag_block(raw, "parent")?;
    let group = tag_value(&section, "groupId").map(|g| resolve_property(&g, properties))?;
    let artifact = tag_value(&section, "artifactId").map(|a| resolve_property(&a, properties))?;
    let version = tag_value(&section, "version")
        .map(|v| resolve_property(&v, properties))
        .or_else(|| properties.get("project.parent.version").cloned())?;
    Some((group, artifact, version))
}

fn parse_properties(raw: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    if let Some(section) = extract_tag_block(raw, "properties") {
        for (key, value) in parse_simple_tags(&section) {
            out.insert(key, resolve_property(&value, &out));
        }
    }
    out
}

fn parse_modules(raw: &str) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(section) = extract_tag_block(raw, "modules") {
        for (tag, value) in parse_simple_tags(&section) {
            if tag == "module" {
                out.push(value.trim().to_string());
            }
        }
    }
    out
}

fn parse_dependencies_in_section(
    raw: &str,
    section_tag: &str,
    properties: &HashMap<String, String>,
) -> Vec<PomDependency> {
    let section = extract_tag_block(raw, section_tag).unwrap_or_default();
    let mut out = Vec::new();
    for block in split_dependency_blocks(&section) {
        let group = tag_value(&block, "groupId").map(|g| resolve_property(&g, properties));
        let artifact = tag_value(&block, "artifactId").map(|a| resolve_property(&a, properties));
        let version = tag_value(&block, "version").map(|v| resolve_property(&v, properties));
        if let (Some(group_id), Some(artifact_id)) = (group, artifact) {
            let scope = tag_value(&block, "scope");
            let optional = tag_value(&block, "optional")
                .is_some_and(|v| v.eq_ignore_ascii_case("true"));
            out.push(PomDependency {
                group_id,
                artifact_id,
                version,
                scope,
                optional,
            });
        }
    }
    out
}

fn split_dependency_blocks(section: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let lower = section.to_ascii_lowercase();
    let mut search = 0;
    while let Some(start) = lower[search..].find("<dependency") {
        let abs_start = search + start;
        let after_open = lower[abs_start..].find('>').map(|i| abs_start + i + 1).unwrap_or(abs_start);
        if let Some(end_rel) = lower[after_open..].find("</dependency>") {
            let abs_end = after_open + end_rel;
            blocks.push(section[abs_start..abs_end].to_string());
            search = abs_end;
        } else {
            break;
        }
    }
    blocks
}

fn parse_simple_tags(section: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut search = section;
    while let Some(start) = search.find('<') {
        let rest = &search[start + 1..];
        if rest.starts_with('/') || rest.starts_with('!') {
            search = &search[start + 1..];
            continue;
        }
        let name_end = rest.find('>').or_else(|| rest.find(' '));
        if name_end.is_none() {
            break;
        }
        let name_end = name_end.unwrap();
        let name = rest[..name_end].trim();
        if name.is_empty() || name.contains('/') {
            search = &rest[name_end..];
            continue;
        }
        if let Some(close) = rest.find(&format!("</{name}>")) {
            let value = rest[name_end + 1..close].trim();
            if !value.is_empty() {
                out.push((name.to_string(), value.to_string()));
            }
            search = &rest[close + name.len() + 3..];
        } else {
            break;
        }
    }
    out
}

fn extract_tag_block(raw: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let start = raw.find(&open)?;
    let after_open = raw[start..].find('>').map(|i| start + i + 1)?;
    let end = raw[after_open..].find(&close)?;
    Some(raw[after_open..after_open + end].to_string())
}

fn tag_value(raw: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = raw.find(&open)?;
    let value_start = start + open.len();
    let end = raw[value_start..].find(&close)?;
    let value = raw[value_start..value_start + end].trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

fn resolve_property(value: &str, properties: &HashMap<String, String>) -> String {
    let mut out = value.to_string();
    for _ in 0..8 {
        if !out.contains("${") {
            break;
        }
        let Some(start) = out.find("${") else { break };
        let rest = &out[start + 2..];
        let Some(end) = rest.find('}') else { break };
        let key = rest[..end].trim();
        let replacement = properties.get(key).cloned().unwrap_or_default();
        let full = format!("${{{key}}}");
        if replacement.is_empty() {
            break;
        }
        out = out.replace(&full, &replacement);
    }
    out
}

fn resolve_version(version: Option<&str>, pom: &PomModel) -> Option<String> {
    let version = version.map(|v| resolve_property(v, &pom.properties))?;
    if version.is_empty() || version.contains("${") {
        return None;
    }
    Some(version)
}

// --- Maven reactor / multi-module workspace ---

/// Reactor root, `-pl` selector, and in-repo `groupId:artifactId` → module dir.
#[derive(Debug, Clone)]
pub struct MavenReactorContext {
    pub reactor_root: PathBuf,
    pub module_pl: String,
    pub workspace_modules: HashMap<String, PathBuf>,
}

/// Innermost reactor POM ancestor of a module (multi-module parent with `<modules>`).
pub fn find_maven_reactor_root(module_root: &Path) -> Option<PathBuf> {
    let mut dir = module_root.to_path_buf();
    let mut innermost = None;
    loop {
        if dir.join("pom.xml").is_file() {
            if let Ok(pom) = read_pom(&dir) {
                if pom.is_reactor() {
                    innermost = dir.canonicalize().ok().or_else(|| Some(dir.clone()));
                }
            }
        }
        if !dir.pop() {
            break;
        }
    }
    innermost
}

pub fn maven_reactor_context(module_root: &Path) -> Option<MavenReactorContext> {
    let reactor_root = find_maven_reactor_root(module_root)?;
    let module_pl = module_pl_selector(module_root, &reactor_root)?;
    let workspace_modules = build_workspace_module_map(&reactor_root);
    Some(MavenReactorContext {
        reactor_root,
        module_pl,
        workspace_modules,
    })
}

fn module_pl_selector(module_root: &Path, reactor_root: &Path) -> Option<String> {
    let module_canon = module_root.canonicalize().ok()?;
    let reactor_canon = reactor_root.canonicalize().ok()?;
    if module_canon == reactor_canon {
        return None;
    }
    let rel = module_canon.strip_prefix(&reactor_canon).ok()?;
    let pl = rel.to_string_lossy().replace('\\', "/");
    if pl.is_empty() {
        None
    } else {
        Some(pl)
    }
}

/// Map `groupId:artifactId` → module directory for every leaf module in a reactor.
pub fn build_workspace_module_map(reactor_root: &Path) -> HashMap<String, PathBuf> {
    let mut map = HashMap::new();
    collect_workspace_modules(reactor_root, &mut map);
    map
}

/// Leaf Maven module directories under a reactor (or the module itself when standalone).
pub fn list_reactor_leaf_modules(module_or_reactor: &Path) -> Vec<PathBuf> {
    let anchor = find_maven_reactor_root(module_or_reactor)
        .unwrap_or_else(|| module_or_reactor.to_path_buf());
    let map = build_workspace_module_map(&anchor);
    if map.is_empty() {
        return vec![anchor
            .canonicalize()
            .unwrap_or(anchor)];
    }
    let mut modules: Vec<PathBuf> = map.into_values().collect();
    modules.sort_by(|a, b| a.display().to_string().cmp(&b.display().to_string()));
    modules.dedup();
    modules
}

fn collect_workspace_modules(dir: &Path, out: &mut HashMap<String, PathBuf>) {
    if !dir.join("pom.xml").is_file() {
        return;
    }
    let Ok(pom) = read_pom(dir) else {
        return;
    };
    if pom.is_reactor() {
        for module in &pom.modules {
            collect_workspace_modules(&dir.join(module), out);
        }
        return;
    }
    if let (Some(group), Some(artifact)) = (&pom.group_id, &pom.artifact_id) {
        let key = format!("{group}:{artifact}");
        if let Ok(path) = dir.canonicalize() {
            out.insert(key, path);
        } else {
            out.insert(key, dir.to_path_buf());
        }
    }
}

/// Run Maven with cwd at an arbitrary directory (reactor root for `-pl` / `-am`).
/// Prefers `./mvnw` at `cwd` or an ancestor; does not add nested `-pl` (caller owns args).
pub fn run_maven_from(cwd: &Path, args: &[&str]) -> Result<std::process::Output> {
    if crate::process_registry::is_shutdown_requested() {
        bail!("Reaper is shutting down");
    }
    let wrapper_root = find_maven_wrapper_root(cwd);
    let (program, run_cwd) =
        if wrapper_root.join("mvnw").is_file() || wrapper_root.join("mvnw.cmd").is_file() {
            let _ = ensure_mvnw_executable(&wrapper_root);
            let program = if cfg!(windows) && wrapper_root.join("mvnw.cmd").is_file() {
                PathBuf::from("./mvnw.cmd")
            } else {
                PathBuf::from("./mvnw")
            };
            (program, wrapper_root)
        } else {
            (system_maven_program(), cwd.to_path_buf())
        };
    let mut process = crate::platform::command_path(&program);
    process.current_dir(&run_cwd).args(args);
    crate::process_registry::configure_command(&mut process);
    jdk::apply_java_env(&mut process);
    let label = format!("mvn {}", args.join(" "));
    let mut child = process
        .spawn()
        .with_context(|| format!("spawn {} in {}", program.display(), run_cwd.display()))?;
    let _guard = crate::process_registry::guard_for_child(&mut child, &label);
    child
        .wait_with_output()
        .with_context(|| format!("wait for {} in {}", program.display(), run_cwd.display()))
}

/// Compiled outputs for workspace sibling modules declared in this module's POM.
pub fn workspace_module_classpath_entries(
    module_root: &Path,
    ctx: &MavenReactorContext,
) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let Ok(pom) = read_pom(module_root) else {
        return (Vec::new(), Vec::new());
    };
    let module_canon = module_root.canonicalize().unwrap_or_else(|_| module_root.to_path_buf());
    let mut classes_dirs = Vec::new();
    let mut jars = Vec::new();

    for dep in &pom.dependencies {
        if dep.optional {
            continue;
        }
        let scope = dep.scope.as_deref().unwrap_or("compile");
        if matches!(scope, "test" | "provided" | "system") {
            continue;
        }
        let group = resolve_property(&dep.group_id, &pom.properties);
        let artifact = resolve_property(&dep.artifact_id, &pom.properties);
        let key = format!("{group}:{artifact}");
        let Some(sibling) = ctx.workspace_modules.get(&key) else {
            continue;
        };
        let sibling_canon = sibling.canonicalize().unwrap_or_else(|_| sibling.clone());
        if sibling_canon == module_canon {
            continue;
        }
        append_module_output_entries(sibling, &mut classes_dirs, &mut jars);
    }

    (classes_dirs, jars)
}

fn append_module_output_entries(
    module_dir: &Path,
    classes_dirs: &mut Vec<PathBuf>,
    jars: &mut Vec<PathBuf>,
) {
    let classes = module_dir.join("target/classes");
    if classes.is_dir() && !classes_dirs.contains(&classes) {
        classes_dirs.push(classes);
    }
    let target = module_dir.join("target");
    if let Ok(entries) = std::fs::read_dir(&target) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if path.extension().and_then(|e| e.to_str()) != Some("jar") {
                continue;
            }
            if name.contains("-sources") || name.contains("-javadoc") || name.ends_with(".original") {
                continue;
            }
            if !jars.contains(&path) {
                jars.push(path);
            }
        }
    }
}

/// Parent coordinates and relativePath for generating a Reaper coverage overlay POM.
#[derive(Debug, Clone)]
pub struct MavenModuleCoords {
    pub artifact_id: String,
    pub parent: Option<(String, String, String, String)>,
}

pub fn maven_module_coords(project_root: &Path) -> Result<MavenModuleCoords> {
    let pom = read_pom(project_root)?;
    let artifact_id = pom
        .artifact_id
        .clone()
        .ok_or_else(|| anyhow::anyhow!("missing artifactId in {}", project_root.display()))?;
    let parent = pom.parent_coords().map(|(group, artifact, version)| {
        let relative_path = parent_relative_path(&pom.raw).unwrap_or_else(|| "../pom.xml".into());
        (group, artifact, version, relative_path)
    });
    Ok(MavenModuleCoords {
        artifact_id,
        parent,
    })
}

/// Surefire JVM args hardcoded in this module or a parent POM (not `${argLine}` / `@{argLine}`).
pub fn effective_surefire_arg_line(project_root: &Path) -> Option<String> {
    let mut current = project_root.to_path_buf();
    for _ in 0..16 {
        let pom_path = current.join("pom.xml");
        if !pom_path.is_file() {
            break;
        }
        let raw = std::fs::read_to_string(&pom_path).ok()?;
        if let Some(line) = surefire_arg_line_in_pom(&raw) {
            return Some(line);
        }
        let model = parse_pom(&raw);
        let Some(next) = parent_pom_dir(&current, &model) else {
            break;
        };
        current = next;
    }
    None
}

fn parent_relative_path(raw: &str) -> Option<String> {
    let section = extract_tag_block(raw, "parent")?;
    tag_value(&section, "relativePath").or_else(|| Some("../pom.xml".into()))
}

fn parent_pom_dir(current: &Path, model: &PomModel) -> Option<PathBuf> {
    if let Some(section) = extract_tag_block(&model.raw, "parent") {
        let rel = tag_value(&section, "relativePath").unwrap_or_else(|| "../pom.xml".into());
        if !rel.is_empty() {
            let candidate = current.join(&rel);
            // relativePath may point at the parent POM file or its directory.
            if candidate.is_file() {
                return candidate.parent().map(|p| p.to_path_buf());
            }
            if candidate.join("pom.xml").is_file() {
                return Some(candidate);
            }
        }
    }
    if let Some((group, artifact, version)) = model.parent_coords() {
        if let Some(dir) = resolve_pom_directory(&group, &artifact, &version) {
            return Some(dir);
        }
    }
    let parent = current.parent()?;
    if parent.join("pom.xml").is_file() {
        Some(parent.to_path_buf())
    } else {
        None
    }
}

fn surefire_arg_line_in_pom(raw: &str) -> Option<String> {
    let properties = parse_properties(raw);
    let mut search = raw;
    while let Some(idx) = search.find("maven-surefire-plugin") {
        let tail = &search[idx..];
        let plugin_block = extract_tag_block(tail, "plugin").unwrap_or_else(|| tail.to_string());
        if let Some(config) = extract_tag_block(&plugin_block, "configuration") {
            if let Some(arg) = tag_value(&config, "argLine") {
                if arg.contains("${argLine}") || arg.contains("@{argLine}") {
                    return None;
                }
                let resolved = resolve_property(&arg, &properties);
                if !resolved.trim().is_empty() {
                    return Some(resolved);
                }
            }
        }
        search = &search[idx + "maven-surefire-plugin".len()..];
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hello_world_pom() {
        let ws = std::env::temp_dir().join("reaper-maven-hello");
        let _ = std::fs::remove_dir_all(&ws);
        std::fs::create_dir_all(&ws).unwrap();
        std::fs::write(
            &ws.join("pom.xml"),
            r#"<?xml version="1.0"?>
<project>
  <groupId>com.helloworld</groupId>
  <artifactId>hello-world</artifactId>
  <version>1.0.0</version>
</project>"#,
        )
        .unwrap();
        let roots = find_all_maven_roots(&ws).unwrap();
        assert_eq!(roots.len(), 1);
        let pom = read_pom(&roots[0]).unwrap();
        assert_eq!(pom.group_id.as_deref(), Some("com.helloworld"));
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn parses_dependency_coordinates() {
        let ws = std::env::temp_dir().join("reaper-maven-deps");
        let _ = std::fs::remove_dir_all(&ws);
        std::fs::create_dir_all(&ws).unwrap();
        std::fs::write(
            &ws.join("pom.xml"),
            r#"<project>
  <groupId>com.example</groupId>
  <artifactId>demo</artifactId>
  <version>1.0</version>
  <dependencies>
    <dependency>
      <groupId>org.junit.jupiter</groupId>
      <artifactId>junit-jupiter</artifactId>
      <version>5.10.2</version>
    </dependency>
  </dependencies>
</project>"#,
        )
        .unwrap();
        let coords = collect_dependency_coordinates(&ws);
        assert!(coords.iter().any(|(g, a, v)| {
            g == "org.junit.jupiter" && a == "junit-jupiter" && v == "5.10.2"
        }));
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn resolves_spring_boot_bom_dependency_versions() {
        let ws = std::env::temp_dir().join("reaper-maven-spring-boot");
        let _ = std::fs::remove_dir_all(&ws);
        std::fs::create_dir_all(&ws).unwrap();
        std::fs::write(
            &ws.join("pom.xml"),
            r#"<?xml version="1.0" encoding="UTF-8"?>
<project xmlns="http://maven.apache.org/POM/4.0.0"
         xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
         xsi:schemaLocation="http://maven.apache.org/POM/4.0.0 https://maven.apache.org/xsd/maven-4.0.0.xsd">
  <modelVersion>4.0.0</modelVersion>
  <parent>
    <groupId>org.springframework.boot</groupId>
    <artifactId>spring-boot-starter-parent</artifactId>
    <version>3.5.0</version>
    <relativePath/>
  </parent>
  <groupId>com.example</groupId>
  <artifactId>demo</artifactId>
  <version>0.0.1-SNAPSHOT</version>
  <dependencies>
    <dependency>
      <groupId>org.springframework.boot</groupId>
      <artifactId>spring-boot-starter-web</artifactId>
    </dependency>
    <dependency>
      <groupId>org.springframework.boot</groupId>
      <artifactId>spring-boot-starter-test</artifactId>
      <scope>test</scope>
    </dependency>
  </dependencies>
</project>"#,
        )
        .unwrap();
        let coords = collect_dependency_coordinates(&ws);
        assert!(
            coords.iter().any(|(g, a, _)| g == "org.springframework.boot" && a == "spring-boot-starter-web"),
            "expected spring-boot-starter-web, got {coords:?}"
        );
        assert!(
            coords.iter().any(|(g, a, _)| g == "org.springframework.boot" && a == "spring-boot-starter-test"),
            "expected spring-boot-starter-test, got {coords:?}"
        );
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn surefire_test_pattern_converts_method_filter() {
        assert_eq!(
            surefire_test_pattern(
                "com.example.taskscheduler.health.SchedulerHealthIndicatorTest.reportsUpWithTaskCounts"
            ),
            "com.example.taskscheduler.health.SchedulerHealthIndicatorTest#reportsUpWithTaskCounts"
        );
    }

    #[test]
    fn surefire_test_pattern_leaves_class_filter() {
        assert_eq!(
            surefire_test_pattern("com.example.AppTest"),
            "com.example.AppTest"
        );
    }

    #[test]
    fn parse_maven_goal_normalizes_dtest() {
        let parts = parse_maven_goal(
            "-Dtest=com.example.AppTest.appHasAGreeting test",
        )
        .unwrap();
        assert_eq!(parts[0], "-Dtest=com.example.AppTest#appHasAGreeting");
        assert_eq!(parts[1], "test");
    }

    #[test]
    fn detects_hardcoded_surefire_arg_line_in_parent() {
        let root = std::env::temp_dir().join("reaper-maven-surefire-parent");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("child")).unwrap();
        std::fs::write(
            root.join("pom.xml"),
            r#"<?xml version="1.0"?>
<project>
  <modelVersion>4.0.0</modelVersion>
  <groupId>com.example</groupId>
  <artifactId>parent</artifactId>
  <version>1.0</version>
  <packaging>pom</packaging>
  <build>
    <pluginManagement>
      <plugins>
        <plugin>
          <groupId>org.apache.maven.plugins</groupId>
          <artifactId>maven-surefire-plugin</artifactId>
          <configuration>
            <argLine>-Dnet.bytebuddy.experimental=true</argLine>
          </configuration>
        </plugin>
      </plugins>
    </pluginManagement>
  </build>
</project>"#,
        )
        .unwrap();
        std::fs::write(
            root.join("child/pom.xml"),
            r#"<?xml version="1.0"?>
<project>
  <modelVersion>4.0.0</modelVersion>
  <parent>
    <groupId>com.example</groupId>
    <artifactId>parent</artifactId>
    <version>1.0</version>
  </parent>
  <artifactId>child</artifactId>
</project>"#,
        )
        .unwrap();
        assert_eq!(
            effective_surefire_arg_line(&root.join("child")).as_deref(),
            Some("-Dnet.bytebuddy.experimental=true")
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn ignores_surefire_arg_line_property_placeholder() {
        let root = std::env::temp_dir().join("reaper-maven-surefire-placeholder");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("pom.xml"),
            r#"<?xml version="1.0"?>
<project>
  <modelVersion>4.0.0</modelVersion>
  <artifactId>demo</artifactId>
  <build>
    <plugins>
      <plugin>
        <groupId>org.apache.maven.plugins</groupId>
        <artifactId>maven-surefire-plugin</artifactId>
        <configuration>
          <argLine>@{argLine} -Xmx512m</argLine>
        </configuration>
      </plugin>
    </plugins>
  </build>
</project>"#,
        )
        .unwrap();
        assert!(effective_surefire_arg_line(&root).is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn resolve_maven_prefers_mvnw_from_reactor_for_nested_module() {
        let root = std::env::temp_dir().join(format!(
            "reaper-mvnw-nested-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let module = root.join("services/app");
        std::fs::create_dir_all(&module).unwrap();
        std::fs::write(
            root.join("pom.xml"),
            r#"<project>
  <modelVersion>4.0.0</modelVersion>
  <groupId>com.example</groupId>
  <artifactId>platform</artifactId>
  <version>1.0</version>
  <packaging>pom</packaging>
  <modules><module>services</module></modules>
</project>"#,
        )
        .unwrap();
        std::fs::create_dir_all(root.join("services")).unwrap();
        std::fs::write(
            root.join("services/pom.xml"),
            r#"<project>
  <modelVersion>4.0.0</modelVersion>
  <parent>
    <groupId>com.example</groupId>
    <artifactId>platform</artifactId>
    <version>1.0</version>
  </parent>
  <artifactId>services</artifactId>
  <packaging>pom</packaging>
  <modules><module>app</module></modules>
</project>"#,
        )
        .unwrap();
        std::fs::write(
            module.join("pom.xml"),
            r#"<project>
  <modelVersion>4.0.0</modelVersion>
  <parent>
    <groupId>com.example</groupId>
    <artifactId>services</artifactId>
    <version>1.0</version>
  </parent>
  <artifactId>app</artifactId>
</project>"#,
        )
        .unwrap();
        std::fs::write(root.join("mvnw"), "#!/bin/sh\nexit 0\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(root.join("mvnw")).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(root.join("mvnw"), perms).unwrap();
        }

        let cmd = resolve_maven_command(&module);
        assert_eq!(cmd.program, PathBuf::from("./mvnw"));
        assert_eq!(
            cmd.cwd.canonicalize().unwrap(),
            root.canonicalize().unwrap()
        );
        assert!(
            cmd.project_args.windows(2).any(|w| w[0] == "-pl" && w[1] == "services/app"),
            "expected -pl services/app, got {:?}",
            cmd.project_args
        );
        assert!(cmd.project_args.iter().any(|a| a == "-am"));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn spring_cloud_bom_expands_nested_imports() {
        let managed = bom_managed_versions(
            "org.springframework.cloud",
            "spring-cloud-dependencies",
            "2023.0.1",
        );
        if managed.is_empty() {
            // ~/.m2 may not have this BOM in CI sandboxes.
            return;
        }
        assert!(
            managed.contains_key(
                "org.springframework.cloud:spring-cloud-starter-netflix-eureka-client"
            ),
            "nested netflix BOM should manage eureka-client starter; got {} entries",
            managed.len()
        );
        assert!(
            managed.contains_key("org.springframework.cloud:spring-cloud-commons"),
            "expected spring-cloud-commons (EnableDiscoveryClient)"
        );
    }

    #[test]
    fn nested_bom_import_resolves_versionless_cloud_dep() {
        let root = std::env::temp_dir().join(format!(
            "reaper-nested-bom-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("pom.xml"),
            r#"<?xml version="1.0" encoding="UTF-8"?>
<project>
  <modelVersion>4.0.0</modelVersion>
  <groupId>com.example</groupId>
  <artifactId>demo</artifactId>
  <version>1.0</version>
  <properties>
    <spring-cloud.version>2023.0.1</spring-cloud.version>
  </properties>
  <dependencyManagement>
    <dependencies>
      <dependency>
        <groupId>org.springframework.cloud</groupId>
        <artifactId>spring-cloud-dependencies</artifactId>
        <version>${spring-cloud.version}</version>
        <type>pom</type>
        <scope>import</scope>
      </dependency>
    </dependencies>
  </dependencyManagement>
  <dependencies>
    <dependency>
      <groupId>org.springframework.cloud</groupId>
      <artifactId>spring-cloud-starter-netflix-eureka-client</artifactId>
    </dependency>
  </dependencies>
</project>"#,
        )
        .unwrap();
        let coords = collect_dependency_coordinates(&root);
        let _ = std::fs::remove_dir_all(&root);
        if read_m2_pom_text(
            "org.springframework.cloud",
            "spring-cloud-dependencies",
            "2023.0.1",
        )
        .is_none()
        {
            return;
        }
        assert!(
            coords.iter().any(|(g, a, _)| {
                g == "org.springframework.cloud"
                    && a == "spring-cloud-starter-netflix-eureka-client"
            }),
            "expected versionless eureka starter resolved via nested BOM imports, got {coords:?}"
        );
    }
}
