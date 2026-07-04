use std::path::Path;

use anyhow::Result;

use super::classpath;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuickFix {
    pub title: String,
    pub edits: Vec<QuickFixEdit>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuickFixEdit {
    pub start_line: u32,
    pub start_column: u32,
    pub end_line: u32,
    pub end_column: u32,
    pub text: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct QuickFixDiagnostic {
    pub line: u32,
    pub column: u32,
    pub message: String,
    #[serde(default)]
    pub severity: String,
}

impl QuickFixEdit {
    pub fn clamp_to_document(&mut self, line_count: u32, line_len: impl Fn(u32) -> u32) {
        self.start_line = self.start_line.clamp(1, line_count);
        self.end_line = self.end_line.clamp(1, line_count);
        let start_max = line_len(self.start_line);
        let end_max = line_len(self.end_line);
        self.start_column = self.start_column.clamp(1, start_max.max(1));
        self.end_column = self.end_column.clamp(1, end_max.max(1));
        if self.end_line == self.start_line && self.end_column < self.start_column {
            self.end_column = self.start_column;
        }
    }
}

pub fn suggest_local_quick_fixes(
    ws: &Path,
    path: &str,
    content: &str,
    diagnostics: &[QuickFixDiagnostic],
) -> Result<Vec<QuickFix>> {
    if !path.ends_with(".java") || diagnostics.is_empty() {
        return Ok(Vec::new());
    }

    let mut fixes = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for diag in diagnostics {
        if let Some(fix) = import_fix_for_diagnostic(ws, path, content, diag)? {
            let key = fix
                .edits
                .iter()
                .map(|e| format!("{}:{}:{}", e.start_line, e.start_column, e.text))
                .collect::<String>();
            if seen.insert(key) {
                fixes.push(fix);
            }
        }
        if let Some(fix) = file_exists_receiver_fix_for_diagnostic(content, diag) {
            let key = fix
                .edits
                .iter()
                .map(|e| format!("{}:{}:{}", e.start_line, e.start_column, e.text))
                .collect::<String>();
            if seen.insert(key) {
                fixes.push(fix);
            }
        }
    }
    Ok(fixes)
}

fn quick_fix_dedupe_key(fix: &QuickFix) -> String {
    format!(
        "{}:{}",
        fix.title,
        fix.edits
            .iter()
            .map(|e| {
                format!(
                    "{}:{}:{}:{}:{}",
                    e.start_line, e.start_column, e.end_line, e.end_column, e.text
                )
            })
            .collect::<String>()
    )
}

/// Append `from` onto `into`, skipping fixes with identical title + edits.
pub fn merge_quick_fixes(into: &mut Vec<QuickFix>, from: Vec<QuickFix>) {
    let mut seen: std::collections::HashSet<String> =
        into.iter().map(quick_fix_dedupe_key).collect();
    for fix in from {
        if seen.insert(quick_fix_dedupe_key(&fix)) {
            into.push(fix);
        }
    }
}

fn file_exists_receiver_fix_for_diagnostic(
    content: &str,
    diag: &QuickFixDiagnostic,
) -> Option<QuickFix> {
    if !diag.message.contains("cannot find symbol") {
        return None;
    }
    let about_exists = diag.message.contains("method exists()")
        || diag.message.contains("symbol:   method exists")
        || diag.message.contains("variable exist")
        || diag.message.contains("variable exists");
    if !about_exists {
        return None;
    }
    let file_var = infer_java_file_receiver_var(content)?;
    let line = content.lines().nth(diag.line.saturating_sub(1) as usize)?;

    if let Some((start, end)) = find_bare_method_call_span(line, "exists") {
        return Some(QuickFix {
            title: format!("Change exists() to {file_var}.exists()"),
            edits: vec![QuickFixEdit {
                start_line: diag.line,
                start_column: start,
                end_line: diag.line,
                end_column: end,
                text: format!("{file_var}.exists()"),
            }],
            provider: Some("local".into()),
        });
    }

    for typo in ["exist", "exists"] {
        let pattern = format!("{file_var}.{typo}");
        if let Some(idx) = line.find(&pattern) {
            let start_col = idx as u32 + 1;
            let end_col = start_col + pattern.len() as u32;
            return Some(QuickFix {
                title: format!("Change {pattern} to {file_var}.exists()"),
                edits: vec![QuickFixEdit {
                    start_line: diag.line,
                    start_column: start_col,
                    end_line: diag.line,
                    end_column: end_col,
                    text: format!("{file_var}.exists()"),
                }],
                provider: Some("local".into()),
            });
        }
    }
    None
}

fn infer_java_file_receiver_var(content: &str) -> Option<String> {
    let mut vars = std::collections::HashSet::new();
    for line in content.lines() {
        let mut search = line;
        while let Some(idx) = search.find("File") {
            let before_ok = idx == 0
                || !search.as_bytes()[idx - 1].is_ascii_alphanumeric()
                    && search.as_bytes()[idx - 1] != b'.';
            let after = search[idx + 4..].trim_start();
            if before_ok {
                if let Some(name) = read_java_ident(after) {
                    if name != "createTempFile" {
                        vars.insert(name);
                    }
                }
            }
            search = &search[idx + 4..];
        }
    }
    if vars.contains("file") {
        return Some("file".into());
    }
    vars.into_iter().next()
}

fn read_java_ident(s: &str) -> Option<String> {
    let s = s.trim_start();
    let mut chars = s.chars();
    let first = chars.next()?;
    if !first.is_ascii_alphabetic() && first != '_' {
        return None;
    }
    let mut name = String::new();
    name.push(first);
    for ch in chars {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            name.push(ch);
        } else {
            break;
        }
    }
    if name.is_empty() { None } else { Some(name) }
}

fn find_bare_method_call_span(line: &str, method: &str) -> Option<(u32, u32)> {
    let open_needle = format!("{method}(");
    let mut search_from = 0usize;
    while let Some(rel) = line[search_from..].find(&open_needle) {
        let start = search_from + rel;
        let before_ok = start == 0
            || {
                let b = line.as_bytes()[start - 1];
                !b.is_ascii_alphanumeric() && b != b'.'
            };
        if before_ok {
            let open_paren = start + method.len();
            if line.as_bytes().get(open_paren) == Some(&b'(') {
                if let Some(close_rel) = line[open_paren + 1..].find(')') {
                    let end = open_paren + 1 + close_rel + 1;
                    return Some((start as u32 + 1, end as u32 + 1));
                }
            }
        }
        search_from = start + 1;
    }
    None
}

fn import_fix_for_diagnostic(
    ws: &Path,
    path: &str,
    content: &str,
    diag: &QuickFixDiagnostic,
) -> Result<Option<QuickFix>> {
    if !diag.message.contains("cannot find symbol") {
        return Ok(None);
    }
    let Some(symbol) = extract_class_symbol(&diag.message) else {
        return Ok(None);
    };
    let Some(fqcn) = classpath::import_fqcn_for_symbol(ws, path, content, &symbol)? else {
        return Ok(None);
    };
    if content.contains(&format!("import {fqcn};")) {
        return Ok(None);
    }
    let edit = import_insert_edit(content, &fqcn);
    Ok(Some(QuickFix {
        title: format!("Add import for {symbol}"),
        edits: vec![edit],
        provider: Some("local".into()),
    }))
}

fn extract_class_symbol(message: &str) -> Option<String> {
    let idx = message.find("symbol:")?;
    let rest = message[idx + "symbol:".len()..].trim();
    let mut parts = rest.split_whitespace();
    let first = parts.next()?;
    if matches!(first, "class" | "variable" | "method" | "interface") {
        parts.next().map(str::to_string)
    } else {
        Some(first.to_string())
    }
}

fn import_insert_edit(content: &str, fqcn: &str) -> QuickFixEdit {
    let import_line = format!("import {fqcn};");
    let lines: Vec<&str> = content.lines().collect();
    let mut insert_after = 0usize;
    let mut last_import = None;

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("package ") {
            insert_after = i + 1;
        } else if trimmed.starts_with("import ") {
            last_import = Some(i);
        } else if !trimmed.is_empty() && !trimmed.starts_with("//") && last_import.is_none() && insert_after == 0
        {
            break;
        }
    }

    let insert_line = last_import.map(|i| i + 2).unwrap_or(insert_after + 1).max(1) as u32;
    let needs_leading_newline = insert_line > 1;
    let text = if needs_leading_newline {
        format!("\n{import_line}")
    } else {
        format!("{import_line}\n")
    };

    QuickFixEdit {
        start_line: insert_line,
        start_column: 1,
        end_line: insert_line,
        end_column: 1,
        text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_class_symbol_from_javac_message() {
        let msg = "cannot find symbol symbol: class RestController location: class App";
        assert_eq!(
            extract_class_symbol(msg).as_deref(),
            Some("RestController")
        );
    }

    #[test]
    fn import_insert_after_package() {
        let content = "package com.example;\n\npublic class App {}\n";
        let edit = import_insert_edit(content, "org.springframework.web.bind.annotation.RestController");
        assert_eq!(edit.start_line, 2);
        assert!(edit.text.contains("RestController"));
    }

    #[test]
    fn file_exists_receiver_fix_replaces_bare_exists_call() {
        let content = r#"import java.io.File;
public class App {
    void m() {
        File file = new File("x");
        System.out.println(exists());
    }
}
"#;
        let diag = QuickFixDiagnostic {
            line: 5,
            column: 28,
            message: "cannot find symbol\n  symbol:   method exists()\n  location: class App"
                .into(),
            severity: "error".into(),
        };
        let fix = file_exists_receiver_fix_for_diagnostic(content, &diag).expect("fix");
        assert_eq!(fix.edits.len(), 1);
        assert_eq!(fix.edits[0].text, "file.exists()");
    }

    #[test]
    fn file_exists_receiver_fix_replaces_member_without_parens() {
        let content = r#"import java.io.File;
public class App {
    void m() {
        File file = new File("x");
        if (file.exist) { }
    }
}
"#;
        let diag = QuickFixDiagnostic {
            line: 5,
            column: 13,
            message: "cannot find symbol\n  symbol:   variable exist\n  location: variable file of type File"
                .into(),
            severity: "error".into(),
        };
        let fix = file_exists_receiver_fix_for_diagnostic(content, &diag).expect("fix");
        assert_eq!(fix.edits[0].text, "file.exists()");
    }
}
