use std::fmt::Write;
use std::path::{Path, PathBuf};

use serde::Serialize;

use super::gradle;
use super::java_diagnostics;
use super::languages;

#[derive(Debug, Clone, Serialize, Default)]
pub struct CompilerToolHint {
    pub id: String,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct LanguageCompilerContext {
    pub language: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dialect: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub java_level: Option<u32>,
    pub compilers: Vec<CompilerToolHint>,
    pub rules: Vec<String>,
}

pub fn detect(ws: &Path, path: &str) -> LanguageCompilerContext {
    let language = languages::language_for_path(path)
        .unwrap_or("plaintext")
        .to_string();
    let compilers = compiler_hints_for_path(path);
    let dialect = detect_dialect(ws, path, &language);
    let java_level = if language == "java" {
        Some(java_diagnostics::java_language_level(ws, path))
    } else {
        None
    };
    let rules = build_rules(&language, &dialect, java_level);
    LanguageCompilerContext {
        language,
        dialect,
        java_level,
        compilers,
        rules,
    }
}

pub fn append_to_prompt(out: &mut String, ctx: &LanguageCompilerContext) {
    writeln!(out, "\n--- Compiler / language target ---").ok();
    writeln!(out, "Language: {}", ctx.language).ok();
    if let Some(d) = &ctx.dialect {
        writeln!(out, "Project dialect / target: {d}").ok();
    }
    if let Some(level) = ctx.java_level {
        writeln!(out, "Java language level: {level}").ok();
    }
    if !ctx.compilers.is_empty() {
        writeln!(out, "Configured compilers (effective on PATH):").ok();
        for c in &ctx.compilers {
            let ver = c.version.as_deref().unwrap_or("unknown");
            writeln!(out, "  {} — {ver}", c.id).ok();
        }
    }
    if !ctx.rules.is_empty() {
        writeln!(out, "Syntax rules for completions:").ok();
        for rule in &ctx.rules {
            writeln!(out, "  - {rule}").ok();
        }
    }
    writeln!(
        out,
        "Suggest only syntax and APIs valid for this language version and compiler — no newer features."
    )
    .ok();
}

fn compiler_hints_for_path(path: &str) -> Vec<CompilerToolHint> {
    languages::compiler_tool_ids_for_path(path)
        .into_iter()
        .map(|id| {
            let effective = crate::toolchain::resolve_program(id);
            let version = effective
                .as_ref()
                .and_then(|p| crate::toolchain::tool_version(id, p));
            CompilerToolHint {
                id: id.to_string(),
                version,
            }
        })
        .collect()
}

fn detect_dialect(ws: &Path, path: &str, language: &str) -> Option<String> {
    match language {
        "java" => Some(java_diagnostics::java_language_level(ws, path).to_string()),
        "kotlin" | "groovy" => detect_jvm_dialect(ws, path),
        "rust" => read_cargo_edition(ws, path),
        "go" => read_go_version(ws, path),
        "typescript" => read_tsconfig_dialect(ws, path),
        "javascript" => read_js_dialect(ws, path),
        "python" => read_python_version(ws, path),
        "php" => read_composer_php(ws, path),
        "ruby" => read_ruby_version(ws, path),
        "swift" => read_swift_tools_version(ws, path),
        "csharp" => read_dotnet_target(ws, path),
        "c" | "cpp" => read_cmake_std(ws, path),
        _ => None,
    }
}

fn build_rules(language: &str, dialect: &Option<String>, java_level: Option<u32>) -> Vec<String> {
    let mut rules = Vec::new();
    match language {
        "java" => {
            let level = java_level.unwrap_or(17);
            rules.push(format!("Use only Java {level} syntax and standard-library APIs"));
            if level >= 21 {
                rules.push("Records, pattern matching, sequenced collections OK".into());
            } else if level >= 17 {
                rules.push("var, records, switch expressions OK".into());
            } else if level >= 11 {
                rules.push("var in locals OK; no records".into());
            } else if level >= 10 {
                rules.push("var in local scopes OK".into());
            } else {
                rules.push("No var — use explicit types".into());
            }
            if level < 8 {
                rules.push("No lambdas or streams".into());
            }
        }
        "kotlin" => {
            rules.push("Kotlin JVM — match project JVM target".into());
            if let Some(d) = dialect {
                rules.push(format!("Project: {d}"));
            }
        }
        "groovy" => {
            rules.push("Groovy on JVM — valid Gradle/Groovy syntax only".into());
        }
        "rust" => {
            let edition = dialect.as_deref().unwrap_or("2021");
            rules.push(format!("Rust edition {edition}"));
            if edition == "2015" || edition == "2018" {
                rules.push("No let-else or edition 2021-only features".into());
            }
        }
        "go" => {
            if let Some(v) = dialect {
                rules.push(format!("Go {v} module language version"));
            } else {
                rules.push("Valid Go syntax for module go version".into());
            }
        }
        "typescript" => {
            if let Some(d) = dialect {
                rules.push(format!("TypeScript compiler options: {d}"));
            }
            rules.push("Valid TypeScript types and ES target — no invalid syntax".into());
        }
        "javascript" => {
            if let Some(d) = dialect {
                rules.push(format!("JavaScript environment: {d}"));
            }
            rules.push("Valid ECMAScript for project target — no TypeScript-only syntax".into());
        }
        "python" => {
            if let Some(v) = dialect {
                rules.push(format!("Python {v}"));
            }
            rules.push("Valid Python 3 syntax for project version".into());
        }
        "ruby" => {
            if let Some(v) = dialect {
                rules.push(format!("Ruby {v}"));
            }
            rules.push("Valid Ruby syntax for project version".into());
        }
        "php" => {
            if let Some(v) = dialect {
                rules.push(format!("PHP {v}"));
            }
            rules.push("Valid PHP syntax for project version".into());
        }
        "swift" => {
            if let Some(v) = dialect {
                rules.push(format!("Swift tools {v}"));
            }
        }
        "csharp" => {
            if let Some(t) = dialect {
                rules.push(format!("Target framework {t}"));
            }
        }
        "c" | "cpp" => {
            rules.push("Valid C/C++ for project standard".into());
            if let Some(d) = dialect {
                rules.push(d.clone());
            }
        }
        "shell" => {
            rules.push("POSIX/bash shell — valid for configured bash version".into());
        }
        "sql" => {
            rules.push("Valid SQL for configured database dialect".into());
        }
        _ => {
            if dialect.is_some() {
                rules.push(format!("Project target: {}", dialect.as_deref().unwrap_or("")));
            }
        }
    }
    rules
}

fn walk_search_dirs(ws: &Path, rel_path: &str) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let rel = rel_path.replace('\\', "/");
    let parent = rel.rsplit_once('/').map(|(p, _)| p).unwrap_or("");
    let mut current = if parent.is_empty() {
        ws.to_path_buf()
    } else {
        ws.join(parent)
    };
    loop {
        dirs.push(current.clone());
        if current == ws {
            break;
        }
        if !current.pop() {
            dirs.push(ws.to_path_buf());
            break;
        }
    }
    dirs
}

