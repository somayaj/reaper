use std::path::Path;

use anyhow::Result;
use serde::Serialize;

use super::gradle::{self, find_gradle_root};
use super::safe_join;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct JavaBuildMarkers {
    pub junit: bool,
    pub spring_test: bool,
    pub lombok: bool,
    pub jacoco: bool,
    pub slf4j: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct TestMethodMarker {
    pub name: String,
    pub line: u32,
    pub glyph_line: u32,
    pub end_line: u32,
    pub filter: String,
    #[serde(default)]
    pub is_class: bool,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct JavaFileContext {
    pub is_java: bool,
    pub is_gradle: bool,
    pub project_root: String,
    pub is_test_file: bool,
    pub test_class: Option<String>,
    pub test_method: Option<String>,
    pub test_filter: Option<String>,
    pub has_junit: bool,
    pub has_spring_test: bool,
    pub has_jacoco: bool,
    pub has_lombok: bool,
    pub uses_lombok: bool,
    pub has_slf4j: bool,
    pub uses_slf4j: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub frameworks: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub test_methods: Vec<TestMethodMarker>,
}

pub fn scan_build_content(content: &str) -> JavaBuildMarkers {
    let lower = content.to_ascii_lowercase();
    let compact: String = lower.chars().filter(|c| !c.is_whitespace()).collect();

    JavaBuildMarkers {
        junit: compact.contains("junit-jupiter")
            || compact.contains("org.junit.jupiter")
            || compact.contains("junit:junit")
            || compact.contains("org.junit:junit")
            || (compact.contains("junit") && compact.contains("testimplementation")),
        spring_test: compact.contains("spring-boot-starter-test")
            || compact.contains("org.springframework.boot:spring-boot-starter-test")
            || compact.contains("spring-test")
            || compact.contains("org.springframework:spring-test"),
        lombok: compact.contains("lombok")
            || compact.contains("io.freefair.lombok")
            || compact.contains("annotationprocessor")
                && compact.contains("lombok"),
        jacoco: compact.contains("jacoco") || compact.contains("org.jacoco"),
        slf4j: compact.contains("slf4j-api")
            || compact.contains("org.slf4j:slf4j-api")
            || compact.contains("org.slf4j")
            || compact.contains("spring-boot-starter-logging")
            || compact.contains("logback-classic")
            || compact.contains("ch.qos.logback"),
    }
}

pub fn scan_maven_pom(content: &str) -> JavaBuildMarkers {
    scan_build_content(content)
}

pub fn scan_gradle_project(root: &Path) -> JavaBuildMarkers {
    let mut markers = JavaBuildMarkers::default();
    merge_markers(&mut markers, scan_build_content(
        &gradle::read_build_file_content(root).unwrap_or_default(),
    ));
    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let name = entry.file_name();
            if name == "build" || name == ".gradle" || name.to_string_lossy().starts_with('.') {
                continue;
            }
            merge_markers(
                &mut markers,
                scan_build_content(&gradle::read_build_file_content(&path).unwrap_or_default()),
            );
        }
    }
    markers
}

pub fn scan_workspace_markers(ws: &Path) -> Result<JavaBuildMarkers> {
    let mut markers = JavaBuildMarkers::default();
    if ws.join("pom.xml").is_file() {
        if let Ok(content) = std::fs::read_to_string(ws.join("pom.xml")) {
            merge_markers(&mut markers, scan_maven_pom(&content));
        }
    }
    for root in gradle::find_all_gradle_roots(ws)? {
        merge_markers(&mut markers, scan_gradle_project(&root));
    }
    Ok(markers)
}

