use std::fmt::Write;
use std::path::Path;

use super::classpath;
use super::java_ecosystem;
use super::languages;
use super::symbols;

pub fn build_inline_completion_context(
    ws: &Path,
    path: &str,
    line: u32,
    column: u32,
    content: &str,
    line_prefix: &str,
) -> String {
    let lang = languages::language_for_path(path).unwrap_or("plaintext");
    let mut out = String::new();
    writeln!(out, "File: {path}").ok();
    writeln!(out, "Language: {lang}").ok();
    writeln!(out, "Line: {line}  Column: {column}").ok();
    writeln!(
        out,
        "Complete {lang} at <CURSOR>. Predict the next character(s), token, punctuation, expression, statement, or lines."
    )
    .ok();
    writeln!(
        out,
        "Text before <CURSOR> on the >>> line is already typed — output ONLY what comes after it."
    )
    .ok();

    if path.ends_with(".java") || path.ends_with(".kt") || path.ends_with(".kts") {
        append_java_scope(&mut out, path, content, line);
    } else {
        append_generic_scope(&mut out, content, line, path);
    }

    if classpath::is_java_like(path) {
        append_java_member_hints(&mut out, ws, path, line, column, content);
    }

    let compiler_ctx = super::language_compiler_context::detect(ws, path);
    super::language_compiler_context::append_to_prompt(&mut out, &compiler_ctx);

    append_language_hints(&mut out, path, line_prefix);

    writeln!(out).ok();
    writeln!(out, "Surrounding code (>>> marks cursor line):").ok();
    out.push_str(&code_snippet(content, line, line_prefix));
    out
}

fn append_language_hints(out: &mut String, path: &str, line_prefix: &str) {
    let lang = languages::language_for_path(path).unwrap_or("plaintext");
    writeln!(out, "\n--- {lang} completion hints ---").ok();
    match lang {
        "markdown" => {
            writeln!(
                out,
                "Markdown: headings (#), lists (- / 1.), links, code fences, blockquotes (>)."
            )
            .ok();
        }
        "yaml" | "toml" | "ini" => {
            writeln!(out, "Config: keys, nesting, indentation, valid {lang} syntax.").ok();
        }
        "json" => {
            writeln!(out, "JSON: keys, strings, arrays, objects — strict syntax.").ok();
        }
        "html" | "xml" => {
            writeln!(out, "Markup: tags, attributes, closing elements, valid nesting.").ok();
        }
        "css" | "scss" | "less" => {
            writeln!(out, "Styles: selectors, properties, values, blocks.").ok();
        }
        "sql" => {
            writeln!(out, "SQL: clauses (SELECT, FROM, WHERE, JOIN), identifiers, literals.").ok();
        }
        "dockerfile" | "makefile" | "cmake" => {
            writeln!(out, "Build file: valid {lang} instructions and targets.").ok();
        }
        _ => {}
    }
    writeln!(
        out,
        "Suggest valid {lang} syntax: keywords, identifiers, operators, delimiters (; ) ] }} ), strings, calls, imports, attributes, blocks — not only loops."
    )
    .ok();
    writeln!(
        out,
        "For control flow (if/for/while/try/switch/case/do/else/def/class/function) and block bodies: infer full statements from surrounding code — never generic placeholders like `n` or `condition`."
    )
    .ok();

    let partial = extract_partial_token(line_prefix);
    let keywords = languages::keywords_for_path(path);
    if keywords.is_empty() {
        return;
    }
    let lower = partial.to_lowercase();
    let mut shown = 0;
    writeln!(out, "Matching keywords / tokens:").ok();
    for kw in keywords {
        if partial.is_empty() || kw.to_lowercase().starts_with(&lower) {
            writeln!(out, "  {kw}").ok();
            shown += 1;
            if shown >= 16 {
                break;
            }
        }
    }
}

