use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Maven `group:artifact:version` extracted from a cached JAR path.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MavenCoord {
    pub group: String,
    pub artifact: String,
    pub version: String,
}

impl MavenCoord {
    pub fn key(&self) -> String {
        format!("{}:{}:{}", self.group, self.artifact, self.version)
    }
}

/// Lex a Gradle or M2 JAR path into Maven coordinates.
pub fn coord_from_jar_path(path: &Path) -> Option<MavenCoord> {
    let name = path.file_name()?.to_str()?;
    if !name.ends_with(".jar") || name.ends_with("-sources.jar") || name.ends_with("-javadoc.jar") {
        return None;
    }
    let normalized = path.to_string_lossy().replace('\\', "/");
    if let Some(coord) = lex_gradle_cache_path(&normalized, name) {
        return Some(coord);
    }
    lex_m2_path(&normalized, name)
}

pub fn coords_from_jar_paths(paths: &[PathBuf]) -> Vec<(String, String, String)> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for path in paths {
        let Some(coord) = coord_from_jar_path(path) else {
            continue;
        };
        if seen.insert(coord.key()) {
            out.push((coord.group, coord.artifact, coord.version));
        }
    }
    out
}

fn lex_gradle_cache_path(normalized: &str, file_name: &str) -> Option<MavenCoord> {
    let marker = "/files-2.1/";
    let idx = normalized.find(marker)?;
    let rest = &normalized[idx + marker.len()..];
    let mut parts = rest.split('/');
    let group = parts.next()?.trim();
    let artifact = parts.next()?.trim();
    let version = parts.next()?.trim();
    if group.is_empty() || artifact.is_empty() || version.is_empty() {
        return None;
    }
    let coord = MavenCoord {
        group: group.to_string(),
        artifact: artifact.to_string(),
        version: version.to_string(),
    };
    if file_name.starts_with(&format!("{}-{}", coord.artifact, coord.version)) {
        Some(coord)
    } else {
        None
    }
}

fn lex_m2_path(normalized: &str, file_name: &str) -> Option<MavenCoord> {
    let marker = "/repository/";
    let idx = normalized.rfind(marker)?;
    let rest = &normalized[idx + marker.len()..];
    let parent = Path::new(rest).parent()?;
    let version = parent.file_name()?.to_str()?;
    let artifact_dir = parent.parent()?;
    let artifact = artifact_dir.file_name()?.to_str()?;
    if !file_name.starts_with(&format!("{artifact}-{version}")) {
        return None;
    }
    let group_path = artifact_dir.parent()?;
    let group = group_path
        .components()
        .filter_map(|c| c.as_os_str().to_str())
        .collect::<Vec<_>>()
        .join(".");
    if group.is_empty() {
        return None;
    }
    Some(MavenCoord {
        group,
        artifact: artifact.to_string(),
        version: version.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lexes_gradle_cache_jar_path() {
        let path = PathBuf::from(
            "/home/.gradle/caches/modules-2/files-2.1/com.fasterxml.jackson.core/jackson-databind/2.17.3/abc/jackson-databind-2.17.3.jar",
        );
        let coord = coord_from_jar_path(&path).unwrap();
        assert_eq!(coord.group, "com.fasterxml.jackson.core");
        assert_eq!(coord.artifact, "jackson-databind");
        assert_eq!(coord.version, "2.17.3");
    }

    #[test]
    fn lexes_m2_jar_path() {
        let path = PathBuf::from(
            "/home/.m2/repository/com/fasterxml/jackson/core/jackson-databind/2.17.3/jackson-databind-2.17.3.jar",
        );
        let coord = coord_from_jar_path(&path).unwrap();
        assert_eq!(coord.group, "com.fasterxml.jackson.core");
        assert_eq!(coord.artifact, "jackson-databind");
        assert_eq!(coord.version, "2.17.3");
    }

    #[test]
    fn dedupes_lexed_coords() {
        let a = PathBuf::from(
            "/cache/files-2.1/com.fasterxml.jackson.core/jackson-databind/2.17.3/a/jackson-databind-2.17.3.jar",
        );
        let b = PathBuf::from(
            "/cache/files-2.1/com.fasterxml.jackson.core/jackson-databind/2.17.3/b/jackson-databind-2.17.3.jar",
        );
        let coords = coords_from_jar_paths(&[a, b]);
        assert_eq!(coords.len(), 1);
    }
}
