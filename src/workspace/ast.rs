//! Multi-language syntax tree via tree-sitter.
//!
//! Shared JSON model for the Structure / AST tool window.

use anyhow::{Context, Result, bail};
use serde::Serialize;
use tree_sitter::{Language, Node, Parser};

use super::languages;

const MAX_AST_BYTES: usize = 1_500_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AstMode {
    /// Declarations / named structural nodes (default Structure view).
    Structure,
    /// All named syntax nodes.
    Full,
}

impl AstMode {
    pub fn parse(raw: Option<&str>) -> Self {
        match raw.map(|s| s.trim().to_ascii_lowercase()).as_deref() {
            Some("full") | Some("ast") => Self::Full,
            _ => Self::Structure,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct AstResponse {
    pub path: String,
    pub language: String,
    pub mode: String,
    pub root: AstNode,
}

#[derive(Debug, Serialize)]
pub struct AstNode {
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub start_line: u32,
    pub start_column: u32,
    pub end_line: u32,
    pub end_column: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<AstNode>,
}

/// Languages with a bundled tree-sitter grammar.
pub fn supports_language(language: &str) -> bool {
    languages::has_ast_grammar(language) && language_grammar(language).is_some()
}

pub fn language_for_ast_path(path: &str) -> Option<&'static str> {
    let lang = languages::language_for_path(path)?;
    if supports_language(lang) {
        Some(lang)
    } else {
        None
    }
}

fn language_grammar(language: &str) -> Option<Language> {
    Some(match language {
        "java" => tree_sitter_java::LANGUAGE.into(),
        "python" => tree_sitter_python::LANGUAGE.into(),
        "javascript" => tree_sitter_javascript::LANGUAGE.into(),
        "typescript" => {
            // Prefer TSX grammar for .tsx; plain TS for .ts — caller passes language id.
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()
        }
        "tsx" => tree_sitter_typescript::LANGUAGE_TSX.into(),
        "go" => tree_sitter_go::LANGUAGE.into(),
        "rust" => tree_sitter_rust::LANGUAGE.into(),
        "c" => tree_sitter_c::LANGUAGE.into(),
        "cpp" => tree_sitter_cpp::LANGUAGE.into(),
        "json" => tree_sitter_json::LANGUAGE.into(),
        "yaml" => tree_sitter_yaml::LANGUAGE.into(),
        _ => return None,
    })
}

/// Resolve grammar language id (tsx vs typescript by extension).
fn resolve_grammar_language<'a>(path: &str, language: &'a str) -> &'a str {
    if language == "typescript" {
        let lower = path.replace('\\', "/").to_lowercase();
        if lower.ends_with(".tsx") {
            return "tsx";
        }
    }
    language
}

pub fn parse_ast(path: &str, content: &str, mode: AstMode) -> Result<AstResponse> {
    if content.len() > MAX_AST_BYTES {
        bail!(
            "file too large for AST ({} bytes; max {})",
            content.len(),
            MAX_AST_BYTES
        );
    }
    let language = languages::language_for_path(path)
        .context("unsupported file type for AST")?;
    let grammar_id = resolve_grammar_language(path, language);
    let grammar = language_grammar(grammar_id)
        .with_context(|| format!("no tree-sitter grammar for '{language}'"))?;

    let mut parser = Parser::new();
    parser
        .set_language(&grammar)
        .context("failed to set tree-sitter language")?;
    let tree = parser
        .parse(content, None)
        .context("tree-sitter parse returned no tree")?;

    let root_node = tree.root_node();
    let root = convert_node(root_node, content.as_bytes(), mode, 0);

    Ok(AstResponse {
        path: path.replace('\\', "/"),
        language: language.to_string(),
        mode: match mode {
            AstMode::Structure => "structure".into(),
            AstMode::Full => "full".into(),
        },
        root,
    })
}

/// Parse from workspace disk.
pub fn parse_ast_file(ws: &std::path::Path, rel_path: &str, mode: AstMode) -> Result<AstResponse> {
    let content = super::read_file(ws, rel_path)?;
    parse_ast(rel_path, &content, mode)
}

