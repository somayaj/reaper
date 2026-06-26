use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::Serialize;

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

const SOURCE_EXTS: &[&str] = &[
    "java", "groovy", "gradle", "kt", "kts", "rs", "js", "mjs", "cjs", "ts", "tsx", "jsx",
    "py", "go", "cs", "rb", "php", "swift", "c", "h", "cpp", "hpp", "cc", "sh",
];

#[derive(Debug, Clone, Serialize)]
pub struct SymbolLocation {
    pub name: String,
    pub kind: String,
    pub path: String,
    pub line: u32,
    pub column: u32,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, Hash)]
pub struct ClassSearchHit {
    pub name: String,
    pub qualified: String,
    pub kind: String,
    pub path: String,
    pub line: u32,
    pub column: u32,
}

pub fn class_name_match_score(query: &str, name: &str, qualified: &str) -> Option<u32> {
    let q = query.trim();
    if q.is_empty() {
        return Some(0);
    }
    let q_lower = q.to_lowercase();
    let name_lower = name.to_lowercase();
    let qual_lower = qualified.to_lowercase();

    if name_lower == q_lower {
        return Some(1000);
    }
    if qual_lower == q_lower {
        return Some(950);
    }
    if name_lower.starts_with(&q_lower) {
        return Some(850);
    }
    if matches_initials(q, name) {
        return Some(750);
    }
    if name_lower.contains(&q_lower) {
        return Some(650);
    }
    if qual_lower.contains(&q_lower) {
        return Some(500);
    }
    if q.contains('.') && qual_lower.ends_with(&q_lower) {
        return Some(900);
    }
    None
}

fn matches_initials(query: &str, name: &str) -> bool {
    let q: Vec<char> = query
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect();
    if q.is_empty() {
        return false;
    }
    let mut qi = 0usize;
    let mut prev_lower = true;
    for ch in name.chars() {
        if qi >= q.len() {
            return true;
        }
        if ch.is_ascii_alphanumeric() && (prev_lower || qi == 0) && ch.eq_ignore_ascii_case(&q[qi]) {
            qi += 1;
        }
        prev_lower = ch.is_ascii_lowercase();
    }
    qi >= q.len()
}

pub fn find_definition(
    ws: &Path,
    from_path: &str,
    line: u32,
    column: u32,
    content: &str,
) -> Result<Option<SymbolLocation>> {
    let symbol = match (
        crate::workspace::ruby_nav::is_ruby_path(from_path)
            .then(|| crate::workspace::ruby_nav::symbol_at(content, line, column))
            .flatten(),
        word_at(content, line, column),
    ) {
        (Some(s), _) | (_, Some(s)) if !s.is_empty() => s,
        _ => return Ok(None),
    };

    if is_keyword(&symbol) {
        return Ok(None);
    }

    if crate::workspace::ruby_nav::is_ruby_path(from_path) {
        if let Some(hit) = crate::workspace::ruby_nav::find_rails_constant(ws, &symbol)? {
            return Ok(Some(hit));
        }
    }

    let mut hits = Vec::new();
    if let Some(hit) = find_in_content(&symbol, from_path, content) {
        hits.push(hit);
    }
    collect_definitions(ws, ws, from_path, &symbol, &mut hits)?;
    Ok(best_definition(&hits, &symbol, from_path))
}

pub(crate) fn word_at(content: &str, line: u32, column: u32) -> Option<String> {
    let line_text = content.lines().nth(line.saturating_sub(1) as usize)?;
    let col = column.saturating_sub(1) as usize;

    if let Some(at) = line_text[..col.min(line_text.len())].rfind('@') {
        let after = &line_text[at + 1..];
        let name: String = after
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        if !name.is_empty() {
            return Some(name);
        }
    }

    if col >= line_text.len() {
        return word_before(line_text, line_text.len());
    }
    if is_ident_char(line_text.as_bytes()[col]) {
        let start = (0..=col)
            .rev()
            .find(|&i| !is_ident_char(line_text.as_bytes()[i]))
            .map(|i| i + 1)
            .unwrap_or(0);
        let end = (col..line_text.len())
            .find(|&i| !is_ident_char(line_text.as_bytes()[i]))
            .unwrap_or(line_text.len());
        return Some(line_text[start..end].to_string());
    }
    word_before(line_text, col)
}

