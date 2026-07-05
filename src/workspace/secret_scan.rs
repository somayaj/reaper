//! Detect likely secrets in paths and diffs before commit/push; redact for AI prompts.

use serde::Serialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::git;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SecretFinding {
    pub path: String,
    pub reason: String,
    pub line: Option<u32>,
}

/// Scan files included in a push preview.
pub fn scan_push_files(
    ws: &Path,
    files: &[String],
    diff_range: Option<&str>,
    branch: &str,
) -> Vec<SecretFinding> {
    let effective_range = diff_range.map(str::to_string).or_else(|| {
        let origin_branch = format!("origin/{branch}");
        let verify = git::run_git(Some(ws), &["rev-parse", "--verify", &origin_branch]).ok()?;
        if verify.success() {
            Some(format!("{origin_branch}..HEAD"))
        } else {
            None
        }
    });

    let mut findings = Vec::new();
    let mut seen = HashSet::new();
    for path in files {
        let diff = effective_range.as_ref().and_then(|range| {
            git::run_git(Some(ws), &["diff", "--no-color", "-U0", range, "--", path])
                .ok()
                .filter(|o| o.success())
                .map(|o| o.stdout)
        });
        for finding in scan_file(ws, path, diff.as_deref()) {
            let key = (finding.path.clone(), finding.reason.clone(), finding.line);
            if seen.insert(key) {
                findings.push(finding);
            }
        }
    }
    findings.sort_by(|a, b| a.path.cmp(&b.path).then(a.line.cmp(&b.line)));
    findings
}

/// Scan paths staged or selected for commit.
pub fn scan_commit_paths(ws: &Path, paths: &[String]) -> Vec<SecretFinding> {
    let mut findings = Vec::new();
    let mut seen = HashSet::new();
    for path in paths {
        let mut diff = String::new();
        if let Ok(out) = git::run_git(Some(ws), &["diff", "HEAD", "--no-color", "-U0", "--", path]) {
            diff.push_str(&out.stdout);
        }
        if let Ok(out) = git::run_git(Some(ws), &["diff", "--cached", "--no-color", "-U0", "--", path]) {
            diff.push_str(&out.stdout);
        }
        for finding in scan_file(ws, path, if diff.is_empty() { None } else { Some(&diff) }) {
            let key = (finding.path.clone(), finding.reason.clone(), finding.line);
            if seen.insert(key) {
                findings.push(finding);
            }
        }
    }
    findings.sort_by(|a, b| a.path.cmp(&b.path).then(a.line.cmp(&b.line)));
    findings
}

fn scan_file(ws: &Path, path: &str, diff: Option<&str>) -> Vec<SecretFinding> {
    let mut findings = Vec::new();
    if let Some(reason) = sensitive_path_reason(path) {
        findings.push(SecretFinding {
            path: path.to_string(),
            reason: reason.to_string(),
            line: None,
        });
    }
    if let Some(diff) = diff {
        findings.extend(scan_diff(diff, path));
    } else {
        let disk = ws.join(path);
        if disk.is_file() {
            if let Ok(content) = std::fs::read_to_string(&disk) {
                findings.extend(scan_text(&content, path));
            }
        }
    }
    findings
}

fn scan_diff(diff: &str, path: &str) -> Vec<SecretFinding> {
    let mut findings = Vec::new();
    let mut line_no = 0u32;
    for line in diff.lines() {
        if let Some(rest) = line.strip_prefix("@@") {
            if let Some(num) = rest.split('+').nth(1).and_then(|s| s.split(',').next()) {
                line_no = num.parse().unwrap_or(line_no).saturating_sub(1);
            }
            continue;
        }
        if line.starts_with('+') && !line.starts_with("+++") {
            line_no += 1;
            if let Some(reason) = sensitive_line_reason(line.trim_start_matches('+')) {
                findings.push(SecretFinding {
                    path: path.to_string(),
                    reason: reason.to_string(),
                    line: Some(line_no),
                });
            }
        } else if line.starts_with(' ') {
            line_no += 1;
        }
    }
    findings
}

fn scan_text(content: &str, path: &str) -> Vec<SecretFinding> {
    content
        .lines()
        .enumerate()
        .filter_map(|(i, line)| {
            sensitive_line_reason(line).map(|reason| SecretFinding {
                path: path.to_string(),
                reason: reason.to_string(),
                line: Some(i as u32 + 1),
            })
        })
        .collect()
}

fn sensitive_path_reason(path: &str) -> Option<&'static str> {
    let path_buf = PathBuf::from(path);
    let base = path_buf
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(path);
    let lower = base.to_lowercase();
    let path_lower = path.replace('\\', "/").to_lowercase();

    if lower == ".env"
        || lower.starts_with(".env.")
        || lower.ends_with(".env")
        || lower == "credentials.json"
        || lower == "secrets.json"
        || lower == "secrets.yaml"
        || lower == "secrets.yml"
    {
        return Some("environment or credentials file");
    }
    if lower.ends_with(".pem")
        || lower.ends_with(".p12")
        || lower.ends_with(".pfx")
        || lower.ends_with(".key")
        || lower == "id_rsa"
        || lower == "id_ed25519"
        || lower == "id_ecdsa"
    {
        return Some("private key or certificate file");
    }
    if path_lower.contains("/.aws/credentials") || path_lower.ends_with("/credentials") {
        return Some("cloud credentials file");
    }
    if lower.contains("application-local")
        || lower.contains("application-secret")
        || lower.contains("application-prod")
    {
        return Some("local or secret Spring config");
    }
    None
}

