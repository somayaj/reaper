//! JaCoCo XML report parsing and Reaper-injected coverage test runs.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Serialize;
use tokio::sync::mpsc as async_mpsc;

use super::exec_stream::{self, ExecStreamEvent};
use super::gradle::{self, find_gradle_root, resolve_gradle_command};
use super::maven::{self, find_maven_root, resolve_maven_command, run_maven};
use super::run_project;
use super::safe_join;

pub const JACOCO_MAVEN_VERSION: &str = "0.8.12";

#[derive(Debug, Clone, Serialize)]
pub struct LineCoverage {
    pub line: u32,
    /// `covered` | `missed` | `partial`
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct FileCoverage {
    pub path: String,
    pub lines: Vec<LineCoverage>,
    pub line_rate: f64,
    pub covered_lines: u32,
    pub total_lines: u32,
    pub summary: String,
    pub has_jacoco: bool,
    /// Workspace path the line data applies to (may differ from `path` for test sources).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coverage_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub report_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

pub fn coverage_for_file(ws: &Path, rel_path: &str) -> Result<FileCoverage> {
    let rel_path = super::normalize_workspace_source_path(rel_path);
    let _ = safe_join(ws, &rel_path)?;
    let project = run_project::run_project_info(ws, &rel_path)?;
    let has_jacoco = project.frameworks.iter().any(|f| f == "jacoco");

    let Some((filename, package)) = java_file_identity(&rel_path) else {
        return Ok(empty_coverage(
            rel_path,
            has_jacoco,
            Some("not a Java source path".into()),
        ));
    };

    if !project.has_project {
        return Ok(empty_coverage(
            rel_path,
            false,
            Some("not inside a Gradle or Maven project".into()),
        ));
    }

    let project_root = safe_join(ws, &project.project_root)?;
    let report = find_jacoco_report(&project_root, &project.build_tool);
    let Some(report_path) = report else {
        return Ok(empty_coverage(
            rel_path,
            has_jacoco,
            Some("JaCoCo report not found — run tests with coverage first".into()),
        ));
    };

    let xml = std::fs::read_to_string(&report_path)
        .with_context(|| format!("read {}", report_path.display()))?;
    let (lines, coverage_path, message) =
        resolve_coverage_lines(&rel_path, &filename, &package, &xml);
    let report_rel = gradle::rel_path_for(ws, &report_path).ok();

    if lines.is_empty() {
        return Ok(FileCoverage {
            path: rel_path.clone(),
            lines,
            line_rate: 0.0,
            covered_lines: 0,
            total_lines: 0,
            summary: "No lines".into(),
            has_jacoco,
            coverage_path: None,
            report_path: report_rel,
            message: message.or_else(|| {
                Some(empty_coverage_message(&rel_path, &filename))
            }),
        });
    }

    let covered_lines = lines
        .iter()
        .filter(|l| l.status == "covered")
        .count() as u32;
    let total_lines = lines.len() as u32;
    let line_rate = if total_lines == 0 {
        0.0
    } else {
        covered_lines as f64 / total_lines as f64
    };
    let pct = (line_rate * 100.0).round() as u32;
    let summary = format!("{pct}% ({covered_lines}/{total_lines} lines)");
    let mapped_path = coverage_path != rel_path;

    Ok(FileCoverage {
        path: rel_path,
        lines,
        line_rate,
        covered_lines,
        total_lines,
        summary,
        has_jacoco,
        coverage_path: mapped_path.then_some(coverage_path),
        report_path: report_rel,
        message,
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct CoverageCounter {
    pub covered: u32,
    pub missed: u32,
    pub total: u32,
    pub rate: f64,
}

impl CoverageCounter {
    fn from_parts(covered: u32, missed: u32) -> Self {
        let total = covered.saturating_add(missed);
        let rate = if total == 0 {
            0.0
        } else {
            covered as f64 / total as f64
        };
        Self {
            covered,
            missed,
            total,
            rate,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CoverageMetrics {
    pub lines: CoverageCounter,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branches: Option<CoverageCounter>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<CoverageCounter>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CoverageFileEntry {
    pub path: String,
    pub name: String,
    pub package: String,
    pub lines: CoverageCounter,
}

#[derive(Debug, Clone, Serialize)]
pub struct CoverageReportSummary {
    pub project_root: String,
    pub query_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub report_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub html_report_path: Option<String>,
    pub totals: CoverageMetrics,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_file: Option<CoverageFileEntry>,
    pub files: Vec<CoverageFileEntry>,
    pub has_jacoco: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

pub fn coverage_report_summary(ws: &Path, rel_path: &str) -> Result<CoverageReportSummary> {
    let rel_path = super::normalize_workspace_source_path(rel_path);
    let _ = safe_join(ws, &rel_path)?;
    let project = run_project::run_project_info(ws, &rel_path)?;
    let has_jacoco = project.frameworks.iter().any(|f| f == "jacoco");

    if !project.has_project {
        return Ok(empty_report_summary(
            rel_path,
            project.project_root,
            false,
            Some("not inside a Gradle or Maven project".into()),
        ));
    }

    let project_root = safe_join(ws, &project.project_root)?;
    let report = find_jacoco_report(&project_root, &project.build_tool);
    let Some(report_path) = report else {
        return Ok(empty_report_summary(
            rel_path,
            project.project_root,
            has_jacoco,
            Some("JaCoCo report not found — run tests with coverage first".into()),
        ));
    };

    let xml = std::fs::read_to_string(&report_path)
        .with_context(|| format!("read {}", report_path.display()))?;
    let report_rel = gradle::rel_path_for(ws, &report_path).ok();
    let html_abs = find_jacoco_html_report(&project_root, &project.build_tool);
    let html_rel = html_abs
        .as_ref()
        .and_then(|p| gradle::rel_path_for(ws, p).ok());

    let totals = parse_report_metrics(&xml);
    let mut files = parse_all_sourcefiles(&xml, &project.project_root);
    files.sort_by(|a, b| {
        a.path
            .cmp(&b.path)
            .then_with(|| a.name.cmp(&b.name))
    });

    let current_file = resolve_current_file_entry(&rel_path, &files);

    Ok(CoverageReportSummary {
        project_root: project.project_root,
        query_path: rel_path,
        report_path: report_rel,
        html_report_path: html_rel,
        totals,
        current_file,
        files,
        has_jacoco,
        message: None,
    })
}

fn empty_report_summary(
    query_path: String,
    project_root: String,
    has_jacoco: bool,
    message: Option<String>,
) -> CoverageReportSummary {
    CoverageReportSummary {
        project_root,
        query_path,
        report_path: None,
        html_report_path: None,
        totals: CoverageMetrics {
            lines: CoverageCounter::from_parts(0, 0),
            branches: None,
            instructions: None,
        },
        current_file: None,
        files: Vec::new(),
        has_jacoco,
        message,
    }
}

fn resolve_current_file_entry(
    rel_path: &str,
    files: &[CoverageFileEntry],
) -> Option<CoverageFileEntry> {
    files
        .iter()
        .find(|f| f.path == rel_path)
        .cloned()
        .or_else(|| {
            if !is_test_source_path(rel_path) {
                return None;
            }
            production_source_candidates(rel_path)
                .into_iter()
                .find_map(|candidate| files.iter().find(|f| f.path == candidate).cloned())
        })
}

fn sourcefile_workspace_path(project_root: &str, package: &str, filename: &str) -> String {
    if package.is_empty() {
        format!("{project_root}/src/main/java/{filename}")
    } else {
        format!("{project_root}/src/main/java/{package}/{filename}")
    }
}

fn parse_all_sourcefiles(xml: &str, project_root: &str) -> Vec<CoverageFileEntry> {
    let mut out = Vec::new();
    let mut search = 0;
    while let Some(prel) = xml[search..].find("<package name=\"") {
        let pidx = search + prel;
        let pkg_rest = &xml[pidx..];
        let package = attr_string(pkg_rest, "name").unwrap_or_default();
        let pkg_end = pkg_rest
            .find("</package>")
            .map(|i| pidx + i)
            .unwrap_or(xml.len());
        let pkg_block = &xml[pidx..pkg_end];
        collect_sourcefiles_in_package(pkg_block, &package, project_root, &mut out);
        search = pkg_end;
    }
    out
}

fn collect_sourcefiles_in_package(
    pkg_block: &str,
    package: &str,
    project_root: &str,
    out: &mut Vec<CoverageFileEntry>,
) {
    let mut search = 0;
    while let Some(rel) = pkg_block[search..].find("<sourcefile name=\"") {
        let idx = search + rel;
        let rest = &pkg_block[idx..];
        let name = attr_string(rest, "name").unwrap_or_default();
        if name.is_empty() {
            search = idx + 1;
            continue;
        }
        let end = rest
            .find("</sourcefile>")
            .map(|i| idx + i)
            .unwrap_or_else(|| idx + rest.len().min(65536));
        let block = &pkg_block[idx..end];
        let lines = parse_line_counter_from_block(block);
        out.push(CoverageFileEntry {
            path: sourcefile_workspace_path(project_root, package, &name),
            name,
            package: package.to_string(),
            lines,
        });
        search = idx + 1;
    }
}

fn parse_line_counter_from_block(block: &str) -> CoverageCounter {
    if let Some(counter) = find_counter(block, "LINE") {
        return counter;
    }
    let lines = parse_line_elements(block);
    let covered = lines
        .iter()
        .filter(|l| l.status == "covered")
        .count() as u32;
    let missed = lines
        .iter()
        .filter(|l| l.status == "missed" || l.status == "partial")
        .count() as u32;
    CoverageCounter::from_parts(covered, missed)
}

fn parse_report_metrics(xml: &str) -> CoverageMetrics {
    let tail = xml
        .rfind("</report>")
        .map(|i| &xml[..i])
        .unwrap_or(xml);
    let lines = find_counter(tail, "LINE").unwrap_or_else(|| CoverageCounter::from_parts(0, 0));
    CoverageMetrics {
        lines,
        branches: find_counter(tail, "BRANCH"),
        instructions: find_counter(tail, "INSTRUCTION"),
    }
}

fn find_counter(block: &str, kind: &str) -> Option<CoverageCounter> {
    let needle = format!(r#"<counter type="{kind}""#);
    let start = block.find(&needle)?;
    let rest = &block[start..];
    let covered = attr_u32(rest, "covered").unwrap_or(0);
    let missed = attr_u32(rest, "missed").unwrap_or(0);
    Some(CoverageCounter::from_parts(covered, missed))
}

fn attr_string(s: &str, key: &str) -> Option<String> {
    let needle = format!("{key}=\"");
    let start = s.find(&needle)? + needle.len();
    let tail = &s[start..];
    let end = tail.find('"')?;
    Some(tail[..end].to_string())
}

fn find_jacoco_html_report(project_root: &Path, build_tool: &str) -> Option<PathBuf> {
    let candidates: &[&str] = match build_tool {
        "gradle" => &["build/reports/jacoco/test/html/index.html"],
        "maven" => &["target/site/jacoco/index.html"],
        _ => return None,
    };
    for rel in candidates {
        let path = project_root.join(rel);
        if path.is_file() {
            return Some(path);
        }
    }
    if build_tool == "gradle" {
        return find_html_index_under(project_root.join("build/reports/jacoco"));
    }
    None
}

fn find_html_index_under(dir: PathBuf) -> Option<PathBuf> {
    if !dir.is_dir() {
        return None;
    }
    let mut stack = vec![dir];
    while let Some(current) = stack.pop() {
        let index = current.join("index.html");
        if index.is_file() {
            return Some(index);
        }
        let entries = std::fs::read_dir(&current).ok()?;
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                stack.push(entry.path());
            }
        }
    }
    None
}

fn resolve_coverage_lines(
    rel_path: &str,
    filename: &str,
    package: &str,
    xml: &str,
) -> (Vec<LineCoverage>, String, Option<String>) {
    let mut lines = parse_sourcefile_lines(xml, package, filename);
    if !lines.is_empty() {
        return (lines, rel_path.to_string(), None);
    }

    if !is_test_source_path(rel_path) {
        return (lines, rel_path.to_string(), None);
    }

    for candidate in production_source_candidates(rel_path) {
        let Some((main_file, main_pkg)) = java_file_identity(&candidate) else {
            continue;
        };
        let main_lines = parse_sourcefile_lines(xml, &main_pkg, &main_file);
        if !main_lines.is_empty() {
            let msg = format!(
                "Showing coverage for {main_file} (code exercised by {filename})"
            );
            return (main_lines, candidate, Some(msg));
        }
    }

    (Vec::new(), rel_path.to_string(), None)
}

fn empty_coverage_message(rel_path: &str, test_filename: &str) -> String {
    if is_test_source_path(rel_path) {
        if let Some(candidate) = production_source_candidates(rel_path).into_iter().next() {
            let main = candidate.rsplit('/').next().unwrap_or("production source");
            return format!(
                "JaCoCo reports production code coverage, not test sources. \
                 No lines recorded for {main} yet — ensure tests ran and exercise that class."
            );
        }
        return format!(
            "JaCoCo reports production code coverage, not {test_filename}. \
             Open the matching class under src/main/java."
        );
    }
    format!("No coverage data for {test_filename} in JaCoCo report")
}

fn is_test_source_path(rel_path: &str) -> bool {
    let p = rel_path.replace('\\', "/");
    p.contains("/src/test/java/")
        || p.ends_with("Test.java")
        || p.ends_with("Tests.java")
        || p.ends_with("IT.java")
}

/// Map `…/src/test/java/…/UserServiceTest.java` → `…/src/main/java/…/UserService.java`.
fn production_source_candidates(rel_path: &str) -> Vec<String> {
    let p = rel_path.replace('\\', "/");
    let Some(test_idx) = p.find("/src/test/java/") else {
        return Vec::new();
    };
    let prefix = &p[..test_idx];
    let tail = &p[test_idx + "/src/test/java/".len()..];
    let Some(filename) = tail.rsplit('/').next() else {
        return Vec::new();
    };
    let Some((package_dir, _)) = tail.rsplit_once('/') else {
        return Vec::new();
    };
    let Some(stem) = filename.strip_suffix(".java") else {
        return Vec::new();
    };

    let mut out = Vec::new();
    for suffix in ["Test", "Tests", "IT", "TestCase"] {
        if let Some(base) = stem.strip_suffix(suffix) {
            if base.is_empty() {
                continue;
            }
            out.push(format!(
                "{prefix}/src/main/java/{package_dir}/{base}.java"
            ));
        }
    }
    out
}

fn empty_coverage(path: String, has_jacoco: bool, message: Option<String>) -> FileCoverage {
    FileCoverage {
        path,
        lines: Vec::new(),
        line_rate: 0.0,
        covered_lines: 0,
        total_lines: 0,
        summary: String::new(),
        has_jacoco,
        coverage_path: None,
        report_path: None,
        message,
    }
}

/// `(filename, package-with-slashes)` e.g. `("Foo.java", "com/example")`.
fn java_file_identity(rel_path: &str) -> Option<(String, String)> {
    let p = rel_path.replace('\\', "/");
    for marker in ["/src/main/java/", "/src/test/java/"] {
        let Some(idx) = p.find(marker) else {
            continue;
        };
        let tail = &p[idx + marker.len()..];
        let filename = tail.rsplit('/').next()?.to_string();
        let package = tail.rsplit_once('/')?.0.to_string();
        if package.is_empty() {
            return Some((filename, String::new()));
        }
        return Some((filename, package));
    }
    None
}

fn find_jacoco_report(project_root: &Path, build_tool: &str) -> Option<PathBuf> {
    let candidates: &[&str] = match build_tool {
        "gradle" => &[
            "build/reports/jacoco/test/jacocoTestReport.xml",
            "build/reports/jacoco/test.xml",
            "build/reports/jacoco/jacocoTestReport.xml",
            "build/jacoco/test/jacocoTest.xml",
        ],
        "maven" => &[
            "target/site/jacoco/jacoco.xml",
            "target/jacoco/jacoco.xml",
        ],
        _ => return None,
    };
    for rel in candidates {
        let path = project_root.join(rel);
        if path.is_file() {
            return Some(path);
        }
    }
    if build_tool == "gradle" {
        return find_jacoco_xml_under(project_root.join("build/reports/jacoco"));
    }
    None
}

fn find_jacoco_xml_under(dir: PathBuf) -> Option<PathBuf> {
    if !dir.is_dir() {
        return None;
    }
    let mut best: Option<PathBuf> = None;
    let mut stack = vec![dir];
    while let Some(current) = stack.pop() {
        let entries = std::fs::read_dir(&current).ok()?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().and_then(|e| e.to_str()) == Some("xml") {
                let name = path.file_name()?.to_string_lossy().to_lowercase();
                if name.contains("jacoco") {
                    best = Some(path);
                }
            }
        }
    }
    best
}

fn parse_sourcefile_lines(xml: &str, package: &str, filename: &str) -> Vec<LineCoverage> {
    let block = match extract_sourcefile_block(xml, package, filename) {
        Some(b) => b,
        None => return Vec::new(),
    };
    parse_line_elements(block)
}

fn extract_sourcefile_block<'a>(xml: &'a str, package: &str, filename: &str) -> Option<&'a str> {
    if let Some(block) = extract_sourcefile_in_package(xml, package, filename) {
        return Some(block);
    }
    // Fallback: first matching sourcefile name anywhere in the report.
    extract_sourcefile_anywhere(xml, filename)
}

fn extract_sourcefile_in_package<'a>(
    xml: &'a str,
    package: &str,
    filename: &str,
) -> Option<&'a str> {
    if package.is_empty() {
        return extract_sourcefile_anywhere(xml, filename);
    }
    let pkg_needle = format!(r#"<package name="{package}""#);
    let pkg_start = xml.find(&pkg_needle)?;
    let pkg_tail = &xml[pkg_start..];
    let pkg_end = pkg_tail[pkg_needle.len()..]
        .find("<package name=\"")
        .map(|i| pkg_start + pkg_needle.len() + i)
        .unwrap_or(xml.len());
    let pkg_block = &xml[pkg_start..pkg_end];
    sourcefile_block_in(pkg_block, filename)
}

fn extract_sourcefile_anywhere<'a>(xml: &'a str, filename: &str) -> Option<&'a str> {
    sourcefile_block_in(xml, filename)
}

fn sourcefile_block_in<'a>(haystack: &'a str, filename: &str) -> Option<&'a str> {
    let needle = format!(r#"<sourcefile name="{filename}""#);
    let start = haystack.find(&needle)?;
    let tail = &haystack[start..];
    let end = tail
        .find("</sourcefile>")
        .map(|i| start + i)
        .unwrap_or_else(|| start + tail.len().min(65536));
    Some(&haystack[start..end])
}

fn parse_line_elements(block: &str) -> Vec<LineCoverage> {
    let mut out = Vec::new();
    let mut search_from = 0;
    while let Some(rel) = block[search_from..].find("<line nr=\"") {
        let idx = search_from + rel;
        let rest = &block[idx..];
        let nr = attr_u32(rest, "nr");
        let mi = attr_u32(rest, "mi").unwrap_or(0);
        let ci = attr_u32(rest, "ci").unwrap_or(0);
        if let Some(line) = nr {
            let status = if ci > 0 && mi == 0 {
                "covered"
            } else if mi > 0 && ci == 0 {
                "missed"
            } else if ci > 0 && mi > 0 {
                "partial"
            } else {
                "missed"
            };
            out.push(LineCoverage {
                line,
                status: status.into(),
            });
        }
        search_from = idx + 1;
    }
    out.sort_by_key(|l| l.line);
    out.dedup_by_key(|l| l.line);
    out
}

fn attr_u32(s: &str, key: &str) -> Option<u32> {
    let needle = format!("{key}=\"");
    let start = s.find(&needle)? + needle.len();
    let tail = &s[start..];
    let end = tail.find('"')?;
    tail[..end].parse().ok()
}

/// Run tests with JaCoCo, injecting Reaper's Gradle init script or Maven agent when the
/// project build does not already declare JaCoCo.
pub fn stream_test_with_coverage(
    ws: &Path,
    rel_path: &str,
    test_filter: &str,
    tx: async_mpsc::Sender<ExecStreamEvent>,
) -> Result<i32> {
    let rel_path = super::normalize_workspace_source_path(rel_path);
    let project = run_project::run_project_info(ws, &rel_path)?;
    if !project.has_project {
        bail!("not inside a Gradle or Maven project");
    }
    let filter = test_filter.trim();
    if filter.is_empty() {
        bail!("test filter required");
    }
    let has_jacoco = project.frameworks.iter().any(|f| f == "jacoco");
    match project.build_tool.as_str() {
        "gradle" => stream_gradle_coverage(ws, &rel_path, filter, has_jacoco, tx),
        "maven" => stream_maven_coverage(ws, &rel_path, filter, has_jacoco, tx),
        other => bail!("unsupported build tool for coverage: {other}"),
    }
}

fn stream_gradle_coverage(
    ws: &Path,
    rel_path: &str,
    test_filter: &str,
    has_jacoco: bool,
    tx: async_mpsc::Sender<ExecStreamEvent>,
) -> Result<i32> {
    let root = find_gradle_root(ws, rel_path)?
        .ok_or_else(|| anyhow::anyhow!("not inside a Gradle project"))?;
    let cmd = resolve_gradle_command(&root)?;
    let mut args = cmd.project_args.clone();
    if !has_jacoco {
        let init = coverage_init_script_path();
        if !init.is_file() {
            bail!("Reaper JaCoCo init script missing at {}", init.display());
        }
        args.push("-I".into());
        args.push(init.to_string_lossy().into_owned());
    }
    args.push("--no-daemon".into());
    args.push("--no-configuration-cache".into());
    args.push("--console=plain".into());
    // --tests applies only to the test task; must come before jacocoTestReport.
    args.push("test".into());
    args.push("--tests".into());
    args.push(test_filter.to_string());
    args.push("jacocoTestReport".into());
    exec_stream::stream_gradle_command(&cmd, &args, tx)
}

fn stream_maven_coverage(
    ws: &Path,
    rel_path: &str,
    test_filter: &str,
    has_jacoco: bool,
    tx: async_mpsc::Sender<ExecStreamEvent>,
) -> Result<i32> {
    let root = find_maven_root(ws, rel_path)?
        .ok_or_else(|| anyhow::anyhow!("not inside a Maven project"))?;
    let cmd = resolve_maven_command(&root);
    let mut args = vec![
        "-q".to_string(),
        "--batch-mode".to_string(),
        format!("-Dtest={test_filter}"),
    ];
    if has_jacoco {
        args.push("test".into());
        args.push("jacoco:report".into());
    } else {
        let agent = ensure_jacoco_agent_jar(&root)?;
        let dest = root.join("target/jacoco.exec");
        if let Some(parent) = dest.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let agent_arg = format!(
            "-javaagent:{}=destfile={},append=false",
            agent.display(),
            dest.display()
        );
        args.push(format!("-DargLine={agent_arg}"));
        args.push("test".into());
        args.push(format!(
            "org.jacoco:jacoco-maven-plugin:{JACOCO_MAVEN_VERSION}:report"
        ));
    }
    exec_stream::stream_maven_command(&cmd, &args, tx)
}

pub fn coverage_init_script_path() -> PathBuf {
    if let Ok(dir) = std::env::var("REAPER_COVERAGE_INIT") {
        return PathBuf::from(dir);
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(mac_os) = exe.parent() {
            let bundled = mac_os.join("../Resources/gradle/reaper-coverage.init.gradle");
            if bundled.is_file() {
                return bundled.canonicalize().unwrap_or(bundled);
            }
        }
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("gradle/reaper-coverage.init.gradle")
}

fn ensure_jacoco_agent_jar(project_root: &Path) -> Result<PathBuf> {
    if let Some(path) = jacoco_agent_in_m2() {
        return Ok(path);
    }
    let artifact = format!(
        "org.jacoco:org.jacoco.agent:{JACOCO_MAVEN_VERSION}:jar:runtime"
    );
    let _ = run_maven(
        project_root,
        &["dependency:get", &format!("-Dartifact={artifact}")],
    );
    jacoco_agent_in_m2().ok_or_else(|| {
        anyhow::anyhow!("JaCoCo agent jar not found; ensure Maven can reach Maven Central")
    })
}

fn jacoco_agent_in_m2() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)?;
    let path = home.join(format!(
        ".m2/repository/org/jacoco/org.jacoco.agent/{JACOCO_MAVEN_VERSION}/org.jacoco.agent-{JACOCO_MAVEN_VERSION}-runtime.jar"
    ));
    path.is_file().then_some(path)
}

/// Normalize a Gradle `--tests` or Maven `-Dtest=` filter from a run-task string.
pub fn parse_test_filter_from_task(task: &str, build_tool: &str) -> String {
    let task = task.trim();
    if task.is_empty() {
        return String::new();
    }
    if build_tool == "maven" {
        if let Some(rest) = task.split("-Dtest=").nth(1) {
            return rest.split_whitespace().next().unwrap_or(rest).trim().to_string();
        }
        return task.to_string();
    }
    if let Some(idx) = task.find("--tests") {
        let rest = task[idx + "--tests".len()..].trim();
        return rest.split_whitespace().next().unwrap_or(rest).to_string();
    }
    task.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<report name="demo">
  <package name="com/example/product">
    <sourcefile name="ProductController.java">
      <line nr="10" mi="0" ci="3" mb="0" cb="0"/>
      <line nr="11" mi="2" ci="0" mb="0" cb="0"/>
      <line nr="12" mi="1" ci="1" mb="0" cb="0"/>
    </sourcefile>
  </package>
</report>"#;

    #[test]
    fn java_file_identity_from_test_path() {
        let (name, pkg) = java_file_identity("services/product/src/test/java/com/example/product/ProductControllerTest.java").unwrap();
        assert_eq!(name, "ProductControllerTest.java");
        assert_eq!(pkg, "com/example/product");
    }

    #[test]
    fn parse_jacoco_lines() {
        let lines = parse_sourcefile_lines(SAMPLE, "com/example/product", "ProductController.java");
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].line, 10);
        assert_eq!(lines[0].status, "covered");
        assert_eq!(lines[1].status, "missed");
        assert_eq!(lines[2].status, "partial");
    }

    #[test]
    fn parse_test_filter_from_gradle_task() {
        assert_eq!(
            parse_test_filter_from_task("test --tests com.foo.BarTest.testX", "gradle"),
            "com.foo.BarTest.testX"
        );
    }

    #[test]
    fn production_source_from_test_path() {
        let candidates = production_source_candidates(
            "services/user-service/src/test/java/com/example/user/UserServiceTest.java",
        );
        assert_eq!(
            candidates.first().map(String::as_str),
            Some("services/user-service/src/main/java/com/example/user/UserService.java")
        );
    }

    #[test]
    fn resolve_coverage_lines_maps_test_to_main() {
        let xml = r#"<?xml version="1.0"?><report>
  <package name="com/example/user">
    <sourcefile name="UserService.java">
      <line nr="5" mi="0" ci="2" mb="0" cb="0"/>
    </sourcefile>
  </package>
</report>"#;
        let test_path =
            "services/user-service/src/test/java/com/example/user/UserServiceTest.java";
        let (lines, cov_path, msg) =
            resolve_coverage_lines(test_path, "UserServiceTest.java", "com/example/user", xml);
        assert_eq!(lines.len(), 1);
        assert!(cov_path.ends_with("UserService.java"));
        assert!(msg.unwrap_or_default().contains("UserService.java"));
    }

    #[test]
    fn parse_test_filter_from_maven_task() {
        assert_eq!(
            parse_test_filter_from_task("-Dtest=com.foo.BarTest#testX test", "maven"),
            "com.foo.BarTest#testX"
        );
    }

    #[test]
    fn parse_report_metrics_from_xml() {
        let xml = r#"<?xml version="1.0"?><report>
  <package name="com/example">
    <sourcefile name="Foo.java">
      <counter type="LINE" missed="1" covered="3"/>
    </sourcefile>
  </package>
  <counter type="INSTRUCTION" missed="4" covered="16"/>
  <counter type="BRANCH" missed="1" covered="1"/>
  <counter type="LINE" missed="1" covered="3"/>
</report>"#;
        let metrics = parse_report_metrics(xml);
        assert_eq!(metrics.lines.covered, 3);
        assert_eq!(metrics.lines.missed, 1);
        assert_eq!(metrics.branches.as_ref().unwrap().total, 2);
        let files = parse_all_sourcefiles(xml, "demo");
        assert_eq!(files.len(), 1);
        assert!(files[0].path.ends_with("Foo.java"));
    }
}
