//! Java structure layer: lexer → compilation unit → symbol index.
//!
//! Semantic type-checking stays in `javac` (`java_diagnostics`). Navigation, imports,
//! and structural indexing use this pipeline.

mod annotations;
mod imports;
mod index;
mod lexer;
mod parse;

pub use annotations::{
    annotation_simple_names, lombok_symbol_in_message, stale_imported_dependency_diag,
};
pub use imports::{ImportMap, type_import_fqcns};
pub use index::{JavaSymbol, index_source, package_name};
pub use parse::{CompilationUnit, find_type_position, parse_compilation_unit};

/// Parse and index a `.java` file.
pub fn analyze_source(content: &str, rel_path: &str, index_members: bool) -> (CompilationUnit, Vec<JavaSymbol>) {
    let unit = parse_compilation_unit(content);
    let symbols = index_source(rel_path, index_members, &unit);
    (unit, symbols)
}

/// Import map only (cached by callers).
pub fn parse_imports(content: &str) -> ImportMap {
    parse_compilation_unit(content).imports
}
