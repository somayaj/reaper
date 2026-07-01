use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::Serialize;

use super::symbols;
use super::{search_classes, should_skip_search_path, should_skip_tree_name};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchScope {
    All,
    Class,
    File,
    Text,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct WorkspaceSearchHit {
    pub kind: String,
    pub label: String,
    pub detail: String,
    pub path: String,
    pub line: u32,
    pub column: u32,
    #[serde(skip)]
    pub score: u32,
}

pub fn parse_search_query(raw: &str) -> (SearchScope, &str) {
    let trimmed = raw.trim();
    if let Some(rest) = trimmed.strip_prefix('#') {
        return (SearchScope::Class, rest.trim());
    }
    if let Some(rest) = trimmed.strip_prefix('@') {
        return (SearchScope::File, rest.trim());
    }
    if let Some(rest) = trimmed.strip_prefix(':') {
        return (SearchScope::Text, rest.trim());
    }
    (SearchScope::All, trimmed)
}

pub fn search_workspace(ws: &Path, raw_query: &str, limit: usize) -> Result<Vec<WorkspaceSearchHit>> {
    let limit = limit.clamp(1, 100);
    let (scope, query) = parse_search_query(raw_query);
    if query.is_empty() {
        return Ok(Vec::new());
    }

    let mut hits = Vec::new();

    if matches!(scope, SearchScope::All | SearchScope::Class) {
        let class_limit = if scope == SearchScope::Class {
            limit
        } else {
            limit.min(15)
        };
        for hit in search_classes(ws, query, class_limit)? {
            let score = symbols::class_name_match_score(query, &hit.name, &hit.qualified).unwrap_or(0);
            let detail = if hit.qualified != hit.name {
                hit.qualified.clone()
            } else {
                hit.path.clone()
            };
            hits.push(WorkspaceSearchHit {
                kind: "class".into(),
                label: hit.name,
                detail,
                path: hit.path,
                line: hit.line,
                column: hit.column,
                score: score.saturating_add(200),
            });
        }
    }

    if matches!(scope, SearchScope::All | SearchScope::File) {
        let file_limit = if scope == SearchScope::File {
            limit
        } else {
            limit.min(20)
        };
        hits.extend(search_files(ws, query, file_limit));
    }

    if matches!(scope, SearchScope::All | SearchScope::Text) && query.len() >= 2 {
        let text_limit = if scope == SearchScope::Text {
            limit
        } else {
            limit.min(25)
        };
        hits.extend(search_text(ws, query, text_limit));
    }

    hits.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.path.cmp(&b.path)));
    hits.retain(|hit| !should_skip_search_path(&hit.path));
    hits.truncate(limit);
    Ok(hits)
}

