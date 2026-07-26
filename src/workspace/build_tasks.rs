use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Serialize;

use super::gradle::{self, find_gradle_wrapper_root};
use super::maven::{self, find_maven_reactor_root, find_maven_root};
use super::native_build_tasks;
use super::safe_join;

#[derive(Debug, Clone, Serialize)]
pub struct BuildTask {
    pub id: String,
    pub label: String,
    pub command: String,
    pub group: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct BuildProjectNode {
    pub name: String,
    pub path: String,
    /// `reactor`, `module`, `gradle-root`, `gradle-subproject`, or `gradle-group`
    pub kind: String,
    pub tasks: Vec<BuildTask>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<BuildProjectNode>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BuildTasksTree {
    pub build_tool: String,
    pub root_name: String,
    pub root_path: String,
    /// Module folder to expand/highlight (workspace-relative, empty = root).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub focus_module: String,
    pub tree: BuildProjectNode,
}

pub fn build_tasks_tree(ws: &Path, rel_path: &str, compose_content: Option<&str>) -> Result<BuildTasksTree> {
    let rel_path = super::normalize_workspace_source_path(rel_path);
    let _ = safe_join(ws, &rel_path)?;

    // When the open file is a Makefile, show Make targets — not ambient docker-compose.
    if native_build_tasks::is_makefile_manifest_path(&rel_path) {
        if let Some(tree) = native_build_tasks::try_make_tree(ws, &rel_path)? {
            return Ok(tree);
        }
    }
    if native_build_tasks::is_compose_manifest_path(&rel_path) {
        if let Some(tree) = native_build_tasks::try_docker_compose_tree(ws, &rel_path, compose_content)? {
            return Ok(tree);
        }
    }
    // elide.pkl wins over ambient pom/gradle when that manifest is open (or nearest).
    if super::elide_pkl::is_elide_manifest_path(&rel_path) {
        if let Some(tree) = native_build_tasks::try_elide_tree(ws, &rel_path)? {
            return Ok(tree);
        }
    }
    // Prefer Maven/Gradle for this file before ambient docker-compose in the repo root.
    if let Some(tree) = try_maven_tree(ws, &rel_path)? {
        return Ok(tree);
    }
    if let Some(tree) = try_gradle_tree(ws, &rel_path)? {
        return Ok(tree);
    }
    if let Some(tree) = native_build_tasks::try_docker_compose_tree(ws, &rel_path, compose_content)? {
        return Ok(tree);
    }
    if let Some(tree) = native_build_tasks::try_native_tree(ws, &rel_path, compose_content)? {
        return Ok(tree);
    }

    Ok(BuildTasksTree {
        build_tool: String::new(),
        root_name: String::new(),
        root_path: String::new(),
        focus_module: focus_module_key(ws, &rel_path),
        tree: BuildProjectNode {
            name: String::new(),
            path: rel_path,
            kind: "unknown".into(),
            tasks: Vec::new(),
            children: Vec::new(),
        },
    })
}

fn try_maven_tree(ws: &Path, rel_path: &str) -> Result<Option<BuildTasksTree>> {
    let Some(module_root) = find_maven_root(ws, rel_path)? else {
        return Ok(None);
    };
    let reactor_root = find_maven_reactor_root(&module_root).unwrap_or_else(|| module_root.clone());
    let tree = build_maven_node(ws, &reactor_root)?;
    let root_path = gradle::rel_path_for(ws, &reactor_root)?;
    let root_name = tree.name.clone();
    Ok(Some(BuildTasksTree {
        build_tool: "maven".into(),
        root_name,
        root_path,
        focus_module: focus_module_key(ws, rel_path),
        tree,
    }))
}

fn build_maven_node(ws: &Path, dir: &Path) -> Result<BuildProjectNode> {
    let pom = maven::pom_tree_info(dir)?;
    let rel = gradle::rel_path_for(ws, dir)?;
    let build_path = if rel.is_empty() {
        "pom.xml".into()
    } else {
        format!("{rel}/pom.xml")
    };
    let name = dir
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("project")
        .to_string();

    let kind = if pom.is_reactor {
        "reactor"
    } else {
        "module"
    };

    let mut children = Vec::new();
    if pom.is_reactor {
        for module in &pom.modules {
            let child_dir = dir.join(module);
            if child_dir.join("pom.xml").is_file() {
                children.push(build_maven_node(ws, &child_dir)?);
            }
        }
    }

    let tasks = maven_module_tasks(&pom, dir);

    Ok(BuildProjectNode {
        name,
        path: build_path,
        kind: kind.into(),
        tasks,
        children,
    })
}

fn maven_module_tasks(pom: &maven::PomTreeInfo, dir: &Path) -> Vec<BuildTask> {
    let markers = super::java_ecosystem::scan_maven_pom(&pom.raw);
    let mut tasks = Vec::new();

    if pom.is_spring_boot {
        tasks.push(task("spring-boot:run", "spring-boot:run", "application"));
    }

    if pom.is_reactor {
        tasks.extend([
            task("install", "install", "lifecycle"),
            task("clean", "clean", "lifecycle"),
            task("verify", "verify", "lifecycle"),
        ]);
    } else {
        tasks.extend([
            task("compile", "compile", "lifecycle"),
            task("test", "test", "verification"),
            task("package", "package", "lifecycle"),
            task("clean", "clean", "lifecycle"),
        ]);
        if markers.jacoco {
            tasks.push(task("jacoco:report", "jacoco:report", "reporting"));
        }
    }

    let _ = dir;
    tasks
}

fn try_gradle_tree(ws: &Path, rel_path: &str) -> Result<Option<BuildTasksTree>> {
    let Some(project_root) = gradle::find_gradle_root(ws, rel_path)? else {
        return Ok(None);
    };
    let settings_root = find_gradle_settings_root(&project_root);
    let tree = build_gradle_root_node(ws, &settings_root)?;
    let root_path = gradle::rel_path_for(ws, &settings_root)?;
    let root_name = tree.name.clone();
    Ok(Some(BuildTasksTree {
        build_tool: "gradle".into(),
        root_name,
        root_path,
        focus_module: focus_module_key(ws, rel_path),
        tree,
    }))
}

fn find_gradle_settings_root(project_root: &Path) -> PathBuf {
    let mut dir = project_root.to_path_buf();
    loop {
        if dir.join("settings.gradle").is_file() || dir.join("settings.gradle.kts").is_file() {
            return dir;
        }
        let Some(parent) = dir.parent() else {
            break;
        };
        dir = parent.to_path_buf();
    }
    find_gradle_wrapper_root(project_root)
}

fn build_gradle_root_node(ws: &Path, settings_root: &Path) -> Result<BuildProjectNode> {
    let includes = parse_gradle_includes(settings_root);
    let root_tasks = gradle_module_tasks(settings_root, None);
    let name = settings_root
        .file_name()
        .and_then(|s| s.to_str())
        .map(String::from)
        .or_else(|| read_gradle_root_name(settings_root))
        .unwrap_or_else(|| "project".to_string());
    let rel = gradle::rel_path_for(ws, settings_root)?;
    let build_path = gradle_build_file_path_for_dir(settings_root, &rel);

    let children = if includes.is_empty() {
        Vec::new()
    } else {
        build_gradle_include_nodes(ws, settings_root, &includes)?
    };

    Ok(BuildProjectNode {
        name,
        path: build_path,
        kind: "gradle-root".into(),
        tasks: root_tasks,
        children,
    })
}

#[derive(Default)]
struct IncludeBranch {
    full_path: Option<String>,
    children: BTreeMap<String, IncludeBranch>,
}

impl IncludeBranch {
    fn insert(&mut self, segments: &[String], full_path: &str) {
        if segments.is_empty() {
            self.full_path = Some(full_path.to_string());
            return;
        }
        self.children
            .entry(segments[0].clone())
            .or_default()
            .insert(&segments[1..], full_path);
    }

    fn into_nodes(
        &self,
        ws: &Path,
        settings_root: &Path,
        prefix: &str,
    ) -> Result<Vec<BuildProjectNode>> {
        let mut out = Vec::new();
        for (name, branch) in &self.children {
            let segment_path = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}/{name}")
            };
            if branch.children.is_empty() {
                let full = branch
                    .full_path
                    .as_deref()
                    .ok_or_else(|| anyhow::anyhow!("missing include path for {name}"))?;
                let dir = settings_root.join(full);
                if dir.join("build.gradle").is_file() || dir.join("build.gradle.kts").is_file() {
                    out.push(build_gradle_leaf_node(ws, settings_root, &dir, full)?);
                }
            } else {
                let group_dir = settings_root.join(&segment_path);
                let path = if group_dir.is_dir() {
                    gradle::rel_path_for(ws, &group_dir)?
                } else {
                    segment_path.clone()
                };
                out.push(BuildProjectNode {
                    name: name.clone(),
                    path,
                    kind: "gradle-group".into(),
                    tasks: Vec::new(),
                    children: branch.into_nodes(ws, settings_root, &segment_path)?,
                });
            }
        }
        Ok(out)
    }
}

