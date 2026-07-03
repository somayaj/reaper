use std::path::Path;

/// Production source roots (Gradle/Maven standard layouts).
pub const MAIN_SUFFIXES: &[&str] = &[
    "src/main/java",
    "src/main/kotlin",
    "src/main/groovy",
    "src/main/scala",
];

/// Test and auxiliary source sets.
pub const TEST_SUFFIXES: &[&str] = &[
    "src/test/java",
    "src/test/kotlin",
    "src/test/groovy",
    "src/integrationTest/java",
    "src/integrationTest/kotlin",
    "src/intTest/java",
    "src/intTest/kotlin",
    "src/testFixtures/java",
    "src/testFixtures/kotlin",
    "src/androidTest/java",
    "src/androidTest/kotlin",
    "src/unitTest/java",
    "src/functionalTest/java",
    "src/nativeTest/java",
];

/// Generated sources (Gradle/Maven output trees).
pub const GENERATED_SUFFIXES: &[&str] = &[
    "build/generated/sources",
    "build/generated/source",
    "target/generated-sources",
    "target/generated-test-sources",
];

fn suffix_kind(suffix: &str) -> &'static str {
    if MAIN_SUFFIXES.contains(&suffix) {
        "main"
    } else if TEST_SUFFIXES.contains(&suffix) {
        "test"
    } else {
        "generated"
    }
}

/// All relative suffixes scanned when discovering Java/Kotlin project sources.
pub fn discovery_suffixes() -> impl Iterator<Item = &'static str> {
    MAIN_SUFFIXES
        .iter()
        .chain(TEST_SUFFIXES.iter())
        .chain(GENERATED_SUFFIXES.iter())
        .copied()
}

/// Every recognized source root under `project_root` (multi-module safe).
pub fn discover_source_prefixes(project_root: &Path) -> Vec<String> {
    let mut prefixes = Vec::new();
    discover_source_prefixes_inner(project_root, project_root, &mut prefixes, 0);
    prefixes.sort();
    prefixes.dedup();
    prefixes
}

fn discover_source_prefixes_inner(
    project_root: &Path,
    dir: &Path,
    out: &mut Vec<String>,
    depth: usize,
) {
    if depth > 12 || !dir.is_dir() {
        return;
    }
    for suffix in discovery_suffixes() {
        push_prefix_if_dir(project_root, dir, suffix, out);
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = entry.file_name();
        if name == "build" || name == "target" {
            discover_generated_under(project_root, &path, out);
            continue;
        }
        if name == ".gradle"
            || name == "node_modules"
            || name == ".git"
            || name == ".reaper"
        {
            continue;
        }
        discover_source_prefixes_inner(project_root, &path, out, depth + 1);
    }
}

fn discover_generated_under(project_root: &Path, dir: &Path, out: &mut Vec<String>) {
    let name = dir.file_name().and_then(|n| n.to_str());
    let rel_suffixes: &[&str] = match name {
        Some("build") => &["generated/sources", "generated/source"],
        Some("target") => &["generated-sources", "generated-test-sources"],
        _ => return,
    };
    for rel in rel_suffixes {
        push_prefix_if_dir(project_root, dir, rel, out);
    }
}

fn push_prefix_if_dir(project_root: &Path, base: &Path, suffix: &str, out: &mut Vec<String>) {
    let full = base.join(suffix);
    if !full.is_dir() {
        return;
    }
    let rel = full
        .strip_prefix(project_root)
        .unwrap_or(&full)
        .to_string_lossy()
        .replace('\\', "/");
    out.push(rel);
}

/// Classify a directory path in the workspace tree (`main`, `test`, or `generated`).
pub fn source_root_kind(rel_path: &str) -> Option<&'static str> {
    classify_source_path(rel_path)
}

/// Classify any workspace-relative path (directory or file under a source root).
pub fn source_kind_for_path(rel_path: &str) -> Option<&'static str> {
    classify_source_path(rel_path)
}

/// Gradle/Maven source-set directory segments under `src/`.
const MAIN_SRC_SEGMENTS: &[&str] = &["src/main"];

const TEST_SRC_SEGMENTS: &[&str] = &[
    "src/test",
    "src/integrationTest",
    "src/intTest",
    "src/testFixtures",
    "src/androidTest",
    "src/unitTest",
    "src/functionalTest",
    "src/nativeTest",
];

