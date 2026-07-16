use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct Config {
    pub host: String,
    pub port: u16,
    pub data_dir: PathBuf,
    pub static_dir: PathBuf,
    pub repos_dir: PathBuf,
    pub workspaces_dir: PathBuf,
    pub metadata_dir: PathBuf,
    pub settings_path: PathBuf,
    pub ui_preferences_path: PathBuf,
    /// App bundle GUI uses plain HTTP; headless `--server` uses TLS + HTTP/2.
    pub uses_tls: bool,
}

impl Config {
    pub fn from_env() -> Self {
        let data_dir = Config::resolve_data_dir();
        let static_dir = resolve_static_dir();

        Self {
            host: std::env::var("REAPER_HOST").unwrap_or_else(|_| "127.0.0.1".into()),
            port: std::env::var("REAPER_PORT")
                .ok()
                .and_then(|p| p.parse().ok())
                .unwrap_or(crate::port::AUTO_PORT),
            repos_dir: data_dir.join("repos"),
            workspaces_dir: data_dir.join("workspaces"),
            metadata_dir: data_dir.join("metadata"),
            settings_path: data_dir.join("settings.json"),
            ui_preferences_path: data_dir.join("ui-preferences.json"),
            uses_tls: !running_in_app_bundle(),
            data_dir,
            static_dir,
        }
    }

    pub fn repo_path(&self, name: &str) -> PathBuf {
        self.repos_dir.join(format!("{name}.git"))
    }

    pub fn workspace_path(&self, name: &str) -> PathBuf {
        self.workspaces_dir.join(name)
    }

    pub fn metadata_path(&self, name: &str) -> PathBuf {
        self.metadata_dir.join(format!("{name}.json"))
    }

    pub fn ensure_dirs(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.repos_dir)?;
        std::fs::create_dir_all(&self.workspaces_dir)?;
        std::fs::create_dir_all(&self.metadata_dir)?;
        Ok(())
    }

    pub fn base_url(&self) -> String {
        let scheme = if self.uses_tls { "https" } else { "http" };
        format!("{scheme}://{}:{}", self.host, self.port)
    }

    pub fn clone_url(&self, name: &str) -> String {
        format!("{}/git/{}.git", self.base_url(), name)
    }

    /// Accepts `repo` or `org/repo` (GitHub-style).
    pub fn is_valid_repo_name(name: &str) -> bool {
        let parts: Vec<&str> = name.split('/').collect();
        match parts.len() {
            1 => valid_segment(parts[0]),
            2 => valid_segment(parts[0]) && valid_segment(parts[1]),
            _ => false,
        }
    }

    pub fn repo_exists(&self, name: &str) -> bool {
        is_bare_repo(&self.repo_path(name))
    }

    /// Default data root: `~/reaper`, or `REAPER_DATA_DIR` when set.
    pub fn resolve_data_dir() -> PathBuf {
        std::env::var("REAPER_DATA_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| default_data_dir())
    }

    /// Server log file: `{data_dir}/reaper.log`
    pub fn resolve_log_path() -> PathBuf {
        Self::resolve_data_dir().join("reaper.log")
    }
}

fn default_data_dir() -> PathBuf {
    home_dir()
        .map(|home| home.join("reaper"))
        .unwrap_or_else(|| PathBuf::from("./data"))
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

fn resolve_static_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("REAPER_STATIC_DIR") {
        return PathBuf::from(dir);
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            // macOS .app bundle
            let bundled = dir.join("../Resources/static");
            if bundled.join("index.html").is_file() {
                return bundled.canonicalize().unwrap_or(bundled);
            }
            // Windows / portable: static/ next to the executable
            let beside = dir.join("static");
            if beside.join("index.html").is_file() {
                return beside.canonicalize().unwrap_or(beside);
            }
        }
    }

    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("static");
    if manifest.join("index.html").is_file() {
        return manifest;
    }

    PathBuf::from("static")
}

/// True when running from a packaged desktop install (.app or portable Windows folder).
pub fn running_in_app_bundle() -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.canonicalize().ok())
        .map(|p| {
            let s = p.to_string_lossy();
            if s.contains(".app/Contents/MacOS/") {
                return true;
            }
            // Portable Windows layout: reaper.exe beside static/index.html
            p.parent()
                .map(|dir| dir.join("static").join("index.html").is_file())
                .unwrap_or(false)
        })
        .unwrap_or(false)
}

/// Node.js shipped with the desktop package (Cursor agent bridge; not the host install).
pub fn bundled_node() -> Option<PathBuf> {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            // Windows portable: node/node.exe next to reaper.exe
            for candidate in [
                dir.join("node").join("node.exe"),
                dir.join("node.exe"),
            ] {
                if candidate.is_file() {
                    return Some(candidate.canonicalize().unwrap_or(candidate));
                }
            }
            // macOS .app: Resources/node…/bin/node
            let resources = dir.join("../Resources");
            let arch = crate::platform::macos_host_arch();
            for candidate in [
                resources.join(format!("node-{arch}/bin/node")),
                resources.join("node/bin/node"),
            ] {
                if candidate.is_file() {
                    return Some(candidate.canonicalize().unwrap_or(candidate));
                }
            }
        }
    }
    None
}

