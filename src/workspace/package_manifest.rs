use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Serialize;
use toml::Value;

use super::native_build_tasks::{
    detect_python_package_manager, go_program_shell, is_runnable_make_target,
    python_install_command,
    python_interpreter_for_project, python_pip_command, python_pytest_all_command,
    python_requirements_install_command, python_ruff_command,
};

#[derive(Debug, Clone, Serialize)]
pub struct PackageManifestView {
    pub manifest_path: String,
    pub package_root: String,
    pub ecosystem: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<ManifestField>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sections: Vec<ManifestSection>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<ManifestAction>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ManifestField {
    pub label: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ManifestSection {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<ManifestItem>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ManifestItem {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ManifestAction {
    pub id: String,
    pub label: String,
    pub command: String,
}

const MANIFEST_NAMES: &[(&str, &str)] = &[
    ("Cargo.toml", "cargo"),
    ("pyproject.toml", "python"),
    ("requirements.txt", "python-reqs"),
    ("Pipfile", "pipfile"),
    ("Gemfile", "ruby"),
    ("Rakefile", "rake"),
    ("rakefile", "rake"),
    ("go.mod", "go"),
    ("CMakeLists.txt", "cmake"),
    ("meson.build", "meson"),
    ("Makefile", "make"),
    ("makefile", "make"),
    ("GNUmakefile", "make"),
    ("vcpkg.json", "vcpkg"),
    ("conanfile.txt", "conan"),
];

pub fn is_package_manifest_path(rel_path: &str) -> bool {
    manifest_kind_for_path(rel_path).is_some()
}

pub fn package_manifest_view(ws: &Path, rel_path: &str) -> Result<PackageManifestView> {
    let rel_path = super::normalize_workspace_source_path(rel_path);
    let (abs, manifest_rel, package_root, kind) = locate_manifest(ws, &rel_path)?;
    let text = std::fs::read_to_string(&abs).with_context(|| format!("read {manifest_rel}"))?;
    parse_manifest(ws, kind, &manifest_rel, &package_root, &text)
}

fn manifest_kind_for_path(path: &str) -> Option<&'static str> {
    let base = path.replace('\\', "/");
    let name = base.rsplit('/').next()?.to_ascii_lowercase();
    match name.as_str() {
        "cargo.toml" => Some("cargo"),
        "pyproject.toml" => Some("python"),
        "requirements.txt" => Some("python-reqs"),
        "pipfile" => Some("pipfile"),
        "gemfile" => Some("ruby"),
        "rakefile" => Some("rake"),
        "go.mod" => Some("go"),
        "cmakelists.txt" => Some("cmake"),
        "meson.build" => Some("meson"),
        "makefile" | "gnumakefile" => Some("make"),
        "vcpkg.json" => Some("vcpkg"),
        "conanfile.txt" => Some("conan"),
        _ if name.ends_with(".gemspec") => Some("gemspec"),
        _ => None,
    }
}

fn locate_manifest(
    ws: &Path,
    rel_path: &str,
) -> Result<(PathBuf, String, String, &'static str)> {
    let rel = rel_path.trim().replace('\\', "/");
    let rel = rel.strip_prefix("./").unwrap_or(&rel);

    if let Some(kind) = manifest_kind_for_path(rel) {
        let abs = ws.join(rel);
        if abs.is_file() {
            let package_root = PathBuf::from(rel)
                .parent()
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default();
            return Ok((abs, rel.to_string(), package_root, kind));
        }
    }

    let mut dir = if rel.is_empty() {
        PathBuf::new()
    } else if rel.ends_with('/') {
        PathBuf::from(rel.trim_end_matches('/'))
    } else {
        PathBuf::from(rel)
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_default()
    };

    loop {
        for (file_name, kind) in MANIFEST_NAMES {
            let candidate = if dir.as_os_str().is_empty() {
                ws.join(file_name)
            } else {
                ws.join(&dir).join(file_name)
            };
            if candidate.is_file() {
                let manifest_rel = if dir.as_os_str().is_empty() {
                    file_name.to_string()
                } else {
                    format!("{}/{}", dir.to_string_lossy().replace('\\', "/"), file_name)
                };
                let package_root = dir.to_string_lossy().replace('\\', "/");
                return Ok((candidate, manifest_rel, package_root, kind));
            }
        }
        if !dir.pop() {
            break;
        }
    }
    bail!("No package manifest found near {rel_path}");
}

fn parse_manifest(
    ws: &Path,
    kind: &str,
    manifest_rel: &str,
    package_root: &str,
    text: &str,
) -> Result<PackageManifestView> {
    match kind {
        "cargo" => parse_cargo(manifest_rel, package_root, text),
        "python" => parse_pyproject(ws, manifest_rel, package_root, text),
        "python-reqs" => parse_requirements(ws, manifest_rel, package_root, text),
        "pipfile" => parse_pipfile(ws, manifest_rel, package_root, text),
        "ruby" | "gemspec" => parse_ruby(manifest_rel, package_root, text, kind),
        "rake" => parse_rakefile(manifest_rel, package_root, text),
        "go" => parse_go_mod(manifest_rel, package_root, text),
        "cmake" => parse_cmake(manifest_rel, package_root, text),
        "meson" => parse_meson(manifest_rel, package_root, text),
        "make" => parse_makefile(manifest_rel, package_root, text),
        "vcpkg" => parse_vcpkg(manifest_rel, package_root, text),
        "conan" => parse_conan(manifest_rel, package_root, text),
        other => bail!("unsupported manifest kind: {other}"),
    }
}

fn parse_cargo(manifest_rel: &str, package_root: &str, text: &str) -> Result<PackageManifestView> {
    let root: Value = toml::from_str(text).context("parse Cargo.toml")?;
    let table = root.as_table().context("Cargo.toml root table")?;

    let mut fields = Vec::new();
    let mut sections = Vec::new();
    let mut title = "Cargo crate".into();
    let mut subtitle = None;

    if let Some(pkg) = table.get("package").and_then(|v| v.as_table()) {
        if let Some(name) = pkg.get("name").and_then(|v| v.as_str()) {
            title = name.to_string();
            fields.push(field("Name", name));
        }
        push_opt_field(&mut fields, "Version", pkg.get("version"));
        push_opt_field(&mut fields, "Edition", pkg.get("edition"));
        push_opt_field(&mut fields, "License", pkg.get("license"));
        push_opt_field(&mut fields, "Description", pkg.get("description"));
        push_opt_field(&mut fields, "Rust", pkg.get("rust-version"));
        if let Some(authors) = pkg.get("authors").and_then(|v| v.as_array()) {
            let joined: Vec<_> = authors.iter().filter_map(|a| a.as_str()).collect();
            if !joined.is_empty() {
                fields.push(field("Authors", &joined.join(", ")));
            }
        }
    }

    if let Some(members) = table
        .get("workspace")
        .and_then(|w| w.get("members"))
        .and_then(|m| m.as_array())
    {
        let items: Vec<_> = members
            .iter()
            .filter_map(|v| v.as_str())
            .map(|m| item(m, None))
            .collect();
        if !items.is_empty() {
            subtitle = Some(format!("Workspace · {} members", items.len()));
            sections.push(section("workspace", "Workspace members", items));
        }
    }

    push_dep_section(&mut sections, "dependencies", "Dependencies", table.get("dependencies"));
    push_dep_section(
        &mut sections,
        "dev-dependencies",
        "Dev dependencies",
        table.get("dev-dependencies"),
    );
    push_dep_section(
        &mut sections,
        "build-dependencies",
        "Build dependencies",
        table.get("build-dependencies"),
    );

    if let Some(feat_table) = table.get("features").and_then(|v| v.as_table()) {
        let items: Vec<_> = feat_table
            .iter()
            .map(|(name, spec)| {
                let detail = match spec {
                    Value::Array(arr) => {
                        let deps: Vec<_> = arr.iter().filter_map(|v| v.as_str()).collect();
                        if deps.is_empty() {
                            None
                        } else {
                            Some(deps.join(", "))
                        }
                    }
                    _ => None,
                };
                item(name, detail.map(String::from))
            })
            .collect();
        if !items.is_empty() {
            sections.push(section("features", "Features", items));
        }
    }

    if let Some(bins) = table.get("bin").and_then(|v| v.as_array()) {
        let items: Vec<_> = bins
            .iter()
            .filter_map(|b| b.as_table())
            .filter_map(|t| {
                let name = t.get("name")?.as_str()?;
                let path = t.get("path").and_then(|p| p.as_str());
                Some(item(name, path.map(String::from)))
            })
            .collect();
        if !items.is_empty() {
            sections.push(section("bins", "Binaries", items));
        }
    }

    let actions = vec![
        action("build", "Build", "cargo build"),
        action("test", "Test", "cargo test"),
        action("check", "Check", "cargo check"),
        action("run", "Run", "cargo run"),
    ];

    Ok(PackageManifestView {
        manifest_path: manifest_rel.to_string(),
        package_root: package_root.to_string(),
        ecosystem: "cargo".into(),
        title,
        subtitle,
        fields,
        sections,
        actions,
    })
}

fn parse_pyproject(
    ws: &Path,
    manifest_rel: &str,
    package_root: &str,
    text: &str,
) -> Result<PackageManifestView> {
    let root: Value = toml::from_str(text).context("parse pyproject.toml")?;
    let table = root.as_table().context("pyproject root")?;
    let dir = if package_root.is_empty() {
        ws.to_path_buf()
    } else {
        ws.join(package_root)
    };
    let pm = detect_python_package_manager(&dir, &root);
    let mut fields = Vec::new();
    let mut sections = Vec::new();
    let mut title = "Python project".into();

    if let Some(project) = table.get("project").and_then(|v| v.as_table()) {
        if let Some(name) = project.get("name").and_then(|v| v.as_str()) {
            title = name.to_string();
            fields.push(field("Name", name));
        }
        push_opt_field(&mut fields, "Version", project.get("version"));
        push_opt_field(&mut fields, "Requires Python", project.get("requires-python"));
        push_opt_field(&mut fields, "Description", project.get("description"));
        push_opt_field(&mut fields, "License", project.get("license"));
        if let Some(scripts) = project.get("scripts").and_then(|v| v.as_table()) {
            let items: Vec<_> = scripts
                .iter()
                .map(|(name, target)| {
                    let detail = target.as_str().map(String::from);
                    item(name, detail)
                })
                .collect();
            if !items.is_empty() {
                sections.push(section("scripts", "Scripts", items));
            }
        }
        if let Some(deps) = project.get("dependencies").and_then(|v| v.as_array()) {
            let items: Vec<_> = deps
                .iter()
                .filter_map(|d| d.as_str())
                .map(|d| item(d, None))
                .collect();
            if !items.is_empty() {
                sections.push(section("dependencies", "Dependencies", items));
            }
        }
        if let Some(opt) = project
            .get("optional-dependencies")
            .and_then(|v| v.as_table())
        {
            for (group, arr) in opt {
                if let Some(deps) = arr.as_array() {
                    let items: Vec<_> = deps
                        .iter()
                        .filter_map(|d| d.as_str())
                        .map(|d| item(d, None))
                        .collect();
                    if !items.is_empty() {
                        sections.push(section(
                            &format!("optional-{group}"),
                            &format!("Optional · {group}"),
                            items,
                        ));
                    }
                }
            }
        }
    }

    if let Some(tool) = table.get("tool").and_then(|v| v.as_table()) {
        if tool.contains_key("poetry") {
            fields.push(field("Tool", "Poetry"));
        }
        if tool.contains_key("uv") {
            fields.push(field("Tool", "uv"));
        }
        if tool.contains_key("pdm") {
            fields.push(field("Tool", "PDM"));
        }
    }
    fields.push(field("Package manager", &pm));

    let test_cmd = python_pytest_all_command(Some(&dir), &pm);
    let lint_cmd = python_ruff_command(Some(&dir), &pm, "check .");
    let actions = vec![
        action("install", "Install", &python_install_command(&pm, Some(&dir))),
        action("test", "Test (pytest)", &test_cmd),
        action("lint", "Ruff check", &lint_cmd),
    ];

    Ok(PackageManifestView {
        manifest_path: manifest_rel.to_string(),
        package_root: package_root.to_string(),
        ecosystem: "python".into(),
        title,
        subtitle: Some("pyproject.toml".into()),
        fields,
        sections,
        actions,
    })
}

fn parse_requirements(
    ws: &Path,
    manifest_rel: &str,
    package_root: &str,
    text: &str,
) -> Result<PackageManifestView> {
    let dir = if package_root.is_empty() {
        ws.to_path_buf()
    } else {
        ws.join(package_root)
    };
    let items: Vec<_> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| item(l, None))
        .collect();
    Ok(PackageManifestView {
        manifest_path: manifest_rel.to_string(),
        package_root: package_root.to_string(),
        ecosystem: "python".into(),
        title: "Requirements".into(),
        subtitle: Some(format!("{} packages", items.len())),
        fields: vec![],
        sections: if items.is_empty() {
            vec![]
        } else {
            vec![section("requirements", "Packages", items)]
        },
        actions: vec![
            action("install", "Install", &python_requirements_install_command(Some(&dir))),
            action("freeze", "Freeze", &python_pip_command(Some(&dir), "freeze")),
        ],
    })
}

fn parse_pipfile(
    ws: &Path,
    manifest_rel: &str,
    package_root: &str,
    text: &str,
) -> Result<PackageManifestView> {
    let dir = if package_root.is_empty() {
        ws.to_path_buf()
    } else {
        ws.join(package_root)
    };
    let root: Value = toml::from_str(text).context("parse Pipfile")?;
    let table = root.as_table().context("Pipfile root")?;
    let mut sections = Vec::new();
    push_dep_section(&mut sections, "packages", "Packages", table.get("packages"));
    push_dep_section(
        &mut sections,
        "dev-packages",
        "Dev packages",
        table.get("dev-packages"),
    );
    Ok(PackageManifestView {
        manifest_path: manifest_rel.to_string(),
        package_root: package_root.to_string(),
        ecosystem: "python".into(),
        title: "Pipfile".into(),
        subtitle: Some("Pipenv".into()),
        fields: vec![],
        sections,
        actions: vec![
            action("install", "Install", "pipenv install"),
            action("shell", "Shell", "pipenv shell"),
            action("run", "Run", &format!("pipenv run {}", python_interpreter_for_project(Some(&dir)))),
        ],
    })
}

fn parse_ruby(
    manifest_rel: &str,
    package_root: &str,
    text: &str,
    kind: &str,
) -> Result<PackageManifestView> {
    let mut fields = Vec::new();
    let mut sections = Vec::new();
    let mut gems = Vec::new();
    let mut title = if kind == "gemspec" {
        manifest_rel
            .rsplit('/')
            .next()
            .unwrap_or("Gem")
            .trim_end_matches(".gemspec")
            .to_string()
    } else {
        "Gemfile".into()
    };

    for line in text.lines() {
        let trimmed = line.split('#').next().unwrap_or("").trim();
        if kind == "gemspec" {
            if let Some(name) = trimmed.strip_prefix("name") {
                if let Some(q) = extract_quoted(name) {
                    title = q;
                    fields.push(field("Name", &title));
                }
            }
            if trimmed.starts_with("version") {
                if let Some(v) = extract_quoted(trimmed) {
                    fields.push(field("Version", &v));
                }
            }
            if trimmed.starts_with("add_dependency") || trimmed.starts_with("add_development_dependency")
            {
                if let Some((name, ver)) = parse_ruby_dep_call(trimmed) {
                    gems.push(item(&name, ver.clone()));
                }
            }
        } else if trimmed.starts_with("gem ") {
            if let Some((name, ver)) = parse_gem_line(trimmed) {
                gems.push(item(&name, ver.clone()));
            }
        } else if trimmed.starts_with("ruby ") {
            if let Some(v) = extract_quoted(trimmed) {
                fields.push(field("Ruby", &v));
            }
        } else if trimmed.starts_with("source ") {
            if let Some(v) = extract_quoted(trimmed) {
                fields.push(field("Source", &v));
            }
        }
    }

    if !gems.is_empty() {
        sections.push(section("gems", "Gems", gems));
    }

    let is_rails = text.contains("rails") || text.contains("Railtie");
    let mut actions = vec![
        action("install", "Bundle install", "bundle install"),
        action("exec", "Bundle exec", "bundle exec ruby -v"),
    ];
    if is_rails {
        actions.push(action("server", "Rails server", "bin/rails server"));
        actions.push(action("test", "Rails test", "bin/rails test"));
        title = if title == "Gemfile" {
            "Rails app".into()
        } else {
            title
        };
    }

    Ok(PackageManifestView {
        manifest_path: manifest_rel.to_string(),
        package_root: package_root.to_string(),
        ecosystem: "ruby".into(),
        title,
        subtitle: Some(if is_rails {
            "Ruby · Rails".into()
        } else {
            "Ruby · Bundler".into()
        }),
        fields,
        sections,
        actions,
    })
}

fn parse_rakefile(manifest_rel: &str, package_root: &str, text: &str) -> Result<PackageManifestView> {
    let mut tasks = Vec::new();
    let mut pending_desc: Option<String> = None;
    let mut namespace: Option<String> = None;
    let is_rails = text.contains("Rails.application.load_tasks")
        || text.contains("rails/tasks")
        || text.contains("Railtie");

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if trimmed.starts_with("desc ") {
            pending_desc = extract_quoted(trimmed.strip_prefix("desc ").unwrap_or(trimmed));
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("namespace ") {
            namespace = parse_rake_symbol_or_string(rest.split_whitespace().next().unwrap_or(""));
            continue;
        }
        if trimmed == "end" {
            namespace = None;
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("task ") {
            if let Some(name) = parse_rake_task_name(rest) {
                let full_name = match &namespace {
                    Some(ns) => format!("{ns}:{name}"),
                    None => name,
                };
                tasks.push(item(&full_name, pending_desc.take()));
            } else {
                pending_desc = None;
            }
        }
    }

    let mut actions = vec![
        action("list", "List tasks", "rake -T"),
        action("default", "Default", "rake default"),
    ];
    if is_rails {
        actions.push(action("test", "Rails test", "bin/rails test"));
        actions.push(action("server", "Rails server", "bin/rails server"));
    } else {
        actions.push(action("test", "Test", "rake test"));
    }

    Ok(PackageManifestView {
        manifest_path: manifest_rel.to_string(),
        package_root: package_root.to_string(),
        ecosystem: "rake".into(),
        title: if is_rails { "Rails Rakefile".into() } else { "Rakefile".into() },
        subtitle: Some(if is_rails {
            format!("Ruby · Rails · {} tasks", tasks.len())
        } else {
            format!("Ruby · Rake · {} tasks", tasks.len())
        }),
        fields: vec![],
        sections: if tasks.is_empty() {
            vec![]
        } else {
            vec![section("tasks", "Tasks", tasks)]
        },
        actions,
    })
}

fn parse_rake_symbol_or_string(token: &str) -> Option<String> {
    let token = token.trim();
    if let Some(s) = token.strip_prefix(':') {
        let name = s.trim_end_matches(|c: char| !c.is_ascii_alphanumeric() && c != '_');
        return (!name.is_empty()).then(|| name.to_string());
    }
    extract_quoted(token)
}

fn parse_rake_task_name(rest: &str) -> Option<String> {
    let token = rest.split([' ', '[', '(']).next()?.trim();
    if token.is_empty() {
        return None;
    }
    if let Some(name) = token.strip_prefix(':') {
        let name = name.trim_end_matches(|c: char| !c.is_ascii_alphanumeric() && c != '_');
        return (!name.is_empty()).then(|| name.to_string());
    }
    if token.ends_with(':') {
        let name = token.trim_end_matches(':');
        return (!name.is_empty()).then(|| name.to_string());
    }
    extract_quoted(token)
}

fn parse_go_mod(manifest_rel: &str, package_root: &str, text: &str) -> Result<PackageManifestView> {
    let mut fields = Vec::new();
    let mut sections = Vec::new();
    let mut title = "Go module".into();
    let mut in_require = false;
    let mut requires = Vec::new();
    let mut replaces = Vec::new();

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("module ") {
            title = trimmed["module ".len()..].trim().to_string();
            fields.push(field("Module", &title));
        } else if trimmed.starts_with("go ") {
            fields.push(field("Go", trimmed["go ".len()..].trim()));
        } else if trimmed == "require (" {
            in_require = true;
        } else if in_require {
            if trimmed == ")" {
                in_require = false;
            } else if !trimmed.is_empty() && !trimmed.starts_with("//") {
                requires.push(item(trimmed, None));
            }
        } else if trimmed.starts_with("require ") {
            requires.push(item(trimmed["require ".len()..].trim(), None));
        } else if trimmed.starts_with("replace ") {
            replaces.push(item(trimmed["replace ".len()..].trim(), None));
        }
    }

    if !requires.is_empty() {
        sections.push(section("require", "Require", requires));
    }
    if !replaces.is_empty() {
        sections.push(section("replace", "Replace", replaces));
    }

    let go = go_program_shell();
    Ok(PackageManifestView {
        manifest_path: manifest_rel.to_string(),
        package_root: package_root.to_string(),
        ecosystem: "go".into(),
        title,
        subtitle: Some("go.mod".into()),
        fields,
        sections,
        actions: vec![
            action("build", "Build", &format!("{go} build ./...")),
            action("test", "Test", &format!("{go} test ./...")),
            action("run", "Run", &format!("{go} run .")),
            action("tidy", "Tidy", &format!("{go} mod tidy")),
        ],
    })
}

fn parse_cmake(manifest_rel: &str, package_root: &str, text: &str) -> Result<PackageManifestView> {
    let mut fields = Vec::new();
    let mut sections = Vec::new();
    let mut title = "CMake project".into();
    let mut targets = Vec::new();
    let mut packages = Vec::new();

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("project(") {
            if let Some(name) = trimmed
                .trim_start_matches("project(")
                .split([')', ' '])
                .next()
            {
                if !name.is_empty() {
                    title = name.to_string();
                    fields.push(field("Project", name));
                }
            }
        } else if trimmed.starts_with("cmake_minimum_required") {
            fields.push(field("CMake min", trimmed.trim_start_matches("cmake_minimum_required")));
        } else if trimmed.starts_with("find_package(") {
            let inner = trimmed
                .trim_start_matches("find_package(")
                .trim_end_matches(')');
            packages.push(item(inner, None));
        } else if trimmed.starts_with("add_executable(") {
            let inner = trimmed
                .trim_start_matches("add_executable(")
                .trim_end_matches(')');
            let name = inner.split_whitespace().next().unwrap_or(inner);
            targets.push(item(name, Some("executable".into())));
        } else if trimmed.starts_with("add_library(") {
            let inner = trimmed
                .trim_start_matches("add_library(")
                .trim_end_matches(')');
            let name = inner.split_whitespace().next().unwrap_or(inner);
            targets.push(item(name, Some("library".into())));
        }
    }

    if !packages.is_empty() {
        sections.push(section("packages", "Find package", packages));
    }
    if !targets.is_empty() {
        sections.push(section("targets", "Targets", targets));
    }

    Ok(PackageManifestView {
        manifest_path: manifest_rel.to_string(),
        package_root: package_root.to_string(),
        ecosystem: "cmake".into(),
        title,
        subtitle: Some("CMakeLists.txt".into()),
        fields,
        sections,
        actions: vec![
            action("configure", "Configure", "cmake -S . -B build"),
            action("build", "Build", "cmake --build build"),
            action("test", "Test", "ctest --test-dir build"),
        ],
    })
}

fn parse_meson(manifest_rel: &str, package_root: &str, text: &str) -> Result<PackageManifestView> {
    let mut fields = Vec::new();
    let mut sections = Vec::new();
    let mut title = "Meson project".into();

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("project(") {
            if let Some(name) = trimmed
                .trim_start_matches("project(")
                .split([',', ')'])
                .next()
            {
                let name = name.trim().trim_matches('\'').trim_matches('"');
                if !name.is_empty() {
                    title = name.to_string();
                    fields.push(field("Project", name));
                }
            }
        } else if trimmed.starts_with("executable(") {
            let inner = trimmed.trim_start_matches("executable(").trim_end_matches(')');
            let name = inner.split(',').next().unwrap_or(inner).trim().trim_matches('\'').trim_matches('"');
            sections
                .entry_or_extend("targets", "Targets")
                .push(item(name, Some("executable".into())));
        }
    }

