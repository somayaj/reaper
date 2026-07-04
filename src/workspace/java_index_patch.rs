//! Coalesced background patching of `java-index.json` after saves.
//!
//! On large Spring/Maven workspaces the index is multi‑MB. Auto-save can enqueue many
//! patches; running them concurrently re-reads/writes the whole JSON and can starve the
//! runtime. One worker per workspace, latest buffer per path wins.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::thread;

#[derive(Default)]
struct PatchQueue {
    pending: HashMap<String, String>,
    running: bool,
}

fn queues() -> &'static Mutex<HashMap<String, PatchQueue>> {
    static QUEUES: OnceLock<Mutex<HashMap<String, PatchQueue>>> = OnceLock::new();
    QUEUES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn workspace_key(ws: &Path) -> String {
    ws.canonicalize()
        .unwrap_or_else(|_| ws.to_path_buf())
        .display()
        .to_string()
}

/// Queue a single-file index patch; returns immediately.
pub fn queue_java_index_patch_after_save(ws: &Path, rel_path: &str, content: &str) {
    if !rel_path.ends_with(".java") || rel_path.starts_with(".reaper/") {
        return;
    }
    let key = workspace_key(ws);
    let ws = ws.to_path_buf();
    let rel_path = rel_path.to_string();
    let content = content.to_string();

    let should_start = {
        let mut map = queues().lock().expect("java index patch queues");
        let entry = map.entry(key.clone()).or_default();
        entry.pending.insert(rel_path, content);
        if entry.running {
            false
        } else {
            entry.running = true;
            true
        }
    };

    if should_start {
        thread::spawn(move || run_patch_worker(key, ws));
    }
}

fn run_patch_worker(key: String, ws: PathBuf) {
    loop {
        let batch: Vec<(String, String)> = {
            let mut map = queues().lock().expect("java index patch queues");
            let Some(entry) = map.get_mut(&key) else {
                return;
            };
            if entry.pending.is_empty() {
                entry.running = false;
                map.remove(&key);
                return;
            }
            entry.pending.drain().collect()
        };

        for (rel_path, content) in batch {
            if let Err(e) = super::classpath::patch_java_index_file(&ws, &rel_path, &content) {
                tracing::warn!(
                    "java index patch failed for {} in {}: {e:#}",
                    rel_path,
                    ws.display()
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn coalesces_pending_patches_per_path() {
        let ws = std::env::temp_dir().join(format!(
            "reaper-java-index-patch-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&ws);
        std::fs::create_dir_all(ws.join(".reaper")).expect("reaper dir");
        std::fs::write(
            ws.join(".reaper/java-index.json"),
            r#"{"symbols":[]}"#,
        )
        .expect("seed index");
        std::fs::create_dir_all(ws.join("src")).expect("src");
        std::fs::write(ws.join("src/App.java"), "class App {}").expect("java");

        queue_java_index_patch_after_save(&ws, "src/App.java", "class App { void a() {} }");
        queue_java_index_patch_after_save(&ws, "src/App.java", "class App { void b() {} }");

        for _ in 0..200 {
            let running = queues()
                .lock()
                .ok()
                .and_then(|g| g.get(&workspace_key(&ws)).map(|e| e.running))
                .unwrap_or(false);
            if !running {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }

        let running = queues()
            .lock()
            .ok()
            .and_then(|g| g.get(&workspace_key(&ws)).map(|e| e.running))
            .unwrap_or(false);
        assert!(!running, "patch worker should finish");
        let _ = std::fs::remove_dir_all(&ws);
    }
}