fn search_files(ws: &Path, query: &str, limit: usize) -> Vec<WorkspaceSearchHit> {
    let mut paths = Vec::new();
    let mut budget = 12_000usize;
    collect_file_paths(ws, ws, &mut paths, &mut budget);
    let mut scored: Vec<(u32, PathBuf)> = paths
        .into_iter()
        .filter_map(|p| {
            let rel = p.strip_prefix(ws).unwrap_or(&p).to_string_lossy().replace('\\', "/");
            if should_skip_search_path(&rel) {
                return None;
            }
            path_match_score(query, &rel).map(|s| (s, p))
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    scored
        .into_iter()
        .take(limit)
        .map(|(score, p)| {
            let rel = p.strip_prefix(ws).unwrap_or(&p).to_string_lossy().replace('\\', "/");
            let label = p.file_name().and_then(|n| n.to_str()).unwrap_or(&rel).to_string();
            WorkspaceSearchHit {
                kind: "file".into(),
                label,
                detail: rel.clone(),
                path: rel,
                line: 0,
                column: 0,
                score,
            }
        })
        .collect()
}

fn collect_file_paths(ws: &Path, dir: &Path, out: &mut Vec<PathBuf>, budget: &mut usize) {
    if *budget == 0 {
        return;
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    let mut names: Vec<_> = entries.filter_map(|e| e.ok()).collect();
    names.sort_by_key(|e| e.file_name());
    for entry in names {
        if *budget == 0 {
            break;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = entry.path();
        let is_dir = path.is_dir();
        if should_skip_tree_name(&name, is_dir) {
            continue;
        }
        if is_dir {
            collect_file_paths(ws, &path, out, budget);
        } else {
            out.push(path);
            *budget = budget.saturating_sub(1);
        }
    }
}

pub fn path_match_score(query: &str, rel_path: &str) -> Option<u32> {
    let q = query.trim();
    if q.is_empty() {
        return Some(0);
    }
    let q_lower = q.to_lowercase();
    let path_lower = rel_path.to_lowercase();
    let base = rel_path.rsplit('/').next().unwrap_or(rel_path).to_lowercase();

    if base == q_lower {
        return Some(1000);
    }
    if path_lower == q_lower {
        return Some(980);
    }
    if base.starts_with(&q_lower) {
        return Some(900 - base.len() as u32);
    }
    if let Some(segment) = path_lower.split('/').find(|part| part.starts_with(&q_lower)) {
        return Some(750 - path_lower.find(segment).unwrap_or(0) as u32);
    }
    if let Some(idx) = base.find(&q_lower) {
        return Some(650 - idx as u32);
    }
    if let Some(idx) = path_lower.find(&q_lower) {
        return Some(500 - idx as u32);
    }
    fuzzy_subsequence_score(&path_lower, &q_lower).or_else(|| fuzzy_subsequence_score(&base, &q_lower))
}

fn fuzzy_subsequence_score(haystack: &str, needle: &str) -> Option<u32> {
    let mut bi = 0usize;
    let mut spread = 0u32;
    for ch in needle.chars() {
        let rest = haystack[bi..].to_lowercase();
        let fi = rest.find(ch)?;
        spread += fi as u32;
        bi += fi + ch.len_utf8();
    }
    Some(300u32.saturating_sub(spread))
}

fn search_text(ws: &Path, query: &str, limit: usize) -> Vec<WorkspaceSearchHit> {
    let q_lower = query.to_lowercase();
    let mut paths = Vec::new();
    let mut budget = 6_000usize;
    collect_text_file_paths(ws, ws, &mut paths, &mut budget);
    let mut hits = Vec::new();
    'files: for path in paths {
        if hits.len() >= limit {
            break;
        }
        let rel = path.strip_prefix(ws).unwrap_or(&path).to_string_lossy().replace('\\', "/");
        let bytes = match std::fs::read(&path) {
            Ok(b) if b.len() <= 512 * 1024 => b,
            _ => continue,
        };
        if bytes.contains(&0u8) {
            continue;
        }
        let content = String::from_utf8_lossy(&bytes);
        for (i, line) in content.lines().enumerate() {
            if !line.to_lowercase().contains(&q_lower) {
                continue;
            }
            let trimmed = line.trim();
            let detail = if trimmed.len() > 120 {
                format!("{}…", &trimmed[..117])
            } else {
                trimmed.to_string()
            };
            let line_no = (i + 1) as u32;
            let col = line.to_lowercase().find(&q_lower).unwrap_or(0) as u32 + 1;
            hits.push(WorkspaceSearchHit {
                kind: "text".into(),
                label: rel.rsplit('/').next().unwrap_or(&rel).to_string(),
                detail: format!("{rel}:{line_no} · {detail}"),
                path: rel.clone(),
                line: line_no,
                column: col,
                score: text_line_score(&rel, line_no, query),
            });
            if hits.len() >= limit {
                break 'files;
            }
        }
    }
    hits.sort_by(|a, b| b.score.cmp(&a.score));
    hits.truncate(limit);
    hits
}

fn text_line_score(path: &str, line: u32, query: &str) -> u32 {
    let mut score = 400u32;
    if path.contains("/src/main/") || path.contains("/src/test/") {
        score += 80;
    }
    if path.ends_with(".java") || path.ends_with(".kt") {
        score += 40;
    }
    if line <= 50 {
        score += 10;
    }
    score.saturating_add(path_match_score(query, path).unwrap_or(0) / 4)
}

fn collect_text_file_paths(ws: &Path, dir: &Path, out: &mut Vec<PathBuf>, budget: &mut usize) {
    if *budget == 0 {
        return;
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    let mut names: Vec<_> = entries.filter_map(|e| e.ok()).collect();
    names.sort_by_key(|e| e.file_name());
    for entry in names {
        if *budget == 0 {
            break;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = entry.path();
        let is_dir = path.is_dir();
        if should_skip_tree_name(&name, is_dir) {
            continue;
        }
        if is_dir {
            collect_text_file_paths(ws, &path, out, budget);
        } else if is_text_searchable(&name) {
            out.push(path);
            *budget = budget.saturating_sub(1);
        }
    }
}

fn is_text_searchable(name: &str) -> bool {
    let Some(ext) = name.rsplit('.').next() else {
        return false;
    };
    matches!(
        ext.to_ascii_lowercase().as_str(),
        "java" | "kt" | "kts" | "groovy" | "gradle" | "xml" | "properties" | "yaml" | "yml"
            | "json" | "md" | "rs" | "py" | "rb" | "js" | "ts" | "tsx" | "jsx" | "html" | "css"
            | "scss" | "toml" | "sh" | "sql" | "txt" | "cfg" | "ini" | "env" | "h" | "cpp" | "c"
            | "go" | "swift" | "vue" | "svelte"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn parse_scope_prefixes() {
        assert_eq!(parse_search_query("#Foo"), (SearchScope::Class, "Foo"));
        assert_eq!(parse_search_query("@bar"), (SearchScope::File, "bar"));
        assert_eq!(parse_search_query(":hello"), (SearchScope::Text, "hello"));
        assert_eq!(parse_search_query("plain"), (SearchScope::All, "plain"));
    }

    #[test]
    fn path_match_prefers_basename() {
        assert!(path_match_score("User", "src/main/java/com/example/User.java").unwrap() > 600);
        assert!(path_match_score("gradle", "build.gradle.kts").is_some());
        assert!(path_match_score("zzzmissing", "src/Foo.java").is_none());
    }

    #[test]
    fn search_text_finds_line() {
        let dir = std::env::temp_dir().join(format!("reaper-search-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("src")).unwrap();
        fs::write(dir.join("src/App.java"), "class App {\n  void uniqueNeedleHere() {}\n}\n").unwrap();
        let hits = search_workspace(&dir, ":uniqueNeedleHere", 10).unwrap();
        assert!(hits.iter().any(|h| h.kind == "text" && h.line == 2));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn skips_class_and_build_paths_in_search() {
        use super::super::should_skip_search_path;
        assert!(should_skip_search_path("build/classes/java/main/com/example/Foo.class"));
        assert!(should_skip_search_path("module/build/classes/java/main"));
        assert!(should_skip_search_path(".reaper/classpath-jar/spring.jar"));
        assert!(!should_skip_search_path("src/main/java/com/example/Foo.java"));
    }
}
