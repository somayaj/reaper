use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiPreferences {
    #[serde(default = "default_true")]
    pub coverage_inline_enabled: bool,
}

impl Default for UiPreferences {
    fn default() -> Self {
        Self {
            coverage_inline_enabled: true,
        }
    }
}

#[derive(Clone)]
pub struct UiPreferencesStore {
    path: Arc<PathBuf>,
    inner: Arc<RwLock<UiPreferences>>,
}

impl UiPreferencesStore {
    pub fn load(path: &Path) -> Result<Self> {
        let prefs = if path.exists() {
            let raw = fs::read_to_string(path)
                .with_context(|| format!("read ui preferences {}", path.display()))?;
            serde_json::from_str(&raw).unwrap_or_default()
        } else {
            UiPreferences::default()
        };

        Ok(Self {
            path: Arc::from(path.to_path_buf()),
            inner: Arc::new(RwLock::new(prefs)),
        })
    }

    pub fn view(&self) -> UiPreferences {
        self.inner
            .read()
            .expect("ui preferences lock poisoned")
            .clone()
    }

    pub fn set_coverage_inline_enabled(&self, enabled: bool) -> Result<UiPreferences> {
        let mut guard = self.inner.write().expect("ui preferences lock poisoned");
        guard.coverage_inline_enabled = enabled;
        self.save(&guard)?;
        Ok(guard.clone())
    }

    fn save(&self, prefs: &UiPreferences) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let raw = serde_json::to_string_pretty(prefs)?;
        fs::write(self.path.as_ref(), raw)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saves_and_loads_coverage_inline_pref() {
        let path = std::env::temp_dir().join(format!(
            "reaper-ui-prefs-test-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = UiPreferencesStore::load(&path).unwrap();
        assert!(store.view().coverage_inline_enabled);

        store.set_coverage_inline_enabled(false).unwrap();
        let reloaded = UiPreferencesStore::load(&path).unwrap();
        assert!(!reloaded.view().coverage_inline_enabled);
        let _ = fs::remove_file(path);
    }
}