    Ok(PackageManifestView {
        manifest_path: manifest_rel.to_string(),
        package_root: package_root.to_string(),
        ecosystem: "meson".into(),
        title,
        subtitle: Some("meson.build".into()),
        fields,
        sections,
        actions: vec![
            action("setup", "Setup", "meson setup build"),
            action("compile", "Compile", "meson compile -C build"),
            action("test", "Test", "meson test -C build"),
        ],
    })
}

fn parse_makefile(manifest_rel: &str, package_root: &str, text: &str) -> Result<PackageManifestView> {
    let mut targets = Vec::new();
    let mut phony = std::collections::HashSet::new();

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with(".PHONY:") {
            for t in trimmed.trim_start_matches(".PHONY:").split_whitespace() {
                phony.insert(t.to_string());
            }
        } else if !trimmed.is_empty()
            && !trimmed.starts_with('#')
            && !trimmed.starts_with('\t')
            && trimmed.contains(':')
            && !trimmed.starts_with('.')
        {
            let name = trimmed.split(':').next().unwrap_or("").trim();
            if is_runnable_make_target(name) {
                let detail = if phony.contains(name) {
                    Some("PHONY".into())
                } else {
                    None
                };
                targets.push(item(name, detail));
            }
        }
    }

    Ok(PackageManifestView {
        manifest_path: manifest_rel.to_string(),
        package_root: package_root.to_string(),
        ecosystem: "make".into(),
        title: "Makefile".into(),
        subtitle: Some(format!("{} targets", targets.len())),
        fields: vec![],
        sections: if targets.is_empty() {
            vec![]
        } else {
            vec![section("targets", "Targets", targets)]
        },
        actions: vec![
            action("make", "Make", "make"),
            action("clean", "Clean", "make clean"),
            action("test", "Test", "make test"),
        ],
    })
}

