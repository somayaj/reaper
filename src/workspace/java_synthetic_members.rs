//! Synthetic Java members not present in source: Lombok-generated fields/accessors,
//! static test/mock helpers, and common Apache Commons utilities from static imports.

use std::collections::HashSet;

use super::classpath::CompletionItem;
use super::java_psi::{annotation_simple_names, parse_imports, ImportMap};

#[derive(Debug, Clone, PartialEq, Eq)]
struct FieldDecl {
    name: String,
    type_hint: String,
    annotations: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ClassDecl {
    line: u32,
    annotations: Vec<String>,
    fields: Vec<FieldDecl>,
}

const LOGGING_ANNOTATIONS: &[(&str, &str, &str)] = &[
    ("Slf4j", "log", "Logger"),
    ("Log", "log", "Logger"),
    ("Log4j", "log", "Logger"),
    ("Log4j2", "log", "Logger"),
    ("CommonsLog", "log", "Log"),
];

const LOGGING_FQCN: &[(&str, &str)] = &[
    ("Logger", "org.slf4j.Logger"),
    ("Log", "org.apache.commons.logging.Log"),
];

const STATIC_MEMBER_TABLES: &[(&str, &[(&str, &str)])] = &[
    (
        "org.junit.jupiter.api.Assertions",
        &[
            ("assertEquals", "method"),
            ("assertNotEquals", "method"),
            ("assertNull", "method"),
            ("assertNotNull", "method"),
            ("assertTrue", "method"),
            ("assertFalse", "method"),
            ("assertSame", "method"),
            ("assertNotSame", "method"),
            ("assertThrows", "method"),
            ("assertDoesNotThrow", "method"),
            ("assertAll", "method"),
            ("assertArrayEquals", "method"),
            ("assertIterableEquals", "method"),
            ("assertLinesMatch", "method"),
            ("assertTimeout", "method"),
            ("fail", "method"),
        ],
    ),
    (
        "org.junit.Assert",
        &[
            ("assertEquals", "method"),
            ("assertNotEquals", "method"),
            ("assertNull", "method"),
            ("assertNotNull", "method"),
            ("assertTrue", "method"),
            ("assertFalse", "method"),
            ("assertSame", "method"),
            ("assertNotSame", "method"),
            ("assertArrayEquals", "method"),
            ("fail", "method"),
            ("failNotEquals", "method"),
            ("failNotSame", "method"),
        ],
    ),
    (
        "org.mockito.Mockito",
        &[
            ("mock", "method"),
            ("spy", "method"),
            ("verify", "method"),
            ("when", "method"),
            ("doReturn", "method"),
            ("doThrow", "method"),
            ("doAnswer", "method"),
            ("doNothing", "method"),
            ("times", "method"),
            ("never", "method"),
            ("atLeast", "method"),
            ("atMost", "method"),
            ("atLeastOnce", "method"),
            ("only", "method"),
            ("inOrder", "method"),
            ("verifyNoMoreInteractions", "method"),
            ("verifyNoInteractions", "method"),
            ("reset", "method"),
        ],
    ),
    (
        "org.mockito.ArgumentMatchers",
        &[
            ("any", "method"),
            ("anyString", "method"),
            ("anyInt", "method"),
            ("anyLong", "method"),
            ("anyBoolean", "method"),
            ("anyList", "method"),
            ("anyMap", "method"),
            ("anySet", "method"),
            ("eq", "method"),
            ("isA", "method"),
            ("isNull", "method"),
            ("notNull", "method"),
            ("nullable", "method"),
        ],
    ),
    (
        "org.apache.commons.lang3.StringUtils",
        &[
            ("isBlank", "method"),
            ("isEmpty", "method"),
            ("isNotBlank", "method"),
            ("isNotEmpty", "method"),
            ("trim", "method"),
            ("capitalize", "method"),
            ("join", "method"),
            ("split", "method"),
            ("equals", "method"),
            ("equalsIgnoreCase", "method"),
            ("strip", "method"),
            ("defaultString", "method"),
            ("abbreviate", "method"),
        ],
    ),
    (
        "org.apache.commons.lang3.ObjectUtils",
        &[
            ("defaultIfNull", "method"),
            ("firstNonNull", "method"),
            ("equals", "method"),
        ],
    ),
    (
        "org.apache.commons.collections4.CollectionUtils",
        &[
            ("isEmpty", "method"),
            ("isNotEmpty", "method"),
            ("size", "method"),
            ("sizeIsEmpty", "method"),
        ],
    ),
    (
        "org.apache.commons.io.FileUtils",
        &[
            ("readFileToString", "method"),
            ("writeStringToFile", "method"),
            ("deleteQuietly", "method"),
            ("copyFile", "method"),
        ],
    ),
    (
        "org.apache.commons.io.IOUtils",
        &[
            ("toString", "method"),
            ("copy", "method"),
            ("closeQuietly", "method"),
        ],
    ),
];

fn class_has_annotation(annotations: &[String], name: &str) -> bool {
    annotations.iter().any(|a| a == name)
}

fn field_has_annotation(field: &FieldDecl, name: &str) -> bool {
    field.annotations.iter().any(|a| a == name)
}

fn capitalize_field(name: &str) -> String {
    let mut chars = name.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_ascii_uppercase().to_string() + chars.as_str(),
    }
}