fn convert_node(node: Node<'_>, source: &[u8], mode: AstMode, depth: usize) -> AstNode {
    let kind = node.kind().to_string();
    let name = extract_name(node, source);
    let label = pretty_label(&kind, name.as_deref());
    let start = node.start_position();
    let end = node.end_position();

    let mut children = Vec::new();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if !should_include_child(child, mode, depth) {
            continue;
        }
        let converted = convert_node(child, source, mode, depth + 1);
        if mode == AstMode::Structure && should_flatten_wrapper(&converted) {
            children.extend(converted.children);
        } else {
            children.push(converted);
        }
    }

    AstNode {
        kind,
        label: Some(label),
        name,
        start_line: (start.row as u32) + 1,
        start_column: (start.column as u32) + 1,
        end_line: (end.row as u32) + 1,
        end_column: (end.column as u32) + 1,
        children,
    }
}

fn should_include_child(node: Node<'_>, mode: AstMode, depth: usize) -> bool {
    if node.is_error() || node.is_missing() {
        return true;
    }
    match mode {
        AstMode::Full => node.is_named(),
        AstMode::Structure => {
            if !node.is_named() {
                return false;
            }
            if depth == 0 {
                return true;
            }
            is_structure_kind(node.kind())
                || node.child_by_field_name("name").is_some()
                || has_structure_descendant(node, 0)
        }
    }
}

fn has_structure_descendant(node: Node<'_>, depth: usize) -> bool {
    if depth > 6 {
        return false;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if !child.is_named() {
            continue;
        }
        if is_structure_kind(child.kind()) || child.child_by_field_name("name").is_some() {
            return true;
        }
        if has_structure_descendant(child, depth + 1) {
            return true;
        }
    }
    false
}

fn is_structure_kind(kind: &str) -> bool {
    const KEYWORDS: &[&str] = &[
        "declaration",
        "definition",
        "declarator",
        "class",
        "interface",
        "enum",
        "struct",
        "trait",
        "impl",
        "module",
        "namespace",
        "package",
        "import",
        "export",
        "function",
        "method",
        "constructor",
        "field",
        "property",
        "variable",
        "const",
        "type_alias",
        "type_definition",
        "record",
        "annotation",
        "protocol",
        "extension",
        "union",
        "typedef",
        "macro",
        "use_declaration",
        "mod_item",
        "fn_item",
        "struct_item",
        "enum_item",
        "trait_item",
        "impl_item",
        "const_item",
        "static_item",
        "type_item",
        "function_definition",
        "function_declaration",
        "method_declaration",
        "method_definition",
        "class_declaration",
        "class_definition",
        "interface_declaration",
        "enum_declaration",
        "record_declaration",
        "annotation_type_declaration",
        "constructor_declaration",
        "field_declaration",
        "constant_declaration",
        "lexical_declaration",
        "variable_declaration",
        "variable_declarator",
        "import_declaration",
        "package_declaration",
        "export_statement",
        "import_statement",
        "type_alias_declaration",
        "abstract_class_declaration",
        "public_field_definition",
        "pair", // JSON/YAML keys
        "block_mapping_pair",
        "document",
    ];
    if KEYWORDS.iter().any(|k| kind == *k) {
        return true;
    }
    KEYWORDS.iter().any(|k| kind.contains(k))
}

/// Drop pure wrapper nodes that only re-host a single child of the same span family.
fn should_flatten_wrapper(node: &AstNode) -> bool {
    if node.name.is_some() {
        return false;
    }
    if node.children.len() != 1 {
        return false;
    }
    matches!(
        node.kind.as_str(),
        "program"
            | "source_file"
            | "compilation_unit"
            | "module"
            | "statement_block"
            | "declaration_list"
            | "class_body"
            | "interface_body"
            | "enum_body"
            | "block"
            | "field_declaration"
            | "expression_statement"
    )
}

