//! Shared LSP response types and parsers used by jdtls, clangd, and solargraph clients.

use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::quick_fix::QuickFixEdit;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReferenceLocation {
    pub path: String,
    pub line: u32,
    pub column: u32,
    pub end_line: u32,
    pub end_column: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenameRange {
    pub line: u32,
    pub column: u32,
    pub end_line: u32,
    pub end_column: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileTextEdits {
    pub path: String,
    pub edits: Vec<QuickFixEdit>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignatureParameter {
    pub label: String,
    pub documentation: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignatureInfo {
    pub label: String,
    pub documentation: Option<String>,
    pub parameters: Vec<SignatureParameter>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignatureHelp {
    pub signatures: Vec<SignatureInfo>,
    pub active_signature: u32,
    pub active_parameter: Option<u32>,
}

pub fn uri_to_workspace_path(ws: &Path, uri: &str) -> Result<String> {
    let url = url::Url::parse(uri).with_context(|| format!("parse uri {uri}"))?;
    let path = url
        .to_file_path()
        .map_err(|_| anyhow::anyhow!("uri is not a file path: {uri}"))?;
    let rel = path
        .strip_prefix(ws)
        .unwrap_or(&path)
        .to_string_lossy()
        .replace('\\', "/");
    Ok(rel)
}

pub fn parse_reference_locations(ws: &Path, result: &Value) -> Vec<ReferenceLocation> {
    let Some(items) = result.as_array() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for item in items {
        let Some((path, line, column, end_line, end_column)) = parse_location(ws, item) else {
            continue;
        };
        out.push(ReferenceLocation {
            path,
            line,
            column,
            end_line,
            end_column,
        });
    }
    out
}

pub fn parse_rename_range(result: &Value) -> Option<RenameRange> {
    if result.is_null() {
        return None;
    }
    let (line, column, end_line, end_column) = parse_lsp_range(result)?;
    Some(RenameRange {
        line,
        column,
        end_line,
        end_column,
    })
}

pub fn parse_location(ws: &Path, loc: &Value) -> Option<(String, u32, u32, u32, u32)> {
    let uri = loc
        .get("uri")
        .or_else(|| loc.get("targetUri"))
        .and_then(|v| v.as_str())?;
    let range = loc
        .get("range")
        .or_else(|| loc.get("targetRange"))
        .or_else(|| loc.get("targetSelectionRange"))?;
    let (line, column, end_line, end_column) = parse_lsp_range(range)?;
    let path = uri_to_workspace_path(ws, uri).ok()?;
    Some((path, line, column, end_line, end_column))
}

pub fn parse_lsp_range(range: &Value) -> Option<(u32, u32, u32, u32)> {
    let start = range.get("start")?;
    let end = range.get("end")?;
    Some((
        start.get("line")?.as_u64()? as u32 + 1,
        start.get("character")?.as_u64()? as u32 + 1,
        end.get("line")?.as_u64()? as u32 + 1,
        end.get("character")?.as_u64()? as u32 + 1,
    ))
}

pub fn parse_workspace_edit(ws: &Path, edit: &Value) -> Result<Vec<FileTextEdits>> {
    if edit.is_null() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    if let Some(changes) = edit.get("changes").and_then(|c| c.as_object()) {
        for (uri, edits) in changes {
            let path = uri_to_workspace_path(ws, uri)?;
            let edits = parse_text_edits(edits)?;
            if !edits.is_empty() {
                out.push(FileTextEdits { path, edits });
            }
        }
    }
    if let Some(document_changes) = edit.get("documentChanges").and_then(|c| c.as_array()) {
        for change in document_changes {
            if let Some(edits) = change.get("edits") {
                let uri = change
                    .get("textDocument")
                    .and_then(|t| t.get("uri"))
                    .and_then(|u| u.as_str())
                    .or_else(|| change.get("uri").and_then(|u| u.as_str()));
                if let Some(uri) = uri {
                    let path = uri_to_workspace_path(ws, uri)?;
                    let parsed = parse_text_edits(edits)?;
                    if !parsed.is_empty() {
                        out.push(FileTextEdits {
                            path,
                            edits: parsed,
                        });
                    }
                }
            }
        }
    }
    Ok(out)
}

pub fn parse_text_edits(edits: &Value) -> Result<Vec<QuickFixEdit>> {
    let Some(items) = edits.as_array() else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for edit in items {
        let range = edit.get("range").context("edit range")?;
        let (start_line, start_column, end_line, end_column) =
            parse_lsp_range(range).context("edit range bounds")?;
        let text = edit
            .get("newText")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        out.push(QuickFixEdit {
            start_line,
            start_column,
            end_line,
            end_column,
            text,
        });
    }
    Ok(out)
}

pub fn parse_signature_help(result: &Value) -> Option<SignatureHelp> {
    if result.is_null() {
        return None;
    }
    let signatures = result
        .get("signatures")?
        .as_array()?
        .iter()
        .filter_map(|sig| {
            let label = sig.get("label")?.as_str()?.to_string();
            let documentation = documentation_text(sig.get("documentation"));
            let parameters = sig
                .get("parameters")
                .and_then(|p| p.as_array())
                .map(|params| {
                    params
                        .iter()
                        .filter_map(|param| {
                            let label = param
                                .get("label")
                                .and_then(|l| {
                                    l.as_str()
                                        .map(str::to_string)
                                        .or_else(|| {
                                            l.as_array().and_then(|a| {
                                                a.first()?.as_str().map(str::to_string)
                                            })
                                        })
                                })?
                                .to_string();
                            Some(SignatureParameter {
                                label,
                                documentation: documentation_text(param.get("documentation")),
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
            Some(SignatureInfo {
                label,
                documentation,
                parameters,
            })
        })
        .collect::<Vec<_>>();
    if signatures.is_empty() {
        return None;
    }
    Some(SignatureHelp {
        active_signature: result
            .get("activeSignature")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32,
        active_parameter: result
            .get("activeParameter")
            .and_then(|v| v.as_u64())
            .map(|v| v as u32),
        signatures,
    })
}

pub fn documentation_text(value: Option<&Value>) -> Option<String> {
    let value = value?;
    if let Some(text) = value.as_str() {
        return Some(text.to_string());
    }
    value.get("value").and_then(|v| v.as_str()).map(str::to_string)
}

pub fn signature_help_trigger(content: &str, line: u32, column: u32) -> Option<(u32, char)> {
    let line_text = content.lines().nth(line.saturating_sub(1) as usize)?;
    let col = column.saturating_sub(1) as usize;
    let ch = line_text[..col.min(line_text.len())].chars().last()?;
    match ch {
        '(' => Some((2, '(')),
        ',' => Some((2, ',')),
        _ => Some((1, '(')),
    }
}

/// Normalized completion item parsed from an LSP `textDocument/completion` response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedCompletionItem {
    pub label: String,
    pub kind: String,
    pub detail: Option<String>,
    pub insert: Option<String>,
    pub documentation: Option<String>,
}

pub fn completion_result_items(result: &Value) -> Vec<&Value> {
    if let Some(items) = result.as_array() {
        return items.iter().collect();
    }
    result
        .get("items")
        .and_then(|v| v.as_array())
        .map(|items| items.iter().collect())
        .unwrap_or_default()
}

pub fn parse_completion_items(result: &Value) -> Vec<ParsedCompletionItem> {
    completion_result_items(result)
        .into_iter()
        .filter_map(parse_completion_item)
        .collect()
}

fn parse_completion_item(item: &Value) -> Option<ParsedCompletionItem> {
    let label = completion_item_label(item)?;
    let kind = lsp_completion_kind(item.get("kind"));
    let detail = item
        .get("detail")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let mut insert = completion_insert_text(item);
    if kind == "method" {
        if let Some(text) = insert.as_deref() {
            if !text.contains('(') {
                insert = Some(format!("{text}()"));
            }
        } else if !label.contains('(') {
            insert = Some(format!("{label}()"));
        }
    }
    let documentation = item
        .get("documentation")
        .and_then(|v| documentation_text(Some(v)));
    Some(ParsedCompletionItem {
        label,
        kind,
        detail,
        insert,
        documentation,
    })
}

fn completion_item_label(item: &Value) -> Option<String> {
    let label = item.get("label")?;
    if let Some(text) = label.as_str() {
        return Some(text.to_string());
    }
    label
        .get("label")
        .and_then(|v| v.as_str())
        .map(str::to_string)
}

fn completion_insert_text(item: &Value) -> Option<String> {
    if let Some(text) = item.get("insertText").and_then(|v| v.as_str()) {
        return Some(text.to_string());
    }
    item.get("textEdit")
        .and_then(|edit| edit.get("newText"))
        .and_then(|v| v.as_str())
        .map(str::to_string)
}

fn lsp_completion_kind(kind: Option<&Value>) -> String {
    match kind.and_then(|v| v.as_u64()) {
        Some(2) => "method".into(),
        Some(3) => "function".into(),
        Some(4) => "constructor".into(),
        Some(5) => "field".into(),
        Some(6) => "variable".into(),
        Some(7) => "class".into(),
        Some(8) => "interface".into(),
        Some(9) => "enum".into(),
        Some(10) => "enum".into(),
        Some(14) => "keyword".into(),
        Some(15) => "snippet".into(),
        Some(22) => "enum".into(),
        Some(23) => "keyword".into(),
        _ => "member".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_signature_help_extracts_labels() {
        let result = json!({
            "signatures": [{
                "label": "add(int a, int b)",
                "documentation": "Adds two integers",
                "parameters": [
                    {"label": "int a", "documentation": "first"},
                    {"label": ["int", " b"]}
                ]
            }],
            "activeSignature": 0,
            "activeParameter": 1
        });
        let parsed = parse_signature_help(&result).expect("signature help payload");
        assert_eq!(parsed.signatures[0].label, "add(int a, int b)");
        assert_eq!(parsed.signatures[0].parameters.len(), 2);
        assert_eq!(parsed.active_parameter, Some(1));
    }

    #[test]
    fn signature_help_trigger_detects_open_paren() {
        let source = "add(1, 2);";
        assert_eq!(signature_help_trigger(source, 1, 5), Some((2, '(')));
        assert_eq!(signature_help_trigger(source, 1, 7), Some((2, ',')));
    }

    #[test]
    fn parse_completion_items_maps_methods_and_fields() {
        let result = json!({
            "items": [
                {
                    "label": "stream",
                    "kind": 2,
                    "detail": "Stream<T> stream()",
                    "documentation": { "kind": "markdown", "value": "Returns a stream." }
                },
                {
                    "label": "size",
                    "kind": 2,
                    "detail": "int size()"
                }
            ]
        });
        let items = parse_completion_items(&result);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].label, "stream");
        assert_eq!(items[0].insert.as_deref(), Some("stream()"));
        assert_eq!(items[0].kind, "method");
        assert!(items[0].documentation.as_deref().unwrap().contains("stream"));
    }
}
