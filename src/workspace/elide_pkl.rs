//! Lightweight `elide.pkl` parsing for Package Manifest + Build Tasks panels.
//! Full Pkl evaluation is not required — we extract the fields that drive UI.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default)]
pub struct ElidePklInfo {
    pub name: Option<String>,
    pub version: Option<String>,
    pub description: Option<String>,
    pub jvm_main: Option<String>,
    /// (source-set name, type) — type is `"test"` or empty for normal sources.
    pub source_sets: Vec<(String, String)>,
    pub maven_packages: Vec<String>,
    pub maven_test_packages: Vec<String>,
    /// (artifact id, constructor kind e.g. `Jvm.Jar`)
    pub artifacts: Vec<(String, String)>,
    /// (script name, command)
    pub scripts: Vec<(String, String)>,
}

pub fn is_elide_manifest_path(rel_path: &str) -> bool {
    let base = rel_path
        .replace('\\', "/")
        .rsplit('/')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    base == "elide.pkl"
}

pub fn parse_elide_pkl(text: &str) -> ElidePklInfo {
    let mut info = ElidePklInfo::default();
    info.name = top_level_string(text, "name");
    info.version = top_level_string(text, "version");
    info.description = top_level_string(text, "description");
    info.jvm_main = block_string(text, "jvm", "main");
    info.source_sets = parse_source_sets(text);
    info.maven_packages = parse_maven_list(text, "packages");
    info.maven_test_packages = parse_maven_list(text, "testPackages");
    info.artifacts = parse_artifacts(text);
    info.scripts = parse_scripts(text);
    info
}

fn top_level_string(text: &str, key: &str) -> Option<String> {
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("//") {
            continue;
        }
        let Some(rest) = trimmed.strip_prefix(key) else {
            continue;
        };
        let rest = rest.trim_start();
        if !rest.starts_with('=') {
            continue;
        }
        return parse_quoted(rest.trim_start_matches('=').trim_start());
    }
    None
}

fn block_string(text: &str, block: &str, key: &str) -> Option<String> {
    let body = extract_block(text, block)?;
    top_level_string(&body, key)
}

fn extract_block(text: &str, name: &str) -> Option<String> {
    let marker = format!("{name} {{");
    let start = text.find(&marker)?;
    let after = start + marker.len();
    let rest = &text[after..];
    let mut depth = 1usize;
    for (i, ch) in rest.char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(rest[..i].to_string());
                }
            }
            _ => {}
        }
    }
    None
}

fn parse_quoted(s: &str) -> Option<String> {
    let s = s.trim();
    if !s.starts_with('"') {
        return None;
    }
    let end = s[1..].find('"')?;
    Some(s[1..1 + end].to_string())
}

fn parse_bracket_key(s: &str) -> Option<&str> {
    let s = s.trim();
    let rest = s.strip_prefix('[')?;
    let rest = rest.trim_start().strip_prefix('"')?;
    let end = rest.find('"')?;
    Some(&rest[..end])
}

fn parse_source_sets(text: &str) -> Vec<(String, String)> {
    let Some(body) = extract_block(text, "sources") else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let lines: Vec<&str> = body.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let trimmed = lines[i].trim();
        if let Some(name) = parse_bracket_key(trimmed) {
            if trimmed.contains('=') {
                let window = lines[i..].iter().take(12).copied().collect::<Vec<_>>().join("\n");
                let kind = if window.lines().any(|l| {
                    let t = l.trim();
                    t.starts_with("type") && t.contains("\"test\"")
                }) {
                    "test".into()
                } else {
                    String::new()
                };
                out.push((name.to_string(), kind));
            }
        }
        i += 1;
    }
    out
}

fn parse_maven_list(text: &str, list_name: &str) -> Vec<String> {
    let Some(deps) = extract_block(text, "dependencies") else {
        return Vec::new();
    };
    let Some(maven) = extract_block(&deps, "maven") else {
        return Vec::new();
    };
    let Some(list) = extract_block(&maven, list_name) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for line in list.lines() {
        let t = line.trim();
        if let Some(q) = parse_quoted(t.trim_end_matches(',')) {
            if q.matches(':').count() >= 2 {
                out.push(q);
            }
        }
    }
    out
}