fn build_gradle_include_nodes(
    ws: &Path,
    settings_root: &Path,
    includes: &[String],
) -> Result<Vec<BuildProjectNode>> {
    let mut tree = IncludeBranch::default();
    for inc in includes {
        let segments: Vec<String> = inc
            .split('/')
            .filter(|s| !s.is_empty())
            .map(String::from)
            .collect();
        if !segments.is_empty() {
            tree.insert(&segments, inc);
        }
    }
    tree.into_nodes(ws, settings_root, "")
}

fn build_gradle_leaf_node(
    ws: &Path,
    settings_root: &Path,
    dir: &Path,
    gradle_path: &str,
) -> Result<BuildProjectNode> {
    let rel = gradle::rel_path_for(ws, dir)?;
    let build_path = gradle_build_file_path_for_dir(dir, &rel);
    let name = dir
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("project")
        .to_string();
    let tasks = gradle_module_tasks(dir, Some(gradle_path));
    let _ = settings_root;
    Ok(BuildProjectNode {
        name,
        path: build_path,
        kind: "gradle-subproject".into(),
        tasks,
        children: Vec::new(),
    })
}

fn gradle_module_tasks(dir: &Path, gradle_path: Option<&str>) -> Vec<BuildTask> {
    let build_content = gradle::read_build_file(dir).unwrap_or_default();
    let prefix = gradle_path
        .filter(|p| !p.is_empty())
        .map(|p| format!(":{}", p.replace('/', ":")))
        .unwrap_or_default();

    let is_spring_boot = gradle::build_file_is_spring_boot(&build_content);
    let has_application = gradle::has_application_plugin(&build_content);
    let is_grails = dir.join("grails-app").is_dir()
        || build_content.contains("org.grails.gradle.plugin")
        || build_content.contains("grails-gradle-plugin")
        || build_content.contains("grails-plugin");
    let markers = super::java_ecosystem::scan_gradle_project(dir);

    let mut tasks = Vec::new();
    let mk = |name: &str, group: &str| {
        let command = if prefix.is_empty() {
            name.to_string()
        } else {
            format!("{prefix}:{name}")
        };
        task(&command, &command, group)
    };

    if is_spring_boot {
        tasks.push(mk("bootRun", "application"));
    } else if is_grails {
        tasks.push(mk("bootRun", "application"));
        tasks.push(mk("test", "verification"));
        tasks.push(mk("console", "application"));
    } else if has_application {
        tasks.push(mk("run", "application"));
    }

    tasks.extend([
        mk("build", "lifecycle"),
        mk("test", "verification"),
        mk("clean", "lifecycle"),
    ]);

    if markers.jacoco {
        tasks.push(mk("jacocoTestReport", "reporting"));
    }

    tasks
}