fn getter_name(field: &str, type_hint: &str) -> String {
    let cap = capitalize_field(field);
    if type_hint.eq_ignore_ascii_case("boolean") || type_hint.eq_ignore_ascii_case("Boolean") {
        format!("is{cap}")
    } else {
        format!("get{cap}")
    }
}

fn setter_name(field: &str) -> String {
    format!("set{}", capitalize_field(field))
}

fn parse_field_line(line: &str) -> Option<(String, String)> {
    let trimmed = line.split("//").next()?.trim();
    if trimmed.is_empty()
        || trimmed.contains('(')
        || !trimmed.ends_with(';')
        || trimmed.starts_with("import ")
        || trimmed.starts_with("package ")
        || trimmed.starts_with('@')
    {
        return None;
    }
    let before_assign = trimmed.trim_end_matches(';').split('=').next()?.trim();
    const MODS: &[&str] = &[
        "public", "private", "protected", "static", "final", "volatile", "transient",
    ];
    let mut parts: Vec<&str> = before_assign.split_whitespace().collect();
    while parts.len() > 2 && MODS.contains(&parts[0]) {
        parts.remove(0);
    }
    if parts.len() < 2 {
        return None;
    }
    let name = parts.last()?.to_string();
    if super::symbols::is_keyword(&name) {
        return None;
    }
    let ty = parts[..parts.len() - 1].join(" ");
    if ty.is_empty() {
        return None;
    }
    Some((name, ty))
}

fn annotations_on_line(line: &str) -> Vec<String> {
    let trimmed = line.trim();
    if !trimmed.starts_with('@') {
        return Vec::new();
    }
    let after = trimmed.trim_start_matches('@');
    let name = after
        .split(|c: char| c.is_whitespace() || c == '(')
        .next()
        .unwrap_or("")
        .split('.')
        .next_back()
        .unwrap_or("")
        .trim()
        .to_string();
    if name.is_empty() {
        Vec::new()
    } else {
        vec![name]
    }
}

fn find_class_line(content: &str, line: u32) -> Option<usize> {
    let lines: Vec<&str> = content.lines().collect();
    let idx = line.saturating_sub(1) as usize;
    if idx >= lines.len() {
        return None;
    }
    for i in (0..=idx).rev() {
        let trimmed = lines[i].trim();
        if trimmed.contains(" class ")
            || trimmed.starts_with("class ")
            || trimmed.contains(" interface ")
            || trimmed.starts_with("interface ")
            || trimmed.contains(" enum ")
            || trimmed.starts_with("enum ")
            || trimmed.contains(" record ")
            || trimmed.starts_with("record ")
        {
            if !trimmed.contains('(') || trimmed.contains('{') {
                return Some(i);
            }
        }
    }
    None
}

