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
}

impl Config {
    pub fn from_env() -> Self {
        let data_dir = std::env::var("REAPER_DATA_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| default_data_dir());
        let static_dir = resolve_static_dir();

        Self {
            host: std::env::var("REAPER_HOST").unwrap_or_else(|_| "127.0.0.1".into()),
            port: std::env::var("REAPER_PORT")
                .ok()
                .and_then(|p| p.parse().ok())
                .unwrap_or(8080),
            repos_dir: data_dir.join("repos"),
            workspaces_dir: data_dir.join("workspaces"),
            metadata_dir: data_dir.join("metadata"),
            settings_path: data_dir.join("settings.json"),
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

    pub fn clone_url(&self, name: &str) -> String {
        format!("http://{}:{}/git/{}.git", self.host, self.port, name)
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
}

fn default_data_dir() -> PathBuf {
    if running_in_app_bundle() {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join("Library/Application Support/Reaper");
        }
    }
    PathBuf::from("./data")
}

fn resolve_static_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("REAPER_STATIC_DIR") {
        return PathBuf::from(dir);
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(mac_os) = exe.parent() {
            let bundled = mac_os.join("../Resources/static");
            if bundled.join("index.html").is_file() {
                return bundled.canonicalize().unwrap_or(bundled);
            }
        }
    }

    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("static");
    if manifest.join("index.html").is_file() {
        return manifest;
    }

    PathBuf::from("static")
}

pub fn running_in_app_bundle() -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.canonicalize().ok())
        .map(|p| {
            p.to_string_lossy()
                .contains(".app/Contents/MacOS/")
        })
        .unwrap_or(false)
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