fn path_under_src_segment(path: &str, segment: &str) -> bool {
    path == segment
        || path.ends_with(&format!("/{segment}"))
        || path.contains(&format!("/{segment}/"))
}

fn classify_source_path(rel_path: &str) -> Option<&'static str> {
    let p = rel_path.replace('\\', "/");
    let p = p.trim_end_matches('/');
    for suffix in discovery_suffixes() {
        if path_matches_suffix(p, suffix) {
            return Some(suffix_kind(suffix));
        }
    }
    let mut best: Option<(&str, usize)> = None;
    for suffix in discovery_suffixes() {
        let marker = format!("{suffix}/");
        if let Some(idx) = p.find(&marker) {
            let end = idx + suffix.len();
            if best.map(|(_, len)| end > len).unwrap_or(true) {
                best = Some((suffix, end));
            }
        }
    }
    if let Some((suffix, _)) = best {
        return Some(suffix_kind(suffix));
    }
    for segment in TEST_SRC_SEGMENTS {
        if path_under_src_segment(p, segment) {
            return Some("test");
        }
    }
    for segment in MAIN_SRC_SEGMENTS {
        if path_under_src_segment(p, segment) {
            return Some("main");
        }
    }
    if p.ends_with("/src") && !p.contains("/src/main") && !p.contains("/src/test") {
        return Some("main");
    }
    None
}

/// Source root prefix for a file path (e.g. `app/src/test/java` for `app/src/test/java/Foo.java`).
pub fn detect_file_source_prefix(rel_path: &str) -> Option<String> {
    let p = rel_path.replace('\\', "/");
    let mut best: Option<(usize, String)> = None;
    for suffix in discovery_suffixes() {
        let marker = format!("{suffix}/");
        if let Some(idx) = p.find(&marker) {
            let end = idx + suffix.len();
            let prefix = p[..end].trim_end_matches('/').to_string();
            match &best {
                Some((len, _)) if *len >= prefix.len() => {}
                _ => best = Some((prefix.len(), prefix)),
            }
        }
    }
    best.map(|(_, prefix)| prefix)
}

fn path_matches_suffix(path: &str, suffix: &str) -> bool {
    path == suffix || path.ends_with(&format!("/{suffix}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovers_kotlin_and_integration_test_roots() {
        let root = std::env::temp_dir().join("reaper-java-sources-kotlin");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("app/src/main/kotlin")).unwrap();
        std::fs::create_dir_all(root.join("app/src/test/kotlin")).unwrap();
        std::fs::create_dir_all(root.join("service/src/integrationTest/java")).unwrap();
        let prefixes = discover_source_prefixes(&root);
        assert!(prefixes.contains(&"app/src/main/kotlin".to_string()));
        assert!(prefixes.contains(&"app/src/test/kotlin".to_string()));
        assert!(prefixes.contains(&"service/src/integrationTest/java".to_string()));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn classifies_source_root_kinds() {
        assert_eq!(source_root_kind("module/src/main/java"), Some("main"));
        assert_eq!(source_root_kind("module/src/main"), Some("main"));
        assert_eq!(source_root_kind("app/src/main"), Some("main"));
        assert_eq!(source_root_kind("module/src/test/java"), Some("test"));
        assert_eq!(source_root_kind("module/src/test"), Some("test"));
        assert_eq!(source_root_kind("app/src/test"), Some("test"));
        assert_eq!(
            source_root_kind("module/src/integrationTest/java"),
            Some("test")
        );
        assert_eq!(
            source_root_kind("module/build/generated/sources/annotationProcessor"),
            Some("generated")
        );
    }

    #[test]
    fn classifies_files_under_source_roots() {
        assert_eq!(
            source_kind_for_path("app/src/test/java/com/example/AppTest.java"),
            Some("test")
        );
        assert_eq!(
            source_kind_for_path("src/main/kotlin/com/example/App.kt"),
            Some("main")
        );
    }

    #[test]
    fn detects_file_source_prefix() {
        assert_eq!(
            detect_file_source_prefix("app/src/test/java/com/example/AppTest.java").as_deref(),
            Some("app/src/test/java")
        );
        assert_eq!(
            detect_file_source_prefix("src/main/kotlin/com/example/App.kt").as_deref(),
            Some("src/main/kotlin")
        );
    }
}