/// Temurin JDK 21 shipped inside Reaper.app (runs jdtls only — not the project JDK).
pub fn bundled_jdtls_java_home() -> Option<PathBuf> {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(mac_os) = exe.parent() {
            let resources = mac_os.join("../Resources");
            let arch = crate::platform::macos_host_arch();
            for candidate in [
                resources.join(format!("jdk-21-{arch}/Contents/Home")),
                resources.join("jdk-21/Contents/Home"),
            ] {
                if candidate.join("bin/java").is_file() {
                    return Some(candidate.canonicalize().unwrap_or(candidate));
                }
            }
        }
    }
    dev_bundled_jdtls_java_home()
}

fn dev_bundled_jdtls_java_home() -> Option<PathBuf> {
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources");
    let arch = crate::platform::macos_host_arch();
    let home = base.join(format!("jdk-macos-{arch}/Contents/Home"));
    if home.join("bin/java").is_file() {
        Some(home)
    } else {
        None
    }
}

/// Eclipse JDT Language Server shipped inside Reaper.app (Java navigation).
pub fn bundled_jdtls() -> Option<PathBuf> {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(mac_os) = exe.parent() {
            let bundled = mac_os.join("../Resources/jdtls/bin/jdtls");
            if bundled.is_file() {
                return Some(bundled.canonicalize().unwrap_or(bundled));
            }
        }
    }
    let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("resources/jdtls/bin/jdtls");
    if dev.is_file() {
        return Some(dev);
    }
    None
}

/// Root directory for debug adapters shipped inside Reaper.app (or dev resources).
pub fn bundled_debug_adapters_dir() -> Option<PathBuf> {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(mac_os) = exe.parent() {
            let resources = mac_os.join("../Resources");
            let arch = crate::platform::macos_host_arch();
            for candidate in [
                resources.join(format!("debug-adapters-{arch}")),
                resources.join("debug-adapters"),
            ] {
                if candidate.join("js-debug").is_dir() {
                    return Some(candidate.canonicalize().unwrap_or(candidate));
                }
            }
        }
    }
    let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
        "resources/debug-adapters-macos-{}",
        crate::platform::macos_host_arch()
    ));
    if dev.join("js-debug").is_dir() {
        return Some(dev);
    }
    None
}

pub fn bundled_js_debug_dap() -> Option<PathBuf> {
    bundled_debug_adapters_dir().map(|d| d.join("js-debug/src/dapDebugServer.js"))
}

pub fn bundled_delve() -> Option<PathBuf> {
    bundled_debug_adapters_dir().map(|d| d.join("delve/bin/dlv"))
}

pub fn bundled_codelldb() -> Option<PathBuf> {
    bundled_debug_adapters_dir().map(|d| d.join("codelldb/adapter/codelldb"))
}

pub fn bundled_debugpy_dir() -> Option<PathBuf> {
    bundled_debug_adapters_dir().and_then(|d| {
        let dir = d.join("debugpy");
        if dir.join("debugpy").is_dir() {
            Some(dir)
        } else {
            None
        }
    })
}

pub fn bundled_java_debug_plugin_jar() -> Option<PathBuf> {
    let dir = bundled_debug_adapters_dir()?.join("java-debug/server");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return None;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = path.file_name()?.to_string_lossy();
        if name.starts_with("com.microsoft.java.debug.plugin-") && name.ends_with(".jar") {
            return Some(path);
        }
    }
    None
}

fn valid_segment(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
}

pub fn is_bare_repo(path: &std::path::Path) -> bool {
    path.is_dir() && path.join("HEAD").exists() && path.join("objects").exists()
}

pub fn repo_name_from_path(base: &std::path::Path, path: &std::path::Path) -> Option<String> {
    if !is_bare_repo(path) {
        return None;
    }
    let rel = path.strip_prefix(base).ok()?;
    let mut name = rel.to_string_lossy().replace('\\', "/");
    if let Some(stripped) = name.strip_suffix(".git") {
        name = stripped.to_string();
    }
    if name.is_empty() || !Config::is_valid_repo_name(&name) {
        return None;
    }
    Some(name)
}

pub fn discover_repos(repos_dir: &std::path::Path) -> std::io::Result<Vec<(String, std::path::PathBuf)>> {
    let mut found = Vec::new();
    discover_repos_inner(repos_dir, repos_dir, &mut found)?;
    found.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(found)
}

fn discover_repos_inner(
    base: &std::path::Path,
    dir: &std::path::Path,
    out: &mut Vec<(String, std::path::PathBuf)>,
) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if is_bare_repo(&path) {
            if let Some(name) = repo_name_from_path(base, &path) {
                out.push((name, path));
            }
        } else {
            discover_repos_inner(base, &path, out)?;
        }
    }
    Ok(())
}
