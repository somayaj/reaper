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

/// File extensions (and special basenames) that use each Settings → Compiler tool.
pub fn file_extensions_for_tool(tool_id: &str) -> &'static [&'static str] {
    match tool_id {
        "java" => &[".java"],
        "google-java-format" => &[".java"],
        "kotlin" => &[".kt", ".kts", ".gradle.kts"],
        "groovy" => &[".groovy", ".gvy", ".gy", ".gsh"],
        "gradle" => &[".gradle", ".gradle.kts", "gradlew"],
        "python" => &[".py", ".pyw"],
        "ruby" => &[".rb"],
        "bundle" => &[".rb", "Gemfile"],
        "rails" => &[".rb"],
        "rustc" => &[".rs"],
        "cargo" => &[".rs"],
        "go" => &[".go"],
        "node" => &[".js", ".mjs", ".cjs", ".jsx", ".json", ".jsonc"],
        "tsc" => &[".ts", ".tsx"],
        "php" => &[".php"],
        "clang" => &[".c", ".h", ".cpp", ".cc", ".cxx", ".hpp", ".hh"],
        "gcc" => &[".c", ".h", ".cpp", ".cc", ".cxx", ".hpp", ".hh"],
        "swiftc" => &[".swift"],
        "luac" => &[".lua"],
        "csc" => &[".cs"],
        "dart" => &[".dart"],
        "bash" => &[".sh", ".bash", ".zsh"],
        "yamllint" => &[".yml", ".yaml"],
        "yamlfmt" => &[".yml", ".yaml"],
        "jsonlint" => &[".json", ".jsonc"],
        "ajv" => &[".json"],
        "prettier" => &[
            ".js", ".mjs", ".cjs", ".jsx", ".ts", ".tsx", ".json", ".css", ".scss", ".less",
            ".md", ".html", ".xml", ".yml", ".yaml",
        ],
        _ => &[],
    }
}

/// Compiler tool ids used to validate or run the file (from extension / language).
pub fn compiler_tool_ids_for_path(path: &str) -> Vec<&'static str> {
    let lower = path.replace('\\', "/").to_lowercase();
    let base = lower.rsplit('/').next().unwrap_or(&lower);

    if base == "dockerfile" || base.starts_with("dockerfile.") {
        return vec!["bash"];
    }
    if base == "makefile" || base == "gnumakefile" {
        return vec!["bash"];
    }
    if base == "cmakelists.txt" {
        return vec!["bash"];
    }
    if base == "gemfile" || base.ends_with(".rb") {
        return vec!["ruby", "bundle"];
    }

    match language_for_path(path) {
        Some("java") => vec!["java"],
        Some("kotlin") => vec!["kotlin"],
        Some("groovy") => vec!["groovy"],
        Some("python") => vec!["python"],
        Some("ruby") => vec!["ruby", "bundle"],
        Some("rust") => vec!["rustc", "cargo"],
        Some("go") => vec!["go"],
        Some("javascript") => vec!["node"],
        Some("typescript") => vec!["tsc", "node"],
        Some("php") => vec!["php"],
        Some("csharp") => vec!["csc"],
        Some("swift") => vec!["swiftc"],
        Some("c") | Some("cpp") => vec!["clang", "gcc"],
        Some("shell") => vec!["bash"],
        Some("lua") => vec!["luac"],
        Some("dart") => vec!["dart"],
        Some("json") | Some("jsonc") => vec!["jsonlint"],
        // YAML: yamllint (+ optional actionlint/kubeconform by content).
        Some("yaml") => vec!["yamllint"],
        _ => Vec::new(),
    }
}

/// Primary compiler tool id for a path (first match from extension mapping).
pub fn primary_compiler_tool_id(path: &str) -> Option<&'static str> {
    compiler_tool_ids_for_path(path).first().copied()
}

/// Language keywords for editor autocomplete (Monaco + workspace completions API).
pub fn keywords_for_path(path: &str) -> &'static [&'static str] {
    match language_for_path(path) {
        Some("rust") => RUST_KW,
        Some("python") => PYTHON_KW,
        Some("go") => GO_KW,
        Some("javascript") => JS_KW,
        Some("typescript") => TS_KW,
        Some("java") => JAVA_KW,
        Some("kotlin") => KOTLIN_KW,
        Some("groovy") => GROOVY_KW,
        Some("ruby") => RUBY_KW,
        Some("php") => PHP_KW,
        Some("csharp") => CSHARP_KW,
        Some("swift") => SWIFT_KW,
        Some("c") => C_KW,
        Some("cpp") => CPP_KW,
        Some("shell") => SHELL_KW,
        Some("lua") => LUA_KW,
        Some("dart") => DART_KW,
        Some("sql") => SQL_KW,
        Some("yaml") => YAML_KW,
        Some("toml") => TOML_KW,
        Some("dockerfile") => DOCKERFILE_KW,
        Some("makefile") => MAKEFILE_KW,
        Some("cmake") => CMAKE_KW,
        Some("html") => HTML_KW,
        Some("css") => CSS_KW,
        Some("scss") => SCSS_KW,
        Some("protobuf") => PROTO_KW,
        Some("graphql") => GRAPHQL_KW,
        Some("ini") => INI_KW,
        _ => &[],
    }
}

