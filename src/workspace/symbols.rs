use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

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

const SOURCE_EXTS: &[&str] = super::languages::SOURCE_EXTENSIONS;

#[derive(Debug, Clone, Serialize)]
pub struct SymbolLocation {
    pub name: String,
    pub kind: String,
    pub path: String,
    pub line: u32,
    pub column: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
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

/// Qualifier and partial member name when the cursor is in a dotted expression (`foo.`, `foo.get`, `this.`).
pub(crate) fn infer_java_receiver_type(content: &str, var_name: &str) -> Option<String> {
    if var_name.is_empty() || var_name == "this" || var_name == "super" {
        return None;
    }

    const COLLECTION_PREFIXES: &[&str] = &[
        "List<",
        "ArrayList<",
        "LinkedList<",
        "Set<",
        "HashSet<",
        "LinkedHashSet<",
        "Collection<",
        "Iterable<",
        "Queue<",
        "Deque<",
        "ArrayDeque<",
    ];
    const MAP_PREFIXES: &[&str] = &[
        "Map<",
        "HashMap<",
        "LinkedHashMap<",
        "TreeMap<",
        "ConcurrentHashMap<",
    ];

    let mut best: Option<(usize, String)> = None;

    for (line_idx, line) in content.lines().enumerate() {
        if !line.contains(var_name) {
            continue;
        }
        let trimmed = line.trim();

        for prefix in COLLECTION_PREFIXES {
            let type_name = prefix.trim_end_matches('<');
            let mut search = 0;
            while let Some(idx) = trimmed[search..].find(prefix) {
                let start = search + idx;
                let after = &trimmed[start + prefix.len()..];
                if let Some(end_gt) = after.find('>') {
                    let rest = after[end_gt + 1..].trim_start();
                    if java_var_decl_matches(rest, var_name) {
                        let pos = line_idx * 10_000 + start;
                        if best.as_ref().map(|(p, _)| pos > *p).unwrap_or(true) {
                            best = Some((pos, type_name.to_string()));
                        }
                    }
                }
                search = start + prefix.len();
            }
        }

        for prefix in MAP_PREFIXES {
            let type_name = prefix.trim_end_matches('<');
            let mut search = 0;
            while let Some(idx) = trimmed[search..].find(prefix) {
                let start = search + idx;
                let after = &trimmed[start + prefix.len()..];
                if let Some(end_gt) = after.find('>') {
                    let rest = after[end_gt + 1..].trim_start();
                    if java_var_decl_matches(rest, var_name) {
                        let pos = line_idx * 10_000 + start;
                        if best.as_ref().map(|(p, _)| pos > *p).unwrap_or(true) {
                            best = Some((pos, type_name.to_string()));
                        }
                    }
                }
                search = start + prefix.len();
            }
        }

        if let Some(type_name) = simple_java_type_before_var(trimmed, var_name) {
            let pos = line_idx * 10_000 + trimmed.len();
            if best.as_ref().map(|(p, _)| pos > *p).unwrap_or(true) {
                best = Some((pos, type_name));
            }
        }

        // var name = new Type or var name = new Type<...>
        if let Some(type_name) = infer_type_from_new_assignment(trimmed, var_name) {
            let pos = line_idx * 10_000 + trimmed.len();
            if best.as_ref().map(|(p, _)| pos > *p).unwrap_or(true) {
                best = Some((pos, type_name));
            }
        }
    }

    best.map(|(_, t)| t)
}

fn java_var_decl_matches(rest: &str, var_name: &str) -> bool {
    let rest = rest.trim_start();
    if !rest.starts_with(var_name) {
        return false;
    }
    let after = rest[var_name.len()..].trim_start();
    after.is_empty()
        || after.starts_with('=')
        || after.starts_with(';')
        || after.starts_with(',')
}

fn simple_java_type_before_var(line: &str, var_name: &str) -> Option<String> {
    let idx = line.find(var_name)?;
    let before = line[..idx].trim_end();
    if before.is_empty() {
        return None;
    }
    let type_token = before.split_whitespace().last()?;
    let type_name = type_token.split('<').next()?.trim();
    if type_name.is_empty() || is_keyword(type_name) {
        return None;
    }
    if type_name.chars().next().is_some_and(|c| c.is_ascii_uppercase() || c == '@') {
        Some(type_name.replace('@', ""))
    } else {
        None
    }
}

fn infer_type_from_new_assignment(line: &str, var_name: &str) -> Option<String> {
    let needle = format!("{var_name} =");
    let idx = line.find(&needle)?;
    let after = line[idx + needle.len()..].trim_start();
    if !after.starts_with("new ") {
        return None;
    }
    let after_new = after[4..].trim_start();
    let type_end = after_new
        .find(|c: char| c == '(' || c == '<' || c.is_whitespace())
        .unwrap_or(after_new.len());
    let type_name = after_new[..type_end].trim();
    if type_name.is_empty() || is_keyword(type_name) {
        None
    } else {
        Some(type_name.to_string())
    }
}

/// Qualifier and partial member name when the cursor is in a dotted expression (`foo.`, `foo.get`, `this.`).
pub(crate) fn java_dot_qualifier(content: &str, line: u32, column: u32) -> Option<(String, String)> {
    let line_text = content.lines().nth(line.saturating_sub(1) as usize)?;
    let col = column.saturating_sub(1) as usize;
    let before = &line_text[..col.min(line_text.len())];
    let trimmed_end = before.trim_end();
    if trimmed_end.is_empty() {
        return None;
    }
    let dot_pos = trimmed_end.rfind('.')?;
    let qual_part = trimmed_end[..dot_pos].trim();
    let member_part = trimmed_end[dot_pos + 1..].trim().to_string();
    let simple = qual_part.rsplit('.').next()?.trim();
    if simple.is_empty()
        || (is_keyword(simple) && simple != "this" && simple != "super")
    {
        return None;
    }
    Some((simple.to_string(), member_part))
}

/// FQCN fragment typed after `import` / `import static` on the current line.
pub(crate) fn java_import_fqcn_prefix(content: &str, line: u32, column: u32) -> Option<String> {
    let line_text = content.lines().nth(line.saturating_sub(1) as usize)?;
    let trimmed = line_text.trim();
    let keyword_len = if trimmed.starts_with("import static ") {
        14
    } else if trimmed.starts_with("import ") {
        7
    } else {
        return None;
    };
    let leading = line_text.len() - line_text.trim_start().len();
    let col = column.saturating_sub(1) as usize;
    if col < leading + keyword_len {
        return Some(String::new());
    }
    let body_start = leading + keyword_len;
    let body = trimmed[keyword_len..].trim_end_matches(';').trim();
    let body_col = col.saturating_sub(body_start);
    let prefix = body[..body_col.min(body.len())].trim();
    Some(prefix.to_string())
}

pub(crate) fn is_java_import_line(content: &str, line: u32) -> bool {
    match content.lines().nth(line.saturating_sub(1) as usize) {
        Some(line_text) => {
            let trimmed = line_text.trim();
            trimmed.starts_with("import ") || trimmed.starts_with("import static ")
        }
        None => false,
    }
}

/// Prefer type names over methods (e.g. `new Foo`, `extends Bar`, uppercase prefix).
pub(crate) fn is_java_type_reference_context(content: &str, line: u32, column: u32) -> bool {
    match content.lines().nth(line.saturating_sub(1) as usize) {
        Some(line_text) => {
            let col = column.saturating_sub(1) as usize;
            let end = col.min(line_text.len());
            let before: &str = line_text.get(..end).unwrap_or(line_text);
            let trimmed_end = before.trim_end();
            trimmed_end.ends_with("new ")
                || trimmed_end.ends_with("extends ")
                || trimmed_end.ends_with("implements ")
                || trimmed_end.ends_with("throws ")
                || trimmed_end.ends_with("catch (")
                || trimmed_end.ends_with("<")
        }
        None => false,
    }
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

    if path.ends_with(".java")
        || path.ends_with(".kt")
        || path.ends_with(".kts")
        || path.ends_with(".groovy")
    {
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

    if path.ends_with(".go") {
        if let Some(rest) = trimmed.strip_prefix("func ") {
            if let Some(method) = go_func_name_from_rest(rest) {
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
        }
    }

    if path.ends_with(".rs") {
        if let Some(rest) = trimmed.strip_prefix("impl ") {
            if let Some(type_name) = rust_impl_type_name(rest.trim_start_matches("pub ")) {
                if type_name == symbol {
                    let col = line.find(&type_name).map(|i| i as u32 + 1).unwrap_or(1);
                    return Some(SymbolLocation {
                        name: symbol.to_string(),
                        kind: "impl".into(),
                        path: path.to_string(),
                        line: line_no,
                        column: col,
                    });
                }
            }
        }
    }

    let lower_path = path.to_lowercase();
    if lower_path.ends_with(".sh") || lower_path.ends_with(".bash") || lower_path.ends_with(".zsh") {
        if let Some(name) = shell_function_name(trimmed) {
            if name == symbol {
                let col = line.find(&name).map(|i| i as u32 + 1).unwrap_or(1);
                return Some(SymbolLocation {
                    name: symbol.to_string(),
                    kind: "function".into(),
                    path: path.to_string(),
                    line: line_no,
                    column: col,
                });
            }
        }
    }

    if path.ends_with(".sql") {
        for object_kw in ["TABLE", "VIEW", "INDEX", "FUNCTION", "PROCEDURE"] {
            if let Some(name) = sql_create_object_name(line, object_kw) {
                if name.eq_ignore_ascii_case(symbol) {
                    let col = line.find(&name).map(|i| i as u32 + 1).unwrap_or(1);
                    return Some(SymbolLocation {
                        name: symbol.to_string(),
                        kind: object_kw.to_lowercase(),
                        path: path.to_string(),
                        line: line_no,
                        column: col,
                    });
                }
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

pub fn format_content(ws: &Path, rel_path: &str, content: &str) -> Result<String> {
    use super::exec::{try_stdin_command, try_tool_stdin};

    let lower = rel_path.to_lowercase();
    let ext = lower.rsplit('.').next().unwrap_or("");

    if matches!(ext, "yaml" | "yml") {
        return format_yaml(ws, rel_path, content);
    }

    if lower.ends_with(".gradle.kts") || ext == "kts" || ext == "kt" {
        return try_stdin_command(ws, "ktfmt", &["-"], content)
            .or_else(|_| try_stdin_command(ws, "ktlint", &["format", "-"], content));
    }
    if ext == "java" {
        return try_stdin_command(ws, "google-java-format", &["-"], content)
            .or_else(|_| try_stdin_command(ws, "clang-format", &["-assume-filename=file.java"], content));
    }
    if ext == "rs" {
        return try_stdin_command(ws, "rustfmt", &["--emit", "stdout"], content);
    }
    if ext == "go" || lower.ends_with("go.mod") {
        return try_stdin_command(ws, "gofmt", &[], content);
    }
    if ext == "py" {
        return try_stdin_command(ws, "black", &["-q", "-"], content)
            .or_else(|_| try_stdin_command(ws, "autopep8", &["-"], content));
    }
    if matches!(ext, "js" | "mjs" | "cjs" | "ts" | "tsx" | "jsx" | "json" | "css" | "scss" | "less" | "md" | "html" | "xml") {
        let parser = match ext {
            "ts" | "tsx" => "typescript",
            "js" | "mjs" | "cjs" | "jsx" => "babel",
            "json" => "json",
            "css" | "scss" | "less" => "css",
            "md" => "markdown",
            "html" => "html",
            "xml" => "xml",
            _ => "babel",
        };
        return try_tool_stdin(
            ws,
            "prettier",
            &["--parser", parser, "--stdin-filepath", rel_path],
            content,
        )
            .or_else(|_| {
                try_stdin_command(
                    ws,
                    "prettier",
                    &["--parser", parser, "--stdin-filepath", rel_path],
                    content,
                )
            });
    }
    if ext == "groovy" || lower.ends_with(".gradle") {
        bail!("no Groovy/Gradle formatter found on PATH (install prettier with a Groovy plugin or format manually)");
    }

    bail!("no formatter available for .{ext} files");
}

fn format_yaml(ws: &Path, rel_path: &str, content: &str) -> Result<String> {
    use super::exec::{try_stdin_command, try_tool_stdin};

    if let Some(program) = crate::toolchain::resolve_program("yamlfmt") {
        if let Ok(formatted) = try_stdin_command(
            ws,
            program.to_string_lossy().as_ref(),
            &["-"],
            content,
        ) {
            return Ok(formatted);
        }
    }
    if let Ok(formatted) = try_tool_stdin(
        ws,
        "prettier",
        &["--parser", "yaml", "--stdin-filepath", rel_path],
        content,
    ) {
        return Ok(formatted);
    }
    if let Ok(formatted) = try_stdin_command(
        ws,
        "prettier",
        &["--parser", "yaml", "--stdin-filepath", rel_path],
        content,
    ) {
        return Ok(formatted);
    }
    format_yaml_builtin(content)
}

fn format_yaml_builtin(content: &str) -> Result<String> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return Ok(String::new());
    }
    let value: serde_yaml::Value = serde_yaml::from_str(content)
        .with_context(|| "invalid YAML — fix syntax errors before formatting")?;
    let mut out = serde_yaml::to_string(&value)?;
    if !out.ends_with('\n') {
        out.push('\n');
    }
    Ok(out)
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
    if query.trim().is_empty() {
        if let Some(cached) = load_symbol_cache(ws) {
            if !cached.is_empty() {
                return Ok(dedupe_class_hits(
                    cached
                        .into_iter()
                        .filter(|(_, hit)| !(skip_java && hit.path.to_lowercase().ends_with(".java")))
                        .take(limit.saturating_mul(4))
                        .collect(),
                    limit,
                ));
            }
        }
    }

    let mut scored: Vec<(u32, ClassSearchHit)> = Vec::new();
    collect_workspace_class_hits(ws, ws, query, skip_java, limit.saturating_mul(4), &mut scored)?;
    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.name.cmp(&b.1.name)));
    Ok(dedupe_class_hits(scored, limit))
}

const WORKSPACE_SYMBOLS_PATH: &str = ".reaper/workspace-symbols.json";

pub fn invalidate_symbol_cache(ws: &Path) {
    let _ = std::fs::remove_file(ws.join(WORKSPACE_SYMBOLS_PATH));
}

pub fn warm_symbol_cache(ws: &Path) -> Result<usize> {
    let mut hits = Vec::new();
    collect_all_workspace_symbols(ws, ws, &mut hits)?;
    let count = hits.len();
    std::fs::create_dir_all(ws.join(".reaper"))?;
    std::fs::write(
        ws.join(WORKSPACE_SYMBOLS_PATH),
        serde_json::to_string_pretty(&WorkspaceSymbolCache {
            version: WORKSPACE_SYMBOLS_VERSION,
            symbol_count: count,
            hits,
        })?,
    )?;
    Ok(count)
}

fn load_symbol_cache(ws: &Path) -> Option<Vec<(u32, ClassSearchHit)>> {
    let text = std::fs::read_to_string(ws.join(WORKSPACE_SYMBOLS_PATH)).ok()?;
    let cache: WorkspaceSymbolCache = serde_json::from_str(&text).ok()?;
    if cache.version != WORKSPACE_SYMBOLS_VERSION {
        return None;
    }
    Some(
        cache
            .hits
            .into_iter()
            .map(|hit| (path_score_bonus(&hit.path), hit))
            .collect(),
    )
}

fn read_ident_prefix(s: &str) -> Option<(String, usize)> {
    let s = s.trim_start();
    let mut end = 0;
    for (i, c) in s.char_indices() {
        if c.is_alphanumeric() || c == '_' || c == '$' {
            end = i + c.len_utf8();
        } else {
            break;
        }
    }
    if end == 0 {
        return None;
    }
    Some((s[..end].to_string(), end))
}

/// Members used with `qualifier.` anywhere in the file, plus language-specific defs.
pub(crate) fn member_completions_from_content(
    content: &str,
    qualifier: &str,
    member_prefix: &str,
    from_path: &str,
) -> Vec<super::classpath::CompletionItem> {
    use super::classpath::CompletionItem;
    use std::collections::HashSet;

    let lang = super::languages::language_for_path(from_path).unwrap_or("plaintext");
    let prefix_lower = member_prefix.to_lowercase();
    let mut seen = HashSet::new();
    let mut items = Vec::new();

    let needle = format!("{qualifier}.");
    let at_needle = format!("@{qualifier}.");
    for line_text in content.lines() {
        for needle in [&needle, &at_needle] {
            let mut search = 0;
            while let Some(idx) = line_text[search..].find(needle) {
                let start = search + idx + needle.len();
                let after = &line_text[start..];
                if let Some((member, _)) = read_ident_prefix(after) {
                    if !member.is_empty()
                        && (member_prefix.is_empty()
                            || member.to_lowercase().starts_with(&prefix_lower))
                        && seen.insert(member.clone())
                    {
                        let is_call = after
                            .chars()
                            .nth(member.len())
                            .map(|c| c == '(')
                            .unwrap_or(false);
                        let kind = if is_call { "method" } else { "field" };
                        items.push(CompletionItem {
                            label: member.clone(),
                            kind: kind.to_string(),
                            detail: Some(format!("{qualifier}.{member}")),
                            insert: None,
                            path: Some(from_path.to_string()),
                            line: None,
                            column: None,
                        });
                    }
                }
                search = start + 1;
            }
        }
    }

    if qualifier == "this" || qualifier == "self" || qualifier == "super" {
        for line_text in content.lines() {
            let trimmed = line_text.trim();
            if lang == "ruby" {
                if let Some(rest) = trimmed.strip_prefix("def ") {
                    if let Some((name, _)) = read_ident_prefix(rest) {
                        if !name.is_empty()
                            && (member_prefix.is_empty()
                                || name.to_lowercase().starts_with(&prefix_lower))
                            && seen.insert(name.clone())
                        {
                            items.push(CompletionItem {
                                label: name,
                                kind: "method".into(),
                                detail: Some("method".into()),
                                insert: None,
                                path: Some(from_path.to_string()),
                                line: None,
                                column: None,
                            });
                        }
                    }
                }
                if trimmed.starts_with("attr_reader ") || trimmed.starts_with("attr_accessor ") {
                    let parts: Vec<&str> = trimmed.split_whitespace().collect();
                    for part in parts.iter().skip(1) {
                        let name = part.trim_matches(',');
                        if !name.is_empty()
                            && (member_prefix.is_empty()
                                || name.to_lowercase().starts_with(&prefix_lower))
                            && seen.insert(name.to_string())
                        {
                            items.push(CompletionItem {
                                label: name.to_string(),
                                kind: "field".into(),
                                detail: Some("attribute".into()),
                                insert: None,
                                path: Some(from_path.to_string()),
                                line: None,
                                column: None,
                            });
                        }
                    }
                }
            }
            if lang == "python" {
                if let Some(rest) = trimmed.strip_prefix("def ") {
                    if let Some((name, _)) = read_ident_prefix(rest) {
                        if name != "self"
                            && (member_prefix.is_empty()
                                || name.to_lowercase().starts_with(&prefix_lower))
                            && seen.insert(name.clone())
                        {
                            items.push(CompletionItem {
                                label: name,
                                kind: "method".into(),
                                detail: Some("def".into()),
                                insert: None,
                                path: Some(from_path.to_string()),
                                line: None,
                                column: None,
                            });
                        }
                    }
                }
            }
            if matches!(lang, "java" | "kotlin" | "groovy" | "csharp" | "typescript" | "javascript") {
                for marker in ["void ", "public ", "private ", "protected ", "static "] {
                    if trimmed.contains(marker) {
                        let after = trimmed.rsplit(marker).next().unwrap_or(trimmed);
                        if let Some((name, _)) = read_ident_prefix(after) {
                            if !name.is_empty()
                                && name != qualifier
                                && (member_prefix.is_empty()
                                    || name.to_lowercase().starts_with(&prefix_lower))
                                && seen.insert(name.clone())
                            {
                                let kind = if after.contains('(') { "method" } else { "field" };
                                items.push(CompletionItem {
                                    label: name,
                                    kind: kind.to_string(),
                                    detail: Some(kind.to_string()),
                                    insert: None,
                                    path: Some(from_path.to_string()),
                                    line: None,
                                    column: None,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    items.sort_by(|a, b| a.label.len().cmp(&b.label.len()).then_with(|| a.label.cmp(&b.label)));
    items.truncate(40);
    items
}

/// Workspace + file symbol and keyword completions for all languages.
pub fn completions(
    ws: &Path,
    from_path: &str,
    content: &str,
    prefix: &str,
    line: u32,
    column: u32,
) -> Result<Vec<super::classpath::CompletionItem>> {
    use super::classpath::CompletionItem;
    use std::collections::HashSet;

    if let Some((qualifier, member_prefix)) = java_dot_qualifier(content, line, column) {
        let member_items =
            member_completions_from_content(content, &qualifier, &member_prefix, from_path);
        if !member_items.is_empty() {
            return Ok(member_items);
        }
    }

    let prefix_lower = prefix.to_lowercase();
    let mut seen = HashSet::new();
    let mut items = Vec::new();
    let mut limit = 80;

    let mut file_hits = Vec::new();
    collect_symbols_in_content(content, from_path, &mut file_hits);
    for hit in file_hits {
        if limit == 0 {
            break;
        }
        if prefix.is_empty()
            || hit.name.to_lowercase().starts_with(&prefix_lower)
            || hit.qualified.to_lowercase().starts_with(&prefix_lower)
        {
            if seen.insert(hit.name.clone()) {
                items.push(CompletionItem {
                    label: hit.name.clone(),
                    kind: hit.kind.clone(),
                    detail: Some(hit.qualified.clone()),
                    insert: None,
                    path: Some(from_path.to_string()),
                    line: Some(hit.line),
                    column: Some(hit.column),
                });
                limit -= 1;
            }
        }
    }

    if limit > 0 {
        if let Some(cached) = load_symbol_cache(ws) {
            for (_, hit) in cached {
                if limit == 0 {
                    break;
                }
                if hit.path == from_path {
                    continue;
                }
                if prefix.is_empty()
                    || hit.name.to_lowercase().starts_with(&prefix_lower)
                    || hit.qualified.to_lowercase().starts_with(&prefix_lower)
                {
                    if seen.insert(hit.name.clone()) {
                        items.push(CompletionItem {
                            label: hit.name.clone(),
                            kind: hit.kind.clone(),
                            detail: Some(format!("{} · {}", hit.qualified, hit.path)),
                            insert: None,
                            path: Some(hit.path.clone()),
                            line: Some(hit.line),
                            column: Some(hit.column),
                        });
                        limit -= 1;
                    }
                }
            }
        }
    }

    for kw in super::languages::keywords_for_path(from_path) {
        if limit == 0 {
            break;
        }
        if prefix.is_empty() || kw.to_lowercase().starts_with(&prefix_lower) {
            if seen.insert(kw.to_string()) {
                items.push(CompletionItem {
                    label: kw.to_string(),
                    kind: "keyword".to_string(),
                    detail: Some("keyword".into()),
                    insert: None,
                    path: None,
                    line: None,
                    column: None,
                });
                limit -= 1;
            }
        }
    }

    items.sort_by(|a, b| {
        let rank = |k: &str| if k == "keyword" { 2 } else { 1 };
        rank(&a.kind)
            .cmp(&rank(&b.kind))
            .then_with(|| a.label.len().cmp(&b.label.len()))
            .then_with(|| a.label.cmp(&b.label))
    });
    items.truncate(80);
    Ok(items)
}

fn path_score_bonus(rel_path: &str) -> u32 {
    if rel_path.starts_with("app/")
        || rel_path.contains("/src/")
        || rel_path.starts_with("cmd/")
        || rel_path.starts_with("lib/")
        || rel_path.contains("/db/migrate/")
        || rel_path.starts_with("db/")
        || rel_path.starts_with("scripts/")
    {
        300
    } else {
        100
    }
}

const WORKSPACE_SYMBOLS_VERSION: u32 = 2;

#[derive(Serialize, Deserialize)]
struct WorkspaceSymbolCache {
    version: u32,
    symbol_count: usize,
    hits: Vec<ClassSearchHit>,
}

fn collect_all_workspace_symbols(
    ws: &Path,
    dir: &Path,
    out: &mut Vec<ClassSearchHit>,
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
            collect_all_workspace_symbols(ws, &path, out)?;
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

        if !super::languages::is_indexable_source_path(&rel) {
            continue;
        }

        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        collect_symbols_in_content(&content, &rel, out);
    }

    Ok(())
}

fn collect_symbols_in_content(content: &str, rel_path: &str, out: &mut Vec<ClassSearchHit>) {
    let lower = rel_path.to_lowercase();
    let ext = lower.rsplit('.').next().unwrap_or("");

    if matches!(
        ext,
        "java" | "kt" | "kts" | "groovy" | "gradle" | "rb" | "rs" | "cs" | "swift" | "php"
            | "dart" | "scala"
    ) || lower.ends_with(".gradle") {
        collect_types_in_content(content, rel_path, out);
    }
    if ext == "rb" {
        collect_keyword_symbols(content, rel_path, out, &[("def", "method"), ("module", "module")]);
    }
    if matches!(ext, "py" | "pyw") {
        collect_keyword_symbols(content, rel_path, out, &[("class", "class"), ("def", "method")]);
    }
    if ext == "go" {
        collect_go_symbols(content, rel_path, out);
    }
    if ext == "rs" {
        collect_rust_symbols(content, rel_path, out);
    }
    if matches!(ext, "js" | "mjs" | "cjs" | "jsx" | "ts" | "tsx") {
        collect_types_in_content(content, rel_path, out);
        collect_keyword_symbols(content, rel_path, out, &[("function", "method")]);
        if matches!(ext, "ts" | "tsx") {
            collect_keyword_symbols(
                content,
                rel_path,
                out,
                &[("interface", "interface"), ("type", "class")],
            );
        }
    }
    if matches!(ext, "c" | "h" | "cpp" | "cc" | "cxx" | "hpp" | "hh") {
        collect_keyword_symbols(
            content,
            rel_path,
            out,
            &[("struct", "struct"), ("class", "class"), ("enum", "enum")],
        );
    }
    if ext == "lua" {
        collect_keyword_symbols(content, rel_path, out, &[("function", "method")]);
    }
    if ext == "sql" {
        collect_sql_objects(content, rel_path, out);
    }
    if matches!(ext, "sh" | "bash" | "zsh") {
        collect_shell_functions(content, rel_path, out);
    }
}

fn collect_go_symbols(content: &str, rel_path: &str, out: &mut Vec<ClassSearchHit>) {
    for (idx, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("//") {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("type ") {
            let name: String = rest
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            if name.is_empty() || !name.chars().next().is_some_and(|c| c.is_uppercase()) {
                continue;
            }
            push_symbol_hit(out, &name, &name, "class", rel_path, idx, line);
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("func ") {
            let Some(name) = go_func_name_from_rest(rest) else {
                continue;
            };
            push_symbol_hit(out, &name, &name, "method", rel_path, idx, line);
        }
    }
}

fn go_func_name_from_rest(rest: &str) -> Option<String> {
    let rest = rest.trim();
    let rest = if rest.starts_with('(') {
        let close = rest.find(')')?;
        rest[close + 1..].trim()
    } else {
        rest
    };
    let name: String = rest
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    if name.is_empty() { None } else { Some(name) }
}

fn collect_rust_symbols(content: &str, rel_path: &str, out: &mut Vec<ClassSearchHit>) {
    for (idx, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("//") {
            continue;
        }
        if let Some(pos) = trimmed.find("fn ") {
            let rest = trimmed[pos + 3..].trim();
            let name: String = rest
                .trim_start_matches("pub ")
                .trim_start_matches("async ")
                .trim_start_matches("const ")
                .trim_start_matches("unsafe ")
                .trim_start_matches("extern ")
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            if !name.is_empty() {
                push_symbol_hit(out, &name, &name, "method", rel_path, idx, line);
            }
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("impl ") {
            let rest = rest.trim_start_matches("pub ");
            if let Some(type_name) = rust_impl_type_name(rest) {
                push_symbol_hit(out, &type_name, &type_name, "impl", rel_path, idx, line);
            }
        }
    }
}

fn rust_impl_type_name(rest: &str) -> Option<String> {
    let rest = rest.trim();
    if rest.is_empty() {
        return None;
    }
    if rest.starts_with('<') {
        let end = rest.find('>')?;
        let inner = &rest[1..end];
        let type_name: String = inner
            .split(',')
            .next()?
            .trim()
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        return (!type_name.is_empty()).then_some(type_name);
    }
    let first: String = rest
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    if first.is_empty() {
        return None;
    }
    if rest[first.len()..].trim_start().starts_with("for ") {
        let after = rest[first.len()..].trim_start().strip_prefix("for ")?.trim();
        let second: String = after
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        return (!second.is_empty()).then_some(second);
    }
    Some(first)
}

fn collect_shell_functions(content: &str, rel_path: &str, out: &mut Vec<ClassSearchHit>) {
    for (idx, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some(name) = shell_function_name(trimmed) {
            push_symbol_hit(out, &name, &name, "function", rel_path, idx, line);
        }
    }
}

fn shell_function_name(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if let Some(rest) = trimmed.strip_prefix("function ") {
        let name: String = rest
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
            .collect();
        return (!name.is_empty()).then_some(name);
    }
    let paren = trimmed.find("()")?;
    let before = trimmed[..paren].trim();
    if before.is_empty() || before.contains('$') {
        return None;
    }
    let name = before.split_whitespace().next_back()?;
    if name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
        Some(name.to_string())
    } else {
        None
    }
}

fn collect_sql_objects(content: &str, rel_path: &str, out: &mut Vec<ClassSearchHit>) {
    for (idx, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("--") {
            continue;
        }
        let upper = trimmed.to_uppercase();
        if !upper.contains("CREATE") {
            continue;
        }
        for (kind, object_kw) in [
            ("table", "TABLE"),
            ("view", "VIEW"),
            ("index", "INDEX"),
            ("function", "FUNCTION"),
            ("procedure", "PROCEDURE"),
        ] {
            if let Some(name) = sql_create_object_name(trimmed, object_kw) {
                push_symbol_hit(out, &name, &name, kind, rel_path, idx, line);
            }
        }
    }
}

fn sql_create_object_name(line: &str, object_keyword: &str) -> Option<String> {
    let upper = line.to_uppercase();
    let kw_upper = object_keyword.to_uppercase();
    let create_pos = upper.find("CREATE")?;
    let after_create = &line[create_pos + "CREATE".len()..];
    let after_upper = after_create.to_uppercase();
    let or_replace = "OR REPLACE ";
    let after_create = if after_upper.starts_with(or_replace) {
        &after_create[or_replace.len()..]
    } else {
        after_create
    };
    let after_upper = after_create.to_uppercase();
    let kw_pos = after_upper.find(&kw_upper)?;
    let after_kw = &after_create[kw_pos + object_keyword.len()..];
    let mut rest = after_kw.trim();
    if rest.to_uppercase().starts_with("IF NOT EXISTS ") {
        rest = rest["IF NOT EXISTS ".len()..].trim();
    } else if rest.to_uppercase().starts_with("UNIQUE ") {
        rest = rest["UNIQUE ".len()..].trim();
    }
    let name: String = rest
        .trim_start_matches('"')
        .trim_start_matches('`')
        .trim_start_matches('[')
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '.')
        .collect();
    (!name.is_empty()).then_some(name)
}

fn push_symbol_hit(
    out: &mut Vec<ClassSearchHit>,
    name: &str,
    qualified: &str,
    kind: &str,
    rel_path: &str,
    line_idx: usize,
    line: &str,
) {
    let col = line.find(name).map(|i| i as u32 + 1).unwrap_or(1);
    out.push(ClassSearchHit {
        name: name.to_string(),
        qualified: qualified.to_string(),
        kind: kind.to_string(),
        path: rel_path.to_string(),
        line: line_idx as u32 + 1,
        column: col,
    });
}

fn collect_keyword_symbols(
    content: &str,
    rel_path: &str,
    out: &mut Vec<ClassSearchHit>,
    keywords: &[(&str, &str)],
) {
    for (idx, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty()
            || trimmed.starts_with("//")
            || trimmed.starts_with('#')
            || trimmed.starts_with("--")
            || trimmed.starts_with('*')
        {
            continue;
        }
        for (keyword, kind) in keywords {
            let pattern = format!("{keyword} ");
            let Some(pos) = line.find(&pattern).or_else(|| {
                if *keyword == "def" || *keyword == "func" {
                    line.find(keyword)
                } else {
                    None
                }
            }) else {
                continue;
            };
            let rest = &line[pos + keyword.len()..].trim_start();
            let name: String = rest
                .trim_start_matches("async ")
                .trim_start_matches("pub ")
                .trim_start_matches("export ")
                .trim_start_matches("static ")
                .trim_start_matches("local ")
                .trim_start_matches("CREATE ")
                .trim_start_matches("create ")
                .trim_start_matches("TABLE ")
                .trim_start_matches("table ")
                .trim_start_matches("VIEW ")
                .trim_start_matches("view ")
                .trim_start_matches("IF NOT EXISTS ")
                .trim_start_matches("if not exists ")
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            if name.is_empty() {
                continue;
            }
            if kind == &"class" && !name.chars().next().is_some_and(|c| c.is_uppercase() || c.is_ascii_digit())
            {
                if !matches!(*keyword, "type" | "interface" | "struct" | "enum" | "table" | "view")
                {
                    continue;
                }
            }
            let col = line
                .find(&name)
                .map(|i| i as u32 + 1)
                .unwrap_or(1);
            out.push(ClassSearchHit {
                name: name.clone(),
                qualified: name,
                kind: (*kind).to_string(),
                path: rel_path.to_string(),
                line: idx as u32 + 1,
                column: col,
            });
        }
    }
}

fn collect_types_in_content(content: &str, rel_path: &str, out: &mut Vec<ClassSearchHit>) {
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
            let col = line
                .find(&type_name)
                .map(|i| i as u32 + 1)
                .unwrap_or(1);
            out.push(ClassSearchHit {
                name: type_name.clone(),
                qualified: type_name,
                kind: (*kind).to_string(),
                path: rel_path.to_string(),
                line: idx as u32 + 1,
                column: col,
            });
        }
    }
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

        if query.trim().is_empty() && !super::languages::is_indexable_source_path(&rel) {
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
    let mut hits = Vec::new();
    collect_symbols_in_content(content, rel_path, &mut hits);
    let bonus = path_score_bonus(rel_path);
    for hit in hits {
        let Some(base) = class_name_match_score(query, &hit.name, &hit.qualified) else {
            continue;
        };
        scored.push((base + bonus, hit));
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
    fn indexes_go_rust_sql_and_shell_symbols() {
        let ws = std::env::temp_dir().join("reaper-multi-lang-symbols");
        let _ = std::fs::remove_dir_all(&ws);
        std::fs::create_dir_all(ws.join("cmd")).unwrap();
        std::fs::create_dir_all(ws.join("src")).unwrap();
        std::fs::create_dir_all(ws.join("db/migrate")).unwrap();
        std::fs::create_dir_all(ws.join("scripts")).unwrap();
        std::fs::write(
            ws.join("cmd/main.go"),
            "package main\n\ntype Server struct {}\n\nfunc (s *Server) Run() {}\n\nfunc main() {}\n",
        )
        .unwrap();
        std::fs::write(
            ws.join("src/lib.rs"),
            "pub struct Widget;\n\npub fn build() {}\n\nimpl Widget {}\n",
        )
        .unwrap();
        std::fs::write(
            ws.join("db/migrate/001_users.sql"),
            "CREATE TABLE IF NOT EXISTS users (id INT);\nCREATE VIEW active_users AS SELECT 1;\n",
        )
        .unwrap();
        std::fs::write(
            ws.join("scripts/deploy.sh"),
            "#!/bin/bash\nfunction deploy() { echo ok; }\nrestart() { echo ok; }\n",
        )
        .unwrap();
        std::fs::create_dir_all(ws.join("app/models")).unwrap();
        std::fs::write(
            ws.join("app/models/order.rb"),
            "class Order\n  def total\n  end\nend\n",
        )
        .unwrap();

        let mut hits = Vec::new();
        collect_all_workspace_symbols(&ws, &ws, &mut hits).unwrap();
        let names: Vec<_> = hits.iter().map(|h| h.name.as_str()).collect();
        assert!(names.contains(&"Server"));
        assert!(names.contains(&"Run"));
        assert!(names.contains(&"Widget"));
        assert!(names.contains(&"build"));
        assert!(names.contains(&"users"));
        assert!(names.contains(&"active_users"));
        assert!(names.contains(&"deploy"));
        assert!(names.contains(&"restart"));
        assert!(names.contains(&"Order"));
        assert!(names.contains(&"total"));

        let go_hits = search_workspace_classes(&ws, "Server", 10, false).unwrap();
        assert!(go_hits.iter().any(|h| h.name == "Server"));

        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn go_receiver_method_definition() {
        let hit = definition_on_line(
            "func (s *Server) Run() error {",
            "Run",
            "cmd/main.go",
            3,
        )
        .expect("Run method");
        assert_eq!(hit.kind, "method");
    }

    #[test]
    fn shell_function_definition() {
        let hit = definition_on_line("deploy() {", "deploy", "scripts/deploy.sh", 2)
            .expect("deploy function");
        assert_eq!(hit.kind, "function");
    }

    #[test]
    fn sql_table_definition() {
        let hit = definition_on_line(
            "CREATE TABLE IF NOT EXISTS users (id INT);",
            "users",
            "db/schema.sql",
            1,
        )
        .expect("users table");
        assert_eq!(hit.kind, "table");
    }

    #[test]
    fn parses_dot_qualifier() {
        assert_eq!(
            java_dot_qualifier("foo.", 1, 5),
            Some(("foo".into(), "".into()))
        );
        assert_eq!(
            java_dot_qualifier("foo.get", 1, 8),
            Some(("foo".into(), "get".into()))
        );
        assert_eq!(
            java_dot_qualifier("        this.", 1, 14),
            Some(("this".into(), "".into()))
        );
    }

    #[test]
    fn infers_java_receiver_type_from_declarations() {
        let src = "List<String> items = new ArrayList<>();\nitems.size();";
        assert_eq!(
            infer_java_receiver_type(src, "items"),
            Some("List".into())
        );
        let src2 = "String name = \"x\";\nname.";
        assert_eq!(
            infer_java_receiver_type(src2, "name"),
            Some("String".into())
        );
        let src3 = "var app = new SpringApplication();\napp.";
        assert_eq!(
            infer_java_receiver_type(src3, "app"),
            Some("SpringApplication".into())
        );
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
