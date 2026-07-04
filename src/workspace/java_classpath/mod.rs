//! Dependency classpath layer: lexer → dependency graph → completed JAR index.
//!
//! Tooling caches and build-file walks can omit transitive JARs. This pipeline
//! re-walks Maven POMs from every resolved artifact so javac gets a full closure.

mod cache;
mod index;
mod lexer;
mod parse;

pub use index::complete_classpath;
pub use lexer::coord_from_jar_path;
pub use cache::{find_cached_jar, gradle_user_home, read_gradle_cached_pom};