const RUST_KW: &[&str] = &[
    "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum",
    "extern", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut",
    "pub", "ref", "return", "self", "Self", "static", "struct", "super", "trait", "true",
    "type", "unsafe", "use", "where", "while", "false",
];
const PYTHON_KW: &[&str] = &[
    "and", "as", "assert", "async", "await", "break", "class", "continue", "def", "del",
    "elif", "else", "except", "False", "finally", "for", "from", "global", "if", "import",
    "in", "is", "lambda", "None", "nonlocal", "not", "or", "pass", "raise", "return",
    "True", "try", "while", "with", "yield",
];
const GO_KW: &[&str] = &[
    "break", "case", "chan", "const", "continue", "default", "defer", "else", "fallthrough",
    "for", "func", "go", "goto", "if", "import", "interface", "map", "package", "range",
    "return", "struct", "switch", "type", "var",
];
const JS_KW: &[&str] = &[
    "await", "break", "case", "catch", "class", "const", "continue", "debugger", "default",
    "delete", "do", "else", "export", "extends", "false", "finally", "for", "function",
    "if", "import", "in", "instanceof", "let", "new", "null", "return", "super", "switch",
    "this", "throw", "true", "try", "typeof", "var", "void", "while", "with", "yield",
];
const TS_KW: &[&str] = &[
    "abstract", "as", "await", "break", "case", "catch", "class", "const", "continue",
    "declare", "default", "delete", "do", "else", "enum", "export", "extends", "false",
    "finally", "for", "from", "function", "if", "implements", "import", "in", "interface",
    "let", "namespace", "new", "null", "package", "private", "protected", "public",
    "readonly", "return", "static", "super", "switch", "this", "throw", "true", "try",
    "type", "undefined", "var", "void", "while", "with", "yield",
];
const JAVA_KW: &[&str] = &[
    "abstract", "assert", "break", "case", "catch", "class", "const", "continue", "default",
    "do", "else", "enum", "extends", "final", "finally", "for", "if", "implements", "import",
    "instanceof", "interface", "native", "new", "package", "private", "protected", "public",
    "return", "static", "strictfp", "super", "switch", "synchronized", "this", "throw",
    "throws", "transient", "try", "void", "volatile", "while", "true", "false", "null",
];
const KOTLIN_KW: &[&str] = &[
    "as", "break", "class", "continue", "do", "else", "false", "for", "fun", "if", "in",
    "interface", "is", "null", "object", "package", "return", "super", "this", "throw",
    "true", "try", "typealias", "val", "var", "when", "while", "by", "constructor",
    "delegate", "dynamic", "field", "file", "finally", "get", "import", "init", "param",
    "property", "receiver", "set", "setparam", "where", "actual", "abstract", "annotation",
    "companion", "const", "crossinline", "data", "enum", "external", "final", "infix",
    "inline", "inner", "internal", "lateinit", "noinline", "open", "operator", "out",
    "override", "private", "protected", "public", "reified", "sealed", "suspend",
    "tailrec", "vararg",
];
const GROOVY_KW: &[&str] = &[
    "as", "assert", "break", "case", "catch", "class", "const", "continue", "def", "default",
    "do", "else", "enum", "extends", "false", "final", "finally", "for", "goto", "if",
    "implements", "import", "in", "instanceof", "interface", "new", "null", "package",
    "return", "static", "super", "switch", "this", "throw", "throws", "trait", "true",
    "try", "while", "with",
];
const RUBY_KW: &[&str] = &[
    "BEGIN", "END", "alias", "and", "begin", "break", "case", "class", "def", "defined?",
    "do", "else", "elsif", "end", "ensure", "false", "for", "if", "in", "module", "next",
    "nil", "not", "or", "redo", "rescue", "retry", "return", "self", "super", "then",
    "true", "undef", "unless", "until", "when", "while", "yield",
];
const PHP_KW: &[&str] = &[
    "abstract", "and", "array", "as", "break", "callable", "case", "catch", "class",
    "clone", "const", "continue", "declare", "default", "do", "else", "elseif", "enddeclare",
    "endfor", "endforeach", "endif", "endswitch", "endwhile", "extends", "final", "finally",
    "for", "foreach", "function", "global", "goto", "if", "implements", "interface",
    "instanceof", "insteadof", "match", "namespace", "new", "null", "or", "print",
    "private", "protected", "public", "readonly", "return", "static", "switch", "throw",
    "trait", "try", "use", "var", "while", "xor", "yield", "true", "false",
];
const CSHARP_KW: &[&str] = &[
    "abstract", "as", "base", "bool", "break", "byte", "case", "catch", "char", "checked",
    "class", "const", "continue", "decimal", "default", "delegate", "do", "double", "else",
    "enum", "event", "explicit", "extern", "false", "finally", "fixed", "float", "for",
    "foreach", "goto", "if", "implicit", "in", "int", "interface", "internal", "is", "lock",
    "long", "namespace", "new", "null", "object", "operator", "out", "override", "params",
    "private", "protected", "public", "readonly", "ref", "return", "sbyte", "sealed",
    "short", "sizeof", "stackalloc", "static", "string", "struct", "switch", "this",
    "throw", "true", "try", "typeof", "uint", "ulong", "unchecked", "unsafe", "ushort",
    "using", "virtual", "void", "volatile", "while", "async", "await", "record", "var",
];
const SWIFT_KW: &[&str] = &[
    "associatedtype", "break", "case", "catch", "class", "continue", "default", "defer",
    "do", "else", "enum", "extension", "fallthrough", "fileprivate", "for", "func", "guard",
    "if", "import", "in", "init", "inout", "internal", "is", "let", "nil", "open",
    "operator", "private", "protocol", "public", "repeat", "rethrows", "return", "self",
    "static", "struct", "subscript", "super", "switch", "throw", "throws", "true", "try",
    "typealias", "var", "weak", "where", "while", "false",
];
const C_KW: &[&str] = &[
    "auto", "break", "case", "char", "const", "continue", "default", "do", "double", "else",
    "enum", "extern", "float", "for", "goto", "if", "inline", "int", "long", "register",
    "return", "short", "signed", "sizeof", "static", "struct", "switch", "typedef", "union",
    "unsigned", "void", "volatile", "while",
];
const CPP_KW: &[&str] = &[
    "alignas", "alignof", "and", "and_eq", "asm", "auto", "bitand", "bitor", "bool",
    "break", "case", "catch", "char", "class", "const", "constexpr", "continue", "decltype",
    "default", "delete", "do", "double", "dynamic_cast", "else", "enum", "explicit",
    "export", "extern", "false", "float", "for", "friend", "goto", "if", "inline", "int",
    "long", "mutable", "namespace", "new", "noexcept", "not", "not_eq", "nullptr", "operator",
    "or", "or_eq", "private", "protected", "public", "register", "reinterpret_cast",
    "return", "short", "signed", "sizeof", "static", "static_cast", "struct", "switch",
    "template", "this", "throw", "true", "try", "typedef", "typeid", "typename", "union",
    "unsigned", "using", "virtual", "void", "volatile", "wchar_t", "while", "xor", "xor_eq",
];
const SHELL_KW: &[&str] = &[
    "if", "then", "else", "elif", "fi", "for", "in", "do", "done", "case", "esac", "while",
    "until", "function", "select", "time", "coproc", "local", "export", "readonly", "declare",
    "unset", "shift", "exit", "return", "source", "alias", "trap", "set", "echo", "printf",
    "test", "cd", "pwd", "exec", "eval",
];
const LUA_KW: &[&str] = &[
    "and", "break", "do", "else", "elseif", "end", "false", "for", "function", "goto", "if",
    "in", "local", "nil", "not", "or", "repeat", "return", "then", "true", "until", "while",
];
const DART_KW: &[&str] = &[
    "abstract", "as", "assert", "async", "await", "break", "case", "catch", "class", "const",
    "continue", "default", "deferred", "do", "else", "enum", "export", "extends", "external",
    "factory", "false", "final", "finally", "for", "get", "if", "implements", "import", "in",
    "interface", "is", "late", "library", "mixin", "new", "null", "on", "operator", "part",
    "required", "rethrow", "return", "set", "show", "static", "super", "switch", "sync",
    "this", "throw", "true", "try", "typedef", "var", "void", "while", "with", "yield",
];
const SQL_KW: &[&str] = &[
    "SELECT", "FROM", "WHERE", "INSERT", "UPDATE", "DELETE", "CREATE", "DROP", "ALTER",
    "TABLE", "INDEX", "VIEW", "JOIN", "LEFT", "RIGHT", "INNER", "OUTER", "ON", "AND", "OR",
    "NOT", "NULL", "IS", "IN", "AS", "ORDER", "BY", "GROUP", "HAVING", "LIMIT", "OFFSET",
    "UNION", "ALL", "DISTINCT", "VALUES", "SET", "INTO", "PRIMARY", "KEY", "FOREIGN",
    "REFERENCES", "CONSTRAINT", "DEFAULT", "CASCADE", "EXISTS", "CASE", "WHEN", "THEN",
    "ELSE", "END", "BEGIN", "COMMIT", "ROLLBACK", "TRANSACTION",
];
const YAML_KW: &[&str] = &[
    "true", "false", "null", "yes", "no", "on", "apiVersion", "kind", "metadata", "spec",
    "name", "labels", "annotations", "namespace", "containers", "ports", "env", "image",
    "replicas", "selector", "template", "data", "type", "jobs", "steps", "runs-on", "uses",
    "with", "run", "on", "push", "pull_request", "workflow", "permissions", "strategy",
];
const TOML_KW: &[&str] = &["true", "false"];
const DOCKERFILE_KW: &[&str] = &[
    "FROM", "RUN", "CMD", "LABEL", "MAINTAINER", "EXPOSE", "ENV", "ADD", "COPY", "ENTRYPOINT",
    "VOLUME", "USER", "WORKDIR", "ARG", "ONBUILD", "STOPSIGNAL", "HEALTHCHECK", "SHELL",
];
const MAKEFILE_KW: &[&str] = &[
    "ifeq", "ifneq", "else", "endif", "include", "export", "define", "endef", ".PHONY",
    "MAKEFLAGS", "SHELL", "wildcard", "patsubst",
];
const CMAKE_KW: &[&str] = &[
    "cmake_minimum_required", "project", "add_executable", "add_library", "target_link_libraries",
    "find_package", "include_directories", "set", "if", "else", "endif", "foreach", "endforeach",
    "function", "endfunction", "option", "install", "add_subdirectory",
];
const HTML_KW: &[&str] = &[
    "html", "head", "body", "title", "meta", "link", "script", "style", "div", "span", "p",
    "a", "img", "ul", "ol", "li", "table", "tr", "td", "th", "form", "input", "button",
    "header", "footer", "nav", "section", "article", "main", "h1", "h2", "h3", "label",
    "textarea", "select", "option", "br", "hr", "pre", "code", "blockquote",
];
const CSS_KW: &[&str] = &[
    "display", "position", "margin", "padding", "border", "width", "height", "color",
    "background", "font-size", "font-weight", "flex", "grid", "align-items", "justify-content",
    "overflow", "z-index", "top", "left", "right", "bottom", "opacity", "visibility",
    "transition", "transform", "animation", "content", "cursor", "pointer-events",
];
const SCSS_KW: &[&str] = &[
    "@import", "@mixin", "@include", "@extend", "@media", "@keyframes", "@function", "@return",
    "@if", "@else", "@for", "@each", "@while", "$primary", "$secondary",
];
const PROTO_KW: &[&str] = &[
    "syntax", "package", "import", "option", "message", "enum", "service", "rpc", "returns",
    "oneof", "map", "reserved", "extend", "extensions", "stream",
];
const GRAPHQL_KW: &[&str] = &[
    "query", "mutation", "subscription", "type", "interface", "union", "enum", "input",
    "schema", "scalar", "implements", "extend", "directive", "on", "fragment", "true", "false",
    "null",
];
const INI_KW: &[&str] = &[
    "spring.application.name", "server.port", "logging.level", "management.endpoints.web.exposure.include",
];

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
    fn keywords_for_rust_and_yaml() {
        assert!(keywords_for_path("main.rs").contains(&"fn"));
        assert!(keywords_for_path("deploy.yaml").contains(&"apiVersion"));
    }

    #[test]
    fn compiler_tool_ids_from_extension() {
        assert_eq!(compiler_tool_ids_for_path("api/openapi.yaml"), vec!["yamllint"]);
        assert_eq!(compiler_tool_ids_for_path("package.json"), vec!["jsonlint"]);
        assert_eq!(compiler_tool_ids_for_path("src/main.py"), vec!["python"]);
        assert_eq!(compiler_tool_ids_for_path("src/main.rs"), vec!["rustc", "cargo"]);
        assert_eq!(compiler_tool_ids_for_path("app.tsx"), vec!["tsc", "node"]);
    }

    #[test]
    fn scans_mixed_repo() {
        let ws = std::env::temp_dir().join("reaper-lang-scan");
        let _ = std::fs::remove_dir_all(&ws);
        std::fs::create_dir_all(ws.join("src")).unwrap();
        std::fs::create_dir_all(ws.join("lib")).unwrap();
        std::fs::write(ws.join("src/main.rs"), "fn main() {}").unwrap();
        std::fs::write(ws.join("lib/util.py"), "def helper(): pass").unwrap();
        let langs = scan_workspace_languages(&ws).unwrap();
        assert!(langs.contains(&"rust".to_string()));
        assert!(langs.contains(&"python".to_string()));
        let _ = std::fs::remove_dir_all(&ws);
    }
}
