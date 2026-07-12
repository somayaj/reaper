use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

const GEMINI_SETTINGS_KEY: &str = "__gemini__";
const CURSOR_SETTINGS_KEY: &str = "__cursor__";
const ANTHROPIC_SETTINGS_KEY: &str = "__anthropic__";
const BEDROCK_SETTINGS_KEY: &str = "__bedrock__";
const DEFAULT_GEMINI_MODEL: &str = "gemini-3.5-flash";
const DEFAULT_ANTHROPIC_MODEL: &str = "claude-sonnet-4-5";
const DEFAULT_BEDROCK_MODEL: &str = "anthropic.claude-3-5-sonnet-20241022-v2:0";
const DEFAULT_BEDROCK_REGION: &str = "us-east-1";

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
    anthropic_model: Option<String>,
    /// "api" | "bedrock"
    #[serde(default)]
    anthropic_backend: Option<String>,
    #[serde(default)]
    bedrock_region: Option<String>,
    #[serde(default)]
    bedrock_model_id: Option<String>,
    #[serde(default)]
    jdk_home: Option<String>,
    /// Explicit Java language level for editor javac when the project does not declare one.
    #[serde(default)]
    java_release: Option<u32>,
    #[serde(default)]
    toolchain_paths: HashMap<String, String>,
    /// Repos hidden from the IDE list (bare repo data is kept on disk).
    #[serde(default)]
    hidden_repos: Vec<String>,
    /// Repository opened automatically on startup when no URL repo is set.
    #[serde(default)]
    default_repo: Option<String>,
    /// Most recently opened repository (resume on next launch).
    #[serde(default)]
    last_repo: Option<String>,
    /// Java dependency index: "standard" (2000 JARs + background) or "light" (400 JARs).
    #[serde(default)]
    java_index_mode: Option<String>,
    /// Periodically fetch remotes while a repo is open (ahead/behind in header). Off by default.
    #[serde(default)]
    git_background_fetch: Option<bool>,
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
pub struct AnthropicSettingsView {
    pub configured: bool,
    /// Anthropic API key present (Claude agent tab).
    pub api_configured: bool,
    /// Bedrock Mantle key or AWS IAM credentials present (Bedrock agent tab).
    pub bedrock_configured: bool,
    pub masked: Option<String>,
    pub model: String,
    pub backend: String,
    pub bedrock_region: String,
    pub bedrock_model_id: String,
    pub bedrock_masked: Option<String>,
    pub source: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct GeneralSettingsView {
    pub default_repo: Option<String>,
    pub last_repo: Option<String>,
    pub java_index_mode: String,
    pub git_background_fetch: bool,
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
            store.sync_java_compiler_cache();
            store.sync_toolchain_cache();
            store
        })
    }

    fn sync_java_compiler_cache(&self) {
        self.sync_java_home_cache();
        let release = self
            .inner
            .read()
            .ok()
            .and_then(|g| g.java_release);
        crate::jdk::set_configured_java_release(release);
    }

    fn sync_toolchain_cache(&self) {
        let mut map = HashMap::new();
        if let Ok(guard) = self.inner.read() {
            for (id, path) in &guard.toolchain_paths {
                if path.is_empty() {
                    continue;
                }
                match crate::toolchain::validate_tool_path(id, path) {
                    Ok(p) => {
                        map.insert(id.clone(), p);
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Ignoring invalid compiler {}={}: {e:#}",
                            id,
                            path
                        );
                    }
                }
            }
        }
        crate::toolchain::set_configured_tools(map);
    }

    fn sync_java_home_cache(&self) {
        let home = self
            .inner
            .read()
            .ok()
            .and_then(|g| g.jdk_home.clone())
            .filter(|h| !h.is_empty())
            .map(PathBuf::from)
            .filter(|p| {
                if crate::jdk::validate_java_home(p).is_ok() {
                    return true;
                }
                tracing::warn!(
                    "Ignoring invalid configured JAVA_HOME {} — clear it in Settings → Compiler",
                    p.display()
                );
                false
            });
        crate::jdk::set_configured_java_home(home);
    }

    pub fn java_release(&self) -> Option<u32> {
        self.inner
            .read()
            .ok()
            .and_then(|g| g.java_release)
    }

    pub fn set_java_release(&self, release: Option<u32>) -> Result<()> {
        if let Some(v) = release {
            if !(8..=30).contains(&v) {
                anyhow::bail!("java_release must be between 8 and 30");
            }
        }
        let mut guard = self.inner.write().expect("settings lock poisoned");
        guard.java_release = release;
        self.save(&guard)?;
        self.sync_java_compiler_cache();
        Ok(())
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
        self.sync_java_compiler_cache();
        Ok(())
    }

    pub fn clear_java_home(&self) -> Result<bool> {
        let mut guard = self.inner.write().expect("settings lock poisoned");
        let removed = guard.jdk_home.take().is_some();
        if removed {
            self.save(&guard)?;
        }
        self.sync_java_compiler_cache();
        Ok(removed)
    }

    pub fn jdk_view(&self) -> crate::jdk::JdkSettingsView {
        let from_env = std::env::var("REAPER_JAVA_HOME")
            .ok()
            .filter(|h| !h.is_empty());

        let guard = self.inner.read().ok();
        let from_file = guard
            .as_ref()
            .and_then(|g| g.jdk_home.clone())
            .filter(|h| !h.is_empty());
        let java_release = guard.as_ref().and_then(|g| g.java_release);

        let (_configured, java_home, source) = if let Some(home) = from_file {
            (true, Some(home), Some("settings".to_string()))
        } else if let Some(home) = from_env {
            (true, Some(home), Some("env:REAPER_JAVA_HOME".to_string()))
        } else {
            (false, None, None)
        };

        crate::jdk::jdk_settings_view(java_home.as_deref(), source.as_deref(), java_release)
    }

    pub fn compilers_view(&self) -> crate::toolchain::CompilersView {
        let from_env = std::env::var("REAPER_JAVA_HOME")
            .ok()
            .filter(|h| !h.is_empty());

        let from_file = self
            .inner
            .read()
            .ok()
            .and_then(|g| g.jdk_home.clone())
            .filter(|h| !h.is_empty());

        let (java_home, java_source) = if let Some(home) = from_file {
            (Some(home), Some("settings".to_string()))
        } else if let Some(home) = from_env {
            (Some(home), Some("env:REAPER_JAVA_HOME".to_string()))
        } else {
            (None, None)
        };

        let configured = self
            .inner
            .read()
            .ok()
            .map(|g| g.toolchain_paths.clone())
            .unwrap_or_default();

        crate::toolchain::compilers_view(&configured, java_home.as_deref(), java_source.as_deref())
    }

    pub fn toolchains_view(&self) -> crate::toolchain::CompilersView {
        self.compilers_view()
    }

    pub fn set_toolchain_path(&self, id: &str, path: String) -> Result<()> {
        let path = path.trim().to_string();
        if path.is_empty() {
            anyhow::bail!("path required");
        }
        if id == "java" {
            return self.set_java_home(path);
        }
        crate::toolchain::validate_tool_path(id, &path)?;
        let mut guard = self.inner.write().expect("settings lock poisoned");
        guard.toolchain_paths.insert(id.to_string(), path);
        self.save(&guard)?;
        self.sync_toolchain_cache();
        Ok(())
    }

    pub fn clear_toolchain_path(&self, id: &str) -> Result<bool> {
        if id == "java" {
            return self.clear_java_home();
        }
        let mut guard = self.inner.write().expect("settings lock poisoned");
        let removed = guard.toolchain_paths.remove(id).is_some();
        if removed {
            self.save(&guard)?;
        }
        self.sync_toolchain_cache();
        Ok(removed)
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
        for key in token_host_lookup_keys(host) {
            if let Some(token) = self.token_from_settings(&key) {
                return Some(token);
            }
            if let Ok(env_token) = std::env::var(env_key_for_host(&key)) {
                if !env_token.is_empty() {
                    return Some(env_token);
                }
            }
        }

        self.token_from_settings("*").or_else(|| {
            std::env::var("REAPER_PAT")
                .ok()
                .filter(|t| !t.is_empty())
        })
    }

    fn token_from_settings(&self, host: &str) -> Option<String> {
        if host == GEMINI_SETTINGS_KEY
            || host == CURSOR_SETTINGS_KEY
            || host == ANTHROPIC_SETTINGS_KEY
            || host == BEDROCK_SETTINGS_KEY
        {
            return None;
        }
        let guard = self.inner.read().ok()?;
        let token = guard.tokens.get(host)?;
        if token.is_empty() {
            return None;
        }
        Some(token.clone())
    }

    pub fn has_token_for_host(&self, host: &str) -> bool {
        self.token_for_host(host).is_some()
    }

    pub fn is_repo_hidden(&self, name: &str) -> bool {
        self.inner
            .read()
            .expect("settings lock poisoned")
            .hidden_repos
            .iter()
            .any(|n| n == name)
    }

    pub fn hide_repo(&self, name: &str) -> Result<()> {
        let mut guard = self.inner.write().expect("settings lock poisoned");
        if guard.hidden_repos.iter().any(|n| n == name) {
            return Ok(());
        }
        guard.hidden_repos.push(name.to_string());
        self.save(&guard)?;
        Ok(())
    }

    pub fn show_repo(&self, name: &str) -> Result<()> {
        let mut guard = self.inner.write().expect("settings lock poisoned");
        let before = guard.hidden_repos.len();
        guard.hidden_repos.retain(|n| n != name);
        if guard.hidden_repos.len() != before {
            self.save(&guard)?;
        }
        Ok(())
    }

    pub fn general_view(&self) -> GeneralSettingsView {
        let (default_repo, last_repo, java_index_mode, git_background_fetch) = self
            .inner
            .read()
            .ok()
            .map(|guard| {
                (
                    guard
                        .default_repo
                        .clone()
                        .filter(|name| !name.is_empty()),
                    guard
                        .last_repo
                        .clone()
                        .filter(|name| !name.is_empty()),
                    guard
                        .java_index_mode
                        .clone()
                        .filter(|m| !m.is_empty())
                        .unwrap_or_else(|| "lazy".into()),
                    guard.git_background_fetch.unwrap_or(false),
                )
            })
            .unwrap_or_else(|| (None, None, "lazy".into(), false));
        GeneralSettingsView {
            default_repo,
            last_repo,
            java_index_mode,
            git_background_fetch,
        }
    }

    /// Repo to prefetch on startup: explicit default, else last opened.
    pub fn prefetch_repo(&self) -> Option<String> {
        self.inner.read().ok().and_then(|guard| {
            guard
                .default_repo
                .clone()
                .filter(|name| !name.is_empty())
                .or_else(|| guard.last_repo.clone().filter(|name| !name.is_empty()))
        })
    }

    pub fn set_last_repo(&self, name: &str) -> Result<()> {
        let name = name.trim();
        if name.is_empty() {
            return Ok(());
        }
        let mut guard = self.inner.write().expect("settings lock poisoned");
        guard.last_repo = Some(name.to_string());
        self.save(&guard)
    }

    pub fn set_java_index_mode(&self, mode: &str) -> Result<()> {
        let mode = mode.trim();
        if mode != "standard" && mode != "light" && mode != "lazy" {
            anyhow::bail!("java_index_mode must be \"standard\", \"light\", or \"lazy\"");
        }
        let mut guard = self.inner.write().expect("settings lock poisoned");
        guard.java_index_mode = Some(mode.to_string());
        self.save(&guard)
    }

    pub fn set_git_background_fetch(&self, enabled: bool) -> Result<()> {
        let mut guard = self.inner.write().expect("settings lock poisoned");
        guard.git_background_fetch = Some(enabled);
        self.save(&guard)
    }

    pub fn set_default_repo(&self, name: Option<String>) -> Result<()> {
        let mut guard = self.inner.write().expect("settings lock poisoned");
        guard.default_repo = name.filter(|n| !n.trim().is_empty());
        self.save(&guard)
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
                if host == GEMINI_SETTINGS_KEY
                    || host == CURSOR_SETTINGS_KEY
                    || host == ANTHROPIC_SETTINGS_KEY
                    || host == BEDROCK_SETTINGS_KEY
                {
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

    pub fn anthropic_api_key(&self) -> Option<String> {
        if let Ok(guard) = self.inner.read() {
            if let Some(key) = guard.tokens.get(ANTHROPIC_SETTINGS_KEY) {
                if !key.is_empty() {
                    return Some(key.clone());
                }
            }
        }
        std::env::var("REAPER_ANTHROPIC_API_KEY")
            .ok()
            .filter(|k| !k.is_empty())
    }

    pub fn bedrock_api_key(&self) -> Option<String> {
        if let Ok(guard) = self.inner.read() {
            if let Some(key) = guard.tokens.get(BEDROCK_SETTINGS_KEY) {
                if !key.is_empty() {
                    return Some(key.clone());
                }
            }
        }
        std::env::var("REAPER_BEDROCK_API_KEY")
            .ok()
            .filter(|k| !k.is_empty())
    }

    /// Returns `"api"` or `"bedrock"`.
    pub fn anthropic_backend(&self) -> String {
        if let Ok(guard) = self.inner.read() {
            if let Some(backend) = &guard.anthropic_backend {
                let b = backend.trim().to_ascii_lowercase();
                if b == "bedrock" {
                    return "bedrock".into();
                }
                if b == "api" {
                    return "api".into();
                }
            }
        }
        match std::env::var("REAPER_ANTHROPIC_BACKEND")
            .ok()
            .map(|s| s.trim().to_ascii_lowercase())
            .as_deref()
        {
            Some("bedrock") => "bedrock".into(),
            _ => "api".into(),
        }
    }

    pub fn anthropic_model(&self) -> String {
        let stored = self
            .inner
            .read()
            .ok()
            .and_then(|g| g.anthropic_model.clone())
            .filter(|m| !m.is_empty())
            .or_else(|| std::env::var("REAPER_ANTHROPIC_MODEL").ok())
            .filter(|m| !m.is_empty())
            .unwrap_or_else(|| DEFAULT_ANTHROPIC_MODEL.into());
        normalize_anthropic_model(&stored)
    }

    pub fn bedrock_model_id(&self) -> String {
        let stored = self
            .inner
            .read()
            .ok()
            .and_then(|g| g.bedrock_model_id.clone())
            .filter(|m| !m.is_empty())
            .or_else(|| std::env::var("REAPER_BEDROCK_MODEL").ok())
            .filter(|m| !m.is_empty())
            .unwrap_or_else(|| DEFAULT_BEDROCK_MODEL.into());
        normalize_bedrock_model(&stored)
    }

    pub fn bedrock_region(&self) -> String {
        self.inner
            .read()
            .ok()
            .and_then(|g| g.bedrock_region.clone())
            .filter(|r| !r.is_empty())
            .or_else(|| std::env::var("REAPER_BEDROCK_REGION").ok())
            .filter(|r| !r.is_empty())
            .unwrap_or_else(|| DEFAULT_BEDROCK_REGION.into())
    }

    pub fn set_anthropic_api_key(&self, api_key: String) -> Result<()> {
        let mut guard = self.inner.write().expect("settings lock poisoned");
        guard
            .tokens
            .insert(ANTHROPIC_SETTINGS_KEY.to_string(), api_key);
        self.save(&guard)
    }

    pub fn clear_anthropic_api_key(&self) -> Result<bool> {
        let mut guard = self.inner.write().expect("settings lock poisoned");
        let removed = guard.tokens.remove(ANTHROPIC_SETTINGS_KEY).is_some();
        if removed {
            self.save(&guard)?;
        }
        Ok(removed)
    }

    pub fn set_bedrock_api_key(&self, api_key: String) -> Result<()> {
        let mut guard = self.inner.write().expect("settings lock poisoned");
        guard
            .tokens
            .insert(BEDROCK_SETTINGS_KEY.to_string(), api_key);
        self.save(&guard)
    }

    pub fn clear_bedrock_api_key(&self) -> Result<bool> {
        let mut guard = self.inner.write().expect("settings lock poisoned");
        let removed = guard.tokens.remove(BEDROCK_SETTINGS_KEY).is_some();
        if removed {
            self.save(&guard)?;
        }
        Ok(removed)
    }

    pub fn set_anthropic_model(&self, model: String) -> Result<()> {
        let mut guard = self.inner.write().expect("settings lock poisoned");
        guard.anthropic_model = Some(normalize_anthropic_model(&model));
        self.save(&guard)
    }

    pub fn set_anthropic_backend(&self, backend: String) -> Result<()> {
        let backend = backend.trim().to_ascii_lowercase();
        if backend != "api" && backend != "bedrock" {
            anyhow::bail!("backend must be \"api\" or \"bedrock\"");
        }
        let mut guard = self.inner.write().expect("settings lock poisoned");
        guard.anthropic_backend = Some(backend);
        self.save(&guard)
    }

    pub fn set_bedrock_region(&self, region: String) -> Result<()> {
        let region = region.trim().to_string();
        if region.is_empty() {
            anyhow::bail!("bedrock_region required");
        }
        let mut guard = self.inner.write().expect("settings lock poisoned");
        guard.bedrock_region = Some(region);
        self.save(&guard)
    }

    pub fn set_bedrock_model_id(&self, model_id: String) -> Result<()> {
        let mut guard = self.inner.write().expect("settings lock poisoned");
        guard.bedrock_model_id = Some(normalize_bedrock_model(&model_id));
        self.save(&guard)
    }

    pub fn claude_api_configured(&self) -> bool {
        self.anthropic_api_key().is_some()
    }

    pub fn bedrock_configured(&self) -> bool {
        self.bedrock_api_key().is_some()
            || std::env::var("AWS_ACCESS_KEY_ID")
                .ok()
                .filter(|k| !k.is_empty())
                .is_some()
            || std::env::var("AWS_PROFILE")
                .ok()
                .filter(|p| !p.is_empty())
                .is_some()
    }

    /// True when either Claude API or Bedrock credentials are available.
    pub fn anthropic_configured(&self) -> bool {
        self.claude_api_configured() || self.bedrock_configured()
    }

    pub fn anthropic_view(&self) -> AnthropicSettingsView {
        let from_env = std::env::var("REAPER_ANTHROPIC_API_KEY")
            .ok()
            .filter(|k| !k.is_empty());
        let from_file = self
            .inner
            .read()
            .ok()
            .and_then(|g| g.tokens.get(ANTHROPIC_SETTINGS_KEY).cloned())
            .filter(|k| !k.is_empty());

        let (masked, source) = if let Some(key) = from_file {
            (Some(mask_token(&key)), Some("settings".into()))
        } else if let Some(key) = from_env {
            (
                Some(mask_token(&key)),
                Some("env:REAPER_ANTHROPIC_API_KEY".into()),
            )
        } else {
            (None, None)
        };

        let bedrock_from_env = std::env::var("REAPER_BEDROCK_API_KEY")
            .ok()
            .filter(|k| !k.is_empty());
        let bedrock_from_file = self
            .inner
            .read()
            .ok()
            .and_then(|g| g.tokens.get(BEDROCK_SETTINGS_KEY).cloned())
            .filter(|k| !k.is_empty());
        let bedrock_masked = bedrock_from_file
            .as_ref()
            .or(bedrock_from_env.as_ref())
            .map(|k| mask_token(k));

        AnthropicSettingsView {
            configured: self.anthropic_configured(),
            api_configured: self.claude_api_configured(),
            bedrock_configured: self.bedrock_configured(),
            masked,
            model: self.anthropic_model(),
            backend: self.anthropic_backend(),
            bedrock_region: self.bedrock_region(),
            bedrock_model_id: self.bedrock_model_id(),
            bedrock_masked,
            source,
        }
    }
}

fn env_key_for_host(host: &str) -> String {
    format!(
        "REAPER_PAT_{}",
        host.replace('.', "_").replace('-', "_").to_uppercase()
    )
}

/// Host keys to try when resolving a PAT (aliases such as www.github.com → github.com).
fn token_host_lookup_keys(host: &str) -> Vec<String> {
    let trimmed = host.trim().to_lowercase();
    let base = trimmed.split(':').next().unwrap_or(&trimmed).to_string();
    let mut keys = vec![base.clone()];
    if base == "www.github.com" {
        keys.push("github.com".into());
    }
    keys.sort();
    keys.dedup();
    keys
}

#[cfg(test)]
mod token_host_tests {
    use super::token_host_lookup_keys;

    #[test]
    fn github_www_alias() {
        assert_eq!(
            token_host_lookup_keys("www.github.com"),
            vec!["github.com".to_string(), "www.github.com".to_string()]
        );
    }

    #[test]
    fn strips_port() {
        assert_eq!(
            token_host_lookup_keys("github.com:443"),
            vec!["github.com".to_string()]
        );
    }
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

pub fn normalize_anthropic_model(model: &str) -> String {
    let trimmed = model.trim();
    if trimmed.is_empty() {
        DEFAULT_ANTHROPIC_MODEL.into()
    } else {
        trimmed.to_string()
    }
}

pub fn normalize_bedrock_model(model: &str) -> String {
    let trimmed = model.trim();
    if trimmed.is_empty() {
        DEFAULT_BEDROCK_MODEL.into()
    } else {
        trimmed.to_string()
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

pub fn cursor_model_error(err: &str) -> Option<String> {
    if err.contains("isn't available for your Cursor API key") {
        return Some(err.into());
    }
    let lower = err.to_ascii_lowercase();
    if lower.contains("model") {
        if lower.contains("not found")
            || lower.contains("unsupported")
            || lower.contains("not available")
            || lower.contains("invalid model")
            || lower.contains("unknown model")
        {
            return Some(
                "That model isn't available for your Cursor API key — choose a supported model in the agent panel."
                    .into(),
            );
        }
    }
    None
}

pub fn cursor_agent_error(err: &str) -> Option<String> {
    cursor_auth_error(err).or_else(|| cursor_model_error(err))
}
