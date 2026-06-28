use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::classpath::CompletionItem;
use super::gradle::find_gradle_root;

const INDEX_PATH: &str = ".reaper/spring-properties.json";

const METADATA_ENTRIES: &[&str] = &[
    "META-INF/spring-configuration-metadata.json",
    "META-INF/additional-spring-configuration-metadata.json",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SpringPropertiesIndex {
    properties: Vec<SpringPropertyEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SpringPropertyEntry {
    name: String,
    prop_type: Option<String>,
    description: Option<String>,
    default_value: Option<String>,
    hint_values: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ConfigMetadata {
    properties: Option<Vec<PropertyMeta>>,
    hints: Option<Vec<HintMeta>>,
}

#[derive(Debug, Deserialize)]
struct PropertyMeta {
    name: String,
    #[serde(rename = "type")]
    prop_type: Option<String>,
    description: Option<String>,
    #[serde(rename = "defaultValue")]
    default_value: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct HintMeta {
    name: String,
    values: Option<Vec<HintValue>>,
}

#[derive(Debug, Deserialize)]
struct HintValue {
    value: String,
}

pub fn is_spring_config_file(path: &str) -> bool {
    let lower = path.to_lowercase();
    if lower.ends_with(".properties") {
        return !lower.ends_with("gradle.properties") && !lower.ends_with(".gradle.properties");
    }
    lower.ends_with(".yml") || lower.ends_with(".yaml")
}

pub fn has_cached_properties(gradle_root: &Path) -> bool {
    let cache = gradle_root.join(INDEX_PATH);
    if !cache.is_file() {
        return false;
    }
    let Ok(text) = std::fs::read_to_string(&cache) else {
        return false;
    };
    serde_json::from_str::<SpringPropertiesIndex>(&text)
        .map(|index| !index.properties.is_empty())
        .unwrap_or(false)
}

pub fn build_index(ws: &Path, gradle_root: &Path, jars: &[PathBuf]) -> Result<()> {
    let mut by_name: HashMap<String, SpringPropertyEntry> = HashMap::new();

    for jar in jars {
        for entry in METADATA_ENTRIES {
            if let Some(text) = read_jar_entry(jar, entry)? {
                merge_metadata(&mut by_name, &text);
            }
        }
    }

    for rel in [
        "src/main/resources/META-INF/spring-configuration-metadata.json",
        "src/main/resources/META-INF/additional-spring-configuration-metadata.json",
    ] {
        let path = gradle_root.join(rel);
        if path.is_file() {
            if let Ok(text) = std::fs::read_to_string(&path) {
                merge_metadata(&mut by_name, &text);
            }
        }
    }

    let mut properties: Vec<_> = by_name.into_values().collect();
    properties.sort_by(|a, b| a.name.cmp(&b.name));

    let index = SpringPropertiesIndex { properties };
    std::fs::create_dir_all(gradle_root.join(".reaper"))?;
    std::fs::write(
        gradle_root.join(INDEX_PATH),
        serde_json::to_string_pretty(&index)?,
    )?;
    Ok(())
}

pub fn completions(
    ws: &Path,
    from_path: &str,
    line: u32,
    column: u32,
    content: &str,
    prefix: &str,
) -> Result<Vec<CompletionItem>> {
    if !is_spring_config_file(from_path) {
        return Ok(Vec::new());
    }

    let Some(root) = find_gradle_root(ws, from_path)? else {
        return Ok(Vec::new());
    };

    let index = ensure_index(ws, &root)?;
    let lower = from_path.to_lowercase();

    if lower.ends_with(".properties") {
        return properties_completions(&index, content, line, column, prefix);
    }

    yaml_completions(&index, content, line, column, prefix)
}

fn ensure_index(ws: &Path, gradle_root: &Path) -> Result<SpringPropertiesIndex> {
    let cache = gradle_root.join(INDEX_PATH);
    if cache.is_file() {
        if let Ok(text) = std::fs::read_to_string(&cache) {
            if let Ok(index) = serde_json::from_str::<SpringPropertiesIndex>(&text) {
                if !index.properties.is_empty() {
                    return Ok(index);
                }
            }
        }
    }

    Ok(SpringPropertiesIndex {
        properties: Vec::new(),
    })
}

fn properties_completions(
    index: &SpringPropertiesIndex,
    content: &str,
    line: u32,
    column: u32,
    prefix: &str,
) -> Result<Vec<CompletionItem>> {
    if let Some((key, partial_value)) = property_value_context(content, line, column) {
        return Ok(value_completions(index, &key, &partial_value));
    }

    let key_prefix = if prefix.is_empty() {
        property_key_prefix(content, line, column).unwrap_or_default()
    } else {
        prefix.to_string()
    };

    if key_prefix.is_empty() {
        return Ok(Vec::new());
    }

    Ok(key_completions(index, &key_prefix))
}

fn yaml_completions(
    index: &SpringPropertiesIndex,
    content: &str,
    line: u32,
    column: u32,
    prefix: &str,
) -> Result<Vec<CompletionItem>> {
    let line_text = content
        .lines()
        .nth(line.saturating_sub(1) as usize)
        .unwrap_or("");

    if line_text.contains(':') {
        let col = column.saturating_sub(1) as usize;
        let before = &line_text[..col.min(line_text.len())];
        if let Some((_, after_colon)) = before.split_once(':') {
            if !after_colon.trim().is_empty() || before.contains(':') {
                let key = yaml_property_path(content, line, column);
                let partial = after_colon.trim().to_string();
                if !key.is_empty() {
                    return Ok(value_completions(index, &key, &partial));
                }
            }
        }
    }

    let key_prefix = if prefix.is_empty() {
        yaml_typing_prefix(content, line, column)
    } else {
        prefix.to_string()
    };

    if key_prefix.is_empty() {
        return Ok(Vec::new());
    }

    Ok(key_completions(index, &key_prefix))
}

fn key_completions(index: &SpringPropertiesIndex, prefix: &str) -> Vec<CompletionItem> {
    let prefix_lower = prefix.to_lowercase();
    let mut items = Vec::new();

    for prop in &index.properties {
        if !prop.name.to_lowercase().starts_with(&prefix_lower) {
            continue;
        }
        let detail = format_detail(prop);
        items.push(CompletionItem {
            label: prop.name.clone(),
            kind: "property".into(),
            detail: Some(detail),
            insert: None,
            path: None,
            line: None,
            column: None,
        });
        if items.len() >= 80 {
            break;
        }
    }

    items
}

fn value_completions(
    index: &SpringPropertiesIndex,
    key: &str,
    partial: &str,
) -> Vec<CompletionItem> {
    let Some(prop) = index.properties.iter().find(|p| p.name == key) else {
        return Vec::new();
    };

    let partial_lower = partial.to_lowercase();
    let mut items = Vec::new();

    for value in &prop.hint_values {
        if !partial.is_empty() && !value.to_lowercase().starts_with(&partial_lower) {
            continue;
        }
        items.push(CompletionItem {
            label: value.clone(),
            kind: "value".into(),
            detail: Some(key.to_string()),
            insert: None,
            path: None,
            line: None,
            column: None,
        });
    }

    if items.is_empty() {
        if let Some(default) = &prop.default_value {
            if partial.is_empty() || default.to_lowercase().starts_with(&partial_lower) {
                items.push(CompletionItem {
                    label: default.clone(),
                    kind: "value".into(),
                    detail: Some(format!("default for {key}")),
                    insert: None,
                    path: None,
                    line: None,
                    column: None,
                });
            }
        }
    }

    items
}

fn format_detail(prop: &SpringPropertyEntry) -> String {
    let mut parts = Vec::new();
    if let Some(t) = &prop.prop_type {
        parts.push(t.clone());
    }
    if let Some(d) = &prop.description {
        let short: String = d.chars().take(120).collect();
        parts.push(short);
    }
    if let Some(d) = &prop.default_value {
        parts.push(format!("default: {d}"));
    }
    parts.join(" · ")
}

fn property_key_prefix(content: &str, line: u32, column: u32) -> Option<String> {
    let line_text = content.lines().nth(line.saturating_sub(1) as usize)?;
    let col = column.saturating_sub(1) as usize;
    let upto = &line_text[..col.min(line_text.len())];
    let upto = upto.split('#').next().unwrap_or(upto);
    if upto.contains('=') {
        let key = upto.split('=').next()?.trim_end();
        return Some(key.to_string());
    }
    Some(upto.trim().to_string())
}

fn property_value_context(content: &str, line: u32, column: u32) -> Option<(String, String)> {
    let line_text = content.lines().nth(line.saturating_sub(1) as usize)?;
    let col = column.saturating_sub(1) as usize;
    let upto = &line_text[..col.min(line_text.len())];
    let upto = upto.split('#').next().unwrap_or(upto);
    let (key, rest) = upto.split_once('=')?;
    let key = key.trim();
    if key.is_empty() {
        return None;
    }
    Some((key.to_string(), rest.trim().to_string()))
}

fn yaml_property_path(content: &str, line: u32, column: u32) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let idx = line.saturating_sub(1) as usize;
    let Some(current) = lines.get(idx) else {
        return String::new();
    };

    let current_indent = line_indent(current);
    let mut segments = Vec::new();

    if let Some(seg) = yaml_key_before_colon(current, column) {
        if !seg.is_empty() {
            segments.push(seg);
        }
    }

    let mut need_indent = current_indent;
    for i in (0..idx).rev() {
        let line = lines[i];
        if line.trim().is_empty() || line.trim().starts_with('#') {
            continue;
        }
        let indent = line_indent(line);
        if indent < need_indent {
            if let Some(key) = yaml_key_only(line) {
                segments.insert(0, key);
                need_indent = indent;
            }
        }
        if need_indent == 0 {
            break;
        }
    }

    segments.join(".")
}

fn yaml_typing_prefix(content: &str, line: u32, column: u32) -> String {
    let path = yaml_property_path(content, line, column);
    if !path.is_empty() {
        return path;
    }
    let line_text = content
        .lines()
        .nth(line.saturating_sub(1) as usize)
        .unwrap_or("");
    let col = column.saturating_sub(1) as usize;
    let upto = &line_text[..col.min(line_text.len())];
    upto.split(':').next().unwrap_or(upto).trim().to_string()
}

fn yaml_key_before_colon(line: &str, column: u32) -> Option<String> {
    let col = column.saturating_sub(1) as usize;
    let upto = &line[..col.min(line.len())];
    let key_part = upto.split(':').next()?.trim();
    if key_part.is_empty() {
        return None;
    }
    Some(key_part.to_string())
}

fn yaml_key_only(line: &str) -> Option<String> {
    let trimmed = line.split('#').next()?.trim();
    let key = trimmed.split(':').next()?.trim();
    if key.is_empty() {
        return None;
    }
    Some(key.to_string())
}

fn line_indent(line: &str) -> usize {
    line.chars().take_while(|c| *c == ' ' || *c == '\t').count()
}

fn merge_metadata(out: &mut HashMap<String, SpringPropertyEntry>, json: &str) {
    let Ok(meta) = serde_json::from_str::<ConfigMetadata>(json) else {
        return;
    };

    let mut hints: HashMap<String, Vec<String>> = HashMap::new();
    if let Some(list) = meta.hints {
        for hint in list {
            if let Some(values) = hint.values {
                hints.insert(
                    hint.name,
                    values.into_iter().map(|v| v.value).collect(),
                );
            }
        }
    }

    if let Some(props) = meta.properties {
        for prop in props {
            let default_value = prop.default_value.map(value_to_string);
            let entry = out.entry(prop.name.clone()).or_insert_with(|| SpringPropertyEntry {
                name: prop.name.clone(),
                prop_type: None,
                description: None,
                default_value: None,
                hint_values: Vec::new(),
            });
            if prop.prop_type.is_some() {
                entry.prop_type = prop.prop_type;
            }
            if prop.description.is_some() {
                entry.description = prop.description;
            }
            if default_value.is_some() {
                entry.default_value = default_value;
            }
            if let Some(values) = hints.remove(&prop.name) {
                entry.hint_values = values;
            }
        }
    }

    for (name, values) in hints {
        out.entry(name.clone())
            .or_insert_with(|| SpringPropertyEntry {
                name,
                prop_type: None,
                description: None,
                default_value: None,
                hint_values: values,
            });
    }
}

fn value_to_string(value: serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s,
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        other => other.to_string(),
    }
}

fn read_jar_entry(jar: &Path, entry: &str) -> Result<Option<String>> {
    let output = Command::new("unzip")
        .args([
            "-p",
            jar.to_str()
                .with_context(|| format!("jar path {}", jar.display()))?,
            entry,
        ])
        .output()
        .with_context(|| format!("read {entry} from {}", jar.display()))?;

    if !output.status.success() {
        return Ok(None);
    }

    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if text.is_empty() {
        Ok(None)
    } else {
        Ok(Some(text))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn property_key_prefix_before_equals() {
        let content = "spring.datasource.url=jdbc:postgresql://localhost\nserver.port=8080";
        assert_eq!(
            property_key_prefix(content, 2, 12),
            Some("server.port".into())
        );
        assert_eq!(
            property_key_prefix(content, 1, 20),
            Some("spring.datasource.u".into())
        );
    }

    #[test]
    fn yaml_builds_dotted_path() {
        let content = "spring:\n  datasource:\n    url: jdbc:postgresql://localhost\n";
        assert_eq!(yaml_property_path(content, 3, 9), "spring.datasource.url");
    }

    #[test]
    fn spring_config_file_detection() {
        assert!(is_spring_config_file("src/main/resources/application.properties"));
        assert!(is_spring_config_file("application.yml"));
        assert!(!is_spring_config_file("gradle.properties"));
    }
}
