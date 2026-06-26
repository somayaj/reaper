use std::path::Path;

use anyhow::Result;

use super::exec::run_shell_command;
use super::symbols::{SymbolLocation, find_in_content, word_at};

const RAILS_CONSTANT_DIRS: &[&str] = &[
    "app/models",
    "app/controllers",
    "app/helpers",
    "app/mailers",
    "app/jobs",
    "app/services",
    "app/policies",
    "app/serializers",
    "app/views",
    "lib",
];

pub fn is_ruby_path(path: &str) -> bool {
    path.to_lowercase().ends_with(".rb")
}

/// Ruby constant under cursor, including `Foo::Bar` namespaces.
pub fn constant_at(content: &str, line: u32, column: u32) -> Option<String> {
    let line_text = content.lines().nth(line.saturating_sub(1) as usize)?;
    let col = column.saturating_sub(1) as usize;
    let col = col.min(line_text.len());
    let bytes = line_text.as_bytes();

    let mut start = col;
    while start > 0 && is_const_byte(bytes[start - 1]) {
        start -= 1;
    }
    let mut end = col;
    while end < bytes.len() && is_const_byte(bytes[end]) {
        end += 1;
    }

    let raw = line_text[start..end].trim_matches(':');
    if raw.is_empty() {
        return None;
    }

    let name: String = raw
        .split("::")
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("::");

    if name.is_empty() || !name.chars().next().is_some_and(|c| c.is_uppercase()) {
        return None;
    }
    Some(name)
}

fn is_const_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b':'
}

pub fn find_definition(
    ws: &Path,
    line: u32,
    column: u32,
    content: &str,
) -> Result<Option<SymbolLocation>> {
    let constant = match constant_at(content, line, column) {
        Some(c) => c,
        None => return Ok(None),
    };

    if let Some(hit) = find_rails_constant(ws, &constant)? {
        return Ok(Some(hit));
    }

    find_via_bundler(ws, &constant)
}

/// Resolve symbol for Ruby go-to-definition: constant, else method/identifier.
pub fn symbol_at(content: &str, line: u32, column: u32) -> Option<String> {
    constant_at(content, line, column).or_else(|| word_at(content, line, column))
}

/// Rails-style constant → file lookup (`User`, `Admin::User`, `UsersController`).
pub fn find_rails_constant(ws: &Path, symbol: &str) -> Result<Option<SymbolLocation>> {
    if symbol.is_empty() || !symbol.chars().next().is_some_and(|c| c.is_uppercase()) {
        return Ok(None);
    }

    let simple = symbol.rsplit("::").next().unwrap_or(symbol);
    let snake = camel_to_snake(simple);
    if snake.is_empty() {
        return Ok(None);
    }

    for rel in rails_constant_paths(symbol) {
        let path = ws.join(&rel);
        if !path.is_file() {
            continue;
        }
        let file_content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        if let Some(hit) = find_ruby_constant_in_file(&file_content, symbol, simple, &rel) {
            return Ok(Some(hit));
        }
    }

    Ok(None)
}

fn rails_constant_paths(symbol: &str) -> Vec<String> {
    let parts: Vec<&str> = symbol.split("::").collect();
    if parts.is_empty() {
        return Vec::new();
    }

    let file_base = camel_to_snake(parts.last().unwrap());
    let mut paths = Vec::new();

    if parts.len() == 1 {
        for dir in RAILS_CONSTANT_DIRS {
            paths.push(format!("{dir}/{file_base}.rb"));
        }
        paths.push(format!("{file_base}.rb"));
    } else {
        let nested: String = parts[..parts.len() - 1]
            .iter()
            .map(|p| camel_to_snake(p))
            .collect::<Vec<_>>()
            .join("/");
        for dir in RAILS_CONSTANT_DIRS {
            paths.push(format!("{dir}/{nested}/{file_base}.rb"));
        }
        paths.push(format!("{nested}/{file_base}.rb"));
    }

    paths
}

fn find_ruby_constant_in_file(
    content: &str,
    qualified: &str,
    simple: &str,
    rel_path: &str,
) -> Option<SymbolLocation> {
    if qualified.contains("::") {
        if let Some(hit) = find_nested_constant(content, qualified, rel_path) {
            return Some(hit);
        }
    }
    find_in_content(simple, rel_path, content)
}

fn find_nested_constant(content: &str, qualified: &str, rel_path: &str) -> Option<SymbolLocation> {
    let parts: Vec<&str> = qualified.split("::").collect();
    let class_name = *parts.last()?;
    let modules = &parts[..parts.len() - 1];

    let mut depth = 0usize;
    for (idx, line) in content.lines().enumerate() {
        let trimmed = line.split('#').next()?.trim();
        if let Some(name) = module_name_on_line(trimmed) {
            if depth < modules.len() && name == modules[depth] {
                depth += 1;
            }
        }
        if depth == modules.len() {
            if let Some(col) = find_class_or_module_decl(line, "class", class_name) {
                return Some(SymbolLocation {
                    name: qualified.to_string(),
                    kind: "class".into(),
                    path: rel_path.to_string(),
                    line: idx as u32 + 1,
                    column: col,
                });
            }
            if let Some(col) = find_class_or_module_decl(line, "module", class_name) {
                return Some(SymbolLocation {
                    name: qualified.to_string(),
                    kind: "module".into(),
                    path: rel_path.to_string(),
                    line: idx as u32 + 1,
                    column: col,
                });
            }
        }
        if trimmed == "end" && depth > 0 {
            depth -= 1;
        }
    }
    None
}

