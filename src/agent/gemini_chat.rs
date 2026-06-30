use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Debug, Clone)]
pub struct ChatTurn {
    pub role: String,
    pub text: String,
}

#[derive(Default)]
pub struct GeminiChatStore {
    inner: Mutex<HashMap<String, Vec<ChatTurn>>>,
}

impl GeminiChatStore {
    pub fn history(&self, repo: &str) -> Vec<ChatTurn> {
        self.inner
            .lock()
            .ok()
            .and_then(|g| g.get(repo).cloned())
            .unwrap_or_default()
    }

    pub fn push(&self, repo: &str, role: &str, text: &str) {
        if text.trim().is_empty() {
            return;
        }
        if let Ok(mut g) = self.inner.lock() {
            g.entry(repo.to_string())
                .or_default()
                .push(ChatTurn {
                    role: role.to_string(),
                    text: text.to_string(),
                });
        }
    }

    pub fn clear(&self, repo: &str) {
        if let Ok(mut g) = self.inner.lock() {
            g.remove(repo);
        }
    }
}