pub fn detect_java_file_context(
    ws: &Path,
    rel_path: &str,
    content: &str,
    line: u32,
) -> Result<JavaFileContext> {
    let mut ctx = JavaFileContext {
        is_java: rel_path.ends_with(".java"),
        ..Default::default()
    };
    if !ctx.is_java {
        return Ok(ctx);
    }

    let _ = safe_join(ws, rel_path)?;
    let markers = if let Some(root) = find_gradle_root(ws, rel_path)? {
        ctx.is_gradle = true;
        ctx.project_root = gradle::rel_path_for(ws, &root)?;
        scan_gradle_project(&root)
    } else {
        scan_workspace_markers(ws)?
    };

    ctx.has_junit = markers.junit;
    ctx.has_spring_test = markers.spring_test;
    ctx.has_jacoco = markers.jacoco;
    ctx.has_lombok = markers.lombok;
    ctx.uses_lombok = file_uses_lombok(content);
    ctx.has_slf4j = markers.slf4j;
    ctx.uses_slf4j = file_uses_slf4j(content);
    ctx.frameworks = frameworks_for(&markers, content);

    ctx.is_test_file = is_test_file_path(rel_path) || file_has_test_annotations(content);
    ctx.test_methods = list_test_methods(rel_path, content);
    if ctx.is_test_file {
        if let Some(class_name) = java_fqcn(rel_path, content) {
            ctx.test_class = Some(class_name.clone());
            if let Some(method) = test_method_at_line(rel_path, content, line) {
                ctx.test_method = Some(method.name.clone());
                ctx.test_filter = Some(method.filter.clone());
            } else {
                ctx.test_filter = Some(class_name);
            }
        }
    }

    Ok(ctx)
}

fn merge_markers(into: &mut JavaBuildMarkers, from: JavaBuildMarkers) {
    into.junit |= from.junit;
    into.spring_test |= from.spring_test;
    into.lombok |= from.lombok;
    into.jacoco |= from.jacoco;
    into.slf4j |= from.slf4j;
}

fn frameworks_for(markers: &JavaBuildMarkers, content: &str) -> Vec<String> {
    let mut out = Vec::new();
    if markers.junit || file_has_test_annotations(content) {
        push_unique(&mut out, "junit");
    }
    if markers.spring_test || file_has_spring_test_annotations(content) {
        push_unique(&mut out, "spring-test");
    }
    if markers.lombok || file_uses_lombok(content) {
        push_unique(&mut out, "lombok");
    }
    if markers.jacoco {
        push_unique(&mut out, "jacoco");
    }
    if markers.slf4j || file_uses_slf4j(content) {
        push_unique(&mut out, "slf4j");
    }
    out
}

fn push_unique(v: &mut Vec<String>, item: &str) {
    if !v.iter().any(|x| x == item) {
        v.push(item.to_string());
    }
}

pub fn is_test_file_path(rel_path: &str) -> bool {
    let normalized = rel_path.replace('\\', "/").to_ascii_lowercase();
    normalized.contains("/src/test/java/")
        || normalized.contains("/test/java/")
        || normalized.ends_with("test.java")
        || normalized.ends_with("tests.java")
        || normalized.ends_with("it.java")
}

fn file_has_test_annotations(content: &str) -> bool {
    content.contains("@Test")
        || content.contains("@ParameterizedTest")
        || content.contains("@RepeatedTest")
        || content.contains("@TestFactory")
        || content.contains("@TestTemplate")
}

fn file_has_spring_test_annotations(content: &str) -> bool {
    content.contains("@SpringBootTest")
        || content.contains("@WebMvcTest")
        || content.contains("@DataJpaTest")
        || content.contains("@JsonTest")
        || content.contains("@RestClientTest")
        || content.contains("@SpringJUnitConfig")
}

fn file_uses_lombok(content: &str) -> bool {
    const ANNOTATIONS: [&str; 12] = [
        "@Data",
        "@Getter",
        "@Setter",
        "@Builder",
        "@AllArgsConstructor",
        "@NoArgsConstructor",
        "@RequiredArgsConstructor",
        "@Slf4j",
        "@Value",
        "@EqualsAndHashCode",
        "@ToString",
        "@lombok.",
    ];
    ANNOTATIONS.iter().any(|a| content.contains(a))
}

fn file_uses_slf4j(content: &str) -> bool {
    content.contains("import org.slf4j.")
        || content.contains("LoggerFactory.getLogger")
        || content.contains("@Slf4j")
        || content.contains("org.slf4j.Logger")
}