fn collect_class_decl(content: &str, class_line: usize, through_line: usize) -> ClassDecl {
    let lines: Vec<&str> = content.lines().collect();
    let mut class_annotations = Vec::new();
    for i in (0..class_line).rev().take(12) {
        let trimmed = lines[i].trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with('@') {
            class_annotations.extend(annotations_on_line(trimmed));
            continue;
        }
        if !trimmed.starts_with("//") {
            break;
        }
    }
    class_annotations.reverse();

    let until = through_line.min(lines.len());
    let mut pending_field_annotations = Vec::new();
    let mut fields = Vec::new();
    let mut seen_fields = HashSet::new();
    for i in class_line..until {
        let trimmed = lines[i].trim();
        if trimmed.is_empty() || trimmed.starts_with("//") {
            continue;
        }
        if trimmed.starts_with('@') {
            pending_field_annotations.extend(annotations_on_line(trimmed));
            let mut rest = trimmed;
            while rest.starts_with('@') {
                rest = rest.trim_start_matches('@');
                if let Some(end) = rest.find(|c: char| c.is_whitespace()) {
                    rest = rest[end..].trim_start();
                } else {
                    rest = "";
                    break;
                }
                if !rest.starts_with('@') {
                    break;
                }
            }
            if let Some((name, ty)) = parse_field_line(rest) {
                if seen_fields.insert(name.clone()) {
                    fields.push(FieldDecl {
                        name,
                        type_hint: ty,
                        annotations: pending_field_annotations.clone(),
                    });
                }
                pending_field_annotations.clear();
            }
            continue;
        }
        if let Some((name, ty)) = parse_field_line(trimmed) {
            if seen_fields.insert(name.clone()) {
                fields.push(FieldDecl {
                    name,
                    type_hint: ty,
                    annotations: pending_field_annotations.clone(),
                });
            }
            pending_field_annotations.clear();
        } else if !trimmed.ends_with(';') {
            pending_field_annotations.clear();
        }
    }

    ClassDecl {
        line: (class_line + 1) as u32,
        annotations: class_annotations,
        fields,
    }
}

fn class_at_line(content: &str, line: u32) -> Option<ClassDecl> {
    let class_line = find_class_line(content, line)?;
    Some(collect_class_decl(content, class_line, line as usize))
}

fn explicit_field_names(content: &str) -> HashSet<String> {
    let mut names = HashSet::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("import ") || trimmed.starts_with("package ") {
            continue;
        }
        let mut rest = trimmed;
        while rest.starts_with('@') {
            rest = rest.trim_start_matches('@');
            if let Some(end) = rest.find(|c: char| c.is_whitespace()) {
                rest = rest[end..].trim_start();
            } else {
                rest = "";
                break;
            }
        }
        if let Some((name, _)) = parse_field_line(rest) {
            names.insert(name);
            continue;
        }
        let mut search = rest;
        while let Some(semi) = search.find(';') {
            let stmt = search[..=semi].trim();
            if let Some((name, _)) = parse_field_line(stmt) {
                names.insert(name);
            } else {
                let before = search[..semi].trim();
                let parts: Vec<&str> = before.split_whitespace().collect();
                if parts.len() >= 2 {
                    let name = parts[parts.len() - 1].trim();
                    if !name.is_empty()
                        && !super::symbols::is_keyword(name)
                        && name.chars().all(|c| c.is_alphanumeric() || c == '_')
                    {
                        names.insert(name.to_string());
                    }
                }
            }
            search = search[semi + 1..].trim_start();
        }
    }
    names
}

fn lombok_logging_fields(class: &ClassDecl, explicit: &HashSet<String>) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for (ann, field, ty) in LOGGING_ANNOTATIONS {
        if class_has_annotation(&class.annotations, ann) && !explicit.contains(*field) {
            out.push((field.to_string(), ty.to_string()));
        }
    }
    out
}