fn read_file_in_parents(ws: &Path, rel_path: &str, name: &str) -> Option<String> {
    for dir in walk_search_dirs(ws, rel_path) {
        let path = dir.join(name);
        if path.is_file() {
            return std::fs::read_to_string(path).ok();
        }
    }
    None
}

fn read_json_in_parents(ws: &Path, rel_path: &str, name: &str) -> Option<serde_json::Value> {
    read_file_in_parents(ws, rel_path, name)
        .and_then(|t| serde_json::from_str(&t).ok())
}

fn detect_jvm_dialect(ws: &Path, path: &str) -> Option<String> {
    if gradle::find_gradle_root(ws, path).ok().flatten().is_some() {
        let level = java_diagnostics::java_language_level(ws, path);
        return Some(format!("JVM source level {level}"));
    }
    None
}

fn read_cargo_edition(ws: &Path, rel_path: &str) -> Option<String> {
    let text = read_file_in_parents(ws, rel_path, "Cargo.toml")?;
    for line in text.lines() {
        let line = line.split('#').next()?.trim();
        if line.starts_with("edition") {
            if let Some(q) = line.split('"').nth(1) {
                return Some(q.to_string());
            }
        }
    }
    None
}

fn read_go_version(ws: &Path, rel_path: &str) -> Option<String> {
    let text = read_file_in_parents(ws, rel_path, "go.mod")?;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with("go ") {
            return Some(line[3..].trim().to_string());
        }
    }
    None
}

fn read_tsconfig_dialect(ws: &Path, rel_path: &str) -> Option<String> {
    let json = read_json_in_parents(ws, rel_path, "tsconfig.json")?;
    let opts = json.get("compilerOptions").and_then(|v| v.as_object());
    if opts.is_none() {
        return None;
    }
    let opts = opts.unwrap();
    let target = opts.get("target").and_then(|v| v.as_str()).unwrap_or("ES2020");
    let module = opts.get("module").and_then(|v| v.as_str());
    let strict = opts.get("strict").and_then(|v| v.as_bool()).unwrap_or(false);
    let mut parts = vec![format!("target={target}")];
    if let Some(m) = module {
        parts.push(format!("module={m}"));
    }
    if strict {
        parts.push("strict=true".into());
    }
    Some(parts.join(", "))
}

