use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct GradleInstall {
    pub path: String,
    pub version: String,
    pub label: String,
}

pub fn gradle_binary_in_home(home: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        home.join("bin").join("gradle.bat")
    }
    #[cfg(not(windows))]
    {
        home.join("bin").join("gradle")
    }
}

pub fn normalize_gradle_binary(path: PathBuf) -> Result<PathBuf> {
    if path.is_file() {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name == "gradle" || name == "gradle.bat" {
            return Ok(path);
        }
        bail!("not a gradle executable: {}", path.display());
    }
    if path.is_dir() {
        let bin = gradle_binary_in_home(&path);
        if bin.is_file() {
            return Ok(bin);
        }
        bail!("no bin/gradle in {}", path.display());
    }
    bail!("gradle path not found: {}", path.display());
}

pub fn validate_gradle_path(path: &str) -> Result<PathBuf> {
    normalize_gradle_binary(PathBuf::from(path.trim()))
}

pub fn gradle_version_string(binary: &Path) -> Result<String> {
    let out = Command::new(binary)
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
        if let Some(rest) = line.strip_prefix("Gradle ") {
            let ver = rest.split_whitespace().next().unwrap_or(rest).trim();
            return Ok(ver.to_string());
        }
    }
    text.lines()
        .next()
        .map(|l| l.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!("could not parse gradle version"))
}

pub fn list_installed_gradles() -> Vec<GradleInstall> {
    let mut installs = Vec::new();
    let mut seen = HashSet::new();

    for bin in scan_gradle_binaries() {
        let key = bin.canonicalize().unwrap_or_else(|_| bin.clone());
        if !seen.insert(key) {
            continue;
        }
        let version = gradle_version_string(&bin).unwrap_or_else(|_| "?".into());
        installs.push(GradleInstall {
            path: bin.display().to_string(),
            version: version.clone(),
            label: format!("Gradle {version} — {}", short_gradle_label(&bin)),
        });
    }

    installs.sort_by(|a, b| b.version.cmp(&a.version));
    installs
}

fn short_gradle_label(bin: &Path) -> String {
    let s = bin.to_string_lossy();
    if s.contains(".gradle/wrapper/dists") {
        return "wrapper cache".into();
    }
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
    "Gradle".into()
}

fn scan_gradle_binaries() -> Vec<PathBuf> {
    let mut out = Vec::new();
    scan_wrapper_dists(&mut out);
    scan_homebrew(&mut out);
    if let Ok(path) = which_gradle_on_path() {
        out.push(path);
    }
    out
}

fn scan_wrapper_dists(out: &mut Vec<PathBuf>) {
    let Some(home) = std::env::var_os("HOME") else {
        return;
    };
    collect_gradle_bins(&PathBuf::from(home).join(".gradle/wrapper/dists"), out);
}

fn scan_homebrew(out: &mut Vec<PathBuf>) {
    for prefix in ["/opt/homebrew/opt", "/usr/local/opt"] {
        let opt = PathBuf::from(prefix);
        let Ok(entries) = std::fs::read_dir(&opt) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !name.starts_with("gradle") {
                continue;
            }
            let bin = entry.path().join("bin").join("gradle");
            if bin.is_file() {
                out.push(bin);
            }
        }
    }
}

fn collect_gradle_bins(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let bin = path.join("gradle").join("bin").join("gradle");
            #[cfg(windows)]
            let bin = path.join("gradle").join("bin").join("gradle.bat");
            if bin.is_file() {
                out.push(bin);
            }
            collect_gradle_bins(&path, out);
        }
    }
}

fn which_gradle_on_path() -> Result<PathBuf> {
    let output = Command::new("which")
        .arg("gradle")
        .output()
        .context("failed to run which gradle")?;
    if !output.status.success() {
        bail!("gradle not on PATH");
    }
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if path.is_empty() {
        bail!("gradle not on PATH");
    }
    Ok(PathBuf::from(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_gradle_home_dir() {
        let tmp = std::env::temp_dir().join(format!("reaper-gradle-test-{}", std::process::id()));
        let bin_dir = tmp.join("bin");
        std::fs::create_dir_all(&bin_dir).unwrap();
        let bin = bin_dir.join("gradle");
        std::fs::write(&bin, b"").unwrap();
        let resolved = normalize_gradle_binary(tmp.clone()).expect("resolve home");
        assert_eq!(resolved, bin);
        let _ = std::fs::remove_dir_all(tmp);
    }
}