fn append_java_scope(out: &mut String, path: &str, content: &str, line: u32) {
    let scope = java_ecosystem::java_editor_scope(path, content, line);
    writeln!(out, "\n--- Structural context ---").ok();
    if let Some(pkg) = &scope.package {
        writeln!(out, "Package: {pkg}").ok();
    }
    if let Some(fqcn) = &scope.class_fqcn {
        writeln!(out, "Class FQCN: {fqcn}").ok();
    }
    if let Some(class) = &scope.class_name {
        writeln!(out, "Enclosing class: {class}").ok();
    }
    if let Some(method) = &scope.method_name {
        writeln!(out, "Enclosing method: {method}").ok();
    }
    if let Some(sig) = &scope.method_signature {
        writeln!(out, "Method signature: {sig}").ok();
    }
    if !scope.fields.is_empty() {
        writeln!(out, "Class fields / members:").ok();
        for field in &scope.fields {
            writeln!(out, "  {field}").ok();
        }
    }
    if !scope.imports.is_empty() {
        writeln!(out, "Imports:").ok();
        for imp in &scope.imports {
            writeln!(out, "  {imp}").ok();
        }
    }
    if !scope.method_body_lines.is_empty() {
        writeln!(out, "Lines above cursor in this method:").ok();
        for line_text in &scope.method_body_lines {
            writeln!(out, "  {line_text}").ok();
        }
    }
}

fn append_generic_scope(out: &mut String, content: &str, line: u32, path: &str) {
    let lines: Vec<&str> = content.lines().collect();
    let idx = line.saturating_sub(1) as usize;
    if idx >= lines.len() {
        return;
    }
    writeln!(out, "\n--- Structural context ---").ok();
    for i in (0..=idx).rev() {
        let trimmed = lines[i].trim();
        if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with("#") {
            continue;
        }
        if looks_like_function_decl(trimmed, path) {
            writeln!(out, "Enclosing function: {trimmed}").ok();
            break;
        }
        if looks_like_type_decl(trimmed) {
            writeln!(out, "Enclosing type: {trimmed}").ok();
        }
    }
    let start = idx.saturating_sub(28);
    if start < idx {
        writeln!(out, "Lines above cursor:").ok();
        for line_text in &lines[start..idx] {
            writeln!(out, "  {line_text}").ok();
        }
    }
}

fn looks_like_function_decl(line: &str, path: &str) -> bool {
    let lower = path.to_lowercase();
    if lower.ends_with(".py") || lower.ends_with(".pyw") {
        return line.starts_with("def ") || line.starts_with("async def ");
    }
    if lower.ends_with(".rb") {
        return line.starts_with("def ");
    }
    if lower.ends_with(".go") {
        return line.starts_with("func ");
    }
    if lower.ends_with(".rs") {
        return line.contains(" fn ") || line.starts_with("fn ");
    }
    if lower.ends_with(".js")
        || lower.ends_with(".jsx")
        || lower.ends_with(".mjs")
        || lower.ends_with(".cjs")
        || lower.ends_with(".ts")
        || lower.ends_with(".tsx")
    {
        return line.contains("function ")
            || line.contains("=>")
            || line.starts_with("export ")
            || line.starts_with("async ");
    }
    if lower.ends_with(".php") {
        return line.contains("function ");
    }
    if lower.ends_with(".swift") {
        return line.contains(" func ") || line.starts_with("func ");
    }
    if lower.ends_with(".kt") || lower.ends_with(".kts") {
        return line.contains(" fun ") || line.starts_with("fun ");
    }
    if lower.ends_with(".java") {
        return line.contains('(') && !line.trim_start().starts_with("//");
    }
    line.contains("function ")
        || line.contains(" fn ")
        || line.starts_with("def ")
        || line.starts_with("func ")
}

fn looks_like_type_decl(line: &str) -> bool {
    line.contains("class ")
        || line.contains("struct ")
        || line.contains("interface ")
        || line.contains("enum ")
        || line.starts_with("type ")
}

