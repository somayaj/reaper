//! Serialize Maven classpath tooling per reactor (one `mvn` chain at a time).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use anyhow::Result;

static REACTOR_LOCKS: OnceLock<Mutex<HashMap<String, Arc<Mutex<()>>>>> = OnceLock::new();

fn locks() -> &'static Mutex<HashMap<String, Arc<Mutex<()>>>> {
    REACTOR_LOCKS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Maven reactor root for locking, or `module_or_reactor` when standalone.
pub fn reactor_lock_anchor(module_or_reactor: &Path) -> PathBuf {
    super::maven::find_maven_reactor_root(module_or_reactor)
        .unwrap_or_else(|| module_or_reactor.to_path_buf())
}

fn lock_for(anchor: &Path) -> Arc<Mutex<()>> {
    let key = anchor
        .canonicalize()
        .unwrap_or_else(|_| anchor.to_path_buf())
        .display()
        .to_string();
    let mut map = locks().lock().expect("maven reactor locks");
    map.entry(key)
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone()
}

/// Run Maven classpath work for a module while holding its reactor lock.
pub fn with_reactor_lock<T, F: FnOnce() -> Result<T>>(
    module_or_reactor: &Path,
    f: F,
) -> Result<T> {
    let anchor = reactor_lock_anchor(module_or_reactor);
    let lock = lock_for(&anchor);
    let _guard = lock.lock().expect("maven reactor lock poisoned");
    f()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standalone_module_is_own_anchor() {
        let dir = std::env::temp_dir().join("reaper-maven-lock-anchor");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("pom.xml"), "<project><artifactId>x</artifactId></project>").unwrap();
        assert_eq!(
            reactor_lock_anchor(&dir).canonicalize().unwrap(),
            dir.canonicalize().unwrap()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
