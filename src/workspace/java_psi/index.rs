use super::parse::{CompilationUnit, MemberKind, TypeDecl, TypeKind};
use super::parse::parse_compilation_unit;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JavaSymbol {
    pub name: String,
    pub qualified: String,
    pub kind: String,
    pub line: u32,
    pub column: u32,
}

pub fn package_name(content: &str, rel_path: &str) -> Option<String> {
    parse_compilation_unit(content)
        .package
        .or_else(|| infer_package_from_path(rel_path))
}

pub fn infer_package_from_path(rel_path: &str) -> Option<String> {
    let norm = rel_path.replace('\\', "/");
    for marker in ["src/main/java/", "src/test/java/"] {
        if let Some(rest) = norm.split_once(marker).map(|(_, tail)| tail) {
            if let Some((pkg_path, file)) = rest.rsplit_once('/') {
                if file.ends_with(".java") && !pkg_path.is_empty() {
                    return Some(pkg_path.replace('/', "."));
                }
            }
        }
    }
    let parts: Vec<&str> = norm.split('/').collect();
    for (i, part) in parts.iter().enumerate() {
        if !matches!(*part, "org" | "com" | "javax" | "jakarta" | "java" | "kotlin") {
            continue;
        }
        if i + 1 >= parts.len() {
            continue;
        }
        let file = parts[parts.len() - 1];
        if !file.ends_with(".java") {
            continue;
        }
        let pkg_parts = &parts[i..parts.len() - 1];
        if !pkg_parts.is_empty() {
            return Some(pkg_parts.join("."));
        }
    }
    None
}

pub fn index_source(rel_path: &str, index_members: bool, unit: &CompilationUnit) -> Vec<JavaSymbol> {
    let pkg = unit
        .package
        .clone()
        .or_else(|| infer_package_from_path(rel_path));
    let pkg_prefix = pkg.map(|p| format!("{p}."));

    let mut out = Vec::new();
    for ty in &unit.types {
        index_type(rel_path, &pkg_prefix, ty, index_members, &mut out);
    }
    out
}

fn index_type(
    rel_path: &str,
    pkg_prefix: &Option<String>,
    ty: &TypeDecl,
    index_members: bool,
    out: &mut Vec<JavaSymbol>,
) {
    let qualified = qualify(&ty.name, pkg_prefix);
    out.push(JavaSymbol {
        name: ty.name.clone(),
        qualified: qualified.clone(),
        kind: type_kind_label(ty.kind).to_string(),
        line: ty.line,
        column: ty.column,
    });

    if index_members {
        for member in &ty.members {
            if matches!(member.kind, MemberKind::Constructor) {
                continue;
            }
            let kind = match member.kind {
                MemberKind::Method => "method",
                MemberKind::Field => "field",
                MemberKind::Constructor => "method",
            };
            out.push(JavaSymbol {
                name: member.name.clone(),
                qualified: format!("{qualified}.{}", member.name),
                kind: kind.to_string(),
                line: member.line,
                column: member.column,
            });
        }
    }

    for nested in &ty.nested {
        index_type(rel_path, &Some(format!("{qualified}.")), nested, index_members, out);
    }
}

fn qualify(name: &str, pkg_prefix: &Option<String>) -> String {
    match pkg_prefix {
        Some(prefix) => format!("{prefix}{name}"),
        None => name.to_string(),
    }
}

fn type_kind_label(kind: TypeKind) -> &'static str {
    match kind {
        TypeKind::Class => "class",
        TypeKind::Interface => "interface",
        TypeKind::Enum => "enum",
        TypeKind::Record => "record",
        TypeKind::Annotation => "annotation",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::java_psi::parse_compilation_unit;

    #[test]
    fn indexes_methods_and_fields() {
        let src = "package com.example;\npublic class Foo {\n  private int count;\n  void run() {}\n}";
        let unit = parse_compilation_unit(src);
        let symbols = index_source("src/main/java/com/example/Foo.java", true, &unit);
        assert!(symbols.iter().any(|s| s.name == "Foo" && s.kind == "class"));
        assert!(symbols.iter().any(|s| s.name == "count" && s.kind == "field"));
        assert!(symbols.iter().any(|s| s.name == "run" && s.kind == "method"));
    }
}