fn parse_vcpkg(manifest_rel: &str, package_root: &str, text: &str) -> Result<PackageManifestView> {
    let root: serde_json::Value =
        serde_json::from_str(text).context("parse vcpkg.json")?;
    let mut fields = Vec::new();
    let mut sections = Vec::new();
    let mut title = "vcpkg manifest".into();

    if let Some(name) = root.get("name").and_then(|v| v.as_str()) {
        title = name.to_string();
        fields.push(field("Name", name));
    }
    if let Some(version) = root.get("version").and_then(|v| v.as_str()) {
        fields.push(field("Version", version));
    }
    if let Some(deps) = root.get("dependencies").and_then(|v| v.as_array()) {
        let items: Vec<_> = deps
            .iter()
            .filter_map(|d| {
                d.as_str()
                    .map(|s| item(s, None))
                    .or_else(|| {
                        d.get("name")
                            .and_then(|n| n.as_str())
                            .map(|n| item(n, None))
                    })
            })
            .collect();
        if !items.is_empty() {
            sections.push(section("dependencies", "Dependencies", items));
        }
    }

    Ok(PackageManifestView {
        manifest_path: manifest_rel.to_string(),
        package_root: package_root.to_string(),
        ecosystem: "cpp".into(),
        title,
        subtitle: Some("vcpkg.json".into()),
        fields,
        sections,
        actions: vec![action("install", "Install deps", "vcpkg install")],
    })
}

