use std::sync::Arc;

use crate::config::Config;
use crate::cursor::{CursorBridge, SessionStore};
use crate::settings::SettingsStore;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub settings: SettingsStore,
    pub cursor_bridge: Arc<CursorBridge>,
    pub cursor_sessions: Arc<SessionStore>,
}

impl AppState {
    pub fn new(config: Config, settings: SettingsStore) -> Self {
        Self {
            config: Arc::new(config),
            settings,
            cursor_bridge: Arc::new(CursorBridge::new()),
            cursor_sessions: Arc::new(SessionStore::default()),
        }
    }
}