fn lombok_accessor_methods(class: &ClassDecl, member_prefix: &str) -> Vec<(String, String)> {
    let class_getter = class_has_annotation(&class.annotations, "Getter")
        || class_has_annotation(&class.annotations, "Data")
        || class_has_annotation(&class.annotations, "Value");
    let class_setter = class_has_annotation(&class.annotations, "Setter")
        || class_has_annotation(&class.annotations, "Data");
    let prefix_lower = member_prefix.to_lowercase();
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for field in &class.fields {
        if field.name.starts_with('$') {
            continue;
        }
        let field_getter = class_getter
            || field_has_annotation(field, "Getter")
            || field_has_annotation(field, "Data");
        let field_setter = class_setter
            || field_has_annotation(field, "Setter")
            || field_has_annotation(field, "Data");
        if field_getter {
            let g = getter_name(&field.name, &field.type_hint);
            if (member_prefix.is_empty() || g.to_lowercase().starts_with(&prefix_lower))
                && seen.insert(g.clone())
            {
                out.push((g, field.type_hint.clone()));
            }
        }
        if field_setter {
            let s = setter_name(&field.name);
            if (member_prefix.is_empty() || s.to_lowercase().starts_with(&prefix_lower))
                && seen.insert(s.clone())
            {
                out.push((s, "void".to_string()));
            }
        }
    }
    out
}

fn synthetic_logging_type(type_hint: &str) -> Option<String> {
    LOGGING_FQCN
        .iter()
        .find(|(simple, _)| simple.eq_ignore_ascii_case(type_hint))
        .map(|(_, fqcn)| fqcn.to_string())
}

/// Variables injected by Lombok / test annotations (e.g. `@Slf4j` → `log`).
pub fn synthetic_scope_variables(content: &str, through_line: u32) -> Vec<(String, String)> {
    let Some(class) = class_at_line(content, through_line) else {
        return Vec::new();
    };
    let explicit = explicit_field_names(content);
    let mut out = lombok_logging_fields(&class, &explicit);
    let mut seen: HashSet<String> = out.iter().map(|(n, _)| n.clone()).collect();

    for field in &class.fields {
        if field_has_annotation(field, "Mock")
            || field_has_annotation(field, "Spy")
            || field_has_annotation(field, "InjectMocks")
            || field_has_annotation(field, "Captor")
        {
            if seen.insert(field.name.clone()) {
                out.push((field.name.clone(), field.type_hint.clone()));
            }
        }
    }
    out
}

pub fn synthetic_receiver_type(content: &str, var_name: &str) -> Option<String> {
    if var_name.is_empty() {
        return None;
    }
    let explicit = explicit_field_names(content);
    let lines: Vec<&str> = content.lines().collect();
    for (idx, _) in lines.iter().enumerate() {
        let line = (idx + 1) as u32;
        let Some(class) = class_at_line(content, line) else {
            continue;
        };
        for (name, ty) in lombok_logging_fields(&class, &explicit) {
            if name == var_name {
                return Some(ty);
            }
        }
        for field in &class.fields {
            if field.name == var_name {
                return Some(field.type_hint.clone());
            }
        }
    }
    None
}

pub fn should_offer_scope_completions(content: &str, line: u32, column: u32) -> bool {
    if super::symbols::is_java_import_line(content, line) {
        return false;
    }
    if super::symbols::is_java_for_type_start(content, line, column) {
        return false;
    }
    if super::symbols::is_java_type_reference_context(content, line, column) {
        return false;
    }
    super::java_ecosystem::list_java_method_scopes(content)
        .iter()
        .any(|m| line >= m.start_line && line <= m.end_line)
}

fn static_members_for_wildcard(pkg: &str) -> Option<&'static [(&'static str, &'static str)]> {
    STATIC_MEMBER_TABLES
        .iter()
        .find(|(fqcn, _)| fqcn == &pkg)
        .map(|(_, members)| *members)
}