pub(crate) fn java_method_name_on_line(line: &str) -> Option<String> {
    let trimmed = line.split("//").next()?.trim();
    if trimmed.is_empty() || !trimmed.contains('(') {
        return None;
    }
    if trimmed.starts_with("import ")
        || trimmed.starts_with("package ")
        || trimmed.starts_with('@')
        || trimmed.starts_with('*')
    {
        return None;
    }
    for pat in ["if (", "while (", "for (", "catch (", "switch (", "return ", "throw ", "new "] {
        if trimmed.contains(pat) {
            return None;
        }
    }

    let paren = trimmed.find('(')?;
    let before = trimmed[..paren].trim();
    if before.is_empty() || before.contains('.') {
        return None;
    }

    let name = before
        .rsplit(|c: char| c.is_whitespace() || c == '<' || c == '>')
        .next()?
        .trim();
    if name.is_empty() || is_keyword(name) {
        return None;
    }
    const PRIMITIVES: &[&str] = &[
        "void", "int", "long", "boolean", "char", "byte", "short", "float", "double",
    ];
    if PRIMITIVES.contains(&name) {
        return None;
    }

    Some(name.to_string())
}

pub(crate) fn java_class_from_source_path(path: &str) -> Option<String> {
    for prefix in ["src/main/java/", "src/test/java/"] {
        if let Some(rest) = path.strip_prefix(prefix) {
            return rest
                .strip_suffix(".java")
                .map(|s| s.replace('\\', "/").replace('/', "."));
        }
    }
    None
}

pub(crate) fn java_member_qualifier(content: &str, line: u32, column: u32, symbol: &str) -> Option<String> {
    let line_text = content.lines().nth(line.saturating_sub(1) as usize)?;
    let col = column.saturating_sub(1) as usize;
    let needle = format!(".{symbol}");
    let mut end = line_text.len();
    while end > 0 {
        let segment = &line_text[..end];
        let pos = segment.rfind(&needle)?;
        let sym_start = pos + 1;
        let sym_end = sym_start + symbol.len();
        if col >= sym_start && col <= sym_end {
            let qual = line_text[..pos].trim();
            let simple = qual.rsplit('.').next()?.trim();
            if simple.is_empty() || is_keyword(simple) {
                return None;
            }
            return Some(simple.to_string());
        }
        end = pos;
    }
    None
}

fn word_before(line: &str, col: usize) -> Option<String> {
    if col == 0 {
        return None;
    }
    let end = col.min(line.len());
    let start = (0..end)
        .rev()
        .find(|&i| !is_ident_char(line.as_bytes()[i]))
        .map(|i| i + 1)
        .unwrap_or(0);
    if start >= end {
        return None;
    }
    Some(line[start..end].to_string())
}

fn is_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
}

pub(crate) fn is_keyword(word: &str) -> bool {
    matches!(
        word,
        "if" | "else"
            | "for"
            | "while"
            | "return"
            | "import"
            | "package"
            | "new"
            | "true"
            | "false"
            | "nil"
            | "null"
            | "this"
            | "super"
            | "public"
            | "private"
            | "protected"
            | "static"
            | "void"
            | "int"
            | "long"
            | "var"
            | "val"
            | "fun"
            | "def"
            | "fn"
            | "func"
            | "class"
            | "interface"
            | "enum"
            | "struct"
            | "trait"
            | "mod"
            | "type"
            | "object"
            | "extends"
            | "implements"
            | "throws"
            | "try"
            | "catch"
            | "finally"
            | "switch"
            | "case"
            | "break"
            | "continue"
            | "default"
            | "in"
            | "as"
            | "with"
            | "module"
            | "end"
            | "elsif"
            | "when"
            | "then"
            | "begin"
            | "ensure"
            | "raise"
            | "unless"
            | "until"
            | "yield"
            | "alias"
            | "and"
            | "or"
            | "not"
            | "redo"
            | "retry"
            | "undef"
            | "defined"
            | "require"
            | "include"
            | "extend"
    )
}

pub(crate) fn find_in_content(symbol: &str, path: &str, content: &str) -> Option<SymbolLocation> {
    for (idx, line) in content.lines().enumerate() {
        if let Some(loc) = definition_on_line(line, symbol, path, idx as u32 + 1) {
            return Some(loc);
        }
    }
    None
}