fn parse_conan(manifest_rel: &str, package_root: &str, text: &str) -> Result<PackageManifestView> {
    let items: Vec<_> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('[') && !l.starts_with('#'))
        .map(|l| item(l, None))
        .collect();
    Ok(PackageManifestView {
        manifest_path: manifest_rel.to_string(),
        package_root: package_root.to_string(),
        ecosystem: "cpp".into(),
        title: "Conan".into(),
        subtitle: Some("conanfile.txt".into()),
        fields: vec![],
        sections: if items.is_empty() {
            vec![]
        } else {
            vec![section("requires", "Requires", items)]
        },
        actions: vec![action("install", "Install", "conan install .")],
    })
}

// --- helpers ---

fn field(label: &str, value: &str) -> ManifestField {
    ManifestField {
        label: label.to_string(),
        value: value.to_string(),
    }
}

fn push_opt_field(fields: &mut Vec<ManifestField>, label: &str, value: Option<&Value>) {
    if let Some(v) = value.and_then(|v| v.as_str()) {
        fields.push(field(label, v));
    }
}

fn item(name: &str, detail: Option<String>) -> ManifestItem {
    ManifestItem {
        name: name.to_string(),
        detail,
    }
}

fn section(id: &str, title: &str, items: Vec<ManifestItem>) -> ManifestSection {
    ManifestSection {
        id: id.to_string(),
        title: title.to_string(),
        items,
    }
}