fn sensitive_line_reason(line: &str) -> Option<&'static str> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("//") {
        return None;
    }
    if trimmed.contains("-----BEGIN") && trimmed.contains("PRIVATE KEY") {
        return Some("private key block");
    }
    if trimmed.contains("AKIA") && trimmed.len() >= 20 {
        return Some("possible AWS access key");
    }
    let lower = trimmed.to_lowercase();
    for key in [
        "password",
        "passwd",
        "secret",
        "api_key",
        "api-key",
        "apikey",
        "access_key",
        "access-key",
        "auth_token",
        "auth-token",
        "client_secret",
        "client-secret",
        "private_key",
        "private-key",
        "token",
    ] {
        if line_has_assignment(&lower, key) {
            let value = assignment_value(trimmed, key);
            if is_placeholder_value(value) {
                continue;
            }
            return Some("possible secret assignment");
        }
    }
    if lower.contains("bearer ") && lower.len() > 12 {
        return Some("bearer token");
    }
    if lower.contains("jdbc:") && lower.contains("password=") {
        return Some("database password in JDBC URL");
    }
    None
}

fn line_has_assignment(lower: &str, key: &str) -> bool {
    lower.contains(&format!("{key}="))
        || lower.contains(&format!("{key}:"))
        || lower.contains(&format!("{key} ="))
        || lower.contains(&format!("{key} :"))
}

fn assignment_value<'a>(line: &'a str, key: &str) -> &'a str {
    let lower = line.to_lowercase();
    let key_lower = key.to_lowercase();
    for sep in ['=', ':'] {
        if let Some(idx) = lower.find(&format!("{key_lower}{sep}")) {
            let rest = line[idx + key_lower.len() + 1..].trim();
            return rest.trim_matches('"').trim_matches('\'');
        }
        if let Some(idx) = lower.find(&format!("{key_lower} {sep}")) {
            let rest = line[idx + key_lower.len() + 2..].trim();
            return rest.trim_matches('"').trim_matches('\'');
        }
    }
    ""
}

fn is_placeholder_value(value: &str) -> bool {
    let v = value.trim().trim_matches('"').trim_matches('\'');
    if v.is_empty() {
        return true;
    }
    let lower = v.to_lowercase();
    lower.starts_with("${")
        || lower.starts_with("{{")
        || lower.starts_with("<")
        || lower.contains("changeme")
        || lower.contains("your-")
        || lower.contains("xxx")
        || lower == "todo"
        || lower == "password"
        || lower == "secret"
        || lower == "none"
        || lower == "null"
}

/// Redact likely secret values before sending text to external AI APIs.
pub fn redact_text(text: &str) -> String {
    text.lines()
        .map(|line| redact_line(line))
        .collect::<Vec<_>>()
        .join("\n")
}

fn redact_line(line: &str) -> String {
    if sensitive_line_reason(line).is_some() {
        if let Some((key, _)) = split_assignment(line) {
            return format!("{key}=[REDACTED]");
        }
        if line.contains("AKIA") {
            return line.replace(
                line.split_whitespace()
                    .find(|w| w.contains("AKIA"))
                    .unwrap_or(""),
                "[REDACTED]",
            );
        }
        if line.contains("-----BEGIN") {
            return "[REDACTED PRIVATE KEY BLOCK]".to_string();
        }
    }
    line.to_string()
}

fn split_assignment(line: &str) -> Option<(String, &str)> {
    let lower = line.to_lowercase();
    for key in [
        "password", "passwd", "secret", "api_key", "api-key", "apikey", "access_key",
        "access-key", "auth_token", "auth-token", "client_secret", "client-secret",
        "private_key", "private-key", "token",
    ] {
        if line_has_assignment(&lower, key) {
            let key_len = key.len();
            if let Some(idx) = lower.find(&format!("{key}=")) {
                return Some((line[..idx + key_len].to_string(), ""));
            }
            if let Some(idx) = lower.find(&format!("{key}:")) {
                return Some((line[..idx + key_len].to_string(), ""));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_env_file_path() {
        assert_eq!(
            sensitive_path_reason(".env"),
            Some("environment or credentials file")
        );
        assert_eq!(
            sensitive_path_reason("config/.env.local"),
            Some("environment or credentials file")
        );
    }

    #[test]
    fn flags_password_in_properties() {
        assert_eq!(
            sensitive_line_reason("spring.datasource.password=SuperSecret123"),
            Some("possible secret assignment")
        );
    }

    #[test]
    fn ignores_placeholders() {
        assert!(sensitive_line_reason("api.key=${API_KEY}").is_none());
        assert!(sensitive_line_reason("password=changeme").is_none());
    }

    #[test]
    fn redacts_assignment() {
        let out = redact_line("export API_KEY=sk-live-abc123");
        assert!(out.contains("[REDACTED]"));
        assert!(!out.contains("sk-live"));
    }

    #[test]
    fn scan_diff_added_lines_only() {
        let diff = "\
diff --git a/app.properties b/app.properties
--- a/app.properties
+++ b/app.properties
@@ -1 +1,2 @@
+db.password=hunter2
";
        let findings = scan_diff(diff, "app.properties");
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].reason, "possible secret assignment");
    }
}