pub(crate) fn task(id: &str, command: &str, group: &str) -> BuildTask {
    task_labeled(id, command, command, group)
}

pub(crate) fn task_labeled(id: &str, label: &str, command: &str, group: &str) -> BuildTask {
    BuildTask {
        id: id.to_string(),
        label: label.to_string(),
        command: command.to_string(),
        group: group.to_string(),
    }
}

fn focus_module_key(ws: &Path, rel_path: &str) -> String {
    let rel_path = super::normalize_workspace_source_path(rel_path);
    let path = Path::new(&rel_path);
    let base = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    if base == "pom.xml" || base == "build.gradle" || base == "build.gradle.kts" {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            let abs = ws.join(parent);
            if let Ok(rel) = gradle::rel_path_for(ws, &abs) {
                return rel;
            }
        }
        return String::new();
    }
    if matches!(
        base.as_str(),
        "settings.gradle"
            | "settings.gradle.kts"
            | "gradle.properties"
            | "package.json"
            | "pyproject.toml"
            | "manage.py"
            | "cargo.toml"
            | "pubspec.yaml"
            | "rakefile"
            | "gemfile"
            | "go.mod"
            | "cmakelists.txt"
            | "meson.build"
            | "makefile"
            | "gnumakefile"
    ) {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            if let Ok(rel) = gradle::rel_path_for(ws, &ws.join(parent)) {
                return rel;
            }
        }
        return String::new();
    }

    if let Ok(Some(root)) = gradle::find_gradle_root(ws, &rel_path) {
        if let Ok(rel) = gradle::rel_path_for(ws, &root) {
            return rel;
        }
    }
    if let Ok(Some(root)) = maven::find_maven_root(ws, &rel_path) {
        if let Ok(rel) = gradle::rel_path_for(ws, &root) {
            return rel;
        }
    }
    String::new()
}