fn read_js_dialect(ws: &Path, rel_path: &str) -> Option<String> {
    if let Some(ts) = read_tsconfig_dialect(ws, rel_path) {
        return Some(format!("from tsconfig: {ts}"));
    }
    let json = read_json_in_parents(ws, rel_path, "package.json")?;
    let obj = json.as_object();
    if let Some(typ) = obj.and_then(|o| o.get("type")).and_then(|v| v.as_str()) {
        return Some(format!("package type={typ}"));
    }
    None
}

fn read_python_version(ws: &Path, rel_path: &str) -> Option<String> {
    if let Some(text) = read_file_in_parents(ws, rel_path, ".python-version") {
        let v = text.lines().next()?.trim();
        if !v.is_empty() {
            return Some(v.to_string());
        }
    }
    let text = read_file_in_parents(ws, rel_path, "pyproject.toml")?;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with("requires-python") {
            if let Some(q) = line.split('"').nth(1) {
                return Some(q.to_string());
            }
        }
    }
    None
}

fn read_composer_php(ws: &Path, rel_path: &str) -> Option<String> {
    let json = read_json_in_parents(ws, rel_path, "composer.json")?;
    json.get("require")
        .and_then(|r| r.get("php"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

fn read_ruby_version(ws: &Path, rel_path: &str) -> Option<String> {
    if let Some(text) = read_file_in_parents(ws, rel_path, ".ruby-version") {
        let v = text.lines().next()?.trim();
        if !v.is_empty() {
            return Some(v.to_string());
        }
    }
    let text = read_file_in_parents(ws, rel_path, "Gemfile")?;
    for line in text.lines() {
        if line.contains("ruby ") {
            if let Some(q) = line.split('"').nth(1) {
                return Some(q.to_string());
            }
        }
    }
    None
}

fn read_swift_tools_version(ws: &Path, rel_path: &str) -> Option<String> {
    let text = read_file_in_parents(ws, rel_path, "Package.swift")?;
    for line in text.lines() {
        if line.contains("swift-tools-version:") {
            let rest = line.split("swift-tools-version:").nth(1)?.trim();
            let ver = rest.split(|c: char| !c.is_ascii_digit() && c != '.')
                .next()
                .unwrap_or(rest);
            return Some(ver.to_string());
        }
    }
    None
}

fn read_dotnet_target(ws: &Path, rel_path: &str) -> Option<String> {
    for dir in walk_search_dirs(ws, rel_path) {
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.ends_with(".csproj") {
                    let text = std::fs::read_to_string(entry.path()).ok()?;
                    for line in text.lines() {
                        if line.contains("TargetFramework") {
                            if let Some(q) = line.split('>').nth(1) {
                                let t = q.split('<').next()?.trim();
                                if !t.is_empty() {
                                    return Some(t.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

fn read_cmake_std(ws: &Path, rel_path: &str) -> Option<String> {
    let text = read_file_in_parents(ws, rel_path, "CMakeLists.txt")?;
    for line in text.lines() {
        if line.contains("CMAKE_CXX_STANDARD") || line.contains("CMAKE_C_STANDARD") {
            return Some(line.trim().to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn cargo_edition_from_parents() {
        let tmp = std::env::temp_dir().join(format!("reaper_lang_ctx_{}", std::process::id()));
        fs::create_dir_all(&tmp).unwrap();
        fs::write(
            tmp.join("Cargo.toml"),
            "[package]\nname = \"x\"\nedition = \"2021\"\n",
        )
        .unwrap();
        let edition = read_cargo_edition(&tmp, "src/main.rs");
        assert_eq!(edition.as_deref(), Some("2021"));
        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn go_version_from_mod() {
        let tmp = std::env::temp_dir().join(format!("reaper_go_ctx_{}", std::process::id()));
        fs::create_dir_all(&tmp).unwrap();
        fs::write(tmp.join("go.mod"), "module example.com/app\n\ngo 1.22\n").unwrap();
        let v = read_go_version(&tmp, "main.go");
        assert_eq!(v.as_deref(), Some("1.22"));
        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn java_rules_respect_level() {
        let rules = build_rules("java", &Some("11".into()), Some(11));
        assert!(rules.iter().any(|r| r.contains("Java 11")));
        assert!(rules.iter().any(|r| r.contains("var in locals")));
    }
}