fn append_java_member_hints(
    out: &mut String,
    ws: &Path,
    path: &str,
    line: u32,
    column: u32,
    content: &str,
) {
    if let Some((qualifier, member_prefix)) = symbols::java_dot_qualifier(content, line, column) {
        if let Ok(items) = super::java_completions(
            ws,
            path,
            line,
            column,
            &member_prefix,
            Some(content),
            &[],
        ) {
            if !items.is_empty() {
                writeln!(out, "Members of `{qualifier}` (type-aware):").ok();
                for item in items.iter().take(20) {
                    let detail = item.detail.as_deref().unwrap_or("");
                    writeln!(out, "  {label}  {detail}", label = item.label, detail = detail).ok();
                }
            }
        }
    }
}

fn code_snippet(content: &str, line: u32, line_prefix: &str) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let line_idx = line.saturating_sub(1) as usize;
    let start = line_idx.saturating_sub(12);
    let end = (line_idx + 8).min(lines.len());
    let mut snippet = String::new();
    for i in start..end {
        if i == line_idx {
            writeln!(snippet, ">>> {line_prefix}<CURSOR>").ok();
        } else {
            writeln!(snippet, "    {}", lines[i]).ok();
        }
    }
    snippet
}

fn extract_partial_token(line_prefix: &str) -> String {
    let trimmed = line_prefix.trim_end();
    if trimmed.is_empty() {
        return String::new();
    }
    let mut start = trimmed.len();
    while start > 0 {
        let ch = trimmed[..start].chars().next_back().unwrap_or('\0');
        if ch.is_alphanumeric() || ch == '_' || ch == '$' {
            start -= ch.len_utf8();
        } else {
            break;
        }
    }
    trimmed[start..].to_string()
}

fn keyword_inline_suffix(path: &str, prefix: &str) -> Option<String> {
    if prefix.is_empty() {
        return None;
    }
    let lower = prefix.to_lowercase();
    let mut best: Option<(usize, String)> = None;
    for kw in languages::keywords_for_path(path) {
        let kl = kw.to_lowercase();
        if kl.starts_with(&lower) && kw.len() > prefix.len() {
            let suffix = kw[prefix.len()..].to_string();
            let len = suffix.len();
            if best.as_ref().map(|(l, _)| *l).unwrap_or(usize::MAX) > len {
                best = Some((len, suffix));
            }
        }
    }
    best.map(|(_, s)| s)
}

fn inline_line_indent(line_prefix: &str) -> String {
    line_prefix
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .collect()
}

fn line_ends_with_keyword(trimmed: &str, kw: &str) -> bool {
    if trimmed == kw {
        return true;
    }
    if trimmed.ends_with(kw) {
        let before = &trimmed[..trimmed.len() - kw.len()];
        if before.is_empty() {
            return true;
        }
        let ch = before.chars().last().unwrap_or('\0');
        return ch.is_whitespace() || ch == '(' || ch == '{' || ch == ';';
    }
    false
}

fn infer_while_condition(path: &str, content: &str, line: u32, java_level: u32) -> String {
    infer_inline_condition(path, content, line, "while", java_level)
}

fn infer_if_condition(path: &str, content: &str, line: u32, java_level: u32) -> String {
    infer_inline_condition(path, content, line, "if", java_level)
}

fn editor_context_around_line(content: &str, line: u32, above: usize, below: usize) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let cur = line.saturating_sub(1) as usize;
    if cur > lines.len() {
        return String::new();
    }
    let start = cur.saturating_sub(above);
    let end = (cur + 1 + below).min(lines.len());
    lines[start..end].join("\n")
}

