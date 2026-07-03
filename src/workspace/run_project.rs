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
        let (project, target) = sql_run_context(ws, &rel_path, database_url, db_ssl)?;
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

fn shell_interpreter_for_content(content: &str) -> &'static str {
    content
        .lines()
        .find(|l| !l.trim().is_empty())
        .and_then(|line| {
            let trimmed = line.trim();
            if !trimmed.starts_with("#!") {
                return None;
            }
            let shebang = trimmed.trim_start_matches("#!").trim();
            if shebang.contains("zsh") {
                Some("zsh")
            } else if shebang.contains("bash") || shebang.contains("/sh") {
                Some("bash")
            } else {
                None
            }
        })
        .unwrap_or("bash")
}

fn sql_run_context(
    ws: &Path,
    rel_path: &str,
    database_url: Option<&str>,
    db_ssl: Option<&crate::repos::metadata::DbSslSettings>,
) -> Result<(RunProjectInfo, JavaRunTarget)> {
    let target = detect_sql_run_target(ws, rel_path, database_url, db_ssl)?;
    let mut project = RunProjectInfo::default();
    let conn = db_viewer::connection_view(ws, database_url, db_ssl);
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
    match db_viewer::sql_run_command(ws, rel_path, database_url, db_ssl) {
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
    if let Some(mc) = main_class.filter(|s| !s.is_empty()) {
        format!("spring-boot:run -Dspring-boot.run.mainClass={mc}")
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
    let task = build_spring_boot_task(ws, &rel_path, &project, main_class.as_deref())?;
    Ok(Some(stream_run_task(ws, &rel_path, &task, false, tx)?))
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