pub fn java_fqcn(rel_path: &str, content: &str) -> Option<String> {
    let simple = Path::new(rel_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .map(str::to_string)?;
    let package = content
        .lines()
        .find_map(|line| {
            let trimmed = line.trim();
            trimmed
                .strip_prefix("package ")
                .and_then(|rest| rest.split(';').next())
                .map(str::trim)
                .filter(|p| !p.is_empty())
        })
        .map(str::to_string);

    if let Some(pkg) = package {
        return Some(format!("{pkg}.{simple}"));
    }

    fqcn_from_test_path(rel_path).or(Some(simple))
}

fn fqcn_from_test_path(rel_path: &str) -> Option<String> {
    let normalized = rel_path.replace('\\', "/");
    for marker in ["/src/test/java/", "/src/main/java/", "/test/java/", "/main/java/"] {
        if let Some(rest) = normalized.find(marker) {
            let suffix = &normalized[rest + marker.len()..];
            if suffix.ends_with(".java") {
                let class_path = suffix.trim_end_matches(".java");
                if !class_path.is_empty() && !class_path.contains("..") {
                    return Some(class_path.replace('/', "."));
                }
            }
        }
    }
    None
}

pub fn list_test_methods(rel_path: &str, content: &str) -> Vec<TestMethodMarker> {
    if !is_test_file_path(rel_path) && !file_has_test_annotations(content) {
        return Vec::new();
    }
    let Some(class_name) = java_fqcn(rel_path, content) else {
        return Vec::new();
    };

    let lines: Vec<&str> = content.lines().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if !line_has_test_annotation(&lines, i) {
            i += 1;
            continue;
        }
        let Some(sig_idx) = find_method_signature_index(&lines, i) else {
            i += 1;
            continue;
        };
        let Some(method_name) = parse_method_name(lines[sig_idx]) else {
            i = sig_idx + 1;
            continue;
        };
        let end = method_block_end(&lines, sig_idx);
        let glyph_idx = find_test_annotation_index(&lines, i, sig_idx);
        out.push(TestMethodMarker {
            name: method_name.clone(),
            line: (sig_idx + 1) as u32,
            glyph_line: (glyph_idx + 1) as u32,
            end_line: (end + 1) as u32,
            filter: format!("{class_name}.{method_name}"),
            is_class: false,
        });
        i = end + 1;
    }

    if let Some(class_idx) = find_test_class_line(&lines) {
        if !out.is_empty() || is_test_file_path(rel_path) {
            let simple = class_name.rsplit('.').next().unwrap_or(&class_name).to_string();
            out.insert(
                0,
                TestMethodMarker {
                    name: simple,
                    line: (class_idx + 1) as u32,
                    glyph_line: (class_idx + 1) as u32,
                    end_line: (class_idx + 1) as u32,
                    filter: class_name.clone(),
                    is_class: true,
                },
            );
        }
    }

    out
}

fn find_test_annotation_index(lines: &[&str], start: usize, sig_idx: usize) -> usize {
    let mut i = sig_idx;
    loop {
        let trimmed = lines[i].trim();
        if !trimmed.is_empty() && !trimmed.starts_with("//") {
            if trimmed.contains("@Test")
                || trimmed.contains("@ParameterizedTest")
                || trimmed.contains("@RepeatedTest")
                || trimmed.contains("@TestFactory")
                || trimmed.contains("@TestTemplate")
            {
                return i;
            }
            if trimmed.starts_with('@') {
                if i == 0 {
                    break;
                }
                i -= 1;
                continue;
            }
            if trimmed.contains('(') && !trimmed.ends_with(';') && !trimmed.starts_with("class ") {
                if i == 0 {
                    break;
                }
                i -= 1;
                continue;
            }
            break;
        }
        if i == 0 {
            break;
        }
        i -= 1;
    }
    start
}

pub fn test_method_at_line(rel_path: &str, content: &str, line: u32) -> Option<TestMethodMarker> {
    if line == 0 {
        return None;
    }
    list_test_methods(rel_path, content)
        .into_iter()
        .find(|m| line >= m.line && line <= m.end_line)
}

fn strip_leading_annotations(line: &str) -> &str {
    let mut rest = line.trim();
    while rest.starts_with('@') {
        if let Some(end) = annotation_end(rest) {
            rest = rest[end..].trim_start();
        } else {
            break;
        }
    }
    rest
}

