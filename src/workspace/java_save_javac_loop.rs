//! Integration test: repeated edit → save (disk) → full javac diagnostics.
//! Mirrors the editor auto-save + save-only javac loop.

use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use super::diagnostics::FileDiagnosticsResult;
use super::java_diagnostics::JavaDiagScope;
use super::{file_diagnostics, read_file, write_file};

const EDIT_COUNT: usize = 25;
const MAX_JAVAC_RETRIES: u32 = 3;

fn edit_loop_java_content(marker: usize) -> String {
    format!(
        r#"package com.example;

public class EditLoopApp {{
  public static void main(String[] args) {{
    int step = {marker};
    int bad = undeclaredVar{marker};
    System.out.println(step + bad);
  }}
}}
"#
    )
}

fn diagnose_full_with_retries(ws: &Path, rel: &str, content: &str) -> FileDiagnosticsResult {
    let mut last = FileDiagnosticsResult::cancelled();
    for attempt in 0..=MAX_JAVAC_RETRIES {
        last = file_diagnostics(ws, rel, content, &[], JavaDiagScope::Full)
            .unwrap_or_else(|e| panic!("diagnostics failed for {rel}: {e:#}"));
        if !last.cancelled || !last.diagnostics.is_empty() {
            return last;
        }
        if attempt < MAX_JAVAC_RETRIES {
            thread::sleep(Duration::from_millis(400 * (attempt + 1) as u64));
        }
    }
    last
}

fn diags_join(result: &FileDiagnosticsResult) -> String {
    result
        .diagnostics
        .iter()
        .map(|d| d.message.to_ascii_lowercase())
        .collect::<Vec<_>>()
        .join("\n")
}

fn assert_marker_diagnostic(result: &FileDiagnosticsResult, marker: usize, sym_prefix: &str) {
    let needle = format!("{sym_prefix}{marker}").to_ascii_lowercase();
    let joined = diags_join(result);
    assert!(
        !result.cancelled || !result.diagnostics.is_empty(),
        "edit {marker}: javac cancelled with no diagnostics"
    );
    assert!(
        joined.contains(&needle) || joined.contains("cannot find symbol"),
        "edit {marker}: expected javac error for {needle}, got: {joined}"
    );
}

fn assert_no_stale_marker(result: &FileDiagnosticsResult, stale_marker: usize, sym_prefix: &str) {
    let stale = format!("{sym_prefix}{stale_marker}").to_ascii_lowercase();
    let joined = diags_join(result);
    assert!(
        !joined.contains(&stale),
        "edit after {stale_marker}: stale squiggle for {stale} still present: {joined}"
    );
}

fn run_ten_edit_save_javac_loop(ws: &Path, rel: &str) {
    if let Some(parent) = ws.join(rel).parent() {
        std::fs::create_dir_all(parent).expect("create parent dirs");
    }

    for marker in 0..EDIT_COUNT {
        let content = edit_loop_java_content(marker);
        write_file(ws, rel, &content).expect("save write_file");
        let disk = read_file(ws, rel).expect("read saved file");
        assert_eq!(
            disk, content,
            "edit {marker}: disk content must match saved buffer"
        );

        let result = diagnose_full_with_retries(ws, rel, &content);
        assert_marker_diagnostic(&result, marker, "undeclaredvar");
        if marker > 0 {
            assert_no_stale_marker(&result, marker - 1, "undeclaredvar");
        }
    }
}

fn temp_loop_workspace() -> PathBuf {
    let ws = std::env::temp_dir().join(format!(
        "reaper-javac-edit-loop-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&ws);
    std::fs::create_dir_all(&ws).expect("temp workspace");
    ws
}

fn spring_analytics_rel() -> &'static str {
    "services/analytics-service/src/main/java/com/enterprise/analytics/AnalyticsServiceApplication.java"
}

fn spring_edit_content(base: &str, marker: usize) -> String {
    let inject = format!(
        "\n    int reaperLoop{marker} = undeclaredSym{marker};\n"
    );
    if let Some(idx) = base.find("SpringApplication.run") {
        format!("{}{inject}{}", &base[..idx], &base[idx..])
    } else {
        format!("{base}{inject}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ten_edit_save_javac_loop_temp_workspace() {
        let ws = temp_loop_workspace();
        let rel = "src/main/java/com/example/EditLoopApp.java";
        run_ten_edit_save_javac_loop(&ws, rel);
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn ten_edit_save_javac_loop_spring_maven_complicated() {
        let ws = Path::new("/Users/sunny/reaper/workspaces/Spring-maven-complicated");
        if !ws.is_dir() {
            eprintln!("skip: Spring-maven-complicated workspace not present");
            return;
        }
        let rel = spring_analytics_rel();
        let base = read_file(ws, rel).unwrap_or_else(|_| {
            r#"package com.enterprise.analytics;

import org.springframework.boot.SpringApplication;
import org.springframework.boot.autoconfigure.SpringBootApplication;

@SpringBootApplication
public class AnalyticsServiceApplication {
  public static void main(String[] args) {
    SpringApplication.run(AnalyticsServiceApplication.class, args);
  }
}
"#
            .to_string()
        });

        let backup = base.clone();
        for marker in 0..EDIT_COUNT {
            let content = spring_edit_content(&base, marker);
            write_file(ws, rel, &content).expect("spring save");
            let disk = read_file(ws, rel).expect("spring read");
            assert_eq!(disk, content, "spring edit {marker}: disk must match save");

            let result = diagnose_full_with_retries(ws, rel, &content);
            assert_marker_diagnostic(&result, marker, "undeclaredsym");
            if marker > 0 {
                assert_no_stale_marker(&result, marker - 1, "undeclaredsym");
            }
        }
        write_file(ws, rel, &backup).expect("restore analytics file after loop test");
    }
}