fn static_import_completions(imports: &ImportMap, prefix: &str) -> Vec<CompletionItem> {
    let prefix_lower = prefix.to_lowercase();
    let mut seen = HashSet::new();
    let mut items = Vec::new();

    for (simple, fqcn) in &imports.explicit {
        let is_static_method = fqcn.contains('.')
            && simple.chars().next().is_some_and(|c| c.is_ascii_lowercase())
            && fqcn.rsplit('.').next().is_some_and(|last| last == simple);
        if !is_static_method {
            continue;
        }
        if !prefix.is_empty() && !simple.to_lowercase().starts_with(&prefix_lower) {
            continue;
        }
        if seen.insert(simple.clone()) {
            items.push(CompletionItem {
                label: simple.clone(),
                kind: "method".to_string(),
                detail: Some(format!("static {fqcn}")),
                insert: None,
                path: None,
                line: None,
                column: None,
                documentation: None,
            });
        }
    }

    for pkg in &imports.wildcards {
        let Some(members) = static_members_for_wildcard(pkg) else {
            if pkg.starts_with("org.apache.") {
                continue;
            }
            continue;
        };
        for (name, kind) in members {
            if !prefix.is_empty() && !name.to_lowercase().starts_with(&prefix_lower) {
                continue;
            }
            if seen.insert(name.to_string()) {
                items.push(CompletionItem {
                    label: name.to_string(),
                    kind: kind.to_string(),
                    detail: Some(format!("static {pkg}.{name}")),
                    insert: None,
                    path: None,
                    line: None,
                    column: None,
                    documentation: None,
                });
            }
        }
    }

    if prefix.is_empty() || "assert".starts_with(&prefix_lower) || prefix_lower.starts_with("assert")
    {
        if imports
            .wildcards
            .iter()
            .any(|w| w.contains("org.junit"))
            || imports.explicit.values().any(|v| v.contains("org.junit"))
        {
            for (name, kind) in STATIC_MEMBER_TABLES
                .iter()
                .find(|(p, _)| *p == "org.junit.jupiter.api.Assertions")
                .map(|(_, m)| *m)
                .unwrap_or(&[])
            {
                if !prefix.is_empty() && !name.to_lowercase().starts_with(&prefix_lower) {
                    continue;
                }
                if seen.insert(name.to_string()) {
                    items.push(CompletionItem {
                        label: name.to_string(),
                        kind: kind.to_string(),
                        detail: Some(format!("static org.junit.jupiter.api.Assertions.{name}")),
                        insert: None,
                        path: None,
                        line: None,
                        column: None,
                        documentation: None,
                    });
                }
            }
        }
    }

    items
}

pub fn scope_completion_items(
    content: &str,
    from_path: &str,
    line: u32,
    prefix: &str,
) -> Vec<CompletionItem> {
    let imports = parse_imports(content);
    let prefix_lower = prefix.to_lowercase();
    let mut seen = HashSet::new();
    let mut items = Vec::new();

    for (name, ty) in super::symbols::collect_java_scope_variables(content, line) {
        if !prefix.is_empty() && !name.to_lowercase().starts_with(&prefix_lower) {
            continue;
        }
        if seen.insert(name.clone()) {
            items.push(CompletionItem {
                label: name,
                kind: "variable".to_string(),
                detail: Some(ty),
                insert: None,
                path: Some(from_path.to_string()),
                line: None,
                column: None,
                documentation: None,
            });
        }
    }

    for item in static_import_completions(&imports, prefix) {
        if seen.insert(item.label.clone()) {
            items.push(item);
        }
    }

    items
}

pub fn synthetic_instance_members(
    content: &str,
    line: u32,
    qualifier: &str,
    member_prefix: &str,
    from_path: &str,
) -> Vec<CompletionItem> {
    if qualifier != "this" && qualifier != "super" {
        return Vec::new();
    }
    let Some(class) = class_at_line(content, line) else {
        return Vec::new();
    };
    let explicit = explicit_field_names(content);
    let prefix_lower = member_prefix.to_lowercase();
    let mut seen = HashSet::new();
    let mut items = Vec::new();

    for (name, ty) in lombok_logging_fields(&class, &explicit) {
        if !member_prefix.is_empty() && !name.to_lowercase().starts_with(&prefix_lower) {
            continue;
        }
        if seen.insert(name.clone()) {
            items.push(CompletionItem {
                label: name,
                kind: "field".to_string(),
                detail: Some(format!("{ty} (lombok)")),
                insert: None,
                path: Some(from_path.to_string()),
                line: None,
                column: None,
                documentation: None,
            });
        }
    }

    for (name, ty) in lombok_accessor_methods(&class, member_prefix) {
        if seen.insert(name.clone()) {
            items.push(CompletionItem {
                label: name,
                kind: "method".to_string(),
                detail: Some(format!("{ty} (lombok)")),
                insert: None,
                path: Some(from_path.to_string()),
                line: None,
                column: None,
                documentation: None,
            });
        }
    }

    items
}

