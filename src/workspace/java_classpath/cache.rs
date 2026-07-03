use std::path::{Path, PathBuf};

/// Gradle user home (`GRADLE_USER_HOME` or `~/.gradle`).
pub fn gradle_user_home() -> PathBuf {
    std::env::var("GRADLE_USER_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            std::env::var("HOME")
                .map(|h| PathBuf::from(h).join(".gradle"))
                .unwrap_or_else(|_| PathBuf::from(".gradle"))
        })
}

pub fn gradle_files_root() -> PathBuf {
    gradle_user_home().join("caches/modules-2/files-2.1")
}

pub fn find_jar(files_root: &Path, group: &str, artifact: &str, version: &str) -> Option<PathBuf> {
    find_cached_jar(files_root, group, artifact, version)
        .or_else(|| super::super::maven::find_m2_jar(group, artifact, version))
}

pub fn read_pom(files_root: &Path, group: &str, artifact: &str, version: &str) -> Option<String> {
    read_gradle_cached_pom(files_root, group, artifact, version)
        .or_else(|| super::super::maven::read_m2_pom_text(group, artifact, version))
}

pub fn find_cached_jar(files_root: &Path, group: &str, artifact: &str, version: &str) -> Option<PathBuf> {
    let version_dir = files_root.join(group).join(artifact).join(version);
    if !version_dir.is_dir() {
        return None;
    }
    for hash_entry in std::fs::read_dir(&version_dir).into_iter().flatten().flatten() {
        let hash_dir = hash_entry.path();
        if !hash_dir.is_dir() {
            continue;
        }
        for file_entry in std::fs::read_dir(&hash_dir).into_iter().flatten().flatten() {
            let path = file_entry.path();
            if !path.is_file() {
                continue;
            }
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name.ends_with(".jar") && !name.ends_with("-sources.jar") && !name.ends_with("-javadoc.jar") {
                return Some(path);
            }
        }
    }
    None
}

pub fn read_gradle_cached_pom(
    files_root: &Path,
    group: &str,
    artifact: &str,
    version: &str,
) -> Option<String> {
    let version_dir = files_root.join(group).join(artifact).join(version);
    if !version_dir.is_dir() {
        return None;
    }
    let expected = format!("{artifact}-{version}.pom");
    for hash_entry in std::fs::read_dir(&version_dir).into_iter().flatten().flatten() {
        let hash_dir = hash_entry.path();
        if !hash_dir.is_dir() {
            continue;
        }
        let pom = hash_dir.join(&expected);
        if pom.is_file() {
            return std::fs::read_to_string(pom).ok();
        }
        for file_entry in std::fs::read_dir(&hash_dir).into_iter().flatten().flatten() {
            let path = file_entry.path();
            if path.is_file()
                && path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.ends_with(".pom"))
            {
                return std::fs::read_to_string(path).ok();
            }
        }
    }
    None
}