fn annotation_end(s: &str) -> Option<usize> {
    if !s.starts_with('@') {
        return None;
    }
    let bytes = s.as_bytes();
    let mut i = 1;
    while i < bytes.len()
        && ((bytes[i] as char).is_ascii_alphanumeric() || bytes[i] == b'_')
    {
        i += 1;
    }
    if i >= bytes.len() || bytes[i] != b'(' {
        return Some(i);
    }
    let mut depth = 0;
    let mut j = i;
    while j < bytes.len() {
        match bytes[j] {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(j + 1);
                }
            }
            _ => {}
        }
        j += 1;
    }
    None
}

fn find_method_signature_index(lines: &[&str], start: usize) -> Option<usize> {
    for i in start..lines.len() {
        let trimmed = lines[i].trim();
        if trimmed.is_empty() || trimmed.starts_with("//") {
            continue;
        }
        let code = strip_leading_annotations(trimmed);
        if code.starts_with('@') {
            continue;
        }
        if parse_method_name(code).is_some() {
            return Some(i);
        }
    }
    None
}

fn find_test_class_line(lines: &[&str]) -> Option<usize> {
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("class ")
            || trimmed.contains(" class ")
            || trimmed.starts_with("public class ")
            || trimmed.starts_with("protected class ")
            || trimmed.starts_with("private class ")
            || trimmed.starts_with("public static class ")
            || trimmed.starts_with("public abstract class ")
        {
            if !trimmed.contains('(') {
                return Some(i);
            }
        }
    }
    None
}

#[allow(dead_code)]
fn test_method_name_at_line(content: &str, line: u32) -> Option<String> {
    test_method_at_line("", content, line).map(|m| m.name)
}

fn line_has_test_annotation(lines: &[&str], idx: usize) -> bool {
    let mut i = idx;
    while i > 0 && lines[i].trim().is_empty() {
        i -= 1;
    }
    let mut scan = i;
    loop {
        let trimmed = lines[scan].trim();
        if trimmed.starts_with('@')
            && (trimmed.contains("@Test")
                || trimmed.contains("@ParameterizedTest")
                || trimmed.contains("@RepeatedTest")
                || trimmed.contains("@TestFactory")
                || trimmed.contains("@TestTemplate"))
        {
            return true;
        }
        if !trimmed.starts_with('@') && !trimmed.is_empty() {
            return false;
        }
        if scan == 0 {
            return false;
        }
        scan -= 1;
    }
}

fn is_method_signature_line(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.starts_with("//") {
        return false;
    }
    if trimmed.starts_with('@') && !trimmed.contains('(') {
        return false;
    }
    trimmed.contains('(') && !trimmed.ends_with(';') && !trimmed.starts_with("class ")
}

fn parse_method_name(line: &str) -> Option<String> {
    let trimmed = line.trim();
    let before_paren = trimmed.split('(').next()?;
    let token = before_paren
        .split_whitespace()
        .filter(|t| *t != "public" && *t != "protected" && *t != "private" && *t != "static"
            && *t != "final" && *t != "synchronized" && !t.ends_with("<"))
        .last()?;
    if token.is_empty() || token == "void" || token == "class" {
        None
    } else if !token
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        None
    } else {
        Some(token.to_string())
    }
}