fn definition_on_line(line: &str, symbol: &str, path: &str, line_no: u32) -> Option<SymbolLocation> {
    let trimmed = line.trim();
    if trimmed.starts_with("//")
        || trimmed.starts_with('#')
        || trimmed.starts_with('*')
        || trimmed.starts_with("import ")
        || trimmed.starts_with("package ")
    {
        return None;
    }

    let prefixes = [
        ("class", "class"),
        ("module", "module"),
        ("interface", "interface"),
        ("enum", "enum"),
        ("record", "record"),
        ("struct", "struct"),
        ("trait", "trait"),
        ("object", "object"),
        ("mod", "mod"),
        ("type", "type"),
        ("def", "def"),
        ("fun", "fun"),
        ("fn", "fn"),
        ("func", "func"),
        ("function", "function"),
    ];

    for (kw, kind) in prefixes {
        if let Some(pos) = find_keyword_definition(line, kw, symbol) {
            let name_start = pos + kw.len() + 1;
            let col = line[..name_start].chars().count() as u32 + 1;
            return Some(SymbolLocation {
                name: symbol.to_string(),
                kind: kind.to_string(),
                path: path.to_string(),
                line: line_no,
                column: col,
            });
        }
    }

    if let Some(method) = java_method_name_on_line(line) {
        if method == symbol {
            let col = line
                .find(&method)
                .map(|i| i as u32 + 1)
                .unwrap_or(1);
            return Some(SymbolLocation {
                name: symbol.to_string(),
                kind: "method".into(),
                path: path.to_string(),
                line: line_no,
                column: col,
            });
        }
    }

    if path.ends_with(".rb") {
        if let Some(method) = crate::workspace::ruby_nav::ruby_method_on_line(line) {
            if method == symbol {
                let col = line.find(&method).map(|i| i as u32 + 1).unwrap_or(1);
                return Some(SymbolLocation {
                    name: symbol.to_string(),
                    kind: "method".into(),
                    path: path.to_string(),
                    line: line_no,
                    column: col,
                });
            }
        }
        if let Some(method) = crate::workspace::ruby_nav::ruby_class_method_on_line(line) {
            if method == symbol {
                let col = line
                    .find(&method)
                    .map(|i| i as u32 + 1)
                    .unwrap_or(1);
                return Some(SymbolLocation {
                    name: symbol.to_string(),
                    kind: "method".into(),
                    path: path.to_string(),
                    line: line_no,
                    column: col,
                });
            }
        }
    }

    if let Some(pos) = find_keyword_definition(line, "def", symbol) {
        let col = line[..pos + 3].chars().count() as u32 + 1;
        return Some(SymbolLocation {
            name: symbol.to_string(),
            kind: "def".into(),
            path: path.to_string(),
            line: line_no,
            column: col,
        });
    }

    None
}

fn find_keyword_definition(line: &str, keyword: &str, symbol: &str) -> Option<usize> {
    let needle = format!("{keyword} {symbol}");
    let needle_bytes = needle.as_bytes();
    let bytes = line.as_bytes();
    if needle_bytes.len() > bytes.len() {
        return None;
    }
    let mut i = 0;
    while i + needle_bytes.len() <= bytes.len() {
        if &bytes[i..i + needle_bytes.len()] == needle_bytes {
            let after = i + needle_bytes.len();
            if after >= bytes.len() || !is_ident_char(bytes[after]) {
                return Some(i);
            }
        }
        i += 1;
    }
    None
}

fn best_definition(hits: &[SymbolLocation], symbol: &str, from_path: &str) -> Option<SymbolLocation> {
    if hits.is_empty() {
        return None;
    }

    let from_dir = from_path.rsplit_once('/').map(|(dir, _)| dir).unwrap_or("");
    let snake = crate::workspace::ruby_nav::camel_to_snake(symbol);
    let exact_names = [
        format!("{symbol}.java"),
        format!("{symbol}.kt"),
        format!("{symbol}.kts"),
        format!("{symbol}.groovy"),
        format!("{symbol}.rs"),
        format!("{symbol}.py"),
        format!("{symbol}.go"),
        format!("{symbol}.cs"),
        format!("{symbol}.rb"),
        format!("{snake}.rb"),
        format!("app/models/{snake}.rb"),
        format!("app/controllers/{snake}.rb"),
        format!("app/helpers/{snake}.rb"),
    ];

    let mut best: Option<(i32, SymbolLocation)> = None;
    for hit in hits {
        let file = hit.path.rsplit('/').next().unwrap_or(&hit.path);
        let score = {
            let mut score = 100;
            if exact_names.iter().any(|name| file == name) {
                score = 0;
            } else if hit.path.ends_with(&format!("/{symbol}")) {
                score = 10;
            }
            if from_dir.is_empty() {
                score
            } else if hit.path.starts_with(from_dir) {
                score
            } else {
                score + 50
            }
        };
        if best.as_ref().is_none_or(|(s, _)| score < *s) {
            best = Some((score, hit.clone()));
        }
    }

    best.map(|(_, hit)| hit)
}