fn parse_artifacts(text: &str) -> Vec<(String, String)> {
    let Some(body) = extract_block(text, "artifacts") else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for line in body.lines() {
        let trimmed = line.trim();
        let Some(name) = parse_bracket_key(trimmed) else {
            continue;
        };
        let Some(eq) = trimmed.find('=') else {
            continue;
        };
        let rhs = trimmed[eq + 1..].trim();
        let kind = if let Some(rest) = rhs.strip_prefix("new ") {
            rest.split_whitespace().next().unwrap_or("Artifact").to_string()
        } else {
            "Artifact".into()
        };
        out.push((name.to_string(), kind));
    }
    out
}

fn parse_scripts(text: &str) -> Vec<(String, String)> {
    let Some(body) = extract_block(text, "scripts") else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("//") || trimmed.is_empty() {
            continue;
        }
        let Some(name) = parse_bracket_key(trimmed) else {
            continue;
        };
        let Some(eq) = trimmed.find('=') else {
            continue;
        };
        let Some(cmd) = parse_quoted(trimmed[eq + 1..].trim()) else {
            continue;
        };
        out.push((name.to_string(), cmd));
    }
    out
}

pub fn script_group(name: &str) -> &'static str {
    let lower = name.to_ascii_lowercase();
    // Ecosystem bridges first (pom:run / gradle:test / cargo:run).
    if lower.starts_with("pom") {
        return "maven";
    }
    if lower.starts_with("gradle") {
        return "gradle";
    }
    if lower.starts_with("cargo") {
        return "cargo";
    }
    if lower.contains("test") || lower.contains("check") || lower.contains("lint") {
        return "verification";
    }
    if lower == "run" || lower.ends_with(":run") || lower.contains("serve") || lower.contains("dev")
    {
        return "application";
    }
    if lower == "build"
        || lower == "install"
        || lower.contains("package")
        || lower.contains("compile")
    {
        return "lifecycle";
    }
    "scripts"
}

/// Native Elide build targets inferred from manifest shape (mirrors `elide build --inspect`).
pub fn inferred_native_tasks(info: &ElidePklInfo) -> Vec<(String, String, &'static str)> {
    let mut tasks = Vec::new();
    let has_main_sources = !info.source_sets.is_empty()
        && info
            .source_sets
            .iter()
            .any(|(n, t)| n == "main" || t.is_empty());
    let has_test_sources = info.source_sets.iter().any(|(_, t)| t == "test")
        || !info.maven_test_packages.is_empty();
    let has_maven = !info.maven_packages.is_empty() || !info.maven_test_packages.is_empty();

    if has_maven {
        tasks.push((
            "maven-dependencies".into(),
            "elide build :maven-dependencies".into(),
            "lifecycle",
        ));
    }
    if has_main_sources {
        tasks.push((
            "compile-java-main".into(),
            "elide build :compile-java-main".into(),
            "lifecycle",
        ));
    }
    if has_test_sources {
        tasks.push((
            "compile-java-test".into(),
            "elide build :compile-java-test".into(),
            "lifecycle",
        ));
        tasks.push(("jvm-test".into(), "elide build :jvm-test".into(), "verification"));
    }
    if info.jvm_main.is_some() {
        tasks.push(("run".into(), "elide build :run".into(), "application"));
    }
    if !info.artifacts.is_empty() {
        tasks.push(("build".into(), "elide build".into(), "lifecycle"));
    }
    tasks.push(("install".into(), "elide install".into(), "lifecycle"));
    tasks
}

/// Prefer `elide build --inspect` when the CLI is available; otherwise inferred targets.
pub fn inspect_or_infer_native_tasks(
    project_dir: &Path,
    info: &ElidePklInfo,
) -> Vec<(String, String, &'static str)> {
    if let Some(tasks) = try_elide_inspect(project_dir) {
        if !tasks.is_empty() {
            return tasks;
        }
    }
    inferred_native_tasks(info)
}

fn try_elide_inspect(project_dir: &Path) -> Option<Vec<(String, String, &'static str)>> {
    let elide = which_elide()?;
    let output = std::process::Command::new(&elide)
        .args(["build", "--inspect", "--no-color"])
        .current_dir(project_dir)
        .output()
        .ok()?;
    if !output.status.success() && output.stdout.is_empty() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut tasks = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim_start();
        if !trimmed.starts_with(':') {
            continue;
        }
        let mut parts = trimmed.splitn(2, char::is_whitespace);
        let id = parts.next()?.trim();
        if !id.starts_with(':') || id.len() < 2 {
            continue;
        }
        let rest = parts.next().unwrap_or("").trim();
        if rest.is_empty() {
            continue;
        }
        let group = if id.contains("test") {
            "verification"
        } else if id == ":run" || id.ends_with(":run") {
            "application"
        } else {
            "lifecycle"
        };
        let cmd = format!("elide build {id}");
        tasks.push((id.trim_start_matches(':').to_string(), cmd, group));
    }
    if tasks.is_empty() {
        None
    } else {
        Some(tasks)
    }
}

