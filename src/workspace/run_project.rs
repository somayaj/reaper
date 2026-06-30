use std::path::Path;

use anyhow::{Result, bail};
use serde::Serialize;
use tokio::sync::mpsc as async_mpsc;

use super::exec_stream::{self, ExecStreamEvent};
use super::gradle::{self, GradleProjectInfo};
use super::java::{self};
use super::java_ecosystem::{self, JavaFileContext};
use super::maven::{self, MavenProjectInfo};

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
    let gradle = gradle::gradle_project_info(ws, rel_path)?;
    if gradle.is_gradle {
        return Ok(from_gradle(gradle));
    }
    let maven = maven::maven_project_info(ws, rel_path)?;
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
) -> Result<RunContext> {
    let project = run_project_info(ws, rel_path)?;
    let target = if rel_path.ends_with(".java") {
        let content = match content {
            Some(c) => c.to_string(),
            None => super::read_file(ws, rel_path)?,
        };
        Some(detect_java_run_target(ws, rel_path, &content, line, &project)?)
    } else if is_build_file(rel_path) && project.has_project {
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
            if project.is_spring_boot {
                target.mode = "spring-boot".into();
                target.task = Some(project.default_task.clone());
                target.runnable = true;
            } else if main.as_ref().is_some_and(|m| m.runnable) {
                target.mode = "main".into();
                target.runnable = true;
                target.reason = Some(
                    "Spring Boot app class without Spring Boot plugin — running via java main".into(),
                );
            } else {
                target.reason =
                    Some("Spring Boot application requires a Spring Boot Gradle/Maven project".into());
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

fn is_build_file(rel_path: &str) -> bool {
    let base = Path::new(rel_path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    matches!(base, "build.gradle" | "build.gradle.kts" | "pom.xml" | "settings.gradle" | "settings.gradle.kts")
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

pub fn stream_run_task(
    ws: &Path,
    rel_path: &str,
    task: &str,
    tx: async_mpsc::Sender<ExecStreamEvent>,
) -> Result<i32> {
    let info = run_project_info(ws, rel_path)?;
    if !info.has_project {
        bail!("not inside a Gradle or Maven project");
    }
    let task = if task.trim().is_empty() {
        info.default_task
    } else {
        task.trim().to_string()
    };
    match info.build_tool.as_str() {
        "gradle" => exec_stream::stream_gradle(ws, rel_path, &task, tx),
        "maven" => exec_stream::stream_maven(ws, rel_path, &task, tx),
        _ => bail!("unsupported build tool"),
    }
}
