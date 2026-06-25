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

pub fn find_definition(
    ws: &Path,
    from_path: &str,
    line: u32,
    column: u32,
    content: &str,
) -> Result<Option<SymbolLocation>> {
    let symbol = match word_at(content, line, column) {
        Some(s) if !s.is_empty() => s,
        _ => return Ok(None),
    };

    if is_keyword(&symbol) {
        return Ok(None);
    }

    let mut hits = Vec::new();
    if let Some(hit) = find_in_content(&symbol, from_path, content) {
        hits.push(hit);
    }
    collect_definitions(ws, ws, &symbol, &mut hits)?;
    Ok(best_definition(&hits, &symbol, from_path))
}

fn word_at(content: &str, line: u32, column: u32) -> Option<String> {
    let line_text = content.lines().nth(line.saturating_sub(1) as usize)?;
    let col = column.saturating_sub(1) as usize;
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

fn is_keyword(word: &str) -> bool {
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
    )
}

fn find_in_content(symbol: &str, path: &str, content: &str) -> Option<SymbolLocation> {
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
    let exact_names = [
        format!("{symbol}.java"),
        format!("{symbol}.kt"),
        format!("{symbol}.kts"),
        format!("{symbol}.groovy"),
        format!("{symbol}.rs"),
        format!("{symbol}.py"),
        format!("{symbol}.go"),
        format!("{symbol}.cs"),
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
            collect_definitions(ws, &path, symbol, hits)?;
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
