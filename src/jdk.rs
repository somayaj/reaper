use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{OnceLock, RwLock};

use anyhow::{Context, Result, bail};
use serde::Serialize;

static JAVA_HOME_OVERRIDE: OnceLock<RwLock<Option<PathBuf>>> = OnceLock::new();

fn override_slot() -> &'static RwLock<Option<PathBuf>> {
    JAVA_HOME_OVERRIDE.get_or_init(|| RwLock::new(None))
}

pub fn set_configured_java_home(home: Option<PathBuf>) {
    if let Ok(mut guard) = override_slot().write() {
        *guard = home.filter(|p| p.is_dir());
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct JdkInstall {
    pub path: String,
    pub version: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct JdkSettingsView {
    pub configured: bool,
    pub java_home: Option<String>,
    pub version: Option<String>,
    pub source: Option<String>,
    pub effective_home: Option<String>,
    pub effective_version: Option<String>,
    pub gradle_home: Option<String>,
    pub gradle_version: Option<String>,
    pub installed: Vec<JdkInstall>,
}

pub fn list_installed_jdks() -> Vec<JdkInstall> {
    let mut installs = Vec::new();

    #[cfg(target_os = "macos")]
    {
        if let Ok(out) = Command::new("/usr/libexec/java_home").arg("-V").output() {
            let text = format!(
                "{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
            for line in text.lines() {
                if let Some(install) = parse_macos_java_home_line(line) {
                    installs.push(install);
                }
            }
        }
    }

    if installs.is_empty() {
        if let Ok(home) = detect_java_home_auto() {
            if let Ok(version) = java_version_string(&home) {
                installs.push(JdkInstall {
                    path: home.display().to_string(),
                    version: version.clone(),
                    label: format!("{version} (default)"),
                });
            }
        }
    }

    installs
}

#[cfg(target_os = "macos")]
fn parse_macos_java_home_line(line: &str) -> Option<JdkInstall> {
    let trimmed = line.trim();
    if !trimmed.chars().next()?.is_ascii_digit() {
        return None;
    }
    let (left, path) = trimmed.rsplit_once(' ')?;
    let path = path.trim();
    if !path.starts_with('/') || !Path::new(path).join("bin/java").is_file() {
        return None;
    }
    let version = left
        .split_whitespace()
        .next()
        .unwrap_or("unknown")
        .to_string();
    let vendor = left
        .split('"')
        .nth(1)
        .unwrap_or("JDK")
        .trim();
    Some(JdkInstall {
        path: path.to_string(),
        version: version.clone(),
        label: format!("Java {version} — {vendor}"),
    })
}

pub fn jdk_settings_view(configured: Option<&str>, source: Option<&str>) -> JdkSettingsView {
    let installed = list_installed_jdks();
    let configured_path = configured.filter(|s| !s.is_empty()).map(PathBuf::from);
    let configured_version = configured_path
        .as_ref()
        .and_then(|p| java_version_string(p).ok());
    let effective = effective_java_home().ok();
    let effective_version = effective
        .as_ref()
        .and_then(|p| java_version_string(p).ok());
    let gradle = gradle_java_home().ok();
    let gradle_version = gradle.as_ref().and_then(|p| java_version_string(p).ok());

    JdkSettingsView {
        configured: configured.is_some(),
        java_home: configured.map(str::to_string),
        version: configured_version,
        source: source.map(str::to_string),
        effective_home: effective.as_ref().map(|p| p.display().to_string()),
        effective_version,
        gradle_home: gradle.as_ref().map(|p| p.display().to_string()),
        gradle_version,
        installed,
    }
}

pub fn effective_java_home() -> Result<PathBuf> {
    if let Ok(guard) = override_slot().read() {
        if let Some(home) = guard.clone() {
            return validate_java_home(&home);
        }
    }

    if let Ok(home) = std::env::var("REAPER_JAVA_HOME") {
        if !home.is_empty() {
            return validate_java_home(&PathBuf::from(home));
        }
    }

    detect_java_home_auto()
}

/// JVM used to run Gradle (classpath resolution, tests, build). Gradle 8+ needs Java 17+.
pub fn gradle_java_home() -> Result<PathBuf> {
    gradle_java_home_with_max(25)
}

/// Pick a JDK whose major version is compatible with the project's Gradle wrapper.
pub fn gradle_java_home_with_max(max_major: u32) -> Result<PathBuf> {
    if let Ok(home) = effective_java_home() {
        if let Some(major) = java_major_version(&home) {
            if major >= 11 && major <= max_major {
                return Ok(home);
            }
        }
    }
    detect_java_home_for_max(max_major)
}

fn detect_java_home_for_max(max_major: u32) -> Result<PathBuf> {
    let mut versions = Vec::new();
    if max_major >= 21 {
        versions.extend(["21", "17", "11"]);
    } else if max_major >= 19 {
        versions.extend(["17", "11", "19"]);
    } else {
        versions.extend(["17", "11"]);
    }
    for v in versions {
        if let Ok(major) = v.parse::<u32>() {
            if major <= max_major {
                if let Ok(home) = detect_java_home_for_versions(&[v]) {
                    return Ok(home);
                }
            }
        }
    }
    detect_gradle_java_home()
}

pub fn validate_java_home(home: &Path) -> Result<PathBuf> {
    let java = home.join("bin/java");
    if !java.is_file() {
        bail!("JDK not found at {} (missing bin/java)", home.display());
    }
    Ok(home.to_path_buf())
}

fn detect_java_home_auto() -> Result<PathBuf> {
    // Use 1.8 for legacy JDK 8; macOS `java_home -v 8` often resolves to a newer JDK.
    detect_java_home_for_versions(&["21", "17", "11", "25", "23", "24", "26", "1.8"])
}

/// JDK used for tooling (Gradle, indexing, JDK sources). Never a legacy Java 8 install.
pub fn toolchain_java_home() -> Result<PathBuf> {
    gradle_java_home()
}

fn detect_gradle_java_home() -> Result<PathBuf> {
    detect_java_home_for_versions(&["21", "17", "11", "25", "23", "24", "26"])
}

fn detect_java_home_for_versions(versions: &[&str]) -> Result<PathBuf> {
    #[cfg(target_os = "macos")]
    for version in versions {
        if let Ok(out) = Command::new("/usr/libexec/java_home")
            .arg("-v")
            .arg(version)
            .output()
        {
            if out.status.success() {
                let home = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim());
                if home.join("bin/java").is_file() {
                    return Ok(home);
                }
            }
        }
    }

    if let Ok(home) = std::env::var("JAVA_HOME") {
        let path = PathBuf::from(&home);
        if path.join("bin/java").is_file() {
            return Ok(path);
        }
    }

    java_home_from_java_cmd()
}

pub fn java_major_version(home: &Path) -> Option<u32> {
    let version = java_version_string(home).ok()?;
    if version.starts_with("1.8") || version.starts_with("1.7") || version.starts_with("1.6") {
        return Some(8);
    }
    version.split('.').next()?.parse().ok()
}

fn java_home_from_java_cmd() -> Result<PathBuf> {
    let out = Command::new("java")
        .args(["-XshowSettings:properties", "-version"])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .context("failed to run java")?;

    let text = String::from_utf8_lossy(&out.stderr);
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("java.home = ") {
            let home = PathBuf::from(rest.trim());
            if home.join("bin/java").is_file() {
                return Ok(home);
            }
        }
    }
    bail!("could not determine JAVA_HOME")
}

pub fn java_version_string(home: &Path) -> Result<String> {
    let java = home.join("bin/java");
    let out = Command::new(&java)
        .arg("-version")
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .with_context(|| format!("failed to run {}", java.display()))?;

    let text = String::from_utf8_lossy(&out.stderr);
    text.lines()
        .next()
        .map(|l| l.trim().trim_start_matches("openjdk version ").trim_matches('"').to_string())
        .filter(|s| !s.is_empty())
        .context("empty java -version output")
}

pub fn apply_java_env(cmd: &mut Command) {
    if let Ok(home) = effective_java_home() {
        apply_java_home(cmd, &home);
    }
}

pub fn apply_java_home(cmd: &mut Command, home: &Path) {
    cmd.env("JAVA_HOME", home);
    let bin = home.join("bin");
    if bin.is_dir() {
        let path = std::env::var("PATH").unwrap_or_default();
        let prefix = bin.to_string_lossy();
        if path.is_empty() {
            cmd.env("PATH", prefix.as_ref());
        } else {
            cmd.env("PATH", format!("{prefix}:{path}"));
        }
    }
}
