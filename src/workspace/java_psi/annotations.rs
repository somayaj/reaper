use super::imports::ImportMap;
use super::lexer::{TokenKind, lex};
use super::parse::parse_compilation_unit;

/// Simple names of annotations used in source (`@Name`), in source order.
pub fn annotation_simple_names(content: &str) -> Vec<String> {
    let tokens = lex(content);
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut i = 0;
    while i + 1 < tokens.len() {
        if matches!(tokens[i].kind, TokenKind::At) {
            if let TokenKind::Identifier(name) = tokens[i + 1].kind.clone() {
                if seen.insert(name.clone()) {
                    out.push(name);
                }
            }
        }
        i += 1;
    }
    out
}

/// True when the file imports from `lombok.*`.
pub fn file_uses_lombok_annotations(content: &str) -> bool {
    let imports = parse_compilation_unit(content).imports;
    if imports
        .explicit
        .values()
        .any(|fqcn| fqcn.starts_with("lombok."))
        || imports.wildcards.iter().any(|w| w.starts_with("lombok."))
    {
        return true;
    }
    content.contains("import lombok.")
}

/// True when a javac missing-symbol message matches a Lombok annotation in this file.
pub fn lombok_symbol_in_message(message: &str, content: &str) -> bool {
    if !super::super::java_ecosystem::file_uses_lombok(content)
        && !file_uses_lombok_annotations(content)
    {
        return false;
    }
    let imports = parse_compilation_unit(content).imports;
    annotation_simple_names(content)
        .into_iter()
        .any(|name| lombok_annotation_symbol_match(message, content, &name, &imports))
}

fn lombok_annotation_symbol_match(
    message: &str,
    content: &str,
    name: &str,
    imports: &ImportMap,
) -> bool {
    if !message.contains(name) {
        return false;
    }
    if let Some(fqcn) = imports.explicit.get(name) {
        return fqcn.starts_with("lombok.");
    }
    content.contains(&format!("@{name}"))
        && (file_uses_lombok_annotations(content)
            || super::super::java_ecosystem::file_uses_lombok(content))
}

/// Missing-symbol / missing-package diag for an imported type whose library is not on the classpath.
pub fn stale_imported_dependency_diag(
    message: &str,
    content: &str,
    package_on_classpath: impl Fn(&str) -> bool,
) -> bool {
    let lower = message.to_ascii_lowercase();
    let missing_package = lower.contains("package") && lower.contains("does not exist");
    let missing_symbol = lower.contains("cannot find symbol");
    if !missing_package && !missing_symbol {
        return false;
    }

    let imports = parse_compilation_unit(content).imports;
    for (simple, fqcn) in &imports.explicit {
        let pkg = fqcn
            .rsplit_once('.')
            .map(|(parent, _)| parent)
            .unwrap_or(fqcn.as_str());
        if missing_package {
            let pkg_missing = lower.contains(&format!("package {pkg}"))
                || lower.contains(&format!("package {pkg}."));
            if pkg_missing && !package_on_classpath(pkg) {
                return true;
            }
        }
        if missing_symbol
            && message.contains(simple)
            && symbol_referenced_in_source(content, simple)
            && !package_on_classpath(pkg)
        {
            return true;
        }
    }
    false
}

fn symbol_referenced_in_source(content: &str, simple: &str) -> bool {
    content.contains(&format!("@{simple}"))
        || content.contains(&format!(" {simple} "))
        || content.contains(&format!("({simple}"))
        || content.contains(&format!("<{simple}"))
        || content.contains(&format!(".{simple}"))
        || content.contains(&format!(" {simple},"))
        || content.contains(&format!("({simple},"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collects_annotation_names_from_lexer() {
        let content = "@SpringBootApplication\n@Slf4j\npublic class App {}\n";
        assert_eq!(
            annotation_simple_names(content),
            vec!["SpringBootApplication".to_string(), "Slf4j".to_string()]
        );
    }

    #[test]
    fn matches_lombok_slf4j_from_source_annotations() {
        let content = "@Slf4j\npublic class App {}\n";
        assert!(lombok_symbol_in_message(
            "cannot find symbol\n  symbol:   class Slf4j",
            content,
        ));
    }

    #[test]
    fn matches_lombok_imported_annotation() {
        let content = "import lombok.RequiredArgsConstructor;\n@RequiredArgsConstructor\nclass T {}\n";
        assert!(lombok_symbol_in_message(
            "cannot find symbol\n  symbol:   class RequiredArgsConstructor",
            content,
        ));
    }

    #[test]
    fn ignores_spring_annotation_when_imported_from_spring() {
        let content = r#"
import org.springframework.boot.autoconfigure.SpringBootApplication;
@SpringBootApplication
@Slf4j
class App {}
"#;
        assert!(!lombok_symbol_in_message(
            "cannot find symbol\n  symbol:   class SpringBootApplication",
            content,
        ));
    }

    #[test]
    fn parses_validation_imports_on_record() {
        let content = r#"package com.example.product.model;

import jakarta.validation.constraints.NotBlank;
import jakarta.validation.constraints.NotNull;
import jakarta.validation.constraints.Positive;
import java.math.BigDecimal;

public record CreateProductRequest(
        @NotBlank String name,
        @NotNull @Positive BigDecimal price) {
}
"#;
        let unit = super::super::parse::parse_compilation_unit(content);
        assert_eq!(unit.imports.explicit.get("NotBlank").map(String::as_str), Some("jakarta.validation.constraints.NotBlank"));
        assert_eq!(unit.imports.explicit.get("Positive").map(String::as_str), Some("jakarta.validation.constraints.Positive"));
    }

    #[test]
    fn stale_imported_validation_annotation_when_api_missing() {
        let content = r#"
import jakarta.validation.constraints.NotBlank;
public record CreateUserRequest(@NotBlank String name) {}
"#;
        assert!(stale_imported_dependency_diag(
            "error: cannot find symbol\n  symbol:   class NotBlank\n  location: class CreateUserRequest",
            content,
            |_| false,
        ));
        assert!(!stale_imported_dependency_diag(
            "error: cannot find symbol\n  symbol:   class NotBlank\n  location: class CreateUserRequest",
            content,
            |_| true,
        ));
    }
}
