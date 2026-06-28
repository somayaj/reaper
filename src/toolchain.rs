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

/// Language compilers and runtimes configurable under Settings → Compiler.
pub const TOOLS: &[ToolDef] = &[
    ToolDef {
        id: "java",
        label: "Java (JAVA_HOME)",
        kind: ToolKind::Home,
        defaults: &[],
        env_key: Some("REAPER_JAVA_HOME"),
    },
    ToolDef {
        id: "kotlin",
        label: "Kotlin (kotlinc)",
        kind: ToolKind::Binary,
        defaults: &["kotlinc"],
        env_key: Some("REAPER_KOTLINC"),
    },
    ToolDef {
        id: "groovy",
        label: "Groovy (groovyc)",
        kind: ToolKind::Binary,
        defaults: &["groovyc"],
        env_key: Some("REAPER_GROOVC"),
    },
    ToolDef {
        id: "python",
        label: "Python (python3)",
        kind: ToolKind::Binary,
        defaults: &["python3", "python"],
        env_key: Some("REAPER_PYTHON"),
    },
    ToolDef {
        id: "ruby",
        label: "Ruby",
        kind: ToolKind::Binary,
        defaults: &["ruby"],
        env_key: Some("REAPER_RUBY"),
    },
    ToolDef {
        id: "bundle",
        label: "Bundler (bundle)",
        kind: ToolKind::Binary,
        defaults: &["bundle"],
        env_key: Some("REAPER_BUNDLE"),
    },
    ToolDef {
        id: "rails",
        label: "Rails",
        kind: ToolKind::Binary,
        defaults: &["rails"],
        env_key: Some("REAPER_RAILS"),
    },
    ToolDef {
        id: "rustc",
        label: "Rust (rustc)",
        kind: ToolKind::Binary,
        defaults: &["rustc"],
        env_key: Some("REAPER_RUSTC"),
    },
    ToolDef {
        id: "cargo",
        label: "Rust (cargo)",
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
        id: "node",
        label: "Node.js",
        kind: ToolKind::Binary,
        defaults: &["node"],
        env_key: Some("REAPER_NODEJS"),
    },
    ToolDef {
        id: "tsc",
        label: "TypeScript (tsc)",
        kind: ToolKind::Binary,
        defaults: &["tsc"],
        env_key: Some("REAPER_TSC"),
    },
    ToolDef {
        id: "php",
        label: "PHP",
        kind: ToolKind::Binary,
        defaults: &["php"],
        env_key: Some("REAPER_PHP"),
    },
    ToolDef {
        id: "clang",
        label: "C/C++ (clang)",
        kind: ToolKind::Binary,
        defaults: &["clang"],
        env_key: Some("REAPER_CLANG"),
    },
    ToolDef {
        id: "gcc",
        label: "C/C++ (gcc)",
        kind: ToolKind::Binary,
        defaults: &["gcc"],
        env_key: Some("REAPER_GCC"),
    },
    ToolDef {
        id: "swiftc",
        label: "Swift (swiftc)",
        kind: ToolKind::Binary,
        defaults: &["swiftc"],
        env_key: Some("REAPER_SWIFTC"),
    },
    ToolDef {
        id: "luac",
        label: "Lua (luac)",
        kind: ToolKind::Binary,
        defaults: &["luac"],
        env_key: Some("REAPER_LUAC"),
    },
    ToolDef {
        id: "csc",
        label: "C# (csc)",
        kind: ToolKind::Binary,
        defaults: &["csc"],
        env_key: Some("REAPER_CSC"),
    },
    ToolDef {
        id: "dart",
        label: "Dart",
        kind: ToolKind::Binary,
        defaults: &["dart"],
        env_key: Some("REAPER_DART"),
    },
    ToolDef {
        id: "bash",
        label: "Shell (bash)",
        kind: ToolKind::Binary,
        defaults: &["bash", "sh", "zsh"],
        env_key: Some("REAPER_BASH"),
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
    resolve_program(id).with_context(|| format!("{id} not found — set it in Settings → Compiler"))
}

pub fn validate_tool_path(id: &str, path: &str) -> Result<PathBuf> {
    let def = tool_def(id).context("unknown compiler")?;
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
pub struct CompilerEntryView {
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
pub struct CompilersView {
    pub compilers: Vec<CompilerEntryView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub java_installed: Option<Vec<crate::jdk::JdkInstall>>,
}

pub fn compilers_view(
    configured: &HashMap<String, String>,
    java_home: Option<&str>,
    java_source: Option<&str>,
) -> CompilersView {
    let mut compilers = Vec::new();
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
            let source = path.as_ref().map(|_| "settings".to_string());
            (path, source)
        };

        let effective = resolve_program(def.id);
        let version = effective
            .as_ref()
            .and_then(|p| tool_version(def.id, p));

        compilers.push(CompilerEntryView {
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

    CompilersView {
        compilers,
        java_installed: Some(crate::jdk::list_installed_jdks()),
    }
}

// Backward-compatible aliases for the previous Toolchains API shape.
pub type ToolchainEntryView = CompilerEntryView;
pub type ToolchainsView = CompilersView;

pub fn toolchains_view(
    configured: &HashMap<String, String>,
    java_home: Option<&str>,
    java_source: Option<&str>,
) -> CompilersView {
    compilers_view(configured, java_home, java_source)
}