fn method_block_end(lines: &[&str], signature_idx: usize) -> usize {
    let mut depth = 0;
    let mut started = false;
    for (offset, line) in lines.iter().enumerate().skip(signature_idx) {
        for ch in line.chars() {
            if ch == '{' {
                depth += 1;
                started = true;
            } else if ch == '}' && started {
                depth -= 1;
                if depth == 0 {
                    return offset;
                }
            }
        }
    }
    lines.len().saturating_sub(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_junit_and_lombok_in_gradle() {
        let content = r#"
plugins { id 'java' }
dependencies {
    testImplementation 'org.junit.jupiter:junit-jupiter:5.10.0'
    compileOnly 'org.projectlombok:lombok:1.18.30'
    annotationProcessor 'org.projectlombok:lombok:1.18.30'
}
"#;
        let m = scan_build_content(content);
        assert!(m.junit);
        assert!(m.lombok);
    }

    #[test]
    fn detects_spring_test_starter() {
        let content =
            "testImplementation 'org.springframework.boot:spring-boot-starter-test'";
        let m = scan_build_content(content);
        assert!(m.spring_test);
    }

    #[test]
    fn fqcn_from_test_source_path() {
        let path = "module/src/test/java/com/example/DemoTest.java";
        assert_eq!(
            java_fqcn(path, "package com.example;\nclass DemoTest {}").as_deref(),
            Some("com.example.DemoTest")
        );
    }

    #[test]
    fn finds_test_method_at_cursor() {
        let content = r#"
package com.example;

import org.junit.jupiter.api.Test;

class DemoTest {
    @Test
    void helloWorld() {
        System.out.println("hi");
    }

    @Test
    void other() {
    }
}
"#;
        let path = "src/test/java/com/example/DemoTest.java";
        assert_eq!(test_method_at_line(path, content, 8).map(|m| m.name), Some("helloWorld".into()));
        assert_eq!(test_method_at_line(path, content, 13).map(|m| m.name), Some("other".into()));
    }

    #[test]
    fn lists_all_test_methods_with_lines() {
        let path = "src/test/java/com/example/DemoTest.java";
        let content = r#"
package com.example;

import org.junit.jupiter.api.Test;

class DemoTest {
    @Test
    void helloWorld() {
        System.out.println("hi");
    }

    @Test void inline() {}

    @Test
    void other() {
    }
}
"#;
        let methods = list_test_methods(path, content);
        assert_eq!(methods.len(), 4);
        assert!(methods[0].is_class);
        assert_eq!(methods[0].filter, "com.example.DemoTest");
        assert_eq!(methods[1].name, "helloWorld");
        assert_eq!(methods[1].filter, "com.example.DemoTest.helloWorld");
        assert!(methods[1].glyph_line <= methods[1].line);
        assert_eq!(methods[2].name, "inline");
        assert_eq!(methods[3].name, "other");
    }

    #[test]
    fn lists_parameterized_test_methods() {
        let path = "src/test/java/com/example/ParamTest.java";
        let content = r#"
package com.example;

import org.junit.jupiter.params.ParameterizedTest;
import org.junit.jupiter.params.provider.ValueSource;

class ParamTest {
    @ParameterizedTest
    @ValueSource(strings = {"a", "b"})
    void parameterized(String s) {
    }

    @ParameterizedTest(name = "{0}")
    @CsvSource({
        "1, 2, 3",
        "4, 5, 9"
    })
    void csvTest(int a, int b, int expected) {
    }
}
"#;
        let methods: Vec<_> = list_test_methods(path, content)
            .into_iter()
            .filter(|m| !m.is_class)
            .collect();
        assert_eq!(methods.len(), 2);
        assert_eq!(methods[0].name, "parameterized");
        assert_eq!(methods[0].filter, "com.example.ParamTest.parameterized");
        assert_eq!(methods[1].name, "csvTest");
    }

    #[test]
    fn includes_class_level_test_marker() {
        let path = "src/test/java/com/example/DemoTest.java";
        let content = r#"
package com.example;

import org.junit.jupiter.api.Test;

class DemoTest {
    @Test
    void helloWorld() {}
}
"#;
        let methods = list_test_methods(path, content);
        assert_eq!(methods.len(), 2);
        assert!(methods[0].is_class);
        assert_eq!(methods[0].filter, "com.example.DemoTest");
        assert_eq!(methods[0].glyph_line, methods[0].line);
        assert!(!methods[1].is_class);
    }

    #[test]
    fn detects_lombok_in_source() {
        assert!(file_uses_lombok("@Data\npublic class User {\n}\n"));
    }

    #[test]
    fn detects_slf4j_in_gradle_and_source() {
        let content = "implementation 'org.slf4j:slf4j-api:2.0.9'";
        assert!(scan_build_content(content).slf4j);
        assert!(file_uses_slf4j(
            "import org.slf4j.Logger;\nprivate static final Logger log = LoggerFactory.getLogger(Foo.class);\n"
        ));
        assert!(file_uses_slf4j("@Slf4j\npublic class Foo {}\n"));
    }
}
