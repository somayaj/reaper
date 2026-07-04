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
    /// Project file target (Gradle, tsconfig, etc.) — informational only, not used for completions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dialect: Option<String>,
    /// Settings → Compiler → Java major version (informational).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jdk_level: Option<u32>,
    /// Gradle/Maven declared source/release level.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_java_level: Option<u32>,
    /// max(configured JDK, project release) — drives inline/AI completion syntax.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub java_level: Option<u32>,
    /// Primary configured compiler tool id for this file type.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completion_tool: Option<String>,
    /// `--version` of the configured primary compiler.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completion_version: Option<String>,
    pub compilers: Vec<CompilerToolHint>,
    pub rules: Vec<String>,
}

pub fn detect(ws: &Path, path: &str) -> LanguageCompilerContext {
    let language = languages::language_for_path(path)
        .unwrap_or("plaintext")
        .to_string();
    let compilers = compiler_hints_for_path(path);
    let dialect = detect_project_target(ws, path, &language);
    let jdk_level = if language == "java" {
        Some(java_diagnostics::configured_jdk_major())
    } else {
        None
    };
    let project_java_level = if language == "java" {
        java_diagnostics::project_java_release(ws, path)
    } else {
        None
    };
    let java_level = if language == "java" {
        Some(java_diagnostics::completion_java_level(ws, path))
    } else {
        None
    };
    let (completion_tool, completion_version) = primary_compiler(&compilers);
    let rules = build_rules(&language, &compilers, java_level);
    LanguageCompilerContext {
        language,
        dialect,
        jdk_level,
        project_java_level,
        java_level,
        completion_tool,
        completion_version,
        compilers,
        rules,
    }
}

pub fn append_to_prompt(out: &mut String, ctx: &LanguageCompilerContext) {
    writeln!(out, "\n--- Compiler / language target ---").ok();
    writeln!(out, "Language: {}", ctx.language).ok();
    if let Some(d) = &ctx.dialect {
        writeln!(out, "Project file target (informational): {d}").ok();
    }
    if let Some(level) = ctx.jdk_level {
        writeln!(out, "Configured JDK (Settings → Compiler → Java): {level}").ok();
    }
    if let Some(level) = ctx.project_java_level {
        writeln!(out, "Project source/release level: {level}").ok();
    }
    if let Some(level) = ctx.java_level {
        writeln!(out, "Completion language level: {level} (max of configured JDK and project)").ok();
    }
    if let (Some(tool), Some(ver)) = (&ctx.completion_tool, &ctx.completion_version) {
        writeln!(out, "Completion compiler: {tool} — {ver}").ok();
    }
    if !ctx.compilers.is_empty() {
        writeln!(out, "Configured compilers (Settings → Compiler):").ok();
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
        "Suggest only syntax and APIs valid for the completion language level — not an older JDK alone."
    )
    .ok();
}

