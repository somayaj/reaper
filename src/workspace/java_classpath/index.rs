use std::path::PathBuf;

use super::cache::gradle_files_root;
use super::lexer::coords_from_jar_paths;
use super::parse::parse_transitive_jars;

/// Index a seed classpath with its Maven transitive closure.
pub fn complete_classpath(entries: Vec<PathBuf>, include_test_scope: bool) -> Vec<PathBuf> {
    let roots = coords_from_jar_paths(&entries);
    if roots.is_empty() {
        return entries;
    }

    let files_root = gradle_files_root();
    let expanded = parse_transitive_jars(&files_root, &roots, include_test_scope);

    let mut out = entries;
    for jar in expanded {
        if jar.is_file() {
            out.push(jar);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::super::lexer::coord_from_jar_path;
    use super::*;

    fn jar_contains(entries: &[PathBuf], artifact: &str) -> bool {
        entries.iter().any(|p| {
            p.to_string_lossy()
                .to_ascii_lowercase()
                .contains(artifact)
        })
    }

    #[test]
    fn indexes_jackson_core_from_databind_seed() {
        let home = std::env::temp_dir().join("reaper-classpath-index-home");
        let _ = std::fs::remove_dir_all(&home);
        let core_dir = home.join(
            "caches/modules-2/files-2.1/com.fasterxml.jackson.core/jackson-core/2.17.3/abc",
        );
        std::fs::create_dir_all(&core_dir).unwrap();
        std::fs::write(core_dir.join("jackson-core-2.17.3.jar"), b"PK").unwrap();
        std::fs::write(
            core_dir.join("jackson-core-2.17.3.pom"),
            r#"<?xml version="1.0" encoding="UTF-8"?>
<project>
  <modelVersion>4.0.0</modelVersion>
  <groupId>com.fasterxml.jackson.core</groupId>
  <artifactId>jackson-core</artifactId>
  <version>2.17.3</version>
</project>"#,
        )
        .unwrap();

        let databind_dir = home.join(
            "caches/modules-2/files-2.1/com.fasterxml.jackson.core/jackson-databind/2.17.3/def",
        );
        std::fs::create_dir_all(&databind_dir).unwrap();
        let databind = databind_dir.join("jackson-databind-2.17.3.jar");
        std::fs::write(&databind, b"PK").unwrap();
        std::fs::write(
            databind_dir.join("jackson-databind-2.17.3.pom"),
            r#"<?xml version="1.0" encoding="UTF-8"?>
<project>
  <modelVersion>4.0.0</modelVersion>
  <groupId>com.fasterxml.jackson.core</groupId>
  <artifactId>jackson-databind</artifactId>
  <version>2.17.3</version>
  <dependencies>
    <dependency>
      <groupId>com.fasterxml.jackson.core</groupId>
      <artifactId>jackson-core</artifactId>
      <version>2.17.3</version>
    </dependency>
  </dependencies>
</project>"#,
        )
        .unwrap();

        let prev = std::env::var("GRADLE_USER_HOME").ok();
        std::env::set_var("GRADLE_USER_HOME", &home);
        let completed = complete_classpath(vec![databind], true);
        if let Some(old) = prev {
            std::env::set_var("GRADLE_USER_HOME", old);
        } else {
            std::env::remove_var("GRADLE_USER_HOME");
        }

        assert!(jar_contains(&completed, "jackson-core"));
        assert!(jar_contains(&completed, "jackson-databind"));
        assert!(coord_from_jar_path(
            completed
                .iter()
                .find(|p| p.to_string_lossy().contains("jackson-core"))
                .unwrap()
        )
        .is_some());
        let _ = std::fs::remove_dir_all(&home);
    }
}
