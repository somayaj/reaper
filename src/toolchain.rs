use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{OnceLock, RwLock};

use anyhow::{Context, Result, bail};
use serde::Serialize;

static TOOL_OVERRIDES: OnceLock<RwLock<HashMap<String, PathBuf>>> = OnceLock::new();

fn overrides() -> &'static RwLock<HashMap<String, PathBuf>> {
    TOOL_OVERRIDES.get_or_init(|| RwLock::new(HashMap::new()))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolKind {
    Home,
    Binary,
}

#[derive(Debug, Clone, Copy)]
pub struct ToolDef {
    pub id: &'static str,
    pub label: &'static str,
    pub kind: ToolKind,
    pub defaults: &'static [&'static str],
    pub env_key: Option<&'static str>,
}

pub const TOOLS: &[ToolDef] = &[
    ToolDef {
        id: "java",
        label: "Java (JAVA_HOME)",
        kind: ToolKind::Home,
        defaults: &[],
        env_key: Some("REAPER_JAVA_HOME"),
    },
    ToolDef {
        id: "python",
        label: "Python",
        kind: ToolKind::Binary,
        defaults: &["python3", "python"],
        env_key: Some("REAPER_PYTHON"),
    },
    ToolDef {
        id: "rustc",
        label: "Rust compiler (rustc)",
        kind: ToolKind::Binary,
        defaults: &["rustc"],
        env_key: Some("REAPER_RUSTC"),
    },
    ToolDef {
        id: "cargo",
        label: "Rust package manager (cargo)",
        kind: ToolKind::Binary,
        defaults: &["cargo"],
        env_key: Some("REAPER_CARGO"),
    },
    ToolDef {
        id: "go",
        label: "Go",
        kind: ToolKind::Binary,
        defaults: &["go"],
        env_key: Some("REAPER_GO"),
    },
    ToolDef {
        id: "ruby",
        label: "Ruby",
        kind: ToolKind::Binary,
        defaults: &["ruby"],
        env_key: Some("REAPER_RUBY"),
    },
    ToolDef {
        id: "rails",
        label: "Rails",
        kind: ToolKind::Binary,
        defaults: &["rails", "bundle"],
        env_key: Some("REAPER_RAILS"),
    },
];

pub fn tool_def(id: &str) -> Option<&'static ToolDef> {
    TOOLS.iter().find(|t| t.id == id)
}

pub fn set_configured_tools(paths: HashMap<String, PathBuf>) {
    if let Ok(mut guard) = overrides().write() {
        *guard = paths;
    }
}

pub fn configured_path(id: &str) -> Option<PathBuf> {
    overrides().read().ok()?.get(id).cloned()
}

pub fn resolve_program(id: &str) -> Option<PathBuf> {
    let def = tool_def(id)?;
    if let Some(path) = configured_path(id) {
        return Some(path);
    }
    if let Some(key) = def.env_key {
        if let Ok(raw) = std::env::var(key) {
            if !raw.is_empty() {
                if def.kind == ToolKind::Home {
                    if crate::jdk::validate_java_home(Path::new(&raw)).is_ok() {
                        return Some(PathBuf::from(raw));
                    }
                } else if Path::new(&raw).is_file() {
                    return Some(PathBuf::from(raw));
                }
            }
        }
    }
    if id == "java" {
        return crate::jdk::effective_java_home().ok();
    }
    for name in def.defaults {
        if let Some(path) = find_on_path(name) {
            return Some(path);
        }
    }
    None
}

fn find_on_path(name: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

pub fn resolve_program_or(id: &str) -> Result<PathBuf> {
    resolve_program(id).with_context(|| format!("{id} not found — set it in Settings → Toolchains"))
}

pub fn validate_tool_path(id: &str, path: &str) -> Result<PathBuf> {
    let def = tool_def(id).context("unknown toolchain")?;
    let path = path.trim();
    if path.is_empty() {
        bail!("path required");
    }
    match def.kind {
        ToolKind::Home => crate::jdk::validate_java_home(Path::new(path)),
        ToolKind::Binary => {
            let p = PathBuf::from(path);
            if !p.is_file() {
                bail!("not a file: {}", p.display());
            }
            Ok(p)
        }
    }
}

pub fn tool_version(id: &str, path: &Path) -> Option<String> {
    match id {
        "java" => crate::jdk::java_version_string(path).ok(),
        _ => {
            let out = Command::new(path)
                .arg("--version")
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output()
                .ok()?;
            let text = if out.stdout.is_empty() {
                String::from_utf8_lossy(&out.stderr).into_owned()
            } else {
                String::from_utf8_lossy(&out.stdout).into_owned()
            };
            text.lines().next().map(|l| l.trim().to_string())
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolchainEntryView {
    pub id: String,
    pub label: String,
    pub kind: String,
    pub configured: bool,
    pub path: Option<String>,
    pub effective: Option<String>,
    pub version: Option<String>,
    pub source: Option<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct ToolchainsView {
    pub tools: Vec<ToolchainEntryView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub java_installed: Option<Vec<crate::jdk::JdkInstall>>,
}

pub fn toolchains_view(
    configured: &HashMap<String, String>,
    java_home: Option<&str>,
    java_source: Option<&str>,
) -> ToolchainsView {
    let mut tools = Vec::new();
    for def in TOOLS {
        let (configured_path, source) = if def.id == "java" {
            (
                java_home.filter(|s| !s.is_empty()).map(str::to_string),
                java_source.map(str::to_string),
            )
        } else {
            let path = configured
                .get(def.id)
                .filter(|s| !s.is_empty())
                .cloned();
            let source = path
                .as_ref()
                .map(|_| "settings".to_string());
            (path, source)
        };

        let effective = resolve_program(def.id);
        let version = effective
            .as_ref()
            .and_then(|p| tool_version(def.id, p));

        tools.push(ToolchainEntryView {
            id: def.id.to_string(),
            label: def.label.to_string(),
            kind: match def.kind {
                ToolKind::Home => "home",
                ToolKind::Binary => "binary",
            }
            .to_string(),
            configured: configured_path.is_some(),
            path: configured_path,
            effective: effective.as_ref().map(|p| p.display().to_string()),
            version,
            source,
        });
    }

    ToolchainsView {
        tools,
        java_installed: Some(crate::jdk::list_installed_jdks()),
    }
}