fn collect_definitions(
    ws: &Path,
    dir: &Path,
    from_path: &str,
    symbol: &str,
    hits: &mut Vec<SymbolLocation>,
) -> Result<()> {
    let entries: Vec<_> = std::fs::read_dir(dir)
        .with_context(|| format!("read dir {}", dir.display()))?
        .filter_map(|e| e.ok())
        .collect();

    for entry in entries {
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = entry.path();

        if path.is_dir() {
            if SKIP_DIRS.contains(&name.as_str()) {
                continue;
            }
            collect_definitions(ws, &path, from_path, symbol, hits)?;
            continue;
        }

        if !is_source_file(&name) {
            continue;
        }

        let rel = path
            .strip_prefix(ws)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");

        if !should_scan_file(from_path, &rel) {
            continue;
        }

        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        if let Some(hit) = find_in_content(symbol, &rel, &content) {
            hits.push(hit);
        }
    }

    Ok(())
}

fn is_java_context(from_path: &str) -> bool {
    let lower = from_path.to_lowercase();
    lower.ends_with(".java")
        || lower.ends_with(".kt")
        || lower.ends_with(".kts")
        || lower.ends_with(".groovy")
}

fn is_ruby_context(from_path: &str) -> bool {
    from_path.to_lowercase().ends_with(".rb")
}

fn should_scan_file(from_path: &str, target_rel: &str) -> bool {
    let lower = target_rel.to_lowercase();
    if is_vendor_asset(&lower) {
        return false;
    }
    if is_java_context(from_path) {
        return lower.ends_with(".java")
            || lower.ends_with(".kt")
            || lower.ends_with(".kts")
            || lower.ends_with(".groovy");
    }
    if is_ruby_context(from_path) {
        return lower.ends_with(".rb");
    }
    true
}

fn is_vendor_asset(lower_path: &str) -> bool {
    lower_path.contains("/vendor/")
        || lower_path.contains("/webapp/")
        || lower_path.contains("/node_modules/")
        || lower_path.contains("jquery")
        || lower_path.ends_with(".min.js")
}

fn is_source_file(name: &str) -> bool {
    let lower = name.to_lowercase();
    if lower.ends_with(".gradle.kts") || lower.ends_with(".gradle") {
        return true;
    }
    SOURCE_EXTS.iter().any(|ext| lower.ends_with(&format!(".{ext}")))
}

pub fn format_content(rel_path: &str, content: &str) -> Result<String> {
    let lower = rel_path.to_lowercase();
    let ext = lower.rsplit('.').next().unwrap_or("");

    if lower.ends_with(".gradle.kts") || ext == "kts" || ext == "kt" {
        return try_stdin_command("ktfmt", &["-"], content)
            .or_else(|_| try_stdin_command("ktlint", &["format", "-"], content));
    }
    if ext == "java" {
        return try_stdin_command("google-java-format", &["-"], content)
            .or_else(|_| try_stdin_command("clang-format", &["-assume-filename=file.java"], content));
    }
    if ext == "rs" {
        return try_stdin_command("rustfmt", &["--emit", "stdout"], content);
    }
    if ext == "go" || lower.ends_with("go.mod") {
        return try_stdin_command("gofmt", &[], content);
    }
    if ext == "py" {
        return try_stdin_command("black", &["-q", "-"], content)
            .or_else(|_| try_stdin_command("autopep8", &["-"], content));
    }
    if matches!(ext, "js" | "mjs" | "cjs" | "ts" | "tsx" | "jsx" | "json" | "css" | "scss" | "less" | "md" | "yaml" | "yml" | "html" | "xml") {
        let parser = match ext {
            "ts" | "tsx" => "typescript",
            "js" | "mjs" | "cjs" | "jsx" => "babel",
            "json" => "json",
            "css" | "scss" | "less" => "css",
            "md" => "markdown",
            "yaml" | "yml" => "yaml",
            "html" => "html",
            "xml" => "xml",
            _ => "babel",
        };
        return try_stdin_command("prettier", &["--parser", parser, "--stdin-filepath", rel_path], content);
    }
    if ext == "groovy" || lower.ends_with(".gradle") {
        bail!("no Groovy/Gradle formatter found on PATH (install prettier with a Groovy plugin or format manually)");
    }

    bail!("no formatter available for .{ext} files");
}

