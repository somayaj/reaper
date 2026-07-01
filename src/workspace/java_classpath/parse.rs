use std::path::{Path, PathBuf};

use super::cache::{find_jar, read_pom};

/// Walk cached Maven POMs and parse transitive dependency edges into resolved JARs.
pub fn parse_transitive_jars(
    files_root: &Path,
    roots: &[(String, String, String)],
    include_test_scope: bool,
) -> Vec<PathBuf> {
    super::super::maven::collect_transitive_jars(
        roots,
        include_test_scope,
        |group, artifact, version| find_jar(files_root, group, artifact, version),
        |group, artifact, version| read_pom(files_root, group, artifact, version),
    )
}
