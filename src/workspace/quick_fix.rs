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

/// Drop AI import-only fixes that guess the wrong type when local/well-known imports exist.
pub fn filter_ai_import_fixes(
    ws: &Path,
    path: &str,
    content: &str,
    local: &[QuickFix],
    ai: Vec<QuickFix>,
    diagnostics: &[QuickFixDiagnostic],
) -> Vec<QuickFix> {
    let preferred = preferred_import_fqcns(ws, path, content, local, diagnostics);
    if preferred.is_empty() {
        return ai;
    }
    ai.into_iter()
        .filter(|fix| !should_drop_ai_import_fix(fix, &preferred))
        .collect()
}

fn preferred_import_fqcns(
    ws: &Path,
    path: &str,
    content: &str,
    local: &[QuickFix],
    diagnostics: &[QuickFixDiagnostic],
) -> std::collections::HashMap<String, String> {
    let mut out = std::collections::HashMap::new();
    for fix in local {
        if fix.provider.as_deref() != Some("local") {
            continue;
        }
        let Some(symbol) = import_fix_symbol(&fix.title) else {
            continue;
        };
        let Some(fqcn) = fix.edits.iter().find_map(|e| parse_import_fqcn(&e.text)) else {
            continue;
        };
        out.insert(symbol.to_string(), fqcn);
    }
    for diag in diagnostics {
        let Some(symbol) = extract_class_symbol(&diag.message) else {
            continue;
        };
        if out.contains_key(&symbol) {
            continue;
        }
        if let Ok(Some(fqcn)) = classpath::import_fqcn_for_symbol(ws, path, content, &symbol) {
            out.entry(symbol).or_insert(fqcn);
        }
    }
    out
}

fn should_drop_ai_import_fix(
    fix: &QuickFix,
    preferred: &std::collections::HashMap<String, String>,
) -> bool {
    if !is_import_only_fix(fix) {
        return false;
    }
    let ai_imports: Vec<String> = fix
        .edits
        .iter()
        .filter_map(|e| parse_import_fqcn(&e.text))
        .collect();
    if ai_imports.is_empty() {
        return false;
    }
    for pref in preferred.values() {
        if ai_imports.iter().any(|imp| imp == pref) {
            return false;
        }
    }
    true
}

fn import_fix_symbol(title: &str) -> Option<&str> {
    title.strip_prefix("Add import for ")
}

fn parse_import_fqcn(text: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let t = line.trim();
        t.strip_prefix("import ")
            .and_then(|rest| rest.strip_suffix(';'))
            .map(|fqcn| fqcn.trim().to_string())
            .filter(|fqcn| !fqcn.is_empty())
    })
}

fn is_import_only_fix(fix: &QuickFix) -> bool {
    !fix.edits.is_empty()
        && fix.edits.iter().all(|e| {
            e.text.lines().all(|line| {
                let t = line.trim();
                t.is_empty() || t.starts_with("import ")
            })
        })
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

fn import_insert_suffix(content: &str, insert_line: u32) -> String {
    let next = content
        .lines()
        .nth(insert_line.saturating_sub(1) as usize)
        .map(str::trim)
        .unwrap_or("");
    let blank_before_body = next.starts_with('@')
        || next.starts_with("public ")
        || next.starts_with("private ")
        || next.starts_with("protected ")
        || next.starts_with("class ")
        || next.starts_with("interface ")
        || next.starts_with("enum ")
        || next.starts_with("record ");
    if blank_before_body {
        "\n\n".to_string()
    } else {
        "\n".to_string()
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
    let suffix = import_insert_suffix(content, insert_line);
    let needs_leading_newline = insert_line > 1;
    let text = if needs_leading_newline {
        format!("\n{import_line}{suffix}")
    } else {
        format!("{import_line}{suffix}")
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
    fn import_insert_before_annotation_leaves_blank_line() {
        let content = "package com.example;\n\nimport org.springframework.boot.SpringApplication;\n@SpringBootApplication\npublic class App {}\n";
        let edit = import_insert_edit(content, "org.springframework.stereotype.Service");
        assert!(edit.text.contains("import org.springframework.stereotype.Service;"));
        assert!(edit.text.ends_with("\n\n"), "expected blank line before @ annotation, got {:?}", edit.text);
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
    fn filters_ai_wrong_import_when_well_known_local_exists() {
        let local = vec![QuickFix {
            title: "Add import for Files".into(),
            edits: vec![QuickFixEdit {
                start_line: 2,
                start_column: 1,
                end_line: 2,
                end_column: 1,
                text: "\nimport java.nio.file.Files;".into(),
            }],
            provider: Some("local".into()),
        }];
        let ai_wrong = QuickFix {
            title: "Import File".into(),
            edits: vec![QuickFixEdit {
                start_line: 2,
                start_column: 1,
                end_line: 2,
                end_column: 1,
                text: "\nimport java.io.File;".into(),
            }],
            provider: Some("gemini".into()),
        };
        let ai_right = QuickFix {
            title: "Import Files".into(),
            edits: vec![QuickFixEdit {
                start_line: 2,
                start_column: 1,
                end_line: 2,
                end_column: 1,
                text: "\nimport java.nio.file.Files;".into(),
            }],
            provider: Some("gemini".into()),
        };
        let diags = vec![QuickFixDiagnostic {
            line: 5,
            column: 22,
            message: "cannot find symbol\n  symbol:   class Files\n  location: class App".into(),
            severity: "error".into(),
        }];
        let ws = std::env::temp_dir().join("reaper-qf-filter");
        let filtered = filter_ai_import_fixes(&ws, "App.java", "class App {}", &local, vec![ai_wrong, ai_right], &diags);
        assert_eq!(filtered.len(), 1);
        assert!(filtered[0].edits[0].text.contains("java.nio.file.Files"));
    }

    #[test]
    fn keeps_ai_fix_when_it_also_edits_code() {
        let local = vec![QuickFix {
            title: "Add import for Files".into(),
            edits: vec![QuickFixEdit {
                start_line: 2,
                start_column: 1,
                end_line: 2,
                end_column: 1,
                text: "\nimport java.nio.file.Files;".into(),
            }],
            provider: Some("local".into()),
        }];
        let ai_mixed = QuickFix {
            title: "Fix Files usage".into(),
            edits: vec![
                QuickFixEdit {
                    start_line: 2,
                    start_column: 1,
                    end_line: 2,
                    end_column: 1,
                    text: "\nimport java.io.File;".into(),
                },
                QuickFixEdit {
                    start_line: 5,
                    start_column: 1,
                    end_line: 5,
                    end_column: 30,
                    text: "Files.exists(path)".into(),
                },
            ],
            provider: Some("gemini".into()),
        };
        let diags = vec![QuickFixDiagnostic {
            line: 5,
            column: 22,
            message: "cannot find symbol\n  symbol:   class Files\n  location: class App".into(),
            severity: "error".into(),
        }];
        let ws = std::env::temp_dir().join("reaper-qf-filter-mixed");
        let filtered = filter_ai_import_fixes(&ws, "App.java", "class App {}", &local, vec![ai_mixed], &diags);
        assert_eq!(filtered.len(), 1);
    }

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