/// Resolve Elide binary: Settings → Compiler `elide`, `REAPER_ELIDE` / `ELIDE_BIN`, then PATH / well-known installs.
pub fn which_elide() -> Option<PathBuf> {
    crate::toolchain::resolve_program("elide")
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
amends "elide:project.pkl"
name = "HelloElide Project"
version = "1.0.0-SNAPSHOT"
description = "A simple Maven Hello World application"
jvm {
  main = "com.example.Hello"
}
sources {
  ["main"] = new Sources.SourceSetSpec {
    paths { "src/main/java/**/*.java" }
  }
  ["test"] = new Sources.SourceSetSpec {
    type = "test"
    paths { "src/test/**/*.java" }
  }
}
dependencies {
  maven {
    testPackages {
      "org.junit.jupiter:junit-jupiter:5.11.4"
    }
  }
}
artifacts {
  ["app"] = new Jvm.Jar {
    name = "hello-world"
  }
}
scripts {
  ["build"] = "elide build"
  ["pom"] = "elide mvn -- -q package"
  ["gradle"] = "./gradlew build"
  ["cargo"] = "cargo build --release"
}
"#;

    #[test]
    fn parses_hello_world_elide_pkl() {
        let info = parse_elide_pkl(SAMPLE);
        assert_eq!(info.name.as_deref(), Some("HelloElide Project"));
        assert_eq!(info.version.as_deref(), Some("1.0.0-SNAPSHOT"));
        assert_eq!(info.jvm_main.as_deref(), Some("com.example.Hello"));
        assert!(info.source_sets.iter().any(|(n, _)| n == "main"));
        assert!(info
            .source_sets
            .iter()
            .any(|(n, t)| n == "test" && t == "test"));
        assert_eq!(
            info.maven_test_packages,
            vec!["org.junit.jupiter:junit-jupiter:5.11.4".to_string()]
        );
        assert!(info
            .artifacts
            .iter()
            .any(|(id, kind)| id == "app" && kind.contains("Jar")));
        assert_eq!(info.scripts.len(), 4);
        assert_eq!(info.scripts[0].0, "build");
        assert_eq!(script_group("pom"), "maven");
        assert_eq!(script_group("gradle:run"), "gradle");
        assert_eq!(script_group("cargo:test"), "cargo");
    }

    #[test]
    fn is_elide_manifest_path_detects_elide_pkl() {
        assert!(is_elide_manifest_path("elide.pkl"));
        assert!(is_elide_manifest_path("apps/demo/elide.pkl"));
        assert!(!is_elide_manifest_path("pom.xml"));
        assert!(!is_elide_manifest_path("elide.toml"));
    }

    #[test]
    fn inferred_native_tasks_cover_jvm_and_maven() {
        let info = parse_elide_pkl(SAMPLE);
        let tasks = inferred_native_tasks(&info);
        let ids: Vec<_> = tasks.iter().map(|(id, _, _)| id.as_str()).collect();
        assert!(ids.contains(&"maven-dependencies"));
        assert!(ids.contains(&"compile-java-main"));
        assert!(ids.contains(&"compile-java-test"));
        assert!(ids.contains(&"jvm-test"));
        assert!(ids.contains(&"run"));
        assert!(ids.contains(&"build"));
        assert!(ids.contains(&"install"));
        assert!(tasks
            .iter()
            .any(|(id, cmd, _)| id == "run" && cmd == "elide build :run"));
    }

    #[test]
    fn script_groups_bridge_ecosystems() {
        assert_eq!(script_group("pom:run"), "maven");
        assert_eq!(script_group("gradle"), "gradle");
        assert_eq!(script_group("cargo:run"), "cargo");
        assert_eq!(script_group("build"), "lifecycle");
        assert_eq!(script_group("test"), "verification");
        assert_eq!(script_group("run"), "application");
        assert_eq!(script_group("hello"), "scripts");
    }
}
