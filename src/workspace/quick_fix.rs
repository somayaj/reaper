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
    }
    Ok(fixes)
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
}
