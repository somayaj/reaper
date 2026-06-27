use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;

/// Language ids align with Monaco / diagnostics (`java`, `typescript`, `shell`, …).
pub const SOURCE_EXTENSIONS: &[&str] = &[
    "java", "kt", "kts", "groovy", "gradle", "rs", "js", "mjs", "cjs", "jsx", "ts", "tsx", "py",
    "pyw", "go", "cs", "rb", "php", "swift", "c", "h", "cpp", "cc", "cxx", "hpp", "hh", "sh",
    "bash", "zsh", "lua", "dart", "sql", "proto", "graphql", "gql", "vue", "svelte", "r",
];

const SKIP_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "build",
    ".gradle",
    ".reaper",
    "dist",
    "out",
    ".idea",
    ".vscode",
    "vendor",
    "webapp",
    "bower_components",
    "tmp",
    "log",
    "storage",
];

pub fn language_for_path(path: &str) -> Option<&'static str> {
    let lower = path.replace('\\', "/").to_lowercase();
    let base = lower.rsplit('/').next()?;

    if base == "dockerfile" || base.starts_with("dockerfile.") {
        return Some("dockerfile");
    }
    if base == "makefile" || base == "gnumakefile" {
        return Some("makefile");
    }
    if base == "cmakelists.txt" {
        return Some("cmake");
    }
    if base.ends_with(".gradle.kts") {
        return Some("kotlin");
    }
    if base.ends_with(".gradle") {
        return Some("groovy");
    }
    if base.ends_with(".gradle.properties") || base.ends_with(".properties") {
        return Some("ini");
    }

    let ext = base.rsplit('.').next()?;
    Some(match ext {
        "rs" => "rust",
        "js" | "mjs" | "cjs" | "jsx" => "javascript",
        "ts" | "tsx" => "typescript",
        "py" | "pyw" => "python",
        "go" => "go",
        "json" | "jsonc" => "json",
        "md" | "mdx" => "markdown",
        "html" | "htm" | "vue" | "svelte" => "html",
        "css" => "css",
        "scss" => "scss",
        "less" => "less",
        "yml" | "yaml" => "yaml",
        "toml" => "toml",
        "sh" | "bash" | "zsh" => "shell",
        "sql" => "sql",
        "xml" => "xml",
        "java" => "java",
        "groovy" | "gvy" | "gy" | "gsh" | "gradle" => "groovy",
        "kt" | "kts" => "kotlin",
        "c" | "h" => "c",
        "cpp" | "cc" | "cxx" | "hpp" | "hh" => "cpp",
        "cs" => "csharp",
        "rb" => "ruby",
        "php" => "php",
        "swift" => "swift",
        "lua" => "lua",
        "r" => "r",
        "dart" => "dart",
        "ini" | "properties" => "ini",
        "dockerfile" => "dockerfile",
        "proto" => "protobuf",
        "graphql" | "gql" => "graphql",
        _ => return None,
    })
}

pub fn is_source_extension(ext: &str) -> bool {
    SOURCE_EXTENSIONS.contains(&ext)
}

pub fn is_indexable_source_path(rel_path: &str) -> bool {
    if rel_path.starts_with(".reaper/") {
        return false;
    }
    let lower = rel_path.to_lowercase();
    if is_vendor_asset(&lower) {
        return false;
    }
    language_for_path(rel_path).is_some()
        || lower.ends_with(".gradle")
        || lower.ends_with(".gradle.kts")
}

pub fn scan_workspace_languages(ws: &Path) -> Result<Vec<String>> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    scan_dir(ws, ws, &mut counts, &mut 0, 50_000)?;
    let mut langs: Vec<(String, usize)> = counts.into_iter().collect();
    langs.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    Ok(langs.into_iter().map(|(lang, _)| lang).collect())
}

fn scan_dir(
    ws: &Path,
    dir: &Path,
    counts: &mut HashMap<String, usize>,
    seen: &mut usize,
    max_files: usize,
) -> Result<()> {
    if *seen >= max_files {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir)? {
        if *seen >= max_files {
            break;
        }
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = entry.path();
        if path.is_dir() {
            if SKIP_DIRS.contains(&name.as_str()) {
                continue;
            }
            scan_dir(ws, &path, counts, seen, max_files)?;
            continue;
        }
        let rel = path
            .strip_prefix(ws)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        if !is_indexable_source_path(&rel) {
            continue;
        }
        if let Some(lang) = language_for_path(&rel) {
            *counts.entry(lang.to_string()).or_default() += 1;
            *seen += 1;
        }
    }
    Ok(())
}

fn is_vendor_asset(lower_path: &str) -> bool {
    lower_path.contains("/vendor/")
        || lower_path.contains("/webapp/")
        || lower_path.contains("/node_modules/")
        || lower_path.contains("jquery")
        || lower_path.ends_with(".min.js")
}

pub fn push_unique(list: &mut Vec<String>, value: &str) {
    if !list.iter().any(|v| v == value) {
        list.push(value.to_string());
    }
}

pub fn merge_languages(into: &mut Vec<String>, from: &[String]) {
    for lang in from {
        push_unique(into, lang);
    }
}

pub fn indexing_label(languages: &[String], frameworks: &[String]) -> String {
    if frameworks.iter().any(|f| f == "spring-boot") {
        return "Spring Boot".into();
    }
    if frameworks.iter().any(|f| f == "rails") {
        return "Rails".into();
    }
    if languages.len() > 2 {
        return format!("{} + {} + …", title_case(&languages[0]), title_case(&languages[1]));
    }
    if languages.len() == 2 {
        return format!("{} + {}", title_case(&languages[0]), title_case(&languages[1]));
    }
    if let Some(lang) = languages.first() {
        return title_case(lang);
    }
    "project".into()
}

fn title_case(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_common_extensions() {
        assert_eq!(language_for_path("src/main.rs"), Some("rust"));
        assert_eq!(language_for_path("app/models/user.rb"), Some("ruby"));
        assert_eq!(language_for_path("main.go"), Some("go"));
        assert_eq!(language_for_path("Dockerfile"), Some("dockerfile"));
    }

    #[test]
    fn scans_mixed_repo() {
        let ws = std::env::temp_dir().join("reaper-lang-scan");
        let _ = std::fs::remove_dir_all(&ws);
        std::fs::create_dir_all(ws.join("src")).unwrap();
        std::fs::write(ws.join("src/main.rs"), "fn main() {}").unwrap();
        std::fs::write(ws.join("lib/util.py"), "def helper(): pass").unwrap();
        let langs = scan_workspace_languages(&ws).unwrap();
        assert!(langs.contains(&"rust".to_string()));
        assert!(langs.contains(&"python".to_string()));
        let _ = std::fs::remove_dir_all(&ws);
    }
}