fn try_stdin_command(program: &str, args: &[&str], content: &str) -> Result<String> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("failed to run {program}"))?;

    if let Some(stdin) = child.stdin.as_mut() {
        stdin.write_all(content.as_bytes())?;
    }

    let out = child.wait_with_output()?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        bail!("{program} failed: {err}");
    }

    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

const TYPE_DEF_KEYWORDS: &[(&str, &str)] = &[
    ("class", "class"),
    ("module", "module"),
    ("interface", "interface"),
    ("enum", "enum"),
    ("record", "record"),
    ("struct", "struct"),
    ("trait", "trait"),
    ("object", "object"),
];

pub fn search_workspace_classes(
    ws: &Path,
    query: &str,
    limit: usize,
    skip_java: bool,
) -> Result<Vec<ClassSearchHit>> {
    let mut scored: Vec<(u32, ClassSearchHit)> = Vec::new();
    collect_workspace_class_hits(ws, ws, query, skip_java, limit.saturating_mul(4), &mut scored)?;
    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.name.cmp(&b.1.name)));
    Ok(dedupe_class_hits(scored, limit))
}

fn collect_workspace_class_hits(
    ws: &Path,
    dir: &Path,
    query: &str,
    skip_java: bool,
    max_hits: usize,
    scored: &mut Vec<(u32, ClassSearchHit)>,
) -> Result<()> {
    if scored.len() >= max_hits {
        return Ok(());
    }

    let entries: Vec<_> = std::fs::read_dir(dir)
        .with_context(|| format!("read dir {}", dir.display()))?
        .filter_map(|e| e.ok())
        .collect();

    for entry in entries {
        if scored.len() >= max_hits {
            return Ok(());
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = entry.path();

        if path.is_dir() {
            if SKIP_DIRS.contains(&name.as_str()) {
                continue;
            }
            collect_workspace_class_hits(ws, &path, query, skip_java, max_hits, scored)?;
            continue;
        }

        if !is_source_file(&name) {
            continue;
        }
        if skip_java && name.to_lowercase().ends_with(".java") {
            continue;
        }

        let rel = path
            .strip_prefix(ws)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");

        if query.trim().is_empty()
            && !rel.starts_with("app/")
            && !rel.contains("/src/")
            && !rel.starts_with("lib/")
        {
            continue;
        }

        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        index_types_in_content(&content, &rel, query, scored);
    }

    Ok(())
}

fn index_types_in_content(
    content: &str,
    rel_path: &str,
    query: &str,
    scored: &mut Vec<(u32, ClassSearchHit)>,
) {
    for (idx, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty()
            || trimmed.starts_with("//")
            || trimmed.starts_with('#')
            || trimmed.starts_with('*')
        {
            continue;
        }
        for (kind, keyword) in TYPE_DEF_KEYWORDS {
            let pattern = format!("{keyword} ");
            let Some(pos) = line.find(&pattern) else {
                continue;
            };
            let rest = &line[pos + pattern.len()..];
            let type_name: String = rest
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            if type_name.is_empty() || !type_name.chars().next().is_some_and(|c| c.is_uppercase()) {
                continue;
            }
            let qualified = type_name.clone();
            let Some(base) = class_name_match_score(query, &type_name, &qualified) else {
                continue;
            };
            let col = line
                .find(&type_name)
                .map(|i| i as u32 + 1)
                .unwrap_or(1);
            let bonus = if rel_path.starts_with("app/") || rel_path.contains("/src/") {
                300
            } else {
                100
            };
            scored.push((
                base + bonus,
                ClassSearchHit {
                    name: type_name,
                    qualified,
                    kind: (*kind).to_string(),
                    path: rel_path.to_string(),
                    line: idx as u32 + 1,
                    column: col,
                },
            ));
        }
    }
}

fn dedupe_class_hits(scored: Vec<(u32, ClassSearchHit)>, limit: usize) -> Vec<ClassSearchHit> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for (_, hit) in scored {
        let key = format!("{}:{}:{}", hit.path, hit.line, hit.name);
        if seen.insert(key) {
            out.push(hit);
            if out.len() >= limit {
                break;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn class_name_match_acronym() {
        assert!(matches_initials("ACB", "ActionControllerBase"));
        assert!(class_name_match_score("User", "User", "com.example.User").is_some());
        assert!(class_name_match_score("ACB", "ActionControllerBase", "ActionControllerBase").is_some());
    }

    #[test]
    fn rails_user_constant() {
        let ws = std::env::temp_dir().join("reaper-rails-nav-test");
        let _ = std::fs::remove_dir_all(&ws);
        std::fs::create_dir_all(ws.join("app/models")).unwrap();
        std::fs::write(
            ws.join("app/models/user.rb"),
            "class User < ApplicationRecord\nend\n",
        )
        .unwrap();
        let hit = crate::workspace::ruby_nav::find_rails_constant(&ws, "User")
            .expect("lookup ok")
            .expect("should find User");
        assert_eq!(hit.path, "app/models/user.rb");
        assert_eq!(hit.name, "User");
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn java_context_skips_jquery_for_string() {
        assert!(!should_scan_file(
            "src/main/java/com/example/App.java",
            "src/main/webapp/js/jquery-1.7.2.js",
        ));
        assert!(should_scan_file(
            "src/main/java/com/example/App.java",
            "src/main/java/com/example/User.java",
        ));
    }

    #[test]
    fn rock_does_not_match_rock_analyzer() {
        assert!(definition_on_line("public class RockAnalyzer {", "Rock", "x.java", 1).is_none());
        assert!(definition_on_line("public class RockCatalog {", "Rock", "x.java", 1).is_none());
    }

    #[test]
    fn rock_matches_abstract_class() {
        let hit = definition_on_line("public abstract class Rock {", "Rock", "rocks/Rock.java", 6)
            .expect("should match Rock");
        assert_eq!(hit.name, "Rock");
        assert_eq!(hit.kind, "class");
    }

    #[test]
    fn rock_formatter_matches_interface() {
        let hit = definition_on_line(
            "public interface RockFormatter {",
            "RockFormatter",
            "rocks/RockFormatter.java",
            8,
        )
        .expect("should match RockFormatter");
        assert_eq!(hit.name, "RockFormatter");
        assert_eq!(hit.kind, "interface");
    }

    #[test]
    fn prefers_exact_filename() {
        let hits = vec![
            SymbolLocation {
                name: "Rock".into(),
                kind: "class".into(),
                path: "rocks/RockAnalyzer.java".into(),
                line: 10,
                column: 1,
            },
            SymbolLocation {
                name: "Rock".into(),
                kind: "class".into(),
                path: "rocks/Rock.java".into(),
                line: 6,
                column: 1,
            },
        ];
        let best = best_definition(&hits, "Rock", "rocks/RockFormatter.java").unwrap();
        assert_eq!(best.path, "rocks/Rock.java");
    }

    #[test]
    fn rock_from_rock_formatter_parameter() {
        let ws = Path::new("data/workspaces/as/rocks");
        if !ws.join("rocks/RockFormatter.java").exists() {
            return;
        }
        let content =
            std::fs::read_to_string(ws.join("rocks/RockFormatter.java")).expect("read formatter");
        let hit = find_definition(ws, "rocks/RockFormatter.java", 10, 20, &content)
            .expect("lookup ok")
            .expect("should find Rock");
        assert_eq!(hit.path, "rocks/Rock.java");
        assert_eq!(hit.name, "Rock");
    }

    #[test]
    fn parses_java_method_declaration() {
        assert_eq!(
            java_method_name_on_line("    public static void main(String[] args) {"),
            Some("main".into())
        );
        assert_eq!(java_method_name_on_line("        SpringApplication.run(Application.class, args);"), None);
    }

    #[test]
    fn parses_member_qualifier() {
        let src = "        SpringApplication.run(Application.class, args);";
        assert_eq!(
            java_member_qualifier(src, 1, 27, "run"),
            Some("SpringApplication".into())
        );
        assert_eq!(
            java_member_qualifier(src, 1, 30, "run"),
            Some("SpringApplication".into())
        );
    }
}
