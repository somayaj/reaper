use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

const GEMINI_SETTINGS_KEY: &str = "__gemini__";
const CURSOR_SETTINGS_KEY: &str = "__cursor__";
const DEFAULT_GEMINI_MODEL: &str = "gemini-3.5-flash";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct SettingsFile {
    #[serde(default)]
    tokens: HashMap<String, String>,
    #[serde(default)]
    gemini_model: Option<String>,
    #[serde(default)]
    cursor_model: Option<String>,
    #[serde(default)]
    cursor_mode: Option<String>,
    #[serde(default)]
    jdk_home: Option<String>,
    #[serde(default)]
    workspaces_root: Option<String>,
}

#[derive(Clone)]
pub struct SettingsStore {
    path: Arc<Path>,
    inner: Arc<RwLock<SettingsFile>>,
}

#[derive(Debug, Serialize)]
pub struct TokenInfo {
    pub host: String,
    pub masked: String,
    pub source: String,
}

#[derive(Debug, Serialize)]
pub struct CursorSettingsView {
    pub configured: bool,
    pub masked: Option<String>,
    pub model: String,
    pub mode: String,
    pub source: Option<String>,
    pub bridge_ok: bool,
    pub bridge_error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct GeminiSettingsView {
    pub configured: bool,
    pub masked: Option<String>,
    pub model: String,
    pub source: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct WorkspacesSettingsView {
    pub workspaces_root: Option<String>,
    pub default_root: String,
    pub source: Option<String>,
    pub folder_picker_available: bool,
}

impl SettingsStore {
    pub fn load(path: &Path) -> Result<Self> {
        let file = if path.exists() {
            let raw = fs::read_to_string(path)
                .with_context(|| format!("read settings {}", path.display()))?;
            serde_json::from_str(&raw).unwrap_or_default()
        } else {
            SettingsFile::default()
        };

        Ok(Self {
            path: Arc::from(path),
            inner: Arc::new(RwLock::new(file)),
        })
        .map(|store| {
            store.sync_java_home_cache();
            store
        })
    }

    fn sync_java_home_cache(&self) {
        let home = self
            .inner
            .read()
            .ok()
            .and_then(|g| g.jdk_home.clone())
            .filter(|h| !h.is_empty())
            .map(PathBuf::from);
        crate::jdk::set_configured_java_home(home);
    }

    pub fn java_home(&self) -> Option<String> {
        if let Ok(guard) = self.inner.read() {
            if let Some(home) = &guard.jdk_home {
                if !home.is_empty() {
                    return Some(home.clone());
                }
            }
        }
        std::env::var("REAPER_JAVA_HOME")
            .ok()
            .filter(|h| !h.is_empty())
    }

    pub fn set_java_home(&self, home: String) -> Result<()> {
        let home = home.trim().to_string();
        if home.is_empty() {
            anyhow::bail!("java_home required");
        }
        crate::jdk::validate_java_home(Path::new(&home))?;
        let mut guard = self.inner.write().expect("settings lock poisoned");
        guard.jdk_home = Some(home);
        self.save(&guard)?;
        self.sync_java_home_cache();
        Ok(())
    }

    pub fn clear_java_home(&self) -> Result<bool> {
        let mut guard = self.inner.write().expect("settings lock poisoned");
        let removed = guard.jdk_home.take().is_some();
        if removed {
            self.save(&guard)?;
        }
        self.sync_java_home_cache();
        Ok(removed)
    }

    pub fn workspaces_root(&self) -> Option<String> {
        if let Ok(guard) = self.inner.read() {
            if let Some(root) = &guard.workspaces_root {
                if !root.is_empty() {
                    return Some(root.clone());
                }
            }
        }
        std::env::var("REAPER_WORKSPACES_DIR")
            .ok()
            .filter(|r| !r.is_empty())
    }

    pub fn set_workspaces_root(&self, root: String) -> Result<()> {
        let root = root.trim().to_string();
        if root.is_empty() {
            anyhow::bail!("workspaces_root required");
        }
        crate::workspace::normalize_workspace_path(&root)?;
        let mut guard = self.inner.write().expect("settings lock poisoned");
        guard.workspaces_root = Some(root);
        self.save(&guard)
    }

    pub fn clear_workspaces_root(&self) -> Result<bool> {
        let mut guard = self.inner.write().expect("settings lock poisoned");
        let removed = guard.workspaces_root.take().is_some();
        if removed {
            self.save(&guard)?;
        }
        Ok(removed)
    }

    pub fn workspaces_view(&self, default_root: &str) -> WorkspacesSettingsView {
        let from_env = std::env::var("REAPER_WORKSPACES_DIR")
            .ok()
            .filter(|r| !r.is_empty());

        let from_file = self
            .inner
            .read()
            .ok()
            .and_then(|g| g.workspaces_root.clone())
            .filter(|r| !r.is_empty());

        let (workspaces_root, source) = if let Some(root) = from_file {
            (Some(root), Some("settings".to_string()))
        } else if let Some(root) = from_env {
            (Some(root), Some("env:REAPER_WORKSPACES_DIR".to_string()))
        } else {
            (None, None)
        };

        WorkspacesSettingsView {
            workspaces_root,
            default_root: default_root.to_string(),
            source,
            folder_picker_available: cfg!(target_os = "macos"),
        }
    }

    pub fn jdk_view(&self) -> crate::jdk::JdkSettingsView {
        let from_env = std::env::var("REAPER_JAVA_HOME")
            .ok()
            .filter(|h| !h.is_empty());

        let from_file = self
            .inner
            .read()
            .ok()
            .and_then(|g| g.jdk_home.clone())
            .filter(|h| !h.is_empty());

        let (configured, java_home, source) = if let Some(home) = from_file {
            (true, Some(home), Some("settings".to_string()))
        } else if let Some(home) = from_env {
            (true, Some(home), Some("env:REAPER_JAVA_HOME".to_string()))
        } else {
            (false, None, None)
        };

        crate::jdk::jdk_settings_view(java_home.as_deref(), source.as_deref())
    }

    fn save(&self, file: &SettingsFile) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let raw = serde_json::to_string_pretty(file)?;
        fs::write(self.path.as_ref(), raw)?;
        Ok(())
    }

    pub fn token_for_host(&self, host: &str) -> Option<String> {
        if let Ok(guard) = self.inner.read() {
            if let Some(token) = guard.tokens.get(host) {
                if !token.is_empty() && host != GEMINI_SETTINGS_KEY {
                    return Some(token.clone());
                }
            }
            if let Some(token) = guard.tokens.get("*") {
                if !token.is_empty() {
                    return Some(token.clone());
                }
            }
        }

        if let Ok(env_token) = std::env::var(env_key_for_host(host)) {
            if !env_token.is_empty() {
                return Some(env_token);
            }
        }

        std::env::var("REAPER_PAT")
            .ok()
            .filter(|t| !t.is_empty())
    }

    pub fn has_token_for_host(&self, host: &str) -> bool {
        self.token_for_host(host).is_some()
    }

    pub fn set_token(&self, host: &str, token: String) -> Result<()> {
        let mut guard = self.inner.write().expect("settings lock poisoned");
        guard.tokens.insert(host.to_string(), token);
        self.save(&guard)
    }

    pub fn remove_token(&self, host: &str) -> Result<bool> {
        let mut guard = self.inner.write().expect("settings lock poisoned");
        let removed = guard.tokens.remove(host).is_some();
        if removed {
            self.save(&guard)?;
        }
        Ok(removed)
    }

    pub fn list_tokens(&self) -> Vec<TokenInfo> {
        let mut entries = Vec::new();

        if let Ok(guard) = self.inner.read() {
            for (host, token) in &guard.tokens {
                if host == GEMINI_SETTINGS_KEY {
                    continue;
                }
                if host == CURSOR_SETTINGS_KEY {
                    continue;
                }
                entries.push(TokenInfo {
                    host: host.clone(),
                    masked: mask_token(token),
                    source: "settings".into(),
                });
            }
        }

        if let Ok(pat) = std::env::var("REAPER_PAT") {
            if !pat.is_empty() && !entries.iter().any(|e| e.host == "*") {
                entries.push(TokenInfo {
                    host: "*".into(),
                    masked: mask_token(&pat),
                    source: "env:REAPER_PAT".into(),
                });
            }
        }

        entries.sort_by(|a, b| a.host.cmp(&b.host));
        entries
    }

    pub fn gemini_api_key(&self) -> Option<String> {
        if let Ok(guard) = self.inner.read() {
            if let Some(key) = guard.tokens.get(GEMINI_SETTINGS_KEY) {
                if !key.is_empty() {
                    return Some(key.clone());
                }
            }
        }
        std::env::var("REAPER_GEMINI_API_KEY")
            .ok()
            .filter(|k| !k.is_empty())
    }

    pub fn gemini_model(&self) -> String {
        let stored = self
            .inner
            .read()
            .ok()
            .map(|guard| stored_gemini_model(&guard))
            .unwrap_or_else(|| DEFAULT_GEMINI_MODEL.into());
        normalize_gemini_model(&stored)
    }

    pub fn migrate_gemini_model(&self) -> Result<()> {
        let stored = self
            .inner
            .read()
            .ok()
            .map(|guard| stored_gemini_model(&guard))
            .unwrap_or_else(|| DEFAULT_GEMINI_MODEL.into());
        let normalized = normalize_gemini_model(&stored);
        if normalized != stored {
            self.set_gemini_model(normalized)?;
        }
        Ok(())
    }

    pub fn set_gemini_api_key(&self, api_key: String) -> Result<()> {
        let mut guard = self.inner.write().expect("settings lock poisoned");
        guard.tokens.insert(GEMINI_SETTINGS_KEY.to_string(), api_key);
        self.save(&guard)
    }

    pub fn clear_gemini_api_key(&self) -> Result<bool> {
        let mut guard = self.inner.write().expect("settings lock poisoned");
        let removed = guard.tokens.remove(GEMINI_SETTINGS_KEY).is_some();
        if removed {
            self.save(&guard)?;
        }
        Ok(removed)
    }

    pub fn set_gemini_model(&self, model: String) -> Result<()> {
        let mut guard = self.inner.write().expect("settings lock poisoned");
        guard.gemini_model = Some(model);
        self.save(&guard)
    }

    pub fn gemini_view(&self) -> GeminiSettingsView {
        let from_env = std::env::var("REAPER_GEMINI_API_KEY")
            .ok()
            .filter(|k| !k.is_empty());

        let from_file = self
            .inner
            .read()
            .ok()
            .and_then(|g| g.tokens.get(GEMINI_SETTINGS_KEY).cloned())
            .filter(|k| !k.is_empty());

        let (configured, masked, source) = if let Some(key) = from_file {
            (true, Some(mask_token(&key)), Some("settings".into()))
        } else if let Some(key) = from_env {
            (true, Some(mask_token(&key)), Some("env:REAPER_GEMINI_API_KEY".into()))
        } else {
            (false, None, None)
        };

        GeminiSettingsView {
            configured,
            masked,
            model: {
                let _ = self.migrate_gemini_model();
                self.gemini_model()
            },
            source,
        }
    }

    pub fn cursor_api_key(&self) -> Option<String> {
        if let Ok(guard) = self.inner.read() {
            if let Some(key) = guard.tokens.get(CURSOR_SETTINGS_KEY) {
                if !key.is_empty() {
                    return Some(key.clone());
                }
            }
        }
        std::env::var("REAPER_CURSOR_API_KEY")
            .ok()
            .or_else(|| std::env::var("CURSOR_API_KEY").ok())
            .filter(|k| !k.is_empty())
    }

    pub fn cursor_model(&self) -> String {
        if let Ok(guard) = self.inner.read() {
            if let Some(model) = &guard.cursor_model {
                if !model.is_empty() {
                    return model.clone();
                }
            }
        }
        std::env::var("REAPER_CURSOR_MODEL").unwrap_or_else(|_| "composer-2.5".into())
    }

    pub fn set_cursor_api_key(&self, api_key: String) -> Result<()> {
        let api_key = normalize_cursor_api_key(&api_key).map_err(|e| anyhow::anyhow!(e))?;
        let mut guard = self.inner.write().expect("settings lock poisoned");
        guard.tokens.insert(CURSOR_SETTINGS_KEY.to_string(), api_key);
        self.save(&guard)
    }

    pub fn clear_cursor_api_key(&self) -> Result<bool> {
        let mut guard = self.inner.write().expect("settings lock poisoned");
        let removed = guard.tokens.remove(CURSOR_SETTINGS_KEY).is_some();
        if removed {
            self.save(&guard)?;
        }
        Ok(removed)
    }

    pub fn set_cursor_model(&self, model: String) -> Result<()> {
        let mut guard = self.inner.write().expect("settings lock poisoned");
        guard.cursor_model = Some(model);
        self.save(&guard)
    }

    pub fn cursor_mode(&self) -> String {
        if let Ok(guard) = self.inner.read() {
            if let Some(mode) = &guard.cursor_mode {
                if matches!(mode.as_str(), "agent" | "plan" | "ask") {
                    return mode.clone();
                }
            }
        }
        std::env::var("REAPER_CURSOR_MODE")
            .ok()
            .filter(|m| matches!(m.as_str(), "agent" | "plan" | "ask"))
            .unwrap_or_else(|| "agent".into())
    }

    pub fn set_cursor_mode(&self, mode: String) -> Result<()> {
        let mode = mode.trim().to_string();
        if !matches!(mode.as_str(), "agent" | "plan" | "ask") {
            anyhow::bail!("mode must be agent, plan, or ask");
        }
        let mut guard = self.inner.write().expect("settings lock poisoned");
        guard.cursor_mode = Some(mode);
        self.save(&guard)
    }

    pub fn cursor_view(&self, bridge_ok: bool, bridge_error: Option<String>) -> CursorSettingsView {
        let from_env = std::env::var("REAPER_CURSOR_API_KEY")
            .ok()
            .or_else(|| std::env::var("CURSOR_API_KEY").ok())
            .filter(|k| !k.is_empty());

        let from_file = self
            .inner
            .read()
            .ok()
            .and_then(|g| g.tokens.get(CURSOR_SETTINGS_KEY).cloned())
            .filter(|k| !k.is_empty());

        let (configured, masked, source) = if let Some(key) = from_file {
            (true, Some(mask_token(&key)), Some("settings".into()))
        } else if let Some(key) = from_env {
            (
                true,
                Some(mask_token(&key)),
                Some("env:REAPER_CURSOR_API_KEY".into()),
            )
        } else {
            (false, None, None)
        };

        CursorSettingsView {
            configured,
            masked,
            model: self.cursor_model(),
            mode: self.cursor_mode(),
            source,
            bridge_ok,
            bridge_error,
        }
    }
}

fn env_key_for_host(host: &str) -> String {
    format!(
        "REAPER_PAT_{}",
        host.replace('.', "_").replace('-', "_").to_uppercase()
    )
}

fn stored_gemini_model(guard: &SettingsFile) -> String {
    guard
        .gemini_model
        .clone()
        .filter(|m| !m.is_empty())
        .or_else(|| std::env::var("REAPER_GEMINI_MODEL").ok())
        .filter(|m| !m.is_empty())
        .unwrap_or_else(|| DEFAULT_GEMINI_MODEL.into())
}

pub fn normalize_gemini_model(model: &str) -> String {
    match model.trim() {
        "gemini-2.0-flash" | "gemini-2.0-flash-001" | "gemini-2.0-flash-lite"
        | "gemini-2.0-flash-lite-001" => DEFAULT_GEMINI_MODEL.into(),
        "gemini-1.5-flash" | "gemini-1.5-flash-8b" | "gemini-1.5-flash-latest" | "gemini-1.5-pro"
        | "gemini-1.5-pro-latest" => "gemini-2.5-flash".into(),
        other => other.to_string(),
    }
}

pub fn mask_token(token: &str) -> String {
    if token.len() <= 8 {
        return "••••••••".to_string();
    }
    format!("{}…{}", &token[..4], &token[token.len() - 4..])
}

pub fn normalize_cursor_api_key(api_key: &str) -> Result<String, String> {
    let key = api_key.trim();
    if key.is_empty() {
        return Err("API key required".into());
    }
    if key.len() < 16 {
        return Err(
            "Paste the full User API Key from cursor.com/dashboard/integrations".into(),
        );
    }
    Ok(key.to_string())
}

pub fn cursor_auth_error(err: &str) -> Option<String> {
    let lower = err.to_ascii_lowercase();
    if lower.contains("invalid user api key")
        || lower.contains("invalid api key")
        || lower.contains("unauthorized")
        || lower.contains("authentication failed")
    {
        Some(
            "Invalid API key — paste a fresh User API Key from cursor.com/dashboard/integrations"
                .into(),
        )
    } else {
        None
    }
}