fn gradle_build_file_path_for_dir(dir: &Path, rel: &str) -> String {
    if dir.join("build.gradle.kts").is_file() {
        if rel.is_empty() {
            "build.gradle.kts".into()
        } else {
            format!("{rel}/build.gradle.kts")
        }
    } else if dir.join("build.gradle").is_file() {
        if rel.is_empty() {
            "build.gradle".into()
        } else {
            format!("{rel}/build.gradle")
        }
    } else if dir.join("settings.gradle.kts").is_file() {
        if rel.is_empty() {
            "settings.gradle.kts".into()
        } else {
            format!("{rel}/settings.gradle.kts")
        }
    } else if dir.join("settings.gradle").is_file() {
        if rel.is_empty() {
            "settings.gradle".into()
        } else {
            format!("{rel}/settings.gradle")
        }
    } else if rel.is_empty() {
        "build.gradle".into()
    } else {
        format!("{rel}/build.gradle")
    }
}

fn gradle_build_file_path(rel: &str) -> String {
    if rel.is_empty() {
        "build.gradle".into()
    } else {
        format!("{rel}/build.gradle")
    }
}

fn read_gradle_root_name(settings_root: &Path) -> Option<String> {
    for name in ["settings.gradle", "settings.gradle.kts"] {
        let path = settings_root.join(name);
        if let Ok(content) = std::fs::read_to_string(&path) {
            for line in content.lines() {
                let line = line.trim();
                if let Some(rest) = line.strip_prefix("rootProject.name") {
                    if let Some(q) = rest.split('=').nth(1) {
                        let parsed = q.trim().trim_matches('"').trim_matches('\'').to_string();
                        if !parsed.is_empty() {
                            return Some(parsed);
                        }
                    }
                }
            }
        }
    }
    None
}

/// Parse `include` / `includeFlat` paths from settings.gradle(.kts).
pub fn parse_gradle_includes(settings_root: &Path) -> Vec<String> {
    let content = read_settings_file(settings_root).unwrap_or_default();
    parse_gradle_includes_from_content(&content)
}

fn read_settings_file(settings_root: &Path) -> Result<String> {
    for name in ["settings.gradle.kts", "settings.gradle"] {
        let path = settings_root.join(name);
        if path.is_file() {
            return std::fs::read_to_string(&path)
                .with_context(|| format!("read {}", path.display()));
        }
    }
    Ok(String::new())
}

