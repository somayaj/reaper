use std::path::Path;

use anyhow::{Result, bail};
use serde::Serialize;
use tokio::sync::mpsc as async_mpsc;

use super::exec_stream::{self, ExecStreamEvent};
use super::gradle::{self, GradleProjectInfo};
use super::java::{self};
use super::java_ecosystem::{self, JavaFileContext};
use super::maven::{self, MavenProjectInfo};
use super::native_build_tasks;
use super::ruby_nav;
use super::db_viewer;

#[derive(Debug, Clone, Serialize, Default)]
pub struct RunProjectInfo {
    pub has_project: bool,
    /// `gradle` or `maven`
    pub build_tool: String,
    pub project_root: String,
    pub is_spring_boot: bool,
    pub default_task: String,
    pub tasks: Vec<String>,
    pub application_main: Option<String>,
    pub has_wrapper: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub frameworks: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct JavaRunTarget {
    pub runnable: bool,
    /// `none` | `test` | `spring-boot` | `main` | `project-task`
    pub mode: String,
    pub class_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub qualified_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub test_filter: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub frameworks: Vec<String>,
    /// Frameworks or tooling required but missing from the project classpath/build.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub missing: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default)]
    pub ai_assisted: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunContext {
    #[serde(flatten)]
    pub project: RunProjectInfo,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<JavaRunTarget>,
}

pub fn run_project_info(ws: &Path, rel_path: &str) -> Result<RunProjectInfo> {
    let rel_path = super::normalize_workspace_source_path(rel_path);
    let gradle = gradle::gradle_project_info(ws, &rel_path)?;
    if gradle.is_gradle {
        return Ok(from_gradle(gradle));
    }
    let maven = maven::maven_project_info(ws, &rel_path)?;
    if maven.is_maven {
        return Ok(from_maven(maven));
    }
    Ok(RunProjectInfo::default())
}

pub fn run_context(
    ws: &Path,
    rel_path: &str,
    content: Option<&str>,
    line: u32,
    database_url: Option<&str>,
    db_ssl: Option<&crate::repos::metadata::DbSslSettings>,
    db_ssh: Option<&crate::repos::metadata::DbSshTunnelSettings>,
) -> Result<RunContext> {
    let rel_path = super::normalize_workspace_source_path(rel_path);
    if ruby_nav::is_ruby_path(&rel_path) {
        let (project, target) = ruby_run_context(ws, &rel_path)?;
        return Ok(RunContext {
            project,
            target: Some(target),
        });
    }
    if is_python_path(&rel_path) {
        let (project, target) = python_run_context(ws, &rel_path)?;
        return Ok(RunContext {
            project,
            target: Some(target),
        });
    }
    if is_go_path(&rel_path) {
        let (project, target) = go_run_context(ws, &rel_path)?;
        return Ok(RunContext {
            project,
            target: Some(target),
        });
    }
    if native_build_tasks::is_rust_source_path(&rel_path) {
        let content = match content {
            Some(c) => c.to_string(),
            None => super::read_file(ws, &rel_path).unwrap_or_default(),
        };
        let (project, target) = rust_run_context(ws, &rel_path, &content)?;
        return Ok(RunContext {
            project,
            target: Some(target),
        });
    }
    if native_build_tasks::is_js_or_ts_source_path(&rel_path) {
        let (project, target) = js_run_context(ws, &rel_path)?;
        return Ok(RunContext {
            project,
            target: Some(target),
        });
    }
    if native_build_tasks::is_kotlin_source_path(&rel_path) {
        let (project, target) = kotlin_run_context(ws, &rel_path)?;
        return Ok(RunContext {
            project,
            target: Some(target),
        });
    }
    if native_build_tasks::is_php_source_path(&rel_path) {
        let (project, target) = php_run_context(ws, &rel_path)?;
        return Ok(RunContext {
            project,
            target: Some(target),
        });
    }
    if native_build_tasks::is_dart_source_path(&rel_path) {
        let (project, target) = dart_run_context(ws, &rel_path)?;
        return Ok(RunContext {
            project,
            target: Some(target),
        });
    }
    if native_build_tasks::is_scala_source_path(&rel_path) {
        let (project, target) = scala_run_context(ws, &rel_path)?;
        return Ok(RunContext {
            project,
            target: Some(target),
        });
    }
    if native_build_tasks::is_clojure_source_path(&rel_path) {
        let (project, target) = clojure_run_context(ws, &rel_path)?;
        return Ok(RunContext {
            project,
            target: Some(target),
        });
    }
    if native_build_tasks::is_native_source_path(&rel_path) {
        let content = match content {
            Some(c) => c.to_string(),
            None => super::read_file(ws, &rel_path).unwrap_or_default(),
        };
        let (project, target) = native_run_context(ws, &rel_path, &content)?;
        return Ok(RunContext {
            project,
            target: Some(target),
        });
    }
    if is_sql_path(&rel_path) {
        let (project, target) = sql_run_context(ws, &rel_path, database_url, db_ssl, db_ssh)?;
        return Ok(RunContext {
            project,
            target: Some(target),
        });
    }
    if is_shell_path(&rel_path) {
        let (project, target) = shell_run_context(ws, &rel_path)?;
        return Ok(RunContext {
            project,
            target: Some(target),
        });
    }
    let project = run_project_info(ws, &rel_path)?;
    let target = if rel_path.ends_with(".java") {
        let content = match content {
            Some(c) => c.to_string(),
            None => super::read_file(ws, &rel_path)?,
        };
        Some(detect_java_run_target(ws, &rel_path, &content, line, &project)?)
    } else if is_build_file(&rel_path) && project.has_project {
        Some(JavaRunTarget {
            runnable: true,
            mode: "project-task".into(),
            class_type: "build-file".into(),
            task: Some(project.default_task.clone()),
            frameworks: project.frameworks.clone(),
            ..Default::default()
        })
    } else {
        None
    };
    Ok(RunContext {
        project,
        target,
    })
}

pub fn detect_java_run_target(
    ws: &Path,
    rel_path: &str,
    content: &str,
    line: u32,
    project: &RunProjectInfo,
) -> Result<JavaRunTarget> {
    let ctx = java_ecosystem::detect_java_file_context(ws, rel_path, content, line)?;
    let main = java::parse_java_main(content, &super::safe_join(ws, rel_path)?).ok();
    let qualified_name = ctx
        .test_class
        .clone()
        .or_else(|| main.as_ref().map(|m| m.qualified_name.clone()))
        .or_else(|| java_ecosystem::java_fqcn(rel_path, content));

    let mut target = JavaRunTarget {
        class_type: ctx.class_type.clone(),
        frameworks: merge_frameworks(&ctx, project),
        qualified_name,
        ..Default::default()
    };

    match ctx.class_type.as_str() {
        "spring-boot-test" | "junit-test" => {
            target.test_filter = ctx.test_filter.clone().or(ctx.test_class.clone());
            if project.has_project {
                target.mode = "test".into();
                target.runnable = target.test_filter.is_some();
                if !ctx.has_junit && !ctx.has_spring_test {
                    target.missing.push("junit".into());
                    target.reason = Some(
                        "Test class detected but JUnit is not on the project classpath".into(),
                    );
                    target.runnable = false;
                }
            } else {
                target.reason = Some("Tests require a Gradle or Maven project".into());
            }
        }
        "spring-boot-app" => {
            if project.has_project {
                target.mode = "spring-boot".into();
                target.task = Some(build_spring_boot_task(
                    ws,
                    rel_path,
                    project,
                    target.qualified_name.as_deref(),
                )?);
                target.runnable = true;
            } else if main.as_ref().is_some_and(|m| m.runnable) {
                target.mode = "main".into();
                target.runnable = true;
                target.reason = Some(
                    "Spring Boot app outside a Gradle/Maven project — running via java main (limited)".into(),
                );
            } else {
                target.reason = Some(
                    "Spring Boot application requires a Gradle or Maven project".into(),
                );
            }
        }
        "plain-main" => {
            if main.as_ref().is_some_and(|m| m.runnable) {
                target.mode = "main".into();
                target.runnable = true;
            } else {
                target.reason = Some("No runnable public static void main found".into());
            }
        }
        "quarkus-app" => {
            if project.has_project {
                target.mode = "project-task".into();
                target.task = Some(if project.build_tool == "maven" {
                    "quarkus:dev".into()
                } else {
                    "quarkusDev".into()
                });
                target.runnable = project.tasks.iter().any(|t| t.contains("quarkus"));
                if !target.runnable {
                    target.missing.push("quarkus".into());
                    target.reason = Some("Quarkus plugin not detected in this project".into());
                }
            } else {
                target.reason = Some("Quarkus apps require a Gradle or Maven project".into());
            }
        }
        _ => {
            if main.as_ref().is_some_and(|m| m.runnable) {
                target.mode = "main".into();
                target.runnable = true;
                target.class_type = "plain-main".into();
            } else {
                target.reason = Some(format!(
                    "{} is not directly runnable",
                    human_class_type(&ctx.class_type)
                ));
            }
        }
    }

    Ok(target)
}

pub fn apply_ai_run_target(base: &mut JavaRunTarget, ai: &AiRunTargetHint) {
    if ai.class_type.is_empty() {
        return;
    }
    base.class_type = ai.class_type.clone();
    if !ai.mode.is_empty() && ai.mode != "none" {
        base.mode = ai.mode.clone();
    }
    base.runnable = ai.runnable;
    if let Some(q) = &ai.qualified_name {
        base.qualified_name = Some(q.clone());
    }
    if let Some(f) = &ai.test_filter {
        base.test_filter = Some(f.clone());
    }
    if let Some(t) = &ai.task {
        base.task = Some(t.clone());
    }
    if !ai.frameworks.is_empty() {
        base.frameworks = ai.frameworks.clone();
    }
    if let Some(r) = &ai.reason {
        base.reason = Some(r.clone());
    }
    base.ai_assisted = true;
}

pub fn needs_ai_run_classification(target: &JavaRunTarget, content: &str) -> bool {
    if target.runnable || target.ai_assisted {
        return false;
    }
    let suspicious = content.contains("@Test")
        || content.contains("@SpringBootApplication")
        || content.contains("@SpringBootTest")
        || content.contains("@QuarkusMain")
        || java_ecosystem::has_static_main(content);
    if !suspicious {
        return false;
    }
    matches!(
        target.class_type.as_str(),
        "library" | "interface" | "enum" | "record" | "spring-component" | "unknown"
    )
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct AiRunTargetHint {
    #[serde(default)]
    pub class_type: String,
    #[serde(default)]
    pub mode: String,
    #[serde(default)]
    pub runnable: bool,
    #[serde(default)]
    pub qualified_name: Option<String>,
    #[serde(default)]
    pub test_filter: Option<String>,
    #[serde(default)]
    pub task: Option<String>,
    #[serde(default)]
    pub frameworks: Vec<String>,
    #[serde(default)]
    pub reason: Option<String>,
}

fn merge_frameworks(ctx: &JavaFileContext, project: &RunProjectInfo) -> Vec<String> {
    let mut out = project.frameworks.clone();
    for f in &ctx.frameworks {
        if !out.iter().any(|x| x == f) {
            out.push(f.clone());
        }
    }
    out
}

fn human_class_type(class_type: &str) -> &'static str {
    match class_type {
        "spring-boot-app" => "Spring Boot application",
        "spring-boot-test" => "Spring Boot test",
        "junit-test" => "JUnit test",
        "plain-main" => "Java main class",
        "quarkus-app" => "Quarkus application",
        "spring-component" => "Spring component",
        "interface" => "Interface",
        "enum" => "Enum",
        "record" => "Record",
        "build-file" => "Build file",
        _ => "Class",
    }
}

fn is_python_path(rel_path: &str) -> bool {
    let lower = rel_path.to_lowercase();
    lower.ends_with(".py") || lower.ends_with(".pyw")
}

fn is_go_path(rel_path: &str) -> bool {
    rel_path.ends_with(".go")
}

fn is_sql_path(rel_path: &str) -> bool {
    rel_path.to_lowercase().ends_with(".sql")
}

fn is_shell_path(rel_path: &str) -> bool {
    let lower = rel_path.to_lowercase();
    lower.ends_with(".sh") || lower.ends_with(".bash") || lower.ends_with(".zsh")
}

fn shell_run_context(ws: &Path, rel_path: &str) -> Result<(RunProjectInfo, JavaRunTarget)> {
    let target = detect_shell_run_target(ws, rel_path)?;
    Ok((RunProjectInfo::default(), target))
}

fn detect_shell_run_target(ws: &Path, rel_path: &str) -> Result<JavaRunTarget> {
    let rel = super::normalize_workspace_source_path(rel_path);
    let abs = ws.join(&rel);
    if !abs.is_file() {
        return Ok(JavaRunTarget {
            runnable: false,
            mode: "shell".into(),
            class_type: "shell-script".into(),
            reason: Some(format!("Script not found: {rel}")),
            frameworks: vec!["shell".into()],
            ..Default::default()
        });
    }
    let content = std::fs::read_to_string(&abs).unwrap_or_default();
    if content.trim().is_empty() {
        return Ok(JavaRunTarget {
            runnable: false,
            mode: "shell".into(),
            class_type: "shell-script".into(),
            reason: Some("Shell script is empty".into()),
            frameworks: vec!["shell".into()],
            ..Default::default()
        });
    }
    let interpreter = shell_interpreter_for_content(&content);
    Ok(JavaRunTarget {
        runnable: true,
        mode: "shell".into(),
        class_type: "shell-script".into(),
        task: Some(format!("{} {}", interpreter, shell_quote(&rel))),
        frameworks: vec!["shell".into()],
        ..Default::default()
    })
}

fn resolve_shell_program(name: &str) -> String {
    crate::toolchain::resolve_program(name)
        .map(|p| shell_quote(&p.to_string_lossy()))
        .unwrap_or_else(|| name.to_string())
}

fn shell_interpreter_for_content(content: &str) -> String {
    let from_shebang = content
        .lines()
        .find(|l| !l.trim().is_empty())
        .and_then(|line| {
            let trimmed = line.trim();
            if !trimmed.starts_with("#!") {
                return None;
            }
            let shebang = trimmed.trim_start_matches("#!").trim();
            // Use the shebang path directly if it's an absolute path (e.g. #!/usr/bin/env bash)
            if shebang.starts_with("/usr/bin/env ") {
                let prog = shebang.trim_start_matches("/usr/bin/env ").trim();
                return Some(resolve_shell_program(prog));
            }
            if shebang.starts_with('/') {
                // Absolute shebang like #!/bin/bash — use it verbatim
                return Some(shebang.split_whitespace().next().unwrap_or(shebang).to_string());
            }
            if shebang.contains("zsh") {
                Some(resolve_shell_program("zsh"))
            } else if shebang.contains("bash") || shebang.contains("/sh") {
                Some(resolve_shell_program("bash"))
            } else {
                None
            }
        });
    from_shebang.unwrap_or_else(|| resolve_shell_program("bash"))
}

fn sql_run_context(
    ws: &Path,
    rel_path: &str,
    database_url: Option<&str>,
    db_ssl: Option<&crate::repos::metadata::DbSslSettings>,
    db_ssh: Option<&crate::repos::metadata::DbSshTunnelSettings>,
) -> Result<(RunProjectInfo, JavaRunTarget)> {
    let target = detect_sql_run_target(ws, rel_path, database_url, db_ssl, db_ssh)?;
    let mut project = RunProjectInfo::default();
    let conn = db_viewer::connection_view(ws, database_url, db_ssl, db_ssh);
    if conn.connected {
        project.has_project = true;
        project.build_tool = conn.kind.clone();
        project.frameworks = vec!["sql".into()];
    }
    Ok((project, target))
}

fn detect_sql_run_target(
    ws: &Path,
    rel_path: &str,
    database_url: Option<&str>,
    db_ssl: Option<&crate::repos::metadata::DbSslSettings>,
    db_ssh: Option<&crate::repos::metadata::DbSshTunnelSettings>,
) -> Result<JavaRunTarget> {
    let rel = super::normalize_workspace_source_path(rel_path);
    match std::fs::read_to_string(ws.join(&rel)) {
        Ok(content) if content.trim().is_empty() => {
            return Ok(JavaRunTarget {
                runnable: false,
                mode: "sql".into(),
                class_type: "sql-script".into(),
                reason: Some("SQL file is empty".into()),
                frameworks: vec!["sql".into()],
                ..Default::default()
            });
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(JavaRunTarget {
                runnable: false,
                mode: "sql".into(),
                class_type: "sql-script".into(),
                reason: Some(format!("SQL file not found: {rel}")),
                frameworks: vec!["sql".into()],
                ..Default::default()
            });
        }
        Err(_) => {}
        Ok(_) => {}
    }
    match db_viewer::sql_run_command(ws, rel_path, database_url, db_ssl, db_ssh) {
        Ok(command) => Ok(JavaRunTarget {
            runnable: true,
            mode: "sql".into(),
            class_type: "sql-script".into(),
            task: Some(command),
            frameworks: vec!["sql".into()],
            ..Default::default()
        }),
        Err(e) => Ok(JavaRunTarget {
            runnable: false,
            mode: "sql".into(),
            class_type: "sql-script".into(),
            reason: Some(e.to_string()),
            frameworks: vec!["sql".into()],
            ..Default::default()
        }),
    }
}

fn go_run_context(ws: &Path, rel_path: &str) -> Result<(RunProjectInfo, JavaRunTarget)> {
    let target = detect_go_run_target(ws, rel_path)?;
    let mut project = RunProjectInfo::default();
    if let Some(dir) = native_build_tasks::go_module_root(ws, rel_path)? {
        project.has_project = true;
        project.build_tool = "go".into();
        project.project_root = gradle::rel_path_for(ws, &dir)?;
        project.frameworks = target.frameworks.clone();
    }
    Ok((project, target))
}

fn detect_go_run_target(ws: &Path, rel_path: &str) -> Result<JavaRunTarget> {
    let file_name = Path::new(rel_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");
    let is_test = file_name.ends_with("_test.go");
    let command = if is_test {
        native_build_tasks::go_test_command(ws, rel_path)?
    } else {
        native_build_tasks::go_run_file_command(rel_path)
    };
    Ok(JavaRunTarget {
        runnable: true,
        mode: if is_test { "go-test".into() } else { "go".into() },
        class_type: if is_test {
            "go-test".into()
        } else {
            "go-program".into()
        },
        task: Some(command),
        frameworks: vec!["go".into()],
        ..Default::default()
    })
}

fn kotlin_run_context(ws: &Path, rel_path: &str) -> Result<(RunProjectInfo, JavaRunTarget)> {
    let is_test = native_build_tasks::is_kotlin_test_path(rel_path);
    let project_dir = find_gradle_or_maven_root(ws, rel_path)?;
    let mut project = RunProjectInfo::default();
    if let Some(ref dir) = project_dir {
        project.has_project = true;
        project.build_tool = if dir.join("pom.xml").is_file() {
            "maven".into()
        } else {
            "gradle".into()
        };
        project.project_root = gradle::rel_path_for(ws, dir)?;
        project.frameworks = vec!["kotlin".into()];
    }
    let content = super::read_file(ws, rel_path).unwrap_or_default();
    let qualified_name = kotlin_entry_fqcn(rel_path, &content);
    let (mode, class_type, task) = if is_test {
        (
            "kotlin-test",
            "kotlin-test",
            native_build_tasks::kotlin_test_command(project_dir.as_deref()),
        )
    } else {
        (
            "kotlin",
            "kotlin-script",
            native_build_tasks::kotlin_run_command(project_dir.as_deref(), rel_path),
        )
    };
    let target = JavaRunTarget {
        runnable: true,
        mode: mode.into(),
        class_type: class_type.into(),
        task: Some(task),
        frameworks: vec!["kotlin".into()],
        qualified_name,
        ..Default::default()
    };
    Ok((project, target))
}

/// JVM class name for a Kotlin file: `FooKt` for top-level `main`, else named class/object.
fn kotlin_entry_fqcn(rel_path: &str, content: &str) -> Option<String> {
    let stem = Path::new(rel_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())?;
    let package = content.lines().find_map(|line| {
        let trimmed = line.trim();
        trimmed
            .strip_prefix("package ")
            .map(|rest| rest.trim().trim_end_matches(';').trim())
            .filter(|p| !p.is_empty())
            .map(str::to_string)
    });
    let named = content.lines().find_map(|line| {
        let t = line.trim();
        for prefix in ["class ", "object ", "data class ", "enum class "] {
            if let Some(rest) = t.strip_prefix(prefix) {
                let name = rest
                    .split(|c: char| c == '(' || c == ':' || c == '<' || c.is_whitespace())
                    .next()
                    .unwrap_or("")
                    .trim();
                if !name.is_empty() && name.chars().next().is_some_and(|c| c.is_ascii_alphabetic()) {
                    return Some(name.to_string());
                }
            }
        }
        None
    });
    let has_top_level_main = content.lines().any(|line| {
        let t = line.trim_start();
        t.starts_with("fun main(") || t.starts_with("fun main (") || t.starts_with("fun main()")
    });
    let simple = if has_top_level_main && named.is_none() {
        format!("{stem}Kt")
    } else {
        named.unwrap_or_else(|| {
            if has_top_level_main {
                format!("{stem}Kt")
            } else {
                stem.to_string()
            }
        })
    };
    Some(match package {
        Some(pkg) => format!("{pkg}.{simple}"),
        None => simple,
    })
}

fn find_gradle_or_maven_root(ws: &Path, rel_path: &str) -> Result<Option<std::path::PathBuf>> {
    if let Some((dir, _)) =
        native_build_tasks::find_nearest_manifest(ws, rel_path, &["build.gradle", "build.gradle.kts"])?
    {
        return Ok(Some(dir));
    }
    if let Some((dir, _)) = native_build_tasks::find_nearest_manifest(ws, rel_path, &["pom.xml"])? {
        return Ok(Some(dir));
    }
    Ok(None)
}

fn php_run_context(ws: &Path, rel_path: &str) -> Result<(RunProjectInfo, JavaRunTarget)> {
    let is_test = native_build_tasks::is_php_test_path(rel_path);
    let mut project = RunProjectInfo::default();
    if let Some((dir, _)) =
        native_build_tasks::find_nearest_manifest(ws, rel_path, &["composer.json"])?
    {
        project.has_project = true;
        project.build_tool = "composer".into();
        project.project_root = gradle::rel_path_for(ws, &dir)?;
        project.frameworks = vec!["php".into()];
    }
    let (mode, class_type, task) = if is_test {
        (
            "php-test",
            "phpunit",
            native_build_tasks::php_test_command(ws, rel_path),
        )
    } else {
        (
            "php",
            "php-script",
            native_build_tasks::php_run_command(ws, rel_path),
        )
    };
    let target = JavaRunTarget {
        runnable: true,
        mode: mode.into(),
        class_type: class_type.into(),
        task: Some(task),
        frameworks: vec!["php".into()],
        ..Default::default()
    };
    Ok((project, target))
}

fn dart_run_context(ws: &Path, rel_path: &str) -> Result<(RunProjectInfo, JavaRunTarget)> {
    let is_test = native_build_tasks::is_dart_test_path(rel_path);
    let mut project = RunProjectInfo::default();
    if let Some(dir) = native_build_tasks::dart_pubspec_root(ws, rel_path)? {
        project.has_project = true;
        project.build_tool = "dart".into();
        project.project_root = gradle::rel_path_for(ws, &dir)?;
        project.frameworks = vec!["dart".into()];
    }
    let (mode, class_type, task) = if is_test {
        (
            "dart-test",
            "dart-test",
            native_build_tasks::dart_test_command(ws, rel_path),
        )
    } else {
        (
            "dart",
            "dart-script",
            native_build_tasks::dart_run_command(ws, rel_path),
        )
    };
    let target = JavaRunTarget {
        runnable: true,
        mode: mode.into(),
        class_type: class_type.into(),
        task: Some(task),
        frameworks: vec!["dart".into()],
        ..Default::default()
    };
    Ok((project, target))
}

fn scala_run_context(ws: &Path, rel_path: &str) -> Result<(RunProjectInfo, JavaRunTarget)> {
    let is_test = native_build_tasks::is_scala_test_path(rel_path);
    let mut project = RunProjectInfo::default();
    if let Some((dir, _)) =
        native_build_tasks::find_nearest_manifest(ws, rel_path, &["build.sbt", "pom.xml"])?
    {
        project.has_project = true;
        project.build_tool = if dir.join("build.sbt").is_file() {
            "sbt"
        } else {
            "maven"
        }
        .into();
        project.project_root = gradle::rel_path_for(ws, &dir)?;
        project.frameworks = vec!["scala".into()];
    }
    let (mode, class_type, task) = if is_test {
        (
            "scala-test",
            "scala-test",
            native_build_tasks::scala_test_command(),
        )
    } else {
        ("scala", "scala-script", native_build_tasks::scala_run_command(rel_path))
    };
    let target = JavaRunTarget {
        runnable: true,
        mode: mode.into(),
        class_type: class_type.into(),
        task: Some(task),
        frameworks: vec!["scala".into()],
        ..Default::default()
    };
    Ok((project, target))
}

fn clojure_run_context(ws: &Path, rel_path: &str) -> Result<(RunProjectInfo, JavaRunTarget)> {
    let is_test = native_build_tasks::is_clojure_test_path(rel_path);
    let mut project = RunProjectInfo::default();
    if let Some((dir, _)) =
        native_build_tasks::find_nearest_manifest(ws, rel_path, &["project.clj"])?
    {
        project.has_project = true;
        project.build_tool = "lein".into();
        project.project_root = gradle::rel_path_for(ws, &dir)?;
        project.frameworks = vec!["clojure".into()];
    }
    let (mode, class_type, task) = if is_test {
        (
            "clojure-test",
            "clojure-test",
            native_build_tasks::clojure_test_command(),
        )
    } else {
        (
            "clojure",
            "clojure-script",
            native_build_tasks::clojure_run_command(rel_path),
        )
    };
    let target = JavaRunTarget {
        runnable: true,
        mode: mode.into(),
        class_type: class_type.into(),
        task: Some(task),
        frameworks: vec!["clojure".into()],
        ..Default::default()
    };
    Ok((project, target))
}

fn js_run_context(ws: &Path, rel_path: &str) -> Result<(RunProjectInfo, JavaRunTarget)> {
    let target = detect_js_run_target(ws, rel_path)?;
    let mut project = RunProjectInfo::default();
    if let Some(dir) = native_build_tasks::node_project_root(ws, rel_path)? {
        project.has_project = true;
        project.build_tool = "npm".into();
        project.project_root = gradle::rel_path_for(ws, &dir)?;
        project.frameworks = target.frameworks.clone();
    }
    Ok((project, target))
}

fn detect_js_run_target(ws: &Path, rel_path: &str) -> Result<JavaRunTarget> {
    let project_dir = native_build_tasks::node_project_root(ws, rel_path)?;
    let is_test = native_build_tasks::is_js_or_ts_test_path(rel_path);
    let is_ts = native_build_tasks::is_ts_source_path(rel_path);

    let mut frameworks = vec![if is_ts { "typescript" } else { "javascript" }.to_string()];

    if is_test {
        frameworks.push("test".into());
        return Ok(JavaRunTarget {
            runnable: true,
            mode: "js-test".into(),
            class_type: "js-test".into(),
            task: Some(native_build_tasks::js_test_file_command(
                project_dir.as_deref(),
                rel_path,
            )),
            frameworks,
            ..Default::default()
        });
    }

    Ok(JavaRunTarget {
        runnable: true,
        mode: "js".into(),
        class_type: if is_ts { "ts-script" } else { "js-script" }.into(),
        task: Some(native_build_tasks::js_run_file_command(
            project_dir.as_deref(),
            rel_path,
        )),
        frameworks,
        ..Default::default()
    })
}

fn rust_run_context(
    ws: &Path,
    rel_path: &str,
    content: &str,
) -> Result<(RunProjectInfo, JavaRunTarget)> {
    let target = detect_rust_run_target(ws, rel_path, content)?;
    let mut project = RunProjectInfo::default();
    if let Some(dir) = native_build_tasks::cargo_manifest_root(ws, rel_path)? {
        project.has_project = true;
        project.build_tool = "cargo".into();
        project.project_root = gradle::rel_path_for(ws, &dir)?;
        project.frameworks = target.frameworks.clone();
    }
    Ok((project, target))
}

fn rust_has_main(content: &str) -> bool {
    content.contains("fn main(") || content.contains("fn main (")
}

fn rust_has_tests(content: &str) -> bool {
    content.contains("#[test]")
        || content.contains("#[cfg(test)]")
        || content.contains("#[tokio::test]")
        || content.contains("#[test ]")
}

fn detect_rust_run_target(ws: &Path, rel_path: &str, content: &str) -> Result<JavaRunTarget> {
    let normalized = rel_path.replace('\\', "/");
    let has_main = rust_has_main(content);
    let has_tests = rust_has_tests(content);
    let crate_root = native_build_tasks::cargo_manifest_root(ws, rel_path)?;

    if let Some(root) = crate_root {
        let rel_from_root = ws
            .join(rel_path)
            .strip_prefix(&root)
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_else(|_| normalized.clone());
        let is_integration_test = rel_from_root.starts_with("tests/");

        if is_integration_test || (has_tests && !has_main) {
            return Ok(JavaRunTarget {
                runnable: true,
                mode: "rust-test".into(),
                class_type: "cargo-test".into(),
                task: Some(native_build_tasks::cargo_test_command(ws, &root, rel_path)),
                frameworks: vec!["cargo".into()],
                ..Default::default()
            });
        }

        let is_runnable_target = has_main
            || rel_from_root == "src/main.rs"
            || rel_from_root.starts_with("src/bin/")
            || rel_from_root.starts_with("examples/");
        if is_runnable_target {
            return Ok(JavaRunTarget {
                runnable: true,
                mode: "rust".into(),
                class_type: "cargo-run".into(),
                task: Some(native_build_tasks::cargo_run_command(ws, &root, rel_path)),
                frameworks: vec!["cargo".into()],
                ..Default::default()
            });
        }

        if has_tests {
            return Ok(JavaRunTarget {
                runnable: true,
                mode: "rust-test".into(),
                class_type: "cargo-test".into(),
                task: Some(native_build_tasks::cargo_test_command(ws, &root, rel_path)),
                frameworks: vec!["cargo".into()],
                ..Default::default()
            });
        }

        return Ok(JavaRunTarget {
            runnable: false,
            mode: "rust".into(),
            class_type: "rust-source".into(),
            reason: Some(
                "No `fn main` or tests here — run the crate's binary or add #[test]".into(),
            ),
            frameworks: vec!["cargo".into()],
            ..Default::default()
        });
    }

    // Standalone `.rs` file with no Cargo project — compile & run via rustc.
    if has_main {
        return Ok(JavaRunTarget {
            runnable: true,
            mode: "rust".into(),
            class_type: "rust-program".into(),
            task: Some(native_build_tasks::rustc_run_single_file_command(rel_path)),
            frameworks: vec!["rust".into()],
            ..Default::default()
        });
    }
    if has_tests {
        return Ok(JavaRunTarget {
            runnable: true,
            mode: "rust-test".into(),
            class_type: "rustc-test".into(),
            task: Some(native_build_tasks::rustc_test_single_file_command(rel_path)),
            frameworks: vec!["rust".into()],
            ..Default::default()
        });
    }

    Ok(JavaRunTarget {
        runnable: false,
        mode: "rust".into(),
        class_type: "rust-source".into(),
        reason: Some("No `fn main` or #[test] found in this file".into()),
        frameworks: vec!["rust".into()],
        ..Default::default()
    })
}

fn native_run_context(
    ws: &Path,
    rel_path: &str,
    content: &str,
) -> Result<(RunProjectInfo, JavaRunTarget)> {
    let target = detect_native_run_target(ws, rel_path, content)?;
    let mut project = RunProjectInfo::default();
    if let Some((dir, build_tool)) = native_build_tasks::native_project_root(ws, rel_path)? {
        project.has_project = true;
        project.build_tool = build_tool;
        project.project_root = gradle::rel_path_for(ws, &dir)?;
        project.frameworks = target.frameworks.clone();
    }
    Ok((project, target))
}

fn detect_native_run_target(ws: &Path, rel_path: &str, content: &str) -> Result<JavaRunTarget> {
    let normalized = rel_path.replace('\\', "/");
    let file_name = Path::new(rel_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");
    let is_cpp = native_build_tasks::is_cpp_source_path(rel_path);
    let project = native_build_tasks::native_project_root(ws, rel_path)?;

    let has_gtest = content.contains("gtest/gtest.h")
        || content.contains("<gtest/gtest.h>")
        || content.contains("TEST(")
        || content.contains("TEST_F(");
    let has_catch2 = content.contains("catch2/")
        || content.contains("Catch2")
        || content.contains("TEST_CASE(");
    let is_test_path = (file_name.starts_with("test_")
        && (file_name.ends_with(".c") || file_name.ends_with(".cpp")))
        || file_name.ends_with("_test.c")
        || file_name.ends_with("_test.cpp")
        || normalized.contains("/tests/")
        || (normalized.contains("/test/") && (file_name.ends_with(".c") || file_name.ends_with(".cpp")));

    let mut frameworks = Vec::new();
    if is_cpp {
        frameworks.push("cpp".into());
    } else {
        frameworks.push("c".into());
    }

    if has_gtest {
        frameworks.push("gtest".into());
        return Ok(JavaRunTarget {
            runnable: true,
            mode: "native-test".into(),
            class_type: "gtest".into(),
            task: Some(native_build_tasks::native_gtest_command(rel_path, is_cpp)),
            frameworks,
            ..Default::default()
        });
    }

    if has_catch2 {
        frameworks.push("catch2".into());
        return Ok(JavaRunTarget {
            runnable: true,
            mode: "native-test".into(),
            class_type: "catch2".into(),
            task: Some(native_build_tasks::native_catch2_command(rel_path)),
            frameworks,
            ..Default::default()
        });
    }

    if is_test_path {
        if let Some((_, build_tool)) = &project {
            frameworks.push(build_tool.clone());
            let (class_type, command) = match build_tool.as_str() {
                "cmake" => ("cmake-test", native_build_tasks::native_cmake_test_command()),
                "make" => ("make-test", native_build_tasks::native_make_test_command()),
                "meson" => ("meson-test", native_build_tasks::native_meson_test_command()),
                _ => ("native-test", native_build_tasks::native_cmake_test_command()),
            };
            return Ok(JavaRunTarget {
                runnable: true,
                mode: "native-test".into(),
                class_type: class_type.into(),
                task: Some(command),
                frameworks,
                ..Default::default()
            });
        }
        return Ok(JavaRunTarget {
            runnable: false,
            mode: "native-test".into(),
            class_type: "native-test".into(),
            reason: Some(
                "Test file detected — add a CMakeLists.txt, Makefile, or Google Test/Catch2 includes"
                    .into(),
            ),
            frameworks,
            ..Default::default()
        });
    }

    if has_c_main(content) {
        let via_cmake = native_build_tasks::native_run_uses_cmake(ws, rel_path);
        if via_cmake {
            frameworks.push("cmake".into());
        }
        return Ok(JavaRunTarget {
            runnable: true,
            mode: "native".into(),
            class_type: if via_cmake {
                "cmake-run".into()
            } else if is_cpp {
                "cpp-program".into()
            } else {
                "c-program".into()
            },
            task: Some(native_build_tasks::native_run_command(ws, rel_path, is_cpp)),
            frameworks,
            ..Default::default()
        });
    }

    Ok(JavaRunTarget {
        runnable: false,
        mode: "none".into(),
        class_type: if is_cpp {
            "cpp-source".into()
        } else {
            "c-source".into()
        },
        reason: Some("No main() or test framework detected in this file".into()),
        frameworks,
        ..Default::default()
    })
}

fn has_c_main(content: &str) -> bool {
    content.contains("int main(")
        || content.contains("int main (")
        || content.contains("void main(")
        || content.contains("auto main(")
        || content.contains("int main\n")
        || content.contains("int main\r\n")
}

fn python_run_context(ws: &Path, rel_path: &str) -> Result<(RunProjectInfo, JavaRunTarget)> {
    let target = detect_python_run_target(ws, rel_path)?;
    let mut project = RunProjectInfo::default();
    let (project_root, pm) = native_build_tasks::python_package_manager_at(ws, rel_path)?;
    if let Some(dir) = project_root {
        project.has_project = true;
        project.build_tool = pm;
        project.project_root = gradle::rel_path_for(ws, &dir)?;
        project.frameworks = target.frameworks.clone();
    }
    Ok((project, target))
}

fn detect_python_run_target(ws: &Path, rel_path: &str) -> Result<JavaRunTarget> {
    let normalized = rel_path.replace('\\', "/");
    let file_name = Path::new(rel_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");
    let (project_root, pm) = native_build_tasks::python_package_manager_at(ws, rel_path)?;
    let is_django = project_root
        .as_ref()
        .is_some_and(|dir| dir.join("manage.py").is_file());
    let rel_from_root = project_root
        .as_ref()
        .map(|root| rel_path_from_root(ws, root, rel_path))
        .unwrap_or_else(|| rel_path.to_string());

    let is_test = (file_name.starts_with("test_") && file_name.ends_with(".py"))
        || file_name.ends_with("_test.py")
        || normalized.contains("/tests/")
        || (normalized.contains("/test/") && file_name.ends_with(".py"));

    let (mode, class_type, command) = if is_django && is_test {
        (
            "python-test",
            "django-test",
            format!(
                "{} manage.py test {}",
                native_build_tasks::python_interpreter_for_project(project_root.as_deref()),
                shell_quote(&rel_from_root)
            ),
        )
    } else if is_test {
        (
            "python-test",
            "pytest",
            native_build_tasks::python_pytest_command(
                project_root.as_deref(),
                &pm,
                &rel_from_root,
            ),
        )
    } else {
        (
            "python",
            "python-script",
            native_build_tasks::python_run_file_command(project_root.as_deref(), &pm, rel_path),
        )
    };

    let mut frameworks = Vec::new();
    if is_django {
        frameworks.push("django".into());
    }
    if pm != "pip" {
        frameworks.push(pm.clone());
    }

    Ok(JavaRunTarget {
        runnable: true,
        mode: mode.into(),
        class_type: class_type.into(),
        task: Some(command),
        frameworks,
        ..Default::default()
    })
}

fn ruby_run_context(ws: &Path, rel_path: &str) -> Result<(RunProjectInfo, JavaRunTarget)> {
    let target = detect_ruby_run_target(ws, rel_path)?;
    let mut project = RunProjectInfo::default();
    if let Some((dir, _manifest)) =
        native_build_tasks::find_nearest_manifest(ws, rel_path, &["Gemfile"])?
    {
        project.has_project = true;
        project.build_tool = "ruby".into();
        project.project_root = gradle::rel_path_for(ws, &dir)?;
        project.frameworks = target.frameworks.clone();
    }
    Ok((project, target))
}

fn detect_ruby_run_target(ws: &Path, rel_path: &str) -> Result<JavaRunTarget> {
    let normalized = rel_path.replace('\\', "/");
    let file_name = Path::new(rel_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");
    let gem_root = native_build_tasks::find_nearest_manifest(ws, rel_path, &["Gemfile"])?
        .map(|(dir, _)| dir);
    let use_bundle = gem_root.is_some();
    let is_rails = gem_root.as_ref().is_some_and(|dir| {
        dir.join("config/application.rb").is_file() || dir.join("bin/rails").is_file()
    });
    let path_arg = shell_quote(rel_path);
    let rel_from_root = gem_root
        .as_ref()
        .map(|root| rel_path_from_root(ws, root, rel_path))
        .unwrap_or_else(|| rel_path.to_string());

    let (mode, class_type, command) = if file_name.ends_with("_spec.rb")
        || (normalized.contains("/spec/") && file_name.ends_with(".rb"))
    {
        let cmd = if use_bundle {
            format!("bundle exec rspec {}", shell_quote(&rel_from_root))
        } else {
            format!("rspec {}", shell_quote(&rel_from_root))
        };
        ("ruby-test", "rspec", cmd)
    } else if is_rails
        && (file_name.ends_with("_test.rb") || normalized.contains("/test/"))
    {
        (
            "ruby-test",
            "rails-test",
            format!("bin/rails test {}", shell_quote(&rel_from_root)),
        )
    } else {
        let cmd = if use_bundle {
            format!("bundle exec ruby {}", path_arg)
        } else {
            format!("ruby {}", path_arg)
        };
        ("ruby", "ruby-script", cmd)
    };

    let mut frameworks = Vec::new();
    if is_rails {
        frameworks.push("rails".into());
    }
    if use_bundle {
        frameworks.push("bundler".into());
    }

    Ok(JavaRunTarget {
        runnable: true,
        mode: mode.into(),
        class_type: class_type.into(),
        task: Some(command),
        frameworks,
        ..Default::default()
    })
}

fn rel_path_from_root(ws: &Path, root: &Path, rel_path: &str) -> String {
    let abs = ws.join(rel_path);
    abs.strip_prefix(root)
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| rel_path.replace('\\', "/"))
}

fn shell_quote(path: &str) -> String {
    format!("'{}'", path.replace('\'', "'\\''"))
}

fn is_build_file(rel_path: &str) -> bool {
    let base = Path::new(rel_path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    matches!(base, "build.gradle" | "build.gradle.kts" | "pom.xml" | "settings.gradle" | "settings.gradle.kts")
}

fn build_spring_boot_task(
    ws: &Path,
    rel_path: &str,
    project: &RunProjectInfo,
    main_class: Option<&str>,
) -> Result<String> {
    match project.build_tool.as_str() {
        "gradle" => gradle::gradle_boot_run_task(ws, rel_path, main_class),
        "maven" => Ok(maven_spring_boot_run_goal(main_class)),
        other => bail!("unsupported build tool for Spring Boot: {other}"),
    }
}

fn maven_spring_boot_run_goal(main_class: Option<&str>) -> String {
    // Always pass mainClass so multi-module reactors don't guess wrong.
    if let Some(mc) = main_class.filter(|s| !s.is_empty()) {
        format!(
            "spring-boot:run -Dspring-boot.run.mainClass={mc} -Dstart-class={mc}"
        )
    } else {
        "spring-boot:run".into()
    }
}

fn from_gradle(info: GradleProjectInfo) -> RunProjectInfo {
    RunProjectInfo {
        has_project: true,
        build_tool: "gradle".into(),
        project_root: info.project_root,
        is_spring_boot: info.is_spring_boot,
        default_task: info.default_task,
        tasks: info.tasks,
        application_main: info.application_main,
        has_wrapper: info.has_wrapper,
        frameworks: project_frameworks(info.is_spring_boot, info.has_junit, info.has_spring_test, info.has_lombok, info.has_slf4j, info.has_jacoco),
    }
}

fn from_maven(info: MavenProjectInfo) -> RunProjectInfo {
    RunProjectInfo {
        has_project: true,
        build_tool: "maven".into(),
        project_root: info.project_root,
        is_spring_boot: info.is_spring_boot,
        default_task: info.default_goal,
        tasks: info.goals,
        application_main: info.application_main,
        has_wrapper: info.has_wrapper,
        frameworks: project_frameworks(
            info.is_spring_boot,
            info.has_junit,
            info.has_spring_test,
            info.has_lombok,
            info.has_slf4j,
            info.has_jacoco,
        ),
    }
}

fn project_frameworks(
    is_spring_boot: bool,
    junit: bool,
    spring_test: bool,
    lombok: bool,
    slf4j: bool,
    jacoco: bool,
) -> Vec<String> {
    let mut out = Vec::new();
    if is_spring_boot {
        out.push("spring-boot".into());
    }
    if junit {
        out.push("junit".into());
    }
    if spring_test {
        out.push("spring-test".into());
    }
    if lombok {
        out.push("lombok".into());
    }
    if slf4j {
        out.push("slf4j".into());
    }
    if jacoco {
        out.push("jacoco".into());
    }
    out
}

/// When `source` is a `@SpringBootApplication` class inside a Gradle/Maven project, run via bootRun.
pub fn try_stream_spring_boot_main(
    ws: &Path,
    rel_path: &str,
    source: &str,
    tx: async_mpsc::Sender<ExecStreamEvent>,
) -> Result<Option<i32>> {
    if !source.contains("@SpringBootApplication") {
        return Ok(None);
    }
    let rel_path = super::normalize_workspace_source_path(rel_path);
    let project = run_project_info(ws, &rel_path)?;
    if !project.has_project {
        return Ok(None);
    }
    let main_class = java::parse_java_main(source, &super::safe_join(ws, &rel_path)?)
        .ok()
        .map(|m| m.qualified_name);
    // Compile module + upstream deps first (`-am`). spring-boot:run itself must not use
    // `-am` or Maven also runs the goal on packaging=pom parents and fails.
    if project.build_tool == "maven" {
        let compile_code = stream_run_task(ws, &rel_path, "compile", false, tx.clone())?;
        if compile_code != 0 {
            return Ok(Some(compile_code));
        }
    }
    let task = build_spring_boot_task(ws, &rel_path, &project, main_class.as_deref())?;
    Ok(Some(stream_run_task(ws, &rel_path, &task, false, tx)?))
}

/// Run a `main` in a Maven/Gradle project with the wrapper + resolved dependency classpath.
/// Avoids bare `java -cp .reaper/java-out` which misses slf4j / Spring / etc.
pub fn try_stream_build_tool_java_main(
    ws: &Path,
    rel_path: &str,
    source: &str,
    tx: async_mpsc::Sender<ExecStreamEvent>,
) -> Result<Option<i32>> {
    if !source.contains("public static void main") && !source.contains("static void main") {
        return Ok(None);
    }
    let rel_path = super::normalize_workspace_source_path(rel_path);
    let project = run_project_info(ws, &rel_path)?;
    if !project.has_project {
        return Ok(None);
    }
    let main = java::parse_java_main(source, &super::safe_join(ws, &rel_path)?)?;
    match project.build_tool.as_str() {
        "maven" => Ok(Some(stream_maven_java_main(
            ws,
            &rel_path,
            &main.qualified_name,
            source,
            tx,
        )?)),
        "gradle" => Ok(Some(stream_gradle_java_main(
            ws,
            &rel_path,
            &main.qualified_name,
            source,
            tx,
        )?)),
        _ => Ok(None),
    }
}

fn stream_maven_java_main(
    ws: &Path,
    rel_path: &str,
    main_class: &str,
    source: &str,
    tx: async_mpsc::Sender<ExecStreamEvent>,
) -> Result<i32> {
    // Compile with mvnw so Lombok / annotation processors run and deps resolve.
    let compile_code = stream_run_task(ws, rel_path, "compile", false, tx.clone())?;
    if compile_code != 0 {
        return Ok(compile_code);
    }
    let module_root = super::maven::find_maven_root(ws, rel_path)?
        .ok_or_else(|| anyhow::anyhow!("not inside a Maven project"))?;
    let cp = runtime_classpath_string(&module_root, rel_path, source);
    if cp.is_empty() {
        bail!(
            "Maven dependency classpath is empty for {} — wait for indexing or run ./mvnw dependency:resolve",
            module_root.display()
        );
    }
    let _ = emit_run(
        &tx,
        &format!("$ java -cp <resolved Maven classpath> {main_class}\n"),
        "java",
    );
    let mut java = std::process::Command::new("java");
    java.current_dir(&module_root)
        .args(["-cp", &cp, main_class]);
    crate::jdk::apply_java_env(&mut java);
    let code = exec_stream::stream_process(&mut java, &tx)?;
    let _ = tx.blocking_send(ExecStreamEvent {
        t: "exit".into(),
        text: None,
        code: Some(code),
        step: Some("java".into()),
    });
    Ok(code)
}

fn stream_gradle_java_main(
    ws: &Path,
    rel_path: &str,
    main_class: &str,
    source: &str,
    tx: async_mpsc::Sender<ExecStreamEvent>,
) -> Result<i32> {
    let compile_code = stream_run_task(ws, rel_path, "classes", false, tx.clone())?;
    if compile_code != 0 {
        return Ok(compile_code);
    }
    let module_root = super::gradle::find_gradle_root(ws, rel_path)?
        .ok_or_else(|| anyhow::anyhow!("not inside a Gradle project"))?;
    let cp = runtime_classpath_string(&module_root, rel_path, source);
    if cp.is_empty() {
        bail!(
            "Gradle dependency classpath is empty for {} — wait for indexing or run ./gradlew dependencies",
            module_root.display()
        );
    }
    let _ = emit_run(
        &tx,
        &format!("$ java -cp <resolved Gradle classpath> {main_class}\n"),
        "java",
    );
    let mut java = std::process::Command::new("java");
    java.current_dir(&module_root)
        .args(["-cp", &cp, main_class]);
    crate::jdk::apply_java_env(&mut java);
    let code = exec_stream::stream_process(&mut java, &tx)?;
    let _ = tx.blocking_send(ExecStreamEvent {
        t: "exit".into(),
        text: None,
        code: Some(code),
        step: Some("java".into()),
    });
    Ok(code)
}

fn runtime_classpath_string(project_root: &Path, rel_path: &str, source: &str) -> String {
    let entries = super::classpath::resolve_javac_classpath_for_file(project_root, rel_path, source);
    let sep = if cfg!(windows) { ";" } else { ":" };
    entries
        .into_iter()
        .filter(|p| p.exists())
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join(sep)
}

fn emit_run(tx: &async_mpsc::Sender<ExecStreamEvent>, text: &str, step: &str) -> bool {
    tx.blocking_send(ExecStreamEvent {
        t: "stdout".into(),
        text: Some(text.to_string()),
        code: None,
        step: Some(step.into()),
    })
    .is_ok()
}

pub fn stream_run_task(
    ws: &Path,
    rel_path: &str,
    task: &str,
    coverage: bool,
    tx: async_mpsc::Sender<ExecStreamEvent>,
) -> Result<i32> {
    let rel_path = super::normalize_workspace_source_path(rel_path);
    let info = run_project_info(ws, &rel_path)?;
    if !info.has_project {
        bail!("not inside a Gradle or Maven project");
    }
    if coverage {
        let filter = super::coverage::parse_test_filter_from_task(task, &info.build_tool);
        if filter.is_empty() {
            bail!("test filter required for coverage run");
        }
        return super::coverage::stream_test_with_coverage(ws, &rel_path, &filter, tx);
    }
    let task = if task.trim().is_empty() {
        info.default_task
    } else {
        task.trim().to_string()
    };
    match info.build_tool.as_str() {
        "gradle" => exec_stream::stream_gradle(ws, &rel_path, &task, tx),
        "maven" => exec_stream::stream_maven(ws, &rel_path, &task, tx),
        _ => bail!("unsupported build tool"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_ws(name: &str) -> std::path::PathBuf {
        let ws = std::env::temp_dir().join(format!("reaper-run-project-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&ws);
        std::fs::create_dir_all(&ws).unwrap();
        ws
    }

    #[test]
    fn rust_standalone_file_runs_via_rustc() {
        let ws = tmp_ws("rustc-standalone");
        std::fs::write(ws.join("hello.rs"), "fn main() { println!(\"hi\"); }").unwrap();
        let target = detect_rust_run_target(&ws, "hello.rs", "fn main() { println!(\"hi\"); }").unwrap();
        assert_eq!(target.mode, "rust");
        assert!(target.runnable);
        assert!(target.task.unwrap().contains("rustc"));
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn rust_cargo_bin_runs_via_cargo_run() {
        let ws = tmp_ws("cargo-bin");
        std::fs::write(ws.join("Cargo.toml"), "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n").unwrap();
        std::fs::create_dir_all(ws.join("src")).unwrap();
        let content = "fn main() {}";
        std::fs::write(ws.join("src/main.rs"), content).unwrap();
        let target = detect_rust_run_target(&ws, "src/main.rs", content).unwrap();
        assert_eq!(target.mode, "rust");
        assert!(target.runnable);
        assert!(target.task.unwrap().ends_with("cargo run"));
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn rust_cargo_integration_test_runs_via_cargo_test() {
        let ws = tmp_ws("cargo-test");
        std::fs::write(ws.join("Cargo.toml"), "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n").unwrap();
        std::fs::create_dir_all(ws.join("tests")).unwrap();
        let content = "#[test]\nfn it_works() {}";
        std::fs::write(ws.join("tests/it_works.rs"), content).unwrap();
        let target = detect_rust_run_target(&ws, "tests/it_works.rs", content).unwrap();
        assert_eq!(target.mode, "rust-test");
        assert!(target.runnable);
        assert!(target.task.unwrap().ends_with("cargo test --test it_works"));
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn js_plain_script_runs_via_node() {
        let ws = tmp_ws("js-plain");
        let target = detect_js_run_target(&ws, "scripts/build.js").unwrap();
        assert_eq!(target.mode, "js");
        assert!(target.runnable);
        assert!(target.task.unwrap().contains("node"));
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn ts_plain_script_uses_tsx_runner() {
        let ws = tmp_ws("ts-plain");
        let target = detect_js_run_target(&ws, "src/index.ts").unwrap();
        assert_eq!(target.mode, "js");
        assert!(target.task.unwrap().contains("tsx"));
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn js_test_file_detected_as_test_mode() {
        let ws = tmp_ws("js-test");
        let target = detect_js_run_target(&ws, "src/util.test.js").unwrap();
        assert_eq!(target.mode, "js-test");
        assert!(target.runnable);
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn js_test_file_prefers_vitest_when_configured() {
        let ws = tmp_ws("js-vitest");
        std::fs::write(
            ws.join("package.json"),
            r#"{"name":"demo","devDependencies":{"vitest":"^1.0.0"}}"#,
        )
        .unwrap();
        let target = detect_js_run_target(&ws, "src/util.test.ts").unwrap();
        assert_eq!(target.mode, "js-test");
        assert!(target.task.unwrap().contains("vitest run"));
        let _ = std::fs::remove_dir_all(&ws);
    }

    fn run_target(ws: &Path, rel_path: &str, content: Option<&str>) -> JavaRunTarget {
        run_context(ws, rel_path, content, 1, None, None, None)
            .unwrap()
            .target
            .unwrap_or_else(|| panic!("no run target for {rel_path}"))
    }

    fn write_file(ws: &Path, rel_path: &str, content: &str) {
        let path = ws.join(rel_path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, content).unwrap();
    }

    fn assert_runnable_run_target(
        ws: &Path,
        rel_path: &str,
        content: Option<&str>,
        expected_mode: &str,
        task_contains: &str,
    ) {
        let target = run_target(ws, rel_path, content);
        assert_eq!(target.mode, expected_mode, "unexpected mode for {rel_path}");
        assert!(target.runnable, "expected runnable target for {rel_path}: {:?}", target.reason);
        let task = target.task.unwrap_or_else(|| panic!("missing task for {rel_path}"));
        assert!(
            task.contains(task_contains),
            "task for {rel_path} should contain `{task_contains}`, got `{task}`"
        );
    }

    #[test]
    fn run_context_shell_script_is_runnable() {
        let ws = tmp_ws("shell-run");
        write_file(&ws, "scripts/run.sh", "#!/bin/bash\necho hi\n");
        assert_runnable_run_target(&ws, "scripts/run.sh", None, "shell", "bash");
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn run_context_kotlin_script_is_runnable() {
        let ws = tmp_ws("kotlin-run");
        write_file(&ws, "hello.kts", "println(\"hi\")\n");
        assert_runnable_run_target(&ws, "hello.kts", None, "kotlin", "kotlinc");
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn run_context_kotlin_test_in_gradle_project_is_runnable() {
        let ws = tmp_ws("kotlin-test");
        write_file(&ws, "build.gradle.kts", "plugins { kotlin(\"jvm\") version \"1.9.0\" }\n");
        write_file(&ws, "src/test/kotlin/FooTest.kt", "class FooTest {}\n");
        assert_runnable_run_target(&ws, "src/test/kotlin/FooTest.kt", None, "kotlin-test", "gradle test");
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn run_context_dart_script_is_runnable() {
        let ws = tmp_ws("dart-run");
        write_file(&ws, "bin/main.dart", "void main() { print('hi'); }\n");
        assert_runnable_run_target(&ws, "bin/main.dart", None, "dart", "dart");
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn run_context_dart_test_file_is_runnable() {
        let ws = tmp_ws("dart-test");
        write_file(&ws, "test/widget_test.dart", "void main() {}\n");
        assert_runnable_run_target(&ws, "test/widget_test.dart", None, "dart-test", "dart test");
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn run_context_php_script_is_runnable() {
        let ws = tmp_ws("php-run");
        write_file(&ws, "index.php", "<?php echo \"hi\";\n");
        assert_runnable_run_target(&ws, "index.php", None, "php", "php");
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn run_context_php_test_file_is_runnable() {
        let ws = tmp_ws("php-test");
        write_file(&ws, "tests/UserTest.php", "<?php class UserTest {}\n");
        assert_runnable_run_target(&ws, "tests/UserTest.php", None, "php-test", "php");
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn run_context_python_script_is_runnable() {
        let ws = tmp_ws("python-run");
        write_file(&ws, "app.py", "print('hi')\n");
        assert_runnable_run_target(&ws, "app.py", None, "python", "python");
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn run_context_python_test_file_is_runnable() {
        let ws = tmp_ws("python-test");
        write_file(&ws, "tests/test_app.py", "def test_ok():\n    assert True\n");
        assert_runnable_run_target(&ws, "tests/test_app.py", None, "python-test", "pytest");
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn run_context_go_program_is_runnable() {
        let ws = tmp_ws("go-run");
        write_file(&ws, "main.go", "package main\nfunc main() {}\n");
        assert_runnable_run_target(&ws, "main.go", None, "go", "go run");
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn run_context_go_test_file_is_runnable() {
        let ws = tmp_ws("go-test");
        write_file(&ws, "main_test.go", "package main\nimport \"testing\"\nfunc TestMain(t *testing.T) {}\n");
        assert_runnable_run_target(&ws, "main_test.go", None, "go-test", "go test");
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn run_context_ruby_script_is_runnable() {
        let ws = tmp_ws("ruby-run");
        write_file(&ws, "app.rb", "puts 'hi'\n");
        assert_runnable_run_target(&ws, "app.rb", None, "ruby", "ruby");
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn run_context_ruby_spec_is_runnable() {
        let ws = tmp_ws("ruby-spec");
        write_file(&ws, "spec/app_spec.rb", "describe 'app' do\nend\n");
        assert_runnable_run_target(&ws, "spec/app_spec.rb", None, "ruby-test", "rspec");
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn run_context_native_c_main_is_runnable() {
        let ws = tmp_ws("native-c-run");
        let content = "#include <stdio.h>\nint main() { return 0; }\n";
        write_file(&ws, "main.c", content);
        assert_runnable_run_target(&ws, "main.c", Some(content), "native", "clang");
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn run_context_java_main_is_runnable() {
        let ws = tmp_ws("java-run");
        let content = "public class Hello { public static void main(String[] args) {} }\n";
        write_file(&ws, "Hello.java", content);
        let target = run_target(&ws, "Hello.java", Some(content));
        assert_eq!(target.mode, "main");
        assert!(target.runnable);
        assert_eq!(target.qualified_name.as_deref(), Some("Hello"));
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn run_context_sql_file_is_detected() {
        let ws = tmp_ws("sql-run");
        write_file(&ws, "queries/users.sql", "SELECT 1;\n");
        let target = run_target(&ws, "queries/users.sql", None);
        assert_eq!(target.mode, "sql");
        assert_eq!(target.class_type, "sql-script");
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn run_context_covers_all_supported_language_modes() {
        let ws = tmp_ws("all-langs");
        write_file(&ws, "scripts/run.sh", "#!/bin/bash\necho hi\n");
        write_file(&ws, "hello.kts", "println(\"hi\")\n");
        write_file(&ws, "main.dart", "void main() {}\n");
        write_file(&ws, "index.php", "<?php echo \"hi\";\n");
        write_file(&ws, "app.py", "print('hi')\n");
        write_file(&ws, "main.go", "package main\nfunc main() {}\n");
        write_file(&ws, "app.rb", "puts 'hi'\n");
        write_file(&ws, "hello.rs", "fn main() {}\n");
        write_file(&ws, "app.js", "console.log('hi')\n");
        write_file(&ws, "main.c", "int main() { return 0; }\n");

        let cases = [
            ("scripts/run.sh", None, "shell"),
            ("hello.kts", None, "kotlin"),
            ("main.dart", None, "dart"),
            ("index.php", None, "php"),
            ("app.py", None, "python"),
            ("main.go", None, "go"),
            ("app.rb", None, "ruby"),
            ("hello.rs", Some("fn main() {}\n"), "rust"),
            ("app.js", None, "js"),
            ("main.c", Some("int main() { return 0; }\n"), "native"),
        ];

        for (path, content, expected_mode) in cases {
            let target = run_target(&ws, path, content);
            assert_eq!(target.mode, expected_mode, "unexpected mode for {path}");
            assert!(target.runnable, "expected runnable target for {path}: {:?}", target.reason);
            assert!(target.task.is_some(), "missing task for {path}");
        }

        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn kotlin_entry_fqcn_top_level_main() {
        let fqcn = kotlin_entry_fqcn(
            "src/main/kotlin/com/example/App.kt",
            "package com.example\n\nfun main() {\n  println(\"hi\")\n}\n",
        );
        assert_eq!(fqcn.as_deref(), Some("com.example.AppKt"));
    }

    #[test]
    fn kotlin_entry_fqcn_named_class() {
        let fqcn = kotlin_entry_fqcn(
            "src/main/kotlin/com/example/App.kt",
            "package com.example\n\nclass App {\n  companion object {\n    @JvmStatic fun main(args: Array<String>) {}\n  }\n}\n",
        );
        assert_eq!(fqcn.as_deref(), Some("com.example.App"));
    }
}
