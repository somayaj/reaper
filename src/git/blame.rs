use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::Serialize;

use super::{run_git, GitOutput};

#[derive(Debug, Clone, Serialize)]
pub struct BlameLine {
    pub line: u32,
    pub commit: String,
    pub author: String,
    pub date: String,
    pub summary: String,
}

pub fn blame_file(ws: &Path, rel_path: &str) -> Result<Vec<BlameLine>> {
    let rel_path = rel_path.trim().trim_start_matches('/');
    if rel_path.is_empty() {
        bail!("path is required");
    }
    let out = run_git(
        Some(ws),
        &["blame", "--line-porcelain", "--", rel_path],
    )?;
    if !out.success() {
        bail!("{}", out.stderr.trim());
    }
    Ok(parse_blame_porcelain(&out))
}

fn parse_blame_porcelain(out: &GitOutput) -> Vec<BlameLine> {
    let mut lines = Vec::new();
    let mut line_no = 0u32;
    let mut commit = String::new();
    let mut author = String::new();
    let mut date = String::new();
    let mut summary = String::new();

    for raw in out.stdout.lines() {
        if let Some(rest) = raw.strip_prefix('\t') {
            line_no += 1;
            lines.push(BlameLine {
                line: line_no,
                commit: commit.clone(),
                author: author.clone(),
                date: date.clone(),
                summary: summary.clone(),
            });
            let _ = rest;
            continue;
        }
        if let Some(hash) = raw.strip_prefix("author ") {
            author = hash.to_string();
            continue;
        }
        if let Some(ts) = raw.strip_prefix("author-time ") {
            if let Ok(secs) = ts.trim().parse::<i64>() {
                date = format_author_time(secs);
            } else {
                date = ts.to_string();
            }
            continue;
        }
        if let Some(msg) = raw.strip_prefix("summary ") {
            summary = msg.to_string();
            continue;
        }
        if let Some((hash, _rest)) = raw.split_once(' ') {
            if hash.len() >= 7 && hash.chars().all(|c| c.is_ascii_hexdigit()) {
                commit = hash.to_string();
            }
        }
    }
    lines
}

fn format_author_time(secs: i64) -> String {
    use std::time::{Duration, SystemTime, UNIX_EPOCH};
    let Some(at) = UNIX_EPOCH.checked_add(Duration::from_secs(secs.max(0) as u64)) else {
        return secs.to_string();
    };
    let Ok(delta) = SystemTime::now().duration_since(at) else {
        return at
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs().to_string())
            .unwrap_or_default();
    };
    let days = delta.as_secs() / 86_400;
    if days < 1 {
        return "today".into();
    }
    if days < 30 {
        return format!("{days}d ago");
    }
    if days < 365 {
        return format!("{}mo ago", days / 30);
    }
    format!("{}y ago", days / 365)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_porcelain_blame() {
        let out = GitOutput {
            stdout: "\
abc1234 1 1 1
author Alice
author-time 1700000000
summary init
\tline one
def5678 2 2 2
author Bob
author-time 1700000100
summary tweak
\tline two
"
            .into(),
            stderr: String::new(),
            exit_code: 0,
        };
        let lines = parse_blame_porcelain(&out);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].author, "Alice");
        assert_eq!(lines[1].commit, "def5678");
    }
}