pub fn parse_gradle_includes_from_content(content: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in content.lines() {
        let line = line.split("//").next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        for prefix in ["includeFlat", "include"] {
            if let Some(rest) = line.strip_prefix(prefix) {
                out.extend(parse_include_arguments(rest));
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

fn parse_include_arguments(rest: &str) -> Vec<String> {
    let trimmed = rest.trim();
    let inner = trimmed
        .strip_prefix('(')
        .and_then(|s| s.strip_suffix(')'))
        .unwrap_or(trimmed);
    let mut paths = Vec::new();
    let mut buf = String::new();
    let mut in_quote = None;

    for ch in inner.chars() {
        match ch {
            '\'' | '"' if in_quote.is_none() => in_quote = Some(ch),
            c if Some(c) == in_quote => in_quote = None,
            ',' if in_quote.is_none() => {
                push_include_path(&mut paths, &buf);
                buf.clear();
            }
            c if !c.is_whitespace() || in_quote.is_some() => buf.push(ch),
            _ => {}
        }
    }
    push_include_path(&mut paths, &buf);
    paths
}

fn push_include_path(out: &mut Vec<String>, raw: &str) {
    let s = raw.trim().trim_matches('"').trim_matches('\'');
    if s.is_empty() {
        return;
    }
    let normalized = s.trim_start_matches(':').replace(':', "/");
    if !normalized.is_empty() {
        out.push(normalized);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn parse_gradle_includes_colon_paths() {
        let content = r#"
rootProject.name = 'demo'
include ':platform-common', ':inventory-service'
include(":services:ordering")
"#;
        let paths = parse_gradle_includes_from_content(content);
        assert_eq!(
            paths,
            vec![
                "inventory-service".to_string(),
                "platform-common".to_string(),
                "services/ordering".to_string(),
            ]
        );
    }

    #[test]
    fn parse_gradle_includes_single_quotes() {
        let content = r#"
include 'libs:common'
include 'services:user-service'
include 'libs:core:core-base'
"#;
        let paths = parse_gradle_includes_from_content(content);
        assert_eq!(
            paths,
            vec![
                "libs/common".to_string(),
                "libs/core/core-base".to_string(),
                "services/user-service".to_string(),
            ]
        );
    }

    #[test]
    fn gradle_tree_nested_services_module() {
        let tmp = std::env::temp_dir().join(format!("reaper-gradle-tasks-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();

        fs::write(
            tmp.join("settings.gradle"),
            r#"rootProject.name = 'demo'
include 'services:inventory-service'
"#,
        )
        .unwrap();
        fs::write(tmp.join("build.gradle"), "plugins { id 'java' }").unwrap();
        let module = tmp.join("services/inventory-service");
        fs::create_dir_all(&module).unwrap();
        fs::write(
            module.join("build.gradle"),
            "plugins { id 'org.springframework.boot' }",
        )
        .unwrap();

        let tree = build_tasks_tree(&tmp, "services/inventory-service/build.gradle", None).expect("tree");
        assert_eq!(tree.build_tool, "gradle");
        assert_eq!(tree.tree.children.len(), 1);
        assert_eq!(tree.tree.children[0].name, "services");
        assert_eq!(tree.tree.children[0].children.len(), 1);
        assert_eq!(tree.tree.children[0].children[0].name, "inventory-service");
        assert!(tree.tree.children[0].children[0]
            .tasks
            .iter()
            .any(|t| t.command == ":services:inventory-service:bootRun"));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn gradle_build_file_path_prefers_kts() {
        let tmp = std::env::temp_dir().join(format!("reaper-gradle-kts-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp.join("app")).unwrap();
        fs::write(
            tmp.join("settings.gradle.kts"),
            "rootProject.name = \"demo\"\ninclude(\"app\")",
        )
        .unwrap();
        fs::write(tmp.join("build.gradle.kts"), "plugins { java }").unwrap();
        fs::write(tmp.join("app/build.gradle.kts"), "plugins { java }").unwrap();

        let tree = build_tasks_tree(&tmp, "app/build.gradle.kts", None).expect("tree");
        assert_eq!(tree.build_tool, "gradle");
        assert_eq!(tree.tree.path, "build.gradle.kts");
        assert_eq!(tree.tree.children.len(), 1);
        assert_eq!(tree.tree.children[0].path, "app/build.gradle.kts");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn spring_gradle_complicated_service_module() {
        let ws = PathBuf::from("/Users/sunny/reaper/workspaces/Spring-gradle-complicated");
        if !ws.is_dir() {
            return;
        }
        let tree =
            build_tasks_tree(&ws, "services/inventory-service/build.gradle", None).expect("tree");
        assert_eq!(tree.build_tool, "gradle");
        assert_eq!(tree.focus_module, "services/inventory-service");
        assert!(tree.tree.name == "spring-gradle-complicated"
            || tree.tree.name == "Spring-gradle-complicated");
        let services = tree
            .tree
            .children
            .iter()
            .find(|c| c.name == "services")
            .expect("services group");
        let inv = services
            .children
            .iter()
            .find(|c| c.name == "inventory-service")
            .expect("inventory-service");
        assert_eq!(inv.path, "services/inventory-service/build.gradle");
        let commands: Vec<_> = inv.tasks.iter().map(|t| t.command.as_str()).collect();
        assert!(commands.contains(&":services:inventory-service:bootRun"));
        assert!(commands.contains(&":services:inventory-service:build"));
        assert!(commands.contains(&":services:inventory-service:test"));
    }

    #[test]
    fn maven_tree_uses_folder_names() {
        let tmp = std::env::temp_dir().join(format!("reaper-build-tasks-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let root = &tmp;
        fs::write(
            root.join("pom.xml"),
            r#"<?xml version="1.0" encoding="UTF-8"?>
<project>
  <artifactId>parent-artifact</artifactId>
  <packaging>pom</packaging>
  <modules><module>inventory-service</module></modules>
</project>"#,
        )
        .unwrap();
        let module = root.join("inventory-service");
        fs::create_dir_all(&module).unwrap();
        fs::write(
            module.join("pom.xml"),
            r#"<?xml version="1.0" encoding="UTF-8"?>
<project>
  <groupId>com.example</groupId>
  <artifactId>inventory-artifact</artifactId>
  <dependencies>
    <dependency>
      <groupId>org.springframework.boot</groupId>
      <artifactId>spring-boot-starter</artifactId>
    </dependency>
  </dependencies>
</project>"#,
        )
        .unwrap();

        let tree = build_tasks_tree(root, "pom.xml", None).expect("tree");
        assert_eq!(tree.build_tool, "maven");
        assert_eq!(
            tree.tree.name,
            root.file_name().unwrap().to_str().unwrap()
        );
        assert_eq!(tree.tree.children.len(), 1);
        assert_eq!(tree.tree.children[0].name, "inventory-service");
        assert!(tree.tree.children[0]
            .tasks
            .iter()
            .any(|t| t.command == "spring-boot:run"));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn maven_tree_wins_over_ambient_docker_compose() {
        let tmp = std::env::temp_dir().join(format!("reaper-build-tasks-docker-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let root = &tmp;
        fs::write(
            root.join("pom.xml"),
            r#"<?xml version="1.0" encoding="UTF-8"?>
<project>
  <artifactId>demo</artifactId>
</project>"#,
        )
        .unwrap();
        fs::write(
            root.join("docker-compose.yml"),
            "services:\n  db:\n    image: postgres:16\n",
        )
        .unwrap();

        let tree = build_tasks_tree(root, "pom.xml", None).expect("tree");
        assert_eq!(tree.build_tool, "maven");
        assert!(tree.tree.tasks.iter().any(|t| t.command == "compile"));

        let compose = build_tasks_tree(root, "docker-compose.yml", None).expect("compose tree");
        assert_eq!(compose.build_tool, "docker");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn elide_pkl_wins_over_sibling_pom_when_open() {
        let tmp = std::env::temp_dir().join(format!("reaper-build-tasks-elide-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        fs::write(
            tmp.join("pom.xml"),
            r#"<?xml version="1.0" encoding="UTF-8"?>
<project>
  <artifactId>demo</artifactId>
</project>"#,
        )
        .unwrap();
        fs::write(
            tmp.join("elide.pkl"),
            r#"
amends "elide:project.pkl"
name = "HelloElide"
jvm { main = "com.example.Hello" }
scripts {
  ["pom"] = "elide mvn -- -q package"
  ["cargo"] = "cargo build --release"
}
"#,
        )
        .unwrap();

        let elide = build_tasks_tree(&tmp, "elide.pkl", None).expect("elide");
        assert_eq!(elide.build_tool, "elide");
        assert!(elide
            .tree
            .tasks
            .iter()
            .any(|t| t.command == "elide mvn -- -q package"));

        let maven = build_tasks_tree(&tmp, "pom.xml", None).expect("maven");
        assert_eq!(maven.build_tool, "maven");

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