fn action(id: &str, label: &str, command: &str) -> ManifestAction {
    ManifestAction {
        id: id.to_string(),
        label: label.to_string(),
        command: command.to_string(),
    }
}

fn push_dep_section(
    sections: &mut Vec<ManifestSection>,
    id: &str,
    title: &str,
    value: Option<&Value>,
) {
    let Some(table) = value.and_then(|v| v.as_table()) else {
        return;
    };
    let mut items: Vec<_> = table
        .iter()
        .map(|(name, spec)| item(name, Some(format_dep_spec(spec))))
        .collect();
    items.sort_by(|a, b| a.name.cmp(&b.name));
    if !items.is_empty() {
        sections.push(section(id, title, items));
    }
}

fn format_dep_spec(spec: &Value) -> String {
    match spec {
        Value::String(s) => s.clone(),
        Value::Integer(i) => i.to_string(),
        Value::Boolean(b) => b.to_string(),
        Value::Table(t) => {
            if t.get("workspace").and_then(|v| v.as_bool()) == Some(true) {
                return "workspace = true".into();
            }
            if let Some(path) = t.get("path").and_then(|v| v.as_str()) {
                return format!("path = \"{path}\"");
            }
            if let Some(version) = t.get("version").and_then(|v| v.as_str()) {
                return version.to_string();
            }
            "{ … }".into()
        }
        _ => spec.to_string(),
    }
}