pub fn resolve_synthetic_receiver_fqcn(type_hint: &str) -> Option<String> {
    synthetic_logging_type(type_hint)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slf4j_injects_log_variable() {
        let content = "@Slf4j\npublic class App {\n  void m() {\n  }\n}\n";
        let vars = synthetic_scope_variables(content, 3);
        assert!(
            vars.iter().any(|(n, t)| n == "log" && t == "Logger"),
            "vars={vars:?}"
        );
    }

    #[test]
    fn commons_log_injects_log_as_apache_type() {
        let content = "@CommonsLog\nclass App { void m() {} }\n";
        let vars = synthetic_scope_variables(content, 2);
        assert!(vars.iter().any(|(n, t)| n == "log" && t == "Log"));
        assert_eq!(
            resolve_synthetic_receiver_fqcn("Log"),
            Some("org.apache.commons.logging.Log".into())
        );
    }

    #[test]
    fn data_generates_getters_on_this() {
        let content = "@Data\nclass User {\n  private String name;\n  void m() { this. }\n}\n";
        let items = synthetic_instance_members(content, 3, "this", "", "User.java");
        assert!(items.iter().any(|i| i.label == "getName"));
        assert!(items.iter().any(|i| i.label == "setName"));
    }

    #[test]
    fn mock_field_in_scope() {
        let content = "import org.mockito.Mock;\nclass T {\n  @Mock UserService userService;\n  @Test void x() {}\n}\n";
        let vars = synthetic_scope_variables(content, 4);
        assert!(vars.iter().any(|(n, _)| n == "userService"));
    }

    #[test]
    fn static_junit_imports_complete() {
        let content = "import static org.junit.jupiter.api.Assertions.*;\nclass T { void x() { } }\n";
        let imports = parse_imports(content);
        let items = static_import_completions(&imports, "assert");
        assert!(items.iter().any(|i| i.label == "assertEquals"));
        assert!(items.iter().any(|i| i.label == "assertNotNull"));
    }

    #[test]
    fn static_mockito_wildcard_completes() {
        let content = "import static org.mockito.Mockito.*;\nclass T { void x() {} }\n";
        let imports = parse_imports(content);
        let items = static_import_completions(&imports, "ver");
        assert!(items.iter().any(|i| i.label == "verify"));
        assert!(items.iter().any(|i| i.label == "verifyNoInteractions"));
    }

    #[test]
    fn apache_string_utils_static_import() {
        let content = "import static org.apache.commons.lang3.StringUtils.isBlank;\nclass T {}\n";
        let imports = parse_imports(content);
        let items = static_import_completions(&imports, "isB");
        assert!(items.iter().any(|i| i.label == "isBlank"));
    }

    #[test]
    fn apache_wildcard_string_utils() {
        let content = "import static org.apache.commons.lang3.StringUtils.*;\nclass T {}\n";
        let imports = parse_imports(content);
        let items = static_import_completions(&imports, "is");
        assert!(items.iter().any(|i| i.label == "isBlank"));
        assert!(items.iter().any(|i| i.label == "isEmpty"));
    }

    #[test]
    fn explicit_log_field_blocks_lombok_duplicate() {
        let content = "@Slf4j\nclass App { Logger log; void m() {} }\n";
        let vars = synthetic_scope_variables(content, 2);
        assert!(!vars.iter().any(|(n, _)| n == "log"));
    }
}