fn module_name_on_line(line: &str) -> Option<String> {
    let rest = line.strip_prefix("module ")?.trim();
    let name: String = rest
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    if name.is_empty() {
        return None;
    }
    Some(name)
}

fn find_class_or_module_decl(line: &str, keyword: &str, name: &str) -> Option<u32> {
    let trimmed = line.split('#').next()?.trim();
    let pattern = format!("{keyword} {name}");
    if !trimmed.starts_with(&pattern) {
        return None;
    }
    let after = trimmed[pattern.len()..].trim();
    if !after.is_empty() && !after.starts_with('<') && !after.starts_with(';') {
        return None;
    }
    let pos = line.find(name)? as u32 + 1;
    Some(pos)
}

/// Gem / stdlib constants via Bundler (`ActionController::Base`, etc.).
fn find_via_bundler(ws: &Path, constant: &str) -> Result<Option<SymbolLocation>> {
    if !ws.join("Gemfile").is_file() {
        return Ok(None);
    }

    let escaped = constant.replace('\\', "\\\\").replace('"', "\\\"");
    let script = format!(
        r#"bundle exec ruby -e 'require "bundler/setup"; c="{escaped}"; begin; parts=c.split("::"); o=Object; parts.each {{|p| o=o.const_get(p)}}; loc=Object.const_source_location(c); if loc; puts loc[0]+"|"+loc[1].to_s; end; rescue StandardError; end'"#
    );

    let out = match run_shell_command(ws, &script) {
        Ok(o) if o.exit_code == 0 => o.stdout,
        _ => return Ok(None),
    };

    let Some(line) = out.lines().find(|l| l.contains('|')) else {
        return Ok(None);
    };
    let Some((abs_path, line_no)) = line.split_once('|') else {
        return Ok(None);
    };
    let line_no: u32 = line_no.trim().parse().unwrap_or(1);

    let abs = Path::new(abs_path.trim());
    if !abs.is_file() {
        return Ok(None);
    }

    let ws_canon = ws.canonicalize().unwrap_or_else(|_| ws.to_path_buf());
    let path = if let Ok(rel) = abs.canonicalize().unwrap_or_else(|_| abs.to_path_buf()).strip_prefix(&ws_canon) {
        rel.to_string_lossy().replace('\\', "/")
    } else {
        abs.to_string_lossy().replace('\\', "/")
    };

    let simple = constant.rsplit("::").next().unwrap_or(constant);
    Ok(Some(SymbolLocation {
        name: constant.to_string(),
        kind: "class".into(),
        path,
        line: line_no,
        column: find_constant_column(abs, line_no, simple).unwrap_or(1),
    }))
}

fn find_constant_column(path: &Path, line: u32, name: &str) -> Option<u32> {
    let content = std::fs::read_to_string(path).ok()?;
    let line_text = content.lines().nth(line.saturating_sub(1) as usize)?;
    line_text.find(name).map(|i| i as u32 + 1)
}

pub fn camel_to_snake(name: &str) -> String {
    let chars: Vec<char> = name.chars().collect();
    let mut out = String::new();
    for (i, &c) in chars.iter().enumerate() {
        if c.is_uppercase() {
            let prev = chars.get(i.wrapping_sub(1)).copied();
            let next = chars.get(i + 1).copied();
            let boundary = i > 0
                && (prev.is_some_and(|p| p.is_lowercase())
                    || next.is_some_and(|n| n.is_lowercase() || (!n.is_ascii_alphabetic())));
            if boundary {
                out.push('_');
            }
            out.extend(c.to_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

pub fn ruby_method_on_line(line: &str) -> Option<String> {
    let trimmed = line.split('#').next()?.trim();
    if !trimmed.starts_with("def ") {
        return None;
    }
    let rest = trimmed.strip_prefix("def ")?.trim();
    if rest.starts_with("self.") {
        return None;
    }
    let name: String = rest
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    if name.is_empty() || name == "self" {
        return None;
    }
    Some(name)
}

pub fn ruby_class_method_on_line(line: &str) -> Option<String> {
    let trimmed = line.split('#').next()?.trim();
    let rest = trimmed.strip_prefix("def self.")?;
    let name: String = rest
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    if name.is_empty() {
        return None;
    }
    Some(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camel_to_snake_cases() {
        assert_eq!(camel_to_snake("User"), "user");
        assert_eq!(camel_to_snake("UsersController"), "users_controller");
        assert_eq!(camel_to_snake("APIError"), "api_error");
    }

    #[test]
    fn constant_at_qualified() {
        let src = "class UsersController < ActionController::Base\n";
        assert_eq!(
            constant_at(src, 1, 35),
            Some("ActionController::Base".into())
        );
        assert_eq!(constant_at(src, 1, 22), Some("UsersController".into()));
    }

    #[test]
    fn rails_nested_constant_path() {
        let paths = rails_constant_paths("Admin::User");
        assert!(paths.contains(&"app/models/admin/user.rb".to_string()));
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
        let hit = find_rails_constant(&ws, "User")
            .expect("lookup ok")
            .expect("should find User");
        assert_eq!(hit.path, "app/models/user.rb");
        assert_eq!(hit.name, "User");
        let _ = std::fs::remove_dir_all(&ws);
    }
}