fn extract_quoted(s: &str) -> Option<String> {
    s.split('"').nth(1).map(String::from).or_else(|| {
        s.split('\'').nth(1).map(String::from)
    })
}

fn parse_gem_line(line: &str) -> Option<(String, Option<String>)> {
    let rest = line.trim_start_matches("gem ").trim();
    let name = extract_quoted(rest)?;
    let ver = rest
        .split(',')
        .nth(1)
        .and_then(|p| extract_quoted(p.trim()));
    Some((name, ver))
}

fn parse_ruby_dep_call(line: &str) -> Option<(String, Option<String>)> {
    let open = line.find('(')? + 1;
    let close = line.rfind(')')?;
    let inner = &line[open..close];
    let name = extract_quoted(inner)?;
    let ver = inner.split(',').nth(1).and_then(|p| extract_quoted(p.trim()));
    Some((name, ver))
}

trait SectionExt {
    fn entry_or_extend(&mut self, id: &str, title: &str) -> &mut Vec<ManifestItem>;
}

impl SectionExt for Vec<ManifestSection> {
    fn entry_or_extend(&mut self, id: &str, title: &str) -> &mut Vec<ManifestItem> {
        if let Some(idx) = self.iter().position(|s| s.id == id) {
            return &mut self[idx].items;
        }
        self.push(section(id, title, vec![]));
        &mut self.last_mut().unwrap().items
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_cargo() {
        let text = r#"
[package]
name = "demo"
version = "0.1.0"
edition = "2021"

[dependencies]
serde = "1"
"#;
        let v = parse_cargo("Cargo.toml", "", text).unwrap();
        assert_eq!(v.title, "demo");
        assert_eq!(v.sections.len(), 1);
    }

    #[test]
    fn parses_go_mod() {
        let text = "module example.com/foo\ngo 1.22\n\nrequire (\n\tgithub.com/a/b v1.0.0\n)\n";
        let v = parse_go_mod("go.mod", "", text).unwrap();
        assert_eq!(v.title, "example.com/foo");
    }

    #[test]
    fn parses_rakefile() {
        let text = r#"
desc "Run tests"
task :test do
end

namespace :db do
  desc "Migrate"
  task :migrate do
  end
end
"#;
        let v = parse_rakefile("Rakefile", "", text).unwrap();
        assert_eq!(v.ecosystem, "rake");
        let tasks = &v.sections[0].items;
        assert!(tasks.iter().any(|t| t.name == "test"));
        assert!(tasks.iter().any(|t| t.name == "db:migrate"));
    }

    #[test]
    fn manifest_path_detection() {
        assert_eq!(manifest_kind_for_path("crates/foo/Cargo.toml"), Some("cargo"));
        assert_eq!(manifest_kind_for_path("Gemfile"), Some("ruby"));
        assert_eq!(manifest_kind_for_path("Rakefile"), Some("rake"));
        assert_eq!(manifest_kind_for_path("CMakeLists.txt"), Some("cmake"));
    }
}
