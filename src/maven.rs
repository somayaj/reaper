use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct MavenInstall {
    pub path: String,
    pub version: String,
    pub label: String,
}

pub fn maven_binary_in_home(home: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        home.join("bin").join("mvn.cmd")
    }
    #[cfg(not(windows))]
    {
        home.join("bin").join("mvn")
    }
}

pub fn normalize_maven_binary(path: PathBuf) -> Result<PathBuf> {
    if path.is_file() {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name == "mvn" || name == "mvn.cmd" {
            return Ok(path);
        }
        bail!("not a maven executable: {}", path.display());
    }
    if path.is_dir() {
        let bin = maven_binary_in_home(&path);
        if bin.is_file() {
            return Ok(bin);
        }
        bail!("no bin/mvn in {}", path.display());
    }
    bail!("maven path not found: {}", path.display());
}

pub fn validate_maven_path(path: &str) -> Result<PathBuf> {
    normalize_maven_binary(PathBuf::from(path.trim()))
}

pub fn maven_version_string(binary: &Path) -> Result<String> {
    let out = crate::platform::command(binary)
        .arg("--version")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .with_context(|| format!("run {}", binary.display()))?;
    let text = if out.stdout.is_empty() {
        String::from_utf8_lossy(&out.stderr).into_owned()
    } else {
        String::from_utf8_lossy(&out.stdout).into_owned()
    };
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("Apache Maven ") {
            let ver = trimmed
                .strip_prefix("Apache Maven ")
                .unwrap_or(trimmed)
                .split_whitespace()
                .next()
                .unwrap_or(trimmed);
            return Ok(ver.to_string());
        }
    }
    text.lines()
        .next()
        .map(|l| l.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!("could not parse maven version"))
}

pub fn list_installed_mavens() -> Vec<MavenInstall> {
    let mut installs = Vec::new();
    let mut seen = HashSet::new();

    for bin in scan_maven_binaries() {
        let key = bin.canonicalize().unwrap_or_else(|_| bin.clone());
        if !seen.insert(key) {
            continue;
        }
        let version = maven_version_string(&bin).unwrap_or_else(|_| "?".into());
        installs.push(MavenInstall {
            path: bin.display().to_string(),
            version: version.clone(),
            label: format!("Maven {version} — {}", short_maven_label(&bin)),
        });
    }

    installs.sort_by(|a, b| b.version.cmp(&a.version));
    installs
}

fn short_maven_label(bin: &Path) -> String {
    let s = bin.to_string_lossy();
    if s.contains("/opt/homebrew/") || s.contains("/usr/local/") {
        if let Some(name) = bin
            .parent()
            .and_then(|p| p.parent())
            .and_then(|p| p.file_name())
        {
            return name.to_string_lossy().into_owned();
        }
        return "Homebrew".into();
    }
    "Maven".into()
}

fn scan_maven_binaries() -> Vec<PathBuf> {
    let mut out = Vec::new();
    scan_homebrew(&mut out);
    if let Ok(path) = which_maven_on_path() {
        out.push(path);
    }
    out
}

fn scan_homebrew(out: &mut Vec<PathBuf>) {
    for prefix in ["/opt/homebrew/opt", "/usr/local/opt"] {
        let opt = PathBuf::from(prefix);
        let Ok(entries) = std::fs::read_dir(&opt) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !name.starts_with("maven") {
                continue;
            }
            let bin = entry.path().join("bin").join("mvn");
            if bin.is_file() {
                out.push(bin);
            }
        }
    }
}

fn which_maven_on_path() -> Result<PathBuf> {
    let output = Command::new("which")
        .arg("mvn")
        .output()
        .context("failed to run which mvn")?;
    if !output.status.success() {
        bail!("mvn not on PATH");
    }
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if path.is_empty() {
        bail!("mvn not on PATH");
    }
    Ok(PathBuf::from(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_maven_home_dir() {
        let tmp = std::env::temp_dir().join(format!("reaper-maven-test-{}", std::process::id()));
        let bin_dir = tmp.join("bin");
        std::fs::create_dir_all(&bin_dir).unwrap();
        let bin = bin_dir.join("mvn");
        std::fs::write(&bin, b"").unwrap();
        let resolved = normalize_maven_binary(tmp.clone()).expect("resolve home");
        assert_eq!(resolved, bin);
        let _ = std::fs::remove_dir_all(tmp);
    }
}
