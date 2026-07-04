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

const HOMEBREW_JDK_FORMULAE: &[&str] = &[
    "openjdk@21",
    "openjdk@17",
    "openjdk@11",
    "openjdk@25",
    "openjdk@24",
    "openjdk@23",
    "openjdk@8",
    "openjdk",
    "temurin@21",
    "temurin@17",
    "temurin@11",
    "temurin",
];

fn homebrew_prefixes() -> Vec<PathBuf> {
    let mut out = Vec::new();
    for p in ["/opt/homebrew", "/usr/local"] {
        let path = PathBuf::from(p);
        if path.is_dir() {
            out.push(path);
        }
    }
    out
}

fn homebrew_jdk_home_at(base: &Path, formula: &str) -> Option<PathBuf> {
    let opt = base.join("opt").join(formula);
    if !opt.exists() {
        return None;
    }
    for home in [
        opt.join("libexec/openjdk.jdk/Contents/Home"),
        opt.clone(),
    ] {
        if validate_java_home(&home).is_ok() {
            return Some(home);
        }
    }
    None
}

fn homebrew_java_home_for_version(version: &str) -> Option<PathBuf> {
    let candidates: Vec<String> = if version == "1.8" {
        vec!["openjdk@8".into()]
    } else {
        vec![format!("openjdk@{version}"), format!("temurin@{version}")]
    };
    for prefix in homebrew_prefixes() {
        for formula in &candidates {
            if let Some(home) = homebrew_jdk_home_at(&prefix, formula) {
                return Some(home);
            }
        }
    }
    None
}

fn scan_homebrew_jdks() -> Vec<JdkInstall> {
    let mut installs = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for prefix in homebrew_prefixes() {
        for formula in HOMEBREW_JDK_FORMULAE {
            let Some(home) = homebrew_jdk_home_at(&prefix, formula) else {
                continue;
            };
            let key = home.canonicalize().unwrap_or_else(|_| home.clone());
            if !seen.insert(key) {
                continue;
            }
            let version = java_version_string(&home).unwrap_or_else(|_| "?".into());
            installs.push(JdkInstall {
                path: home.display().to_string(),
                version: version.clone(),
                label: format!("{version} — Homebrew {formula}"),
            });
        }
    }
    installs
}

pub fn list_installed_jdks() -> Vec<JdkInstall> {
    let mut installs = Vec::new();
    let mut seen = std::collections::HashSet::new();

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
                    let key = PathBuf::from(&install.path);
                    let key = key.canonicalize().unwrap_or(key);
                    if seen.insert(key) {
                        installs.push(install);
                    }
                }
            }
        }
    }

    for install in scan_homebrew_jdks() {
        let key = PathBuf::from(&install.path);
        let key = key.canonicalize().unwrap_or(key);
        if seen.insert(key) {
            installs.push(install);
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
            if (11..=max_major).contains(&major) {
                return Ok(home);
            }
        }
    }
    detect_java_home_for_max(max_major)
}

/// Map JVM classfile major version to Java release (e.g. 70 → 26).
pub fn java_major_from_classfile(class_major: u32) -> u32 {
    if class_major >= 45 {
        class_major.saturating_sub(44)
    } else {
        class_major
    }
}

fn detect_java_home_for_max(max_major: u32) -> Result<PathBuf> {
    let candidates: &[&str] = if max_major >= 24 {
        &["24", "23", "21", "17", "11"]
    } else if max_major >= 21 {
        &["21", "17", "11"]
    } else if max_major >= 19 {
        &["19", "17", "11"]
    } else {
        &["17", "11"]
    };

    for v in candidates {
        let Ok(major) = v.parse::<u32>() else {
            continue;
        };
        if major > max_major {
            continue;
        }
        if let Ok(home) = detect_java_home_for_versions(&[v]) {
            if java_major_version(&home).is_some_and(|m| m <= max_major) {
                return Ok(home);
            }
        }
    }

    if let Ok(home) = effective_java_home() {
        if let Some(major) = java_major_version(&home) {
            if major > max_major {
                bail!(
                    "Gradle requires Java 11–{max_major}, but Settings → Java is Java {major}. Install Java 21 or 17 and select it in Settings → Java."
                );
            }
            if major < 11 {
                bail!(
                    "Gradle requires Java 11–{max_major}, but Settings → Java is Java {major}."
                );
            }
        }
    }

    bail!(
        "Gradle requires Java 11–{max_major}. Install Java 21 or 17 and set it in Settings → Java."
    );
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
    for version in versions {
        #[cfg(target_os = "macos")]
        {
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
            if let Some(home) = homebrew_java_home_for_version(version) {
                return Ok(home);
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = version;
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

/// JDK used to launch jdtls (must be 21+). Never Settings → Java / project JDK 17.
pub fn jdtls_java_home() -> Result<PathBuf> {
    if let Some(home) = crate::config::bundled_jdtls_java_home() {
        if java_major_version(&home).is_some_and(|m| m >= 21) {
            return validate_java_home(&home);
        }
    }
    detect_java_home_for_versions(&["21", "25", "23", "24", "26"]).context(
        "jdtls requires JDK 21 or newer to run. Rebuild Reaper.app to bundle JDK 21, \
         or install openjdk@21 (e.g. brew install openjdk@21). Keep JDK 17 in Settings → Java for your project.",
    )
}

/// JDK for jdtls project analysis — Settings → Java when it matches `release`, else Homebrew/PATH.
pub fn project_java_home_for_release(release: u32) -> Result<PathBuf> {
    if let Ok(home) = effective_java_home() {
        if java_major_version(&home) == Some(release) {
            return validate_java_home(&home);
        }
    }
    let version = if release == 8 {
        "1.8".to_string()
    } else {
        release.to_string()
    };
    detect_java_home_for_versions(&[&version]).with_context(|| {
        format!(
            "JDK {release} required for this project. Set it in Settings → Java or install openjdk@{release}."
        )
    })
}

pub fn apply_jdtls_java_env(cmd: &mut std::process::Command) {
    if let Ok(home) = jdtls_java_home() {
        apply_java_home(cmd, &home);
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn homebrew_openjdk21_path_when_installed() {
        let base = PathBuf::from("/opt/homebrew");
        if !base.is_dir() {
            return;
        }
        let home = homebrew_jdk_home_at(&base, "openjdk@21");
        assert!(
            home.is_some(),
            "expected /opt/homebrew/opt/openjdk@21 to resolve to a JDK"
        );
        let installs = scan_homebrew_jdks();
        assert!(
            installs.iter().any(|j| j.path.contains("openjdk@21") || j.label.contains("openjdk@21")),
            "scan_homebrew_jdks should include openjdk@21: {installs:?}"
        );
    }
}
