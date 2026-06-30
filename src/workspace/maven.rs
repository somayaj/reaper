use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use serde::Serialize;

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
}

/// Prefer `./mvnw` in the project root, fall back to `mvn` on PATH.
pub fn resolve_maven_command(project_root: &Path) -> MavenCommand {
    let mvnw = if cfg!(windows) {
        project_root.join("mvnw.cmd")
    } else {
        project_root.join("mvnw")
    };
    if mvnw.is_file() {
        return MavenCommand {
            program: mvnw,
            cwd: project_root.to_path_buf(),
        };
    }
    MavenCommand {
        program: PathBuf::from("mvn"),
        cwd: project_root.to_path_buf(),
    }
}

pub fn run_maven(project_root: &Path, args: &[&str]) -> Result<std::process::Output> {
    let cmd = resolve_maven_command(project_root);
    Command::new(&cmd.program)
        .current_dir(&cmd.cwd)
        .args(args)
        .output()
        .with_context(|| format!("spawn {}", cmd.program.display()))
}

/// Discover Maven module roots (skips Gradle projects and reactor parents without sources).
pub fn find_all_maven_roots(ws: &Path) -> Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    collect_maven_roots(ws, ws, 0, 8, &mut found)?;
    found.sort_by(|a, b| a.display().to_string().cmp(&b.display().to_string()));
    found.dedup();
    Ok(found)
}

fn collect_maven_roots(
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

    if gradle::is_gradle_project_dir(dir) {
        return Ok(());
    }

    if dir.join("pom.xml").is_file() {
        let pom = read_pom(dir)?;
        if pom.is_reactor() {
            for module in &pom.modules {
                let module_dir = dir.join(module);
                if module_dir.is_dir() {
                    collect_maven_roots(ws, &module_dir, depth + 1, max_depth, out)?;
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
        let name = entry.file_name();
        if name == ".git"
            || name == ".reaper"
            || name == "node_modules"
            || name == "target"
            || name == "build"
        {
            continue;
        }
        collect_maven_roots(ws, &path, depth + 1, max_depth, out)?;
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
    let has_wrapper = root.join("mvnw").is_file() || root.join("mvnw.cmd").is_file();
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
            meta.modified()?.elapsed()?.as_nanos()
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

/// Upper bound when walking Maven/Gradle transitive dependency trees offline.
pub const MAX_TRANSITIVE_CLASSPATH_JARS: usize = 800;

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

    const MAX_JARS: usize = MAX_TRANSITIVE_CLASSPATH_JARS;
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

        if jars.len() >= MAX_JARS {
            break;
        }
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
    if let Some((group, artifact, version)) = pom.parent_coords() {
        if let Some(parent_pom) = read_m2_pom_model(&group, &artifact, &version) {
            if let Some(parent_dir) = resolve_pom_directory(&group, &artifact, &version) {
                management = effective_dependency_management(&parent_dir, &parent_pom, depth + 1);
            }
        }
    }
    merge_pom_dependency_management(pom, &mut management);
    let _ = root;
    management
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
                    for bom_dep in &bom_pom.dependency_management {
                        if let Some(ver) = resolve_version(bom_dep.version.as_deref(), &bom_pom) {
                            management
                                .entry(format!("{}:{}", bom_dep.group_id, bom_dep.artifact_id))
                                .or_insert((ver, bom_dep.artifact_id.clone()));
                        }
                    }
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

fn read_pom(root: &Path) -> Result<PomModel> {
    let path = root.join("pom.xml");
    let raw = std::fs::read_to_string(&path)
        .with_context(|| format!("read {}", path.display()))?;
    Ok(parse_pom(&raw))
}

fn parse_pom(raw: &str) -> PomModel {
    let properties = parse_properties(raw);
    let parent = parse_parent(raw, &properties);
    let dependency_management = parse_dependencies_in_section(raw, "dependencyManagement", &properties);
    let dependencies = parse_dependencies_in_section(raw, "dependencies", &properties);
    let modules = parse_modules(raw);
    PomModel {
        group_id: tag_value(raw, "groupId").or_else(|| parent.as_ref().map(|(g, _, _)| g.clone())),
        artifact_id: tag_value(raw, "artifactId"),
        version: tag_value(raw, "version")
            .or_else(|| parent.as_ref().map(|(_, _, v)| v.clone())),
        packaging: tag_value(raw, "packaging"),
        parent,
        properties,
        dependencies,
        dependency_management,
        modules,
        raw: raw.to_string(),
    }
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
}
