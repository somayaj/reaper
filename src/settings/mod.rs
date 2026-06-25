use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::{Arc, RwLock};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

const GEMINI_SETTINGS_KEY: &str = "__gemini__";
const CURSOR_SETTINGS_KEY: &str = "__cursor__";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct SettingsFile {
    #[serde(default)]
    tokens: HashMap<String, String>,
    #[serde(default)]
    gemini_model: Option<String>,
    #[serde(default)]
    cursor_model: Option<String>,
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
        if let Ok(guard) = self.inner.read() {
            if let Some(model) = &guard.gemini_model {
                if !model.is_empty() {
                    return model.clone();
                }
            }
        }
        std::env::var("REAPER_GEMINI_MODEL").unwrap_or_else(|_| "gemini-2.0-flash".into())
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
            model: self.gemini_model(),
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