fn primary_compiler(compilers: &[CompilerToolHint]) -> (Option<String>, Option<String>) {
    let Some(c) = compilers
        .iter()
        .find(|c| c.version.is_some())
        .or_else(|| compilers.first())
    else {
        return (None, None);
    };
    (Some(c.id.clone()), c.version.clone())
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

fn detect_project_target(ws: &Path, path: &str, language: &str) -> Option<String> {
    match language {
        "java" => java_diagnostics::project_java_release(ws, path).map(|v| v.to_string()),
        "kotlin" | "groovy" => detect_jvm_project_target(ws, path),
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

fn configured_java_from_compilers(compilers: &[CompilerToolHint]) -> Option<u32> {
    compilers
        .iter()
        .find(|c| c.id == "java")
        .and_then(|c| c.version.as_deref())
        .and_then(parse_major_version)
}

fn parse_major_version(version: &str) -> Option<u32> {
    version
        .split(|c: char| !c.is_ascii_digit())
        .find(|s| !s.is_empty())
        .and_then(|s| s.parse().ok())
}

fn build_rules(language: &str, compilers: &[CompilerToolHint], jdk_level: Option<u32>) -> Vec<String> {
    let mut rules = Vec::new();
    let primary = compilers
        .iter()
        .find(|c| c.version.is_some())
        .or(compilers.first());

    match language {
        "java" => {
            let level = jdk_level
                .or_else(|| configured_java_from_compilers(compilers))
                .unwrap_or(17);
            rules.push(format!(
                "Use only Java {level} syntax and standard-library APIs (completion language level)"
            ));
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
            push_configured_compiler_rule(&mut rules, primary, "Kotlin");
            if let Some(jdk) = configured_java_from_compilers(compilers) {
                rules.push(format!("Configured JVM/JDK for Kotlin: Java {jdk}"));
            }
        }
        "groovy" => {
            push_configured_compiler_rule(&mut rules, primary, "Groovy");
            if let Some(jdk) = configured_java_from_compilers(compilers) {
                rules.push(format!("Configured JVM/JDK for Groovy: Java {jdk}"));
            }
        }
        "rust" => push_configured_compiler_rule(&mut rules, primary, "Rust"),
        "go" => push_configured_compiler_rule(&mut rules, primary, "Go"),
        "typescript" => push_configured_compiler_rule(&mut rules, primary, "TypeScript"),
        "javascript" => push_configured_compiler_rule(&mut rules, primary, "JavaScript"),
        "python" => push_configured_compiler_rule(&mut rules, primary, "Python"),
        "ruby" => push_configured_compiler_rule(&mut rules, primary, "Ruby"),
        "php" => push_configured_compiler_rule(&mut rules, primary, "PHP"),
        "swift" => push_configured_compiler_rule(&mut rules, primary, "Swift"),
        "csharp" => push_configured_compiler_rule(&mut rules, primary, "C#"),
        "c" | "cpp" => {
            if let Some(c) = compilers.iter().find(|c| matches!(c.id.as_str(), "clang" | "gcc" | "clangd")) {
                push_configured_compiler_rule(&mut rules, Some(c), "C/C++");
            } else {
                push_configured_compiler_rule(&mut rules, primary, "C/C++");
            }
        }
        "shell" => push_configured_compiler_rule(&mut rules, primary, "Shell"),
        "sql" => push_configured_compiler_rule(&mut rules, primary, "SQL"),
        "lua" => push_configured_compiler_rule(&mut rules, primary, "Lua"),
        "dart" => push_configured_compiler_rule(&mut rules, primary, "Dart"),
        _ => push_configured_compiler_rule(&mut rules, primary, language),
    }
    rules
}

fn push_configured_compiler_rule(
    rules: &mut Vec<String>,
    primary: Option<&CompilerToolHint>,
    label: &str,
) {
    if let Some(c) = primary {
        if let Some(v) = &c.version {
            rules.push(format!(
                "Use only {label} syntax and APIs supported by configured {} ({v})",
                c.id
            ));
            return;
        }
        rules.push(format!(
            "Use only {label} syntax valid for configured compiler {}",
            c.id
        ));
        return;
    }
    rules.push(format!("Use only valid {label} syntax for configured compiler"));
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

fn detect_jvm_project_target(ws: &Path, path: &str) -> Option<String> {
    if gradle::find_gradle_root(ws, path).ok().flatten().is_some() {
        return java_diagnostics::project_java_release(ws, path)
            .map(|v| format!("JVM project source {v}"));
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
    fn java_rules_use_configured_jdk_not_project() {
        let rules = build_rules("java", &[], Some(21));
        assert!(rules.iter().any(|r| r.contains("Java 21")));
        assert!(rules.iter().any(|r| r.contains("configured compiler JDK")));
        assert!(rules.iter().any(|r| r.contains("sequenced collections")));
        let rules17 = build_rules("java", &[], Some(17));
        assert!(rules17.iter().any(|r| r.contains("Java 17")));
        assert!(!rules17.iter().any(|r| r.contains("sequenced collections")));
    }

    #[test]
    fn python_rules_use_configured_compiler_not_project() {
        let compilers = vec![CompilerToolHint {
            id: "python".into(),
            version: Some("Python 3.12.1".into()),
        }];
        let rules = build_rules("python", &compilers, None);
        assert!(rules.iter().any(|r| r.contains("configured python")));
        assert!(rules.iter().any(|r| r.contains("3.12.1")));
    }

    #[test]
    fn typescript_rules_use_configured_tsc() {
        let compilers = vec![CompilerToolHint {
            id: "tsc".into(),
            version: Some("Version 5.4.5".into()),
        }];
        let rules = build_rules("typescript", &compilers, None);
        assert!(rules.iter().any(|r| r.contains("configured tsc")));
    }

    #[test]
    fn kotlin_rules_include_configured_jdk() {
        let compilers = vec![
            CompilerToolHint {
                id: "kotlin".into(),
                version: Some("Kotlin 2.0.0".into()),
            },
            CompilerToolHint {
                id: "java".into(),
                version: Some("21.0.2".into()),
            },
        ];
        let rules = build_rules("kotlin", &compilers, None);
        assert!(rules.iter().any(|r| r.contains("configured kotlin")));
        assert!(rules.iter().any(|r| r.contains("Java 21")));
    }

    #[test]
    fn parse_major_version_handles_common_strings() {
        assert_eq!(parse_major_version("21.0.2"), Some(21));
        assert_eq!(parse_major_version("Python 3.12.1"), Some(3));
        assert_eq!(parse_major_version("openjdk version \"17.0.9\""), Some(17));
    }
}