fn extract_name(node: Node<'_>, source: &[u8]) -> Option<String> {
    if let Some(name_node) = node.child_by_field_name("name") {
        return text_of(name_node, source);
    }
    // JSON object keys / YAML pairs
    if matches!(node.kind(), "pair" | "block_mapping_pair") {
        if let Some(key) = node.child_by_field_name("key") {
            return text_of(key, source);
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.is_named() {
                return text_of(child, source);
            }
        }
    }
    // Go / C-style declarators often nest the name.
    if node.kind().contains("declarator") || node.kind().ends_with("_item") {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if matches!(
                child.kind(),
                "identifier"
                    | "type_identifier"
                    | "property_identifier"
                    | "field_identifier"
                    | "constant"
                    | "name"
            ) {
                return text_of(child, source);
            }
        }
    }
    None
}

fn text_of(node: Node<'_>, source: &[u8]) -> Option<String> {
    let raw = node.utf8_text(source).ok()?.trim();
    if raw.is_empty() || raw.len() > 120 {
        return None;
    }
    Some(raw.to_string())
}

fn pretty_label(kind: &str, name: Option<&str>) -> String {
    let pretty_kind = kind
        .split('_')
        .filter(|s| !s.is_empty())
        .map(|s| {
            let mut chars = s.chars();
            match chars.next() {
                Some(c) => format!("{}{}", c.to_uppercase(), chars.as_str()),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    match name {
        Some(n) if !n.is_empty() => format!("{pretty_kind} · {n}"),
        _ => pretty_kind,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_java_structure() {
        let src = r#"
package com.example;
public class Hello {
  private int x;
  public void run() {}
}
"#;
        let ast = parse_ast("src/Hello.java", src, AstMode::Structure).unwrap();
        assert_eq!(ast.language, "java");
        assert_eq!(ast.mode, "structure");
        assert!(find_name(&ast.root, "Hello"));
        assert!(find_name(&ast.root, "run") || find_kind(&ast.root, "method_declaration"));
        assert!(ast.root.start_line >= 1);
    }

    #[test]
    fn parses_python_full() {
        let src = "def greet(name):\n    return name\n";
        let ast = parse_ast("a.py", src, AstMode::Full).unwrap();
        assert_eq!(ast.language, "python");
        assert_eq!(ast.mode, "full");
        assert!(find_name(&ast.root, "greet") || find_kind(&ast.root, "function_definition"));
    }

    #[test]
    fn parses_javascript_structure() {
        let src = "export function add(a, b) { return a + b; }\n";
        let ast = parse_ast("lib/math.js", src, AstMode::Structure).unwrap();
        assert_eq!(ast.language, "javascript");
        assert!(find_name(&ast.root, "add") || find_kind(&ast.root, "function_declaration"));
    }

    #[test]
    fn parses_typescript_and_tsx() {
        let ts = "export type Id = string;\nexport function id(x: Id): Id { return x; }\n";
        let ast = parse_ast("src/id.ts", ts, AstMode::Structure).unwrap();
        assert_eq!(ast.language, "typescript");
        assert!(find_name(&ast.root, "id") || find_kind(&ast.root, "function_declaration"));

        let tsx = "export function App() { return <div/>; }\n";
        let ast_tsx = parse_ast("src/App.tsx", tsx, AstMode::Full).unwrap();
        assert_eq!(ast_tsx.language, "typescript");
        assert!(find_name(&ast_tsx.root, "App") || find_kind(&ast_tsx.root, "function_declaration"));
    }

    #[test]
    fn parses_go_rust_c_cpp() {
        let go = "package main\nfunc Hello() {}\n";
        let go_ast = parse_ast("main.go", go, AstMode::Structure).unwrap();
        assert_eq!(go_ast.language, "go");
        assert!(find_name(&go_ast.root, "Hello") || find_kind(&go_ast.root, "function_declaration"));

        let rs = "fn main() {}\npub struct Point { x: i32 }\n";
        let rs_ast = parse_ast("src/main.rs", rs, AstMode::Structure).unwrap();
        assert_eq!(rs_ast.language, "rust");
        assert!(find_name(&rs_ast.root, "main") || find_kind(&rs_ast.root, "function_item"));
        assert!(find_name(&rs_ast.root, "Point") || find_kind(&rs_ast.root, "struct_item"));

        let c = "int add(int a, int b) { return a + b; }\n";
        let c_ast = parse_ast("add.c", c, AstMode::Structure).unwrap();
        assert_eq!(c_ast.language, "c");
        assert!(find_name(&c_ast.root, "add") || find_kind(&c_ast.root, "function_definition"));

        let cpp = "class Widget { public: void draw(); };\n";
        let cpp_ast = parse_ast("widget.cpp", cpp, AstMode::Structure).unwrap();
        assert_eq!(cpp_ast.language, "cpp");
        assert!(find_name(&cpp_ast.root, "Widget") || find_kind(&cpp_ast.root, "class_specifier"));
    }

    #[test]
    fn parses_json_and_yaml() {
        let json = r#"{ "name": "reaper", "version": 1 }"#;
        let json_ast = parse_ast("package.json", json, AstMode::Structure).unwrap();
        assert_eq!(json_ast.language, "json");
        assert!(find_name(&json_ast.root, "name") || find_kind(&json_ast.root, "pair"));

        let yaml = "name: reaper\nversion: 1\n";
        let yaml_ast = parse_ast("config.yaml", yaml, AstMode::Full).unwrap();
        assert_eq!(yaml_ast.language, "yaml");
        assert!(count_nodes(&yaml_ast.root) >= 2);
    }

    #[test]
    fn full_mode_keeps_more_named_nodes_than_structure() {
        let src = r#"
public class Demo {
  public int value() {
    int x = 1;
    return x + 1;
  }
}
"#;
        let structure = parse_ast("Demo.java", src, AstMode::Structure).unwrap();
        let full = parse_ast("Demo.java", src, AstMode::Full).unwrap();
        assert!(
            count_nodes(&full.root) >= count_nodes(&structure.root),
            "full={} structure={}",
            count_nodes(&full.root),
            count_nodes(&structure.root)
        );
    }

    #[test]
    fn rejects_unsupported_and_oversized() {
        assert!(parse_ast("a.md", "# hi", AstMode::Structure).is_err());
        let huge = "x".repeat(MAX_AST_BYTES + 1);
        assert!(parse_ast("a.java", &huge, AstMode::Structure).is_err());
    }

    #[test]
    fn mode_parse_and_language_mapping() {
        assert_eq!(AstMode::parse(None), AstMode::Structure);
        assert_eq!(AstMode::parse(Some("full")), AstMode::Full);
        assert_eq!(AstMode::parse(Some("AST")), AstMode::Full);
        assert_eq!(language_for_ast_path("src/A.java"), Some("java"));
        assert_eq!(language_for_ast_path("a.py"), Some("python"));
        assert_eq!(language_for_ast_path("a.ts"), Some("typescript"));
        assert_eq!(language_for_ast_path("a.go"), Some("go"));
        assert_eq!(language_for_ast_path("a.rs"), Some("rust"));
        assert_eq!(language_for_ast_path("a.json"), Some("json"));
        assert_eq!(language_for_ast_path("README.md"), None);
        assert!(supports_language("java"));
        assert!(!supports_language("markdown"));
    }

    #[test]
    fn ranges_are_one_based() {
        let src = "class A {}\n";
        let ast = parse_ast("A.java", src, AstMode::Full).unwrap();
        assert!(ast.root.start_line >= 1);
        assert!(ast.root.start_column >= 1);
        assert!(ast.root.end_line >= ast.root.start_line);
    }

    fn find_name(node: &AstNode, name: &str) -> bool {
        if node.name.as_deref() == Some(name) {
            return true;
        }
        node.children.iter().any(|c| find_name(c, name))
    }

    fn find_kind(node: &AstNode, kind: &str) -> bool {
        if node.kind == kind {
            return true;
        }
        node.children.iter().any(|c| find_kind(c, kind))
    }

    fn count_nodes(node: &AstNode) -> usize {
        1 + node.children.iter().map(count_nodes).sum::<usize>()
    }
}
