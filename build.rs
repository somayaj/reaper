//! Stamp UI build into the binary and run editor JS regression tests.
use std::path::Path;
use std::process::Command;

fn main() {
    let build = std::fs::read_to_string("static/BUILD")
        .unwrap_or_else(|_| "0".to_string())
        .trim()
        .to_string();
    println!("cargo:rerun-if-changed=static/BUILD");
    println!("cargo:rustc-env=REAPER_UI_BUILD={build}");

    println!("cargo:rerun-if-changed=static/monaco-languages.js");
    println!("cargo:rerun-if-changed=static/reaper-lang-core.js");
    println!("cargo:rerun-if-changed=static/app.js");
    println!("cargo:rerun-if-changed=static/reaper-ui.css");
    println!("cargo:rerun-if-changed=static/index.html");
    println!("cargo:rerun-if-changed=scripts/test-editor-regression.mjs");
    println!("cargo:rerun-if-changed=scripts/test-editor-regression.sh");

    if std::env::var("REAPER_SKIP_EDITOR_TESTS").is_ok() {
        return;
    }

    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".into());
    let script = Path::new(&manifest_dir).join("scripts/test-editor-regression.sh");
    if !script.is_file() {
        eprintln!(
            "cargo:warning=editor regression script missing: {}",
            script.display()
        );
        return;
    }

    let status = Command::new("bash")
        .arg(&script)
        .current_dir(&manifest_dir)
        .status();

    match status {
        Ok(s) if s.success() => {}
        Ok(s) => {
            panic!(
                "Editor regression suite failed (exit {}). Fix static/monaco-languages.js or set REAPER_SKIP_EDITOR_TESTS=1 to bypass.",
                s.code().unwrap_or(-1)
            );
        }
        Err(e) => {
            eprintln!(
                "cargo:warning=Could not run editor regression suite: {e}. Install Node or vendor it with scripts/vendor-node-macos.sh"
            );
        }
    }
}
