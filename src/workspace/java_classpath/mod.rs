//! Dependency classpath layer: lexer → dependency graph → completed JAR index.
//!
//! Tooling caches and build-file walks can omit transitive JARs. This pipeline
//! re-walks Maven POMs from every resolved artifact so javac gets a full closure.

mod cache;
mod index;
mod lexer;
mod parse;

pub use index::complete_classpath;
pub use lexer::{MavenCoord, coord_from_jar_path, coords_from_jar_paths};
pub use cache::{
    find_cached_jar, find_jar, gradle_files_root, gradle_user_home, read_gradle_cached_pom, read_pom,
};