fn read_ident_at_start(s: &str) -> Option<(String, usize)> {
    let s = s.trim_start();
    let mut end = 0;
    for (i, c) in s.char_indices() {
        if c.is_ascii_alphanumeric() || c == '_' || c == '$' {
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

fn scan_list_declaration(ctx: &str) -> Option<(String, String)> {
    for prefix in [
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
    ] {
        let mut search = 0;
        while let Some(idx) = ctx[search..].find(prefix) {
            let start = search + idx;
            let after = &ctx[start + prefix.len()..];
            if let Some(end_gt) = after.find('>') {
                let elem = after[..end_gt].trim();
                let rest = after[end_gt + 1..].trim_start();
                if let Some((name, _)) = read_ident_at_start(rest) {
                    return Some((elem.to_string(), name));
                }
            }
            search = start + prefix.len();
        }
    }
    None
}

fn last_list_declaration(ctx: &str) -> Option<(String, String)> {
    let mut best: Option<(usize, String, String)> = None;
    for prefix in [
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
    ] {
        let mut search = 0;
        while let Some(idx) = ctx[search..].find(prefix) {
            let start = search + idx;
            let after = &ctx[start + prefix.len()..];
            if let Some(end_gt) = after.find('>') {
                let elem = after[..end_gt].trim();
                let rest = after[end_gt + 1..].trim_start();
                if let Some((name, _)) = read_ident_at_start(rest) {
                    if best.as_ref().map(|(p, _, _)| start > *p).unwrap_or(true) {
                        best = Some((start, elem.to_string(), name));
                    }
                }
            }
            search = start + prefix.len();
        }
    }
    best.map(|(_, elem, name)| (elem, name))
}

fn last_map_declaration(ctx: &str) -> Option<(String, String, String)> {
    let mut best: Option<(usize, String, String, String)> = None;
    for prefix in ["Map<", "HashMap<", "LinkedHashMap<", "TreeMap<", "ConcurrentHashMap<"] {
        let mut search = 0;
        while let Some(idx) = ctx[search..].find(prefix) {
            let start = search + idx;
            let after = &ctx[start + prefix.len()..];
            if let Some(end_gt) = after.find('>') {
                let inner = after[..end_gt].trim();
                let rest = after[end_gt + 1..].trim_start();
                if let Some((name, _)) = read_ident_at_start(rest) {
                    let (key, val) = if let Some(comma) = inner.find(',') {
                        (
                            inner[..comma].trim().to_string(),
                            inner[comma + 1..].trim().to_string(),
                        )
                    } else {
                        ("Object".into(), "Object".into())
                    };
                    if best.as_ref().map(|(p, _, _, _)| start > *p).unwrap_or(true) {
                        best = Some((start, key, val, name));
                    }
                }
            }
            search = start + prefix.len();
        }
    }
    best.map(|(_, k, v, n)| (k, v, n))
}

fn last_typed_java_array(ctx: &str) -> Option<(String, String)> {
    let mut best: Option<(usize, String, String)> = None;
    for (line_off, line) in ctx.lines().enumerate() {
        let trimmed = line.trim();
        if let Some(bracket) = trimmed.find('[') {
            let before = trimmed[..bracket].trim_end();
            let type_part = before.split_whitespace().last().unwrap_or("").trim();
            if type_part.is_empty() {
                continue;
            }
            let rest = &trimmed[bracket..];
            if rest.starts_with('[') {
                if let Some(end) = rest.find(']') {
                    let after = rest[end + 1..].trim_start();
                    if let Some((name, _)) = read_ident_at_start(after) {
                        let pos = ctx.lines().take(line_off).map(|l| l.len() + 1).sum::<usize>()
                            + line.find(trimmed).unwrap_or(0);
                        if best.as_ref().map(|(p, _, _)| pos > *p).unwrap_or(true) {
                            best = Some((pos, type_part.to_string(), name));
                        }
                    }
                }
            }
        }
    }
    best.map(|(_, t, n)| (t, n))
}

fn for_each_var_name(init: &str) -> Option<String> {
    let init = init.trim();
    if !init.contains(':') || init.contains(';') {
        return None;
    }
    let colon = init.find(':')?;
    let lhs = init[..colon].trim();
    let parts: Vec<&str> = lhs.split_whitespace().collect();
    parts.last().map(|s| s.to_string())
}

fn scan_array_declaration(ctx: &str) -> Option<String> {
    let mut i = 0;
    while i < ctx.len() {
        if let Some((name, consumed)) = read_ident_at_start(&ctx[i..]) {
            let rest = ctx[i + consumed..].trim_start();
            if rest.starts_with('[') {
                return Some(name);
            }
            i += consumed.max(1);
        } else {
            i += 1;
        }
    }
    None
}

fn scan_const_array(ctx: &str) -> Option<String> {
    if let Some(idx) = ctx.find("const ") {
        let after = &ctx[idx + 6..];
        if let Some((name, consumed)) = read_ident_at_start(after) {
            let rest = after[consumed..].trim_start();
            if rest.starts_with('=') {
                let rest = rest[1..].trim_start();
                if rest.starts_with('[') {
                    return Some(name);
                }
            }
        }
    }
    None
}

fn collect_boolean_vars(ctx: &str) -> Vec<String> {
    let mut out = Vec::new();
    for marker in ["boolean ", "bool "] {
        let mut search = 0;
        while let Some(idx) = ctx[search..].find(marker) {
            let after = &ctx[search + idx + marker.len()..];
            if let Some((name, _)) = read_ident_at_start(after) {
                if !out.iter().any(|v| v == &name) {
                    out.push(name);
                }
            }
            search += idx + marker.len();
        }
    }
    out
}

fn collect_condition_exprs(ctx: &str) -> Vec<String> {
    let mut out = Vec::new();
    for kw in ["if", "while"] {
        let pattern = format!("{kw}(");
        let mut search = 0;
        while let Some(idx) = ctx[search..].find(&pattern) {
            let after = &ctx[search + idx + pattern.len()..];
            if let Some(end) = after.find(')') {
                let c = after[..end].trim();
                if !c.is_empty() && c != "condition" && c.len() < 72 {
                    out.push(c.to_string());
                }
            }
            search += idx + pattern.len();
        }
        let pattern_sp = format!("{kw} (");
        search = 0;
        while let Some(idx) = ctx[search..].find(&pattern_sp) {
            let after = &ctx[search + idx + pattern_sp.len()..];
            if let Some(end) = after.find(')') {
                let c = after[..end].trim();
                if !c.is_empty() && c != "condition" && c.len() < 72 {
                    out.push(c.to_string());
                }
            }
            search += idx + pattern_sp.len();
        }
    }
    out
}

fn infer_inline_condition(path: &str, content: &str, line: u32, kind: &str, _java_level: u32) -> String {
    let lang = languages::language_for_path(path).unwrap_or("plaintext");
    let ctx = editor_context_around_line(content, line, 25, 15);
    let bool_vars = collect_boolean_vars(&ctx);
    let conds = collect_condition_exprs(&ctx);

    let is_java_like = lang == "java" || lang == "kotlin" || lang == "groovy";
    let is_c_like = is_java_like
        || lang == "c"
        || lang == "cpp"
        || lang == "csharp"
        || lang == "swift";

    if kind == "while" {
        if is_java_like {
            if let Some(name) = scan_iterator_assignment(&ctx) {
                return format!("{name}.hasNext()");
            }
            if ctx.contains("Iterator<") || ctx.contains("iterator()") {
                return "iterator.hasNext()".to_string();
            }
            if let Some(name) = scan_list_declaration(&ctx).map(|(_, n)| n)
                .or_else(|| last_list_declaration(&ctx).map(|(_, n)| n))
            {
                return format!("i < {name}.size()");
            }
            if let Some(name) = scan_array_declaration(&ctx) {
                return format!("i < {name}.length");
            }
        }
        if lang == "javascript" || lang == "typescript" {
            if let Some(name) = scan_const_array(&ctx) {
                return format!("i < {name}.length");
            }
        }
        for flag in ["running", "done", "hasMore", "active", "valid", "found"] {
            if ctx.split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                .any(|w| w == flag)
            {
                if flag == "hasMore" && is_java_like {
                    return "iterator.hasNext()".to_string();
                }
                return flag.to_string();
            }
        }
        if let Some(v) = bool_vars.last() {
            return v.clone();
        }
        if let Some(c) = conds.last() {
            return c.clone();
        }
        if is_c_like {
            return "true".to_string();
        }
        if lang == "python" {
            return "True".to_string();
        }
        return "condition".to_string();
    }

    if kind == "if" {
        if let Some(c) = conds.last() {
            return c.clone();
        }
        if let Some(v) = bool_vars.last() {
            return v.clone();
        }
        if is_c_like {
            return "true".to_string();
        }
        if lang == "python" {
            return "True".to_string();
        }
        return "condition".to_string();
    }

    "condition".to_string()
}

fn scan_iterator_assignment(ctx: &str) -> Option<String> {
    if let Some(idx) = ctx.find(".iterator()") {
        let before = ctx[..idx].trim_end();
        if let Some(eq_idx) = before.rfind('=') {
            let lhs = before[..eq_idx].trim();
            let name = lhs
                .split(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '$')
                .filter(|s| !s.is_empty())
                .last()?;
            return Some(name.to_string());
        }
    }
    None
}

const CONTROL_KEYWORD_PREFIXES: &[&str] = &[
    "if", "else", "for", "while", "do", "switch", "case", "try", "catch", "finally",
    "elif", "def", "class", "function", "fun", "when", "break", "continue", "return",
];

fn is_inside_control_paren(line_prefix: &str) -> bool {
    let trimmed = line_prefix.trim_end();
    for kw in ["for", "if", "while", "switch"] {
        for open in [format!("{kw} ("), format!("{kw}(")] {
            if let Some(idx) = trimmed.rfind(&open) {
                let after_paren = trimmed[idx..].split_once('(').map(|(_, r)| r).unwrap_or("");
                if !after_paren.contains(')') {
                    return true;
                }
            }
        }
    }
    false
}

fn is_control_keyword_prefix(token: &str) -> bool {
    if token.is_empty() {
        return false;
    }
    let lower = token.to_lowercase();
    CONTROL_KEYWORD_PREFIXES.iter().any(|kw| kw.starts_with(&lower))
}

fn line_ends_with_control_keyword(trimmed: &str) -> bool {
    CONTROL_KEYWORD_PREFIXES
        .iter()
        .any(|kw| line_ends_with_keyword(trimmed, kw))
        || trimmed.ends_with("for (")
        || trimmed.ends_with("for(")
        || trimmed.ends_with("if (")
        || trimmed.ends_with("if(")
        || trimmed.ends_with("while (")
        || trimmed.ends_with("while(")
        || trimmed.ends_with("switch (")
        || trimmed.ends_with("switch(")
}

fn markup_or_config_language(path: &str) -> bool {
    matches!(
        languages::language_for_path(path),
        Some(
            "markdown" | "plaintext" | "yaml" | "json" | "html" | "xml" | "toml" | "ini" | "css"
                | "scss" | "less" | "sql" | "dockerfile" | "makefile" | "cmake" | "graphql"
                | "protobuf"
        )
    )
}

/// Prefer AI over local symbol/keyword fallback for statements and block bodies (all languages).
pub fn should_prefer_ai_statement_inline(
    path: &str,
    line_prefix: &str,
    content: &str,
    line: u32,
) -> bool {
    if is_import_typing_line(path, content, line, line_prefix) {
        return false;
    }
    if markup_or_config_language(path) {
        return true;
    }
    if is_whitespace_only_line(line_prefix) {
        return true;
    }
    let trimmed = line_prefix.trim_end();
    if line_ends_with_control_keyword(trimmed) {
        return true;
    }
    if is_inside_control_paren(line_prefix) {
        return true;
    }
    let partial = extract_partial_token(line_prefix);
    if !partial.is_empty() && is_control_keyword_prefix(&partial) {
        return true;
    }
    false
}

/// Import/using lines — classpath index ghost only (respects project compiler version), not AI.
pub fn is_import_typing_line(path: &str, content: &str, line: u32, line_prefix: &str) -> bool {
    if super::symbols::is_java_import_line(content, line) {
        return true;
    }
    let trimmed = line_prefix.trim_start();
    match languages::language_for_path(path).unwrap_or("plaintext") {
        "csharp" => trimmed.starts_with("using "),
        "python" => trimmed.starts_with("import ") || trimmed.starts_with("from "),
        "rust" => trimmed.starts_with("use "),
        "javascript" | "typescript" => {
            trimmed.starts_with("import ") || trimmed.contains(" from ")
        }
        "go" | "swift" | "dart" | "kotlin" | "groovy" | "java" => trimmed.starts_with("import "),
        _ => trimmed.starts_with("import "),
    }
}

fn control_structure_inline_suffix(
    _line_prefix: &str,
    _path: &str,
    _content: &str,
    _line: u32,
    _java_level: u32,
) -> Option<String> {
    // Statement templates disabled — use AI inline completion for if/for/while/etc.
    None
}

fn is_whitespace_only_line(line_prefix: &str) -> bool {
    extract_partial_token(line_prefix).is_empty()
}

fn find_enclosing_block(content: &str, line: u32) -> Option<(String, String, String)> {
    let lines: Vec<&str> = content.lines().collect();
    let cur = line.saturating_sub(1) as usize;
    if cur >= lines.len() {
        return None;
    }
    let cur_indent = inline_line_indent(lines[cur]);
    for i in (0..cur).rev() {
        let line_text = lines[i];
        let ind = inline_line_indent(line_text);
        let trimmed = line_text.trim_end();
        if ind.len() < cur_indent.len() && trimmed.ends_with('{') {
            if let Some(cond) = trimmed.strip_prefix("while (").and_then(|s| s.strip_suffix('{').or(Some(s))) {
                if let Some(c) = cond.strip_suffix(')').or(Some(cond)) {
                    return Some(("while".into(), c.trim().to_string(), cur_indent.clone()));
                }
            }
            if let Some(start) = trimmed.find("while (") {
                let rest = &trimmed[start + 7..];
                if let Some(end) = rest.find(')') {
                    return Some(("while".into(), rest[..end].trim().to_string(), cur_indent.clone()));
                }
            }
            if let Some(start) = trimmed.find("for (") {
                let rest = &trimmed[start + 5..];
                if let Some(end) = rest.find(')') {
                    let inside = rest[..end].trim();
                    let kind = if inside.contains(':') && !inside.contains(';') {
                        "for-each"
                    } else {
                        "for"
                    };
                    return Some((kind.into(), inside.to_string(), cur_indent.clone()));
                }
            }
            if let Some(start) = trimmed.find("if (") {
                let rest = &trimmed[start + 4..];
                if let Some(end) = rest.find(')') {
                    return Some(("if".into(), rest[..end].trim().to_string(), cur_indent.clone()));
                }
            }
            if trimmed.contains("else") {
                return Some(("else".into(), String::new(), cur_indent.clone()));
            }
        }
        if trimmed == "{" && i > 0 {
            let prev = lines[i - 1].trim_end();
            if let Some(start) = prev.find("while (") {
                let rest = &prev[start + 7..];
                if let Some(end) = rest.find(')') {
                    return Some(("while".into(), rest[..end].trim().to_string(), cur_indent.clone()));
                }
            }
            if let Some(start) = prev.find("for (") {
                let rest = &prev[start + 5..];
                if let Some(end) = rest.find(')') {
                    let inside = rest[..end].trim();
                    let kind = if inside.contains(':') && !inside.contains(';') {
                        "for-each"
                    } else {
                        "for"
                    };
                    return Some((kind.into(), inside.to_string(), cur_indent.clone()));
                }
            }
        }
    }
    None
}

fn empty_line_inline_suffix(
    _path: &str,
    _content: &str,
    _line: u32,
    _line_prefix: &str,
    _java_level: u32,
) -> Option<String> {
    None
}

fn suffix_after_prefix(full_line: &str, line_prefix: &str) -> Option<String> {
    if full_line.starts_with(line_prefix) {
        return Some(full_line[line_prefix.len()..].to_string());
    }
    let want = inline_line_indent(full_line);
    let have = inline_line_indent(line_prefix);
    if full_line.len() >= have.len() && want.starts_with(&have) {
        return Some(full_line[have.len()..].to_string());
    }
    None
}

pub fn inline_completion_fallback(
    ws: &Path,
    path: &str,
    line: u32,
    column: u32,
    content: &str,
    line_prefix: &str,
) -> Option<String> {
    if super::symbols::is_java_import_line(content, line) {
        if let Ok(items) =
            super::java_completions(ws, path, line, column, "", Some(content), &[])
        {
            if let Some(insert) = items.first().map(|i| {
                i.insert
                    .as_deref()
                    .unwrap_or(&i.label)
                    .to_string()
            }) {
                if !insert.is_empty() {
                    return Some(insert);
                }
            }
        }
    }

    let word = symbols::word_at(content, line, column).unwrap_or_default();
    let prefix = if word.is_empty() {
        symbols::java_dot_qualifier(content, line, column)
            .map(|(_, member)| member)
            .unwrap_or_else(|| extract_partial_token(line_prefix))
    } else {
        word
    };

    if !prefix.is_empty() {
        if classpath::is_java_like(path) || path.ends_with(".java") {
            if let Ok(items) =
                super::java_completions(ws, path, line, column, &prefix, Some(content), &[])
            {
                if let Some(insert) = items.first().map(|i| {
                    i.insert
                        .as_deref()
                        .unwrap_or(&i.label)
                        .to_string()
                }) {
                    if !insert.is_empty() {
                        return Some(insert);
                    }
                }
            }
        }
        if let Ok(items) = super::symbols::completions(ws, path, content, &prefix, line, column) {
            if let Some(insert) = items.first().map(|i| {
                i.insert
                    .as_deref()
                    .unwrap_or(&i.label)
                    .to_string()
            }) {
                if !insert.is_empty() {
                    return Some(insert);
                }
            }
        }
        if let Some(suffix) = keyword_inline_suffix(path, &prefix) {
            return Some(suffix);
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::should_prefer_ai_statement_inline;
    use super::is_import_typing_line;

    #[test]
    fn import_lines_skip_ai() {
        let java = "package com.example;\nimport org.spring\n";
        assert!(is_import_typing_line("src/App.java", java, 2, "import org.spring"));
        assert!(!should_prefer_ai_statement_inline(
            "src/App.java",
            "import org.spring",
            java,
            2,
        ));
    }

    #[test]
    fn prefer_ai_for_all_markup_and_config_languages() {
        let langs = [
            ("README.md", "    ", 2),
            ("notes.txt", "    ", 2),
            ("config.yaml", "title: ", 1),
            ("data.json", "  \"a\": ", 2),
            ("index.html", "  <p>", 2),
            ("pom.xml", "  <root>", 2),
            ("Cargo.toml", "name = ", 2),
            ("app.ini", "key=", 2),
            ("style.css", "  color: ", 2),
            ("query.sql", "SELECT ", 1),
            ("Dockerfile", "RUN ", 2),
            ("Makefile", "\t", 2),
        ];
        for (path, prefix, line) in langs {
            assert!(
                should_prefer_ai_statement_inline(path, prefix, "ctx\n", line),
                "expected AI for {path}"
            );
        }
    }

    #[test]
    fn prefer_ai_for_empty_lines_in_code_languages() {
        let body = "class A {\n  void m() {\n    \n  }\n}\n";
        assert!(should_prefer_ai_statement_inline("src/App.java", "    ", body, 3));
        assert!(should_prefer_ai_statement_inline("app.py", "    ", "def m():\n    \n", 2));
    }
}
