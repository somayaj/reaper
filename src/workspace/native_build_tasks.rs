use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde_json::Value;

use super::build_tasks::{BuildProjectNode, BuildTask, BuildTasksTree, task, task_labeled};
use super::gradle;

pub fn try_native_tree(
    ws: &Path,
    rel_path: &str,
    compose_content: Option<&str>,
) -> Result<Option<BuildTasksTree>> {
    if let Some(tree) = try_docker_tree(ws, rel_path, compose_content)? {
        return Ok(Some(tree));
    }
    if let Some(tree) = try_npm_tree(ws, rel_path)? {
        return Ok(Some(tree));
    }
    if let Some(tree) = try_pyproject_tree(ws, rel_path)? {
        return Ok(Some(tree));
    }
    if let Some(tree) = try_django_tree(ws, rel_path)? {
        return Ok(Some(tree));
    }
    if let Some(tree) = try_rake_tree(ws, rel_path)? {
        return Ok(Some(tree));
    }
    if let Some(tree) = try_go_tree(ws, rel_path)? {
        return Ok(Some(tree));
    }
    if let Some(tree) = try_cmake_tree(ws, rel_path)? {
        return Ok(Some(tree));
    }
    if let Some(tree) = try_make_tree(ws, rel_path)? {
        return Ok(Some(tree));
    }
    if let Some(tree) = try_meson_tree(ws, rel_path)? {
        return Ok(Some(tree));
    }
    if let Some(tree) = try_vcpkg_tree(ws, rel_path)? {
        return Ok(Some(tree));
    }
    if let Some(tree) = try_conan_tree(ws, rel_path)? {
        return Ok(Some(tree));
    }
    Ok(None)
}

pub fn find_nearest_manifest(
    ws: &Path,
    rel_path: &str,
    file_names: &[&str],
) -> Result<Option<(PathBuf, String)>> {
    let rel_path = super::normalize_workspace_source_path(rel_path);
    let mut dir = manifest_search_start(ws, &rel_path);

    loop {
        for name in file_names {
            if dir.join(name).is_file() {
                let rel_dir = gradle::rel_path_for(ws, &dir)?;
                let manifest_rel = if rel_dir.is_empty() {
                    (*name).to_string()
                } else {
                    format!("{rel_dir}/{name}")
                };
                return Ok(Some((dir, manifest_rel)));
            }
        }
        if dir == ws {
            break;
        }
        if !dir.pop() {
            break;
        }
        if !dir.starts_with(ws) {
            break;
        }
    }
    Ok(None)
}

fn manifest_search_start(ws: &Path, rel_path: &str) -> PathBuf {
    if rel_path.is_empty() {
        return ws.to_path_buf();
    }
    let path = Path::new(rel_path);
    if path
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| is_manifest_file_name(n))
    {
        return ws.join(path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new(".")));
    }
    let abs = ws.join(path);
    if abs.is_file() {
        ws.join(path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new(".")))
    } else {
        ws.join(path)
    }
}

fn is_manifest_file_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "pom.xml"
            | "build.gradle"
            | "build.gradle.kts"
            | "settings.gradle"
            | "settings.gradle.kts"
            | "package.json"
            | "pyproject.toml"
            | "manage.py"
            | "cargo.toml"
            | "rakefile"
            | "gemfile"
            | "go.mod"
            | "cmakelists.txt"
            | "meson.build"
            | "makefile"
            | "gnumakefile"
            | "docker-compose.yml"
            | "docker-compose.yaml"
            | "compose.yml"
            | "compose.yaml"
            | "dockerfile"
    )
}

const COMPOSE_FILE_NAMES: &[&str] = &[
    "docker-compose.yml",
    "docker-compose.yaml",
    "compose.yml",
    "compose.yaml",
];

const COMPOSE_FALLBACK_DIRS: &[&str] = &["docker", "deploy", "infra", "compose", ".docker"];

pub fn workspace_has_compose(ws: &Path) -> bool {
    for name in COMPOSE_FILE_NAMES {
        if ws.join(name).is_file() {
            return true;
        }
    }
    for sub in COMPOSE_FALLBACK_DIRS {
        let dir = ws.join(sub);
        if !dir.is_dir() {
            continue;
        }
        for name in COMPOSE_FILE_NAMES {
            if dir.join(name).is_file() {
                return true;
            }
        }
    }
    false
}

fn find_compose_manifest(ws: &Path, rel_path: &str) -> Result<Option<(PathBuf, String)>> {
    if let Some(found) = find_nearest_manifest(ws, rel_path, COMPOSE_FILE_NAMES)? {
        return Ok(Some(found));
    }
    if is_compose_manifest_path(rel_path) {
        let path = Path::new(rel_path);
        let dir = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let abs_dir = ws.join(dir);
        if abs_dir.is_dir() {
            return Ok(Some((abs_dir, rel_path.to_string())));
        }
    }
    for sub in COMPOSE_FALLBACK_DIRS {
        let dir = ws.join(sub);
        if !dir.is_dir() {
            continue;
        }
        for name in COMPOSE_FILE_NAMES {
            if dir.join(name).is_file() {
                return Ok(Some((dir.clone(), format!("{sub}/{name}"))));
            }
        }
    }
    for name in COMPOSE_FILE_NAMES {
        if ws.join(name).is_file() {
            return Ok(Some((ws.to_path_buf(), (*name).to_string())));
        }
    }
    Ok(None)
}

fn try_docker_tree(
    ws: &Path,
    rel_path: &str,
    compose_content: Option<&str>,
) -> Result<Option<BuildTasksTree>> {
    if let Some(tree) = try_docker_compose_tree(ws, rel_path, compose_content)? {
        return Ok(Some(tree));
    }
    try_dockerfile_tree(ws, rel_path)
}

pub fn is_compose_manifest_path(rel_path: &str) -> bool {
    let base = rel_path
        .replace('\\', "/")
        .rsplit('/')
        .next()
        .unwrap_or(rel_path)
        .to_ascii_lowercase();
    matches!(
        base.as_str(),
        "docker-compose.yml" | "docker-compose.yaml" | "compose.yml" | "compose.yaml"
    )
}

pub fn try_docker_compose_tree(
    ws: &Path,
    rel_path: &str,
    compose_content: Option<&str>,
) -> Result<Option<BuildTasksTree>> {
    let Some((_dir, manifest_path)) = find_compose_manifest(ws, rel_path)? else {
        return Ok(None);
    };
    let path = ws.join(&manifest_path);
    let rel_norm = rel_path.replace('\\', "/");
    let manifest_norm = manifest_path.replace('\\', "/");
    let use_overlay = compose_content.is_some() && rel_norm == manifest_norm;
    let text = if use_overlay {
        compose_content
            .filter(|content| !content.trim().is_empty())
            .map(String::from)
            .unwrap_or_else(|| std::fs::read_to_string(&path).unwrap_or_default())
    } else {
        std::fs::read_to_string(&path).with_context(|| format!("read {manifest_path}"))?
    };
    let services = parse_compose_services(&text);
    let project_name = parse_compose_project_name(&text)
        .or_else(|| {
            manifest_path
                .rsplit('/')
                .nth(1)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        })
        .unwrap_or_else(|| "compose".into());
    let mut tasks = docker_compose_base_tasks();
    for service in services {
        tasks.extend(docker_compose_service_tasks(&service));
    }
    Ok(Some(leaf_tree(
        ws,
        rel_path,
        "docker",
        &project_name,
        &manifest_path,
        tasks,
    )?))
}

fn try_dockerfile_tree(ws: &Path, rel_path: &str) -> Result<Option<BuildTasksTree>> {
    let Some((_dir, manifest_path)) = find_nearest_dockerfile(ws, rel_path)? else {
        return Ok(None);
    };
    let name = manifest_path
        .rsplit('/')
        .nth(1)
        .filter(|s| !s.is_empty())
        .unwrap_or("docker")
        .to_string();
    let tag = docker_image_tag_from_path(&manifest_path);
    let mut tasks = vec![
        task_labeled("build", "docker build", &format!("docker build -t {tag} ."), "lifecycle"),
        task_labeled(
            "build-no-cache",
            "docker build (no cache)",
            &format!("docker build --no-cache -t {tag} ."),
            "lifecycle",
        ),
        task_labeled(
            "run",
            "docker run",
            &format!("docker run --rm -it {tag}"),
            "application",
        ),
    ];
    tasks.extend(docker_cleanup_tasks());
    Ok(Some(leaf_tree(
        ws,
        rel_path,
        "docker",
        &name,
        &manifest_path,
        tasks,
    )?))
}

fn find_nearest_dockerfile(ws: &Path, rel_path: &str) -> Result<Option<(PathBuf, String)>> {
    for name in ["Dockerfile", "dockerfile"] {
        if let Some(found) = find_nearest_manifest(ws, rel_path, &[name])? {
            return Ok(Some(found));
        }
    }
    Ok(None)
}

fn docker_compose_cmd(args: &str) -> String {
    format!(
        "if docker compose version >/dev/null 2>&1; then docker compose {args}; \
         elif command -v docker-compose >/dev/null 2>&1; then docker-compose {args}; \
         else docker compose {args}; fi"
    )
}

fn docker_compose_base_tasks() -> Vec<BuildTask> {
    vec![
        task_labeled("up", "docker compose up", &docker_compose_cmd("up"), "lifecycle"),
        task_labeled(
            "up-d",
            "docker compose up -d",
            &docker_compose_cmd("up -d"),
            "lifecycle",
        ),
        task_labeled(
            "apply",
            "docker compose apply",
            &docker_compose_cmd("up -d --force-recreate"),
            "lifecycle",
        ),
        task_labeled(
            "down",
            "docker compose down",
            &docker_compose_cmd("down"),
            "lifecycle",
        ),
        task_labeled(
            "down-v",
            "docker compose down -v",
            &docker_compose_cmd("down -v"),
            "lifecycle",
        ),
        task_labeled(
            "build",
            "docker compose build",
            &docker_compose_cmd("build"),
            "lifecycle",
        ),
        task_labeled(
            "build-no-cache",
            "docker compose build (no cache)",
            &docker_compose_cmd("build --no-cache"),
            "lifecycle",
        ),
        task_labeled(
            "pull",
            "docker compose pull",
            &docker_compose_cmd("pull"),
            "lifecycle",
        ),
        task_labeled("ps", "docker ps", "docker ps", "status"),
        task_labeled(
            "stop",
            "docker compose stop",
            &docker_compose_cmd("stop"),
            "lifecycle",
        ),
        task_labeled(
            "start",
            "docker compose start",
            &docker_compose_cmd("start"),
            "lifecycle",
        ),
        task_labeled(
            "restart",
            "docker compose restart",
            &docker_compose_cmd("restart"),
            "lifecycle",
        ),
        task_labeled(
            "logs",
            "docker compose logs",
            &docker_compose_cmd("logs --tail=100"),
            "logs",
        ),
        task_labeled(
            "logs-follow",
            "docker compose logs -f",
            &docker_compose_cmd("logs -f --tail=100"),
            "logs",
        ),
    ]
    .into_iter()
    .chain(docker_cleanup_tasks())
    .collect()
}

fn docker_cleanup_tasks() -> Vec<BuildTask> {
    vec![
        task_labeled(
            "kill-all",
            "docker kill all",
            "docker ps -q | xargs docker kill 2>/dev/null || true",
            "cleanup",
        ),
        task_labeled(
            "prune",
            "docker prune",
            "docker system prune -f",
            "cleanup",
        ),
        task_labeled(
            "prune-all",
            "docker prune all",
            "docker system prune -af --volumes",
            "cleanup",
        ),
    ]
}

fn docker_compose_service_tasks(service: &str) -> Vec<BuildTask> {
    vec![
        task_labeled(
            &format!("logs-{service}"),
            &format!("docker compose logs {service}"),
            &docker_compose_cmd(&format!("logs -f --tail=100 {service}")),
            "logs",
        ),
        task_labeled(
            &format!("up-{service}"),
            &format!("docker compose up {service}"),
            &docker_compose_cmd(&format!("up -d --force-recreate {service}")),
            "lifecycle",
        ),
        task_labeled(
            &format!("restart-{service}"),
            &format!("docker compose restart {service}"),
            &docker_compose_cmd(&format!("restart {service}")),
            "lifecycle",
        ),
    ]
}

fn parse_compose_services(text: &str) -> Vec<String> {
    let Ok(value) = serde_yaml::from_str::<serde_yaml::Value>(text) else {
        return Vec::new();
    };
    value
        .get("services")
        .and_then(|s| s.as_mapping())
        .map(|mapping| {
            mapping
                .keys()
                .filter_map(|k| k.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn parse_compose_project_name(text: &str) -> Option<String> {
    let value = serde_yaml::from_str::<serde_yaml::Value>(text).ok()?;
    value
        .get("name")
        .and_then(|n| n.as_str())
        .map(str::to_string)
}

fn docker_image_tag_from_path(manifest_path: &str) -> String {
    manifest_path
        .rsplit('/')
        .nth(1)
        .filter(|s| !s.is_empty())
        .unwrap_or("app")
        .to_ascii_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
        .collect()
}

fn leaf_tree(
    ws: &Path,
    rel_path: &str,
    build_tool: &str,
    name: &str,
    manifest_path: &str,
    tasks: Vec<BuildTask>,
) -> Result<BuildTasksTree> {
    let focus = focus_for_manifest(ws, rel_path, manifest_path);
    Ok(BuildTasksTree {
        build_tool: build_tool.into(),
        root_name: name.to_string(),
        root_path: manifest_path.to_string(),
        focus_module: focus,
        tree: BuildProjectNode {
            name: name.to_string(),
            path: manifest_path.to_string(),
            kind: format!("{build_tool}-root"),
            tasks,
            children: Vec::new(),
        },
    })
}

fn focus_for_manifest(ws: &Path, rel_path: &str, manifest_path: &str) -> String {
    let rel_path = super::normalize_workspace_source_path(rel_path);
    if rel_path.eq_ignore_ascii_case(manifest_path) {
        return manifest_path
            .rsplit_once('/')
            .map(|(parent, _)| parent.to_string())
            .unwrap_or_default();
    }
    if is_native_source_path(&rel_path) {
        if let Ok(abs) = super::safe_join(ws, &rel_path) {
            if let Some(parent) = abs.parent() {
                if let Ok(rel) = gradle::rel_path_for(ws, parent) {
                    return rel;
                }
            }
        }
    }
    String::new()
}

fn try_npm_tree(ws: &Path, rel_path: &str) -> Result<Option<BuildTasksTree>> {
    let Some((dir, manifest_path)) = find_nearest_manifest(ws, rel_path, &["package.json"])? else {
        return Ok(None);
    };
    let text = std::fs::read_to_string(dir.join("package.json"))
        .with_context(|| format!("read {manifest_path}"))?;
    let pkg: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
    let name = pkg
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("project")
        .to_string();
    let pm = detect_node_package_manager(&dir);
    let mut tasks = Vec::new();
    tasks.push(task(
        "install",
        &node_install_command(&pm),
        "lifecycle",
    ));
    if let Some(scripts) = pkg.get("scripts").and_then(|v| v.as_object()) {
        for script_name in scripts.keys() {
            let group = npm_script_group(script_name);
            tasks.push(task(
                script_name,
                &node_run_script_command(&pm, script_name),
                group,
            ));
        }
    } else {
        tasks.extend(default_node_tasks(&pm));
    }
    Ok(Some(leaf_tree(
        ws,
        rel_path,
        &pm,
        &name,
        &manifest_path,
        tasks,
    )?))
}

fn detect_node_package_manager(dir: &Path) -> String {
    if dir.join("pnpm-lock.yaml").is_file() {
        "pnpm".into()
    } else if dir.join("yarn.lock").is_file() {
        "yarn".into()
    } else if dir.join("bun.lockb").is_file() || dir.join("bun.lock").is_file() {
        "bun".into()
    } else {
        "npm".into()
    }
}

fn node_install_command(pm: &str) -> String {
    match pm {
        "pnpm" => "pnpm install".into(),
        "yarn" => "yarn install".into(),
        "bun" => "bun install".into(),
        _ => "npm install".into(),
    }
}

fn node_run_script_command(pm: &str, script: &str) -> String {
    match pm {
        "pnpm" => format!("pnpm run {script}"),
        "yarn" => format!("yarn {script}"),
        "bun" => format!("bun run {script}"),
        _ if script == "start" || script == "test" || script == "restart" => {
            format!("npm {script}")
        }
        _ => format!("npm run {script}"),
    }
}

fn default_node_tasks(pm: &str) -> Vec<BuildTask> {
    let mut tasks = Vec::new();
    for script in ["start", "test", "build", "dev", "lint"] {
        tasks.push(task(
            script,
            &node_run_script_command(pm, script),
            npm_script_group(script),
        ));
    }
    tasks
}

fn npm_script_group(name: &str) -> &'static str {
    match name {
        "start" | "dev" | "serve" => "application",
        "test" | "lint" | "check" | "typecheck" => "verification",
        "build" | "compile" | "prepare" | "prepublishOnly" => "lifecycle",
        _ => "scripts",
    }
}

fn try_cargo_tree(ws: &Path, rel_path: &str) -> Result<Option<BuildTasksTree>> {
    let Some((dir, manifest_path)) = find_nearest_manifest(ws, rel_path, &["Cargo.toml"])? else {
        return Ok(None);
    };
    let text = std::fs::read_to_string(dir.join("Cargo.toml"))
        .with_context(|| format!("read {manifest_path}"))?;
    let name = parse_cargo_package_name(&text).unwrap_or_else(|| "crate".into());
    let mut tasks = vec![
        task("build", "cargo build", "lifecycle"),
        task("test", "cargo test", "verification"),
        task("run", "cargo run", "application"),
        task("check", "cargo check", "verification"),
        task("clippy", "cargo clippy", "verification"),
    ];
    for bin in parse_cargo_bins(&text) {
        tasks.push(task(
            &format!("run-{bin}"),
            &format!("cargo run --bin {bin}"),
            "application",
        ));
    }
    Ok(Some(leaf_tree(
        ws,
        rel_path,
        "cargo",
        &name,
        &manifest_path,
        tasks,
    )?))
}

fn parse_cargo_package_name(text: &str) -> Option<String> {
    let table: toml::Value = toml::from_str(text).ok()?;
    table
        .get("package")?
        .get("name")?
        .as_str()
        .map(String::from)
}

fn parse_cargo_bins(text: &str) -> Vec<String> {
    let Ok(table) = toml::from_str::<toml::Value>(text) else {
        return Vec::new();
    };
    let Some(bins) = table.get("bin").and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    bins.iter()
        .filter_map(|b| b.get("name").and_then(|n| n.as_str()).map(String::from))
        .collect()
}

pub fn detect_python_package_manager(dir: &Path, root: &toml::Value) -> String {
    if dir.join("Pipfile").is_file() {
        return "pipenv".into();
    }
    if dir.join("uv.lock").is_file() {
        return "uv".into();
    }
    if let Some(tool) = root.get("tool").and_then(|v| v.as_table()) {
        if tool.contains_key("uv") {
            return "uv".into();
        }
        if tool.contains_key("pdm") {
            return "pdm".into();
        }
        if tool.contains_key("poetry") {
            return "poetry".into();
        }
    }
    "pip".into()
}

fn format_python_shell(path: &str) -> String {
    if path.contains([' ', '\'', '"']) {
        shell_quote_path(path)
    } else {
        path.to_string()
    }
}

pub fn find_project_venv_python(dir: &Path) -> Option<String> {
    for venv_name in [".venv", "venv"] {
        let bin = dir.join(venv_name).join("bin");
        for name in ["python", "python3"] {
            let candidate = bin.join(name);
            if candidate.is_file() {
                return Some(format_python_shell(&candidate.to_string_lossy()));
            }
        }
    }
    None
}

fn venv_python_relative(dir: &Path) -> Option<String> {
    for venv_name in [".venv", "venv"] {
        let bin = dir.join(venv_name).join("bin");
        if bin.join("python").is_file() {
            return Some(format!("{venv_name}/bin/python"));
        }
        if bin.join("python3").is_file() {
            return Some(format!("{venv_name}/bin/python3"));
        }
    }
    None
}

fn has_project_venv(dir: &Path) -> bool {
    venv_python_relative(dir).is_some()
}

/// Prefer project `.venv`/`venv`, else Settings → Compiler python, else `python3`.
pub fn python_interpreter_for_project(project_dir: Option<&Path>) -> String {
    if let Some(dir) = project_dir {
        if let Some(rel) = venv_python_relative(dir) {
            return rel;
        }
    }
    python_interpreter_shell()
}

pub fn python_pip_command(project_dir: Option<&Path>, args: &str) -> String {
    format!(
        "{} -m pip {args}",
        python_interpreter_for_project(project_dir)
    )
}

pub fn python_requirements_install_command(project_dir: Option<&Path>) -> String {
    match project_dir {
        Some(dir) if has_project_venv(dir) => {
            python_pip_command(Some(dir), "install -r requirements.txt")
        }
        Some(_) => {
            let boot = python_interpreter_shell();
            format!(
                "{boot} -m venv .venv && .venv/bin/python -m pip install -U pip && .venv/bin/python -m pip install -r requirements.txt"
            )
        }
        None => python_pip_command(None, "install -r requirements.txt"),
    }
}

pub fn python_install_command(pm: &str, project_dir: Option<&Path>) -> String {
    match pm {
        "pipenv" => "pipenv install".into(),
        "uv" => "uv sync".into(),
        "poetry" => "poetry install".into(),
        "pdm" => "pdm install".into(),
        _ if project_dir.is_some_and(has_project_venv) => {
            python_pip_command(project_dir, "install -e .")
        }
        _ if project_dir.is_some() => {
            let boot = python_interpreter_shell();
            format!(
                "{boot} -m venv .venv && .venv/bin/python -m pip install -U pip && .venv/bin/python -m pip install -e ."
            )
        }
        _ => python_pip_command(None, "install -e ."),
    }
}

pub fn python_run_command(pm: &str, script: &str) -> String {
    match pm {
        "pipenv" => format!("pipenv run {script}"),
        "uv" => format!("uv run {script}"),
        "poetry" => format!("poetry run {script}"),
        "pdm" => format!("pdm run {script}"),
        _ => script.to_string(),
    }
}

pub fn python_interpreter_shell() -> String {
    match crate::toolchain::resolve_program("python") {
        Some(path) => {
            let s = path.to_string_lossy();
            if s.contains([' ', '\'', '"']) {
                shell_quote_path(&s)
            } else {
                s.into_owned()
            }
        }
        None => "python3".into(),
    }
}

pub fn python_run_file_command(project_dir: Option<&Path>, pm: &str, rel_path: &str) -> String {
    let quoted = shell_quote_path(rel_path);
    let py = python_interpreter_for_project(project_dir);
    python_run_command(pm, &format!("{py} {quoted}"))
}

pub fn python_module_command(project_dir: Option<&Path>, pm: &str, module: &str, args: &str) -> String {
    let py = python_interpreter_for_project(project_dir);
    let inner = if args.trim().is_empty() {
        format!("{py} -m {module}")
    } else {
        format!("{py} -m {module} {args}")
    };
    python_run_command(pm, &inner)
}

pub fn python_pytest_command(project_dir: Option<&Path>, pm: &str, rel_path: &str) -> String {
    let quoted = shell_quote_path(rel_path);
    match pm {
        "pipenv" => format!("pipenv run pytest {quoted}"),
        "uv" => format!("uv run pytest {quoted}"),
        "poetry" => format!("poetry run pytest {quoted}"),
        "pdm" => format!("pdm run pytest {quoted}"),
        _ => python_module_command(project_dir, pm, "pytest", &quoted),
    }
}

pub fn python_pytest_all_command(project_dir: Option<&Path>, pm: &str) -> String {
    match pm {
        "pipenv" => "pipenv run pytest".into(),
        "uv" => "uv run pytest".into(),
        "poetry" => "poetry run pytest".into(),
        "pdm" => "pdm run pytest".into(),
        _ => python_module_command(project_dir, pm, "pytest", ""),
    }
}

pub fn python_ruff_command(project_dir: Option<&Path>, pm: &str, args: &str) -> String {
    match pm {
        "pipenv" => format!("pipenv run ruff {args}"),
        "uv" => format!("uv run ruff {args}"),
        "poetry" => format!("poetry run ruff {args}"),
        "pdm" => format!("pdm run ruff {args}"),
        _ => python_module_command(project_dir, pm, "ruff", args),
    }
}

pub fn python_package_manager_at(ws: &Path, rel_path: &str) -> Result<(Option<PathBuf>, String)> {
    if let Some((dir, _)) = find_nearest_manifest(ws, rel_path, &["pyproject.toml"])? {
        let pm = std::fs::read_to_string(dir.join("pyproject.toml"))
            .ok()
            .and_then(|text| toml::from_str::<toml::Value>(&text).ok())
            .map(|root| detect_python_package_manager(&dir, &root))
            .unwrap_or_else(|| "pip".into());
        return Ok((Some(dir), pm));
    }
    if let Some((dir, _)) = find_nearest_manifest(ws, rel_path, &["Pipfile"])? {
        return Ok((Some(dir), "pipenv".into()));
    }
    Ok((None, "pip".into()))
}

fn shell_quote_path(path: &str) -> String {
    format!("'{}'", path.replace('\'', "'\\''"))
}

fn parse_pyproject_name(root: &toml::Value) -> Option<String> {
    root.get("project")?
        .get("name")?
        .as_str()
        .map(String::from)
        .or_else(|| {
            root.get("tool")?
                .get("poetry")?
                .get("name")?
                .as_str()
                .map(String::from)
        })
}

fn collect_pyproject_script_tasks(root: &toml::Value, pm: &str) -> Vec<BuildTask> {
    let mut tasks = Vec::new();
    let mut seen = std::collections::BTreeSet::new();

    let mut push_script = |name: &str, command: &str, group: &str| {
        if name.is_empty() || !seen.insert(name.to_string()) {
            return;
        }
        tasks.push(task(name, command, group));
    };

    if let Some(scripts) = root.get("project").and_then(|v| v.get("scripts")).and_then(|v| v.as_table())
    {
        for name in scripts.keys() {
            push_script(name, &python_run_command(pm, name), python_script_group(name));
        }
    }
    if let Some(scripts) = root
        .get("tool")
        .and_then(|v| v.get("poetry"))
        .and_then(|v| v.get("scripts"))
        .and_then(|v| v.as_table())
    {
        for name in scripts.keys() {
            push_script(name, &python_run_command(pm, name), python_script_group(name));
        }
    }
    if let Some(scripts) = root
        .get("tool")
        .and_then(|v| v.get("pdm"))
        .and_then(|v| v.get("scripts"))
        .and_then(|v| v.as_table())
    {
        for (name, spec) in scripts {
            let command = pdm_script_command(spec).unwrap_or_else(|| python_run_command(pm, name));
            push_script(name, &command, python_script_group(name));
        }
    }
    if let Some(scripts) = root
        .get("tool")
        .and_then(|v| v.get("taskipy"))
        .and_then(|v| v.get("tasks"))
        .and_then(|v| v.as_table())
    {
        for (name, spec) in scripts {
            if let Some(command) = spec.as_str() {
                push_script(name, command, python_script_group(name));
            }
        }
    }
    if let Some(scripts) = root
        .get("tool")
        .and_then(|v| v.get("hatch"))
        .and_then(|v| v.get("envs"))
        .and_then(|v| v.get("default"))
        .and_then(|v| v.get("scripts"))
        .and_then(|v| v.as_table())
    {
        for (name, spec) in scripts {
            if let Some(command) = spec.as_str() {
                push_script(name, command, python_script_group(name));
            }
        }
    }

    tasks
}

fn pdm_script_command(spec: &toml::Value) -> Option<String> {
    if let Some(cmd) = spec.as_str() {
        return Some(cmd.to_string());
    }
    spec.as_table()?
        .get("cmd")?
        .as_str()
        .map(String::from)
}

fn python_script_group(name: &str) -> &'static str {
    match name {
        "test" | "tests" | "lint" | "check" | "typecheck" | "mypy" | "pytest" => "verification",
        "serve" | "server" | "run" | "start" | "dev" | "shell" | "console" => "application",
        "build" | "install" | "sync" | "format" | "fmt" => "lifecycle",
        _ => "scripts",
    }
}

fn default_python_tasks(project_dir: &Path, pm: &str) -> Vec<BuildTask> {
    let dir = Some(project_dir);
    vec![
        task("test", &python_pytest_all_command(dir, pm), "verification"),
        task("lint", &python_ruff_command(dir, pm, "check ."), "verification"),
    ]
}

fn django_tasks(project_dir: &Path) -> Vec<BuildTask> {
    let py = python_interpreter_for_project(Some(project_dir));
    vec![
        task("runserver", &format!("{py} manage.py runserver"), "application"),
        task("test", &format!("{py} manage.py test"), "verification"),
        task("migrate", &format!("{py} manage.py migrate"), "database"),
        task("shell", &format!("{py} manage.py shell"), "application"),
    ]
}

fn try_pyproject_tree(ws: &Path, rel_path: &str) -> Result<Option<BuildTasksTree>> {
    let Some((dir, manifest_path)) = find_nearest_manifest(ws, rel_path, &["pyproject.toml"])? else {
        return Ok(None);
    };
    let text = std::fs::read_to_string(dir.join("pyproject.toml"))
        .with_context(|| format!("read {manifest_path}"))?;
    let root: toml::Value = toml::from_str(&text).unwrap_or(toml::Value::Table(Default::default()));
    let name = parse_pyproject_name(&root).unwrap_or_else(|| "project".to_string());
    let pm = detect_python_package_manager(&dir, &root);
    let mut tasks = vec![task(
        "install",
        &python_install_command(&pm, Some(&dir)),
        "lifecycle",
    )];
    let mut script_tasks = collect_pyproject_script_tasks(&root, &pm);
    if script_tasks.is_empty() {
        tasks.extend(default_python_tasks(&dir, &pm));
    } else {
        tasks.append(&mut script_tasks);
    }
    if dir.join("manage.py").is_file() {
        tasks.extend(django_tasks(&dir));
    }
    Ok(Some(leaf_tree(
        ws,
        rel_path,
        &pm,
        &name,
        &manifest_path,
        tasks,
    )?))
}

fn try_django_tree(ws: &Path, rel_path: &str) -> Result<Option<BuildTasksTree>> {
    let Some((dir, manifest_path)) = find_nearest_manifest(ws, rel_path, &["manage.py"])? else {
        return Ok(None);
    };
    if dir.join("pyproject.toml").is_file() {
        return Ok(None);
    }
    let tasks = django_tasks(&dir);
    Ok(Some(leaf_tree(
        ws,
        rel_path,
        "django",
        "Django app",
        &manifest_path,
        tasks,
    )?))
}

fn try_rake_tree(ws: &Path, rel_path: &str) -> Result<Option<BuildTasksTree>> {
    let Some((dir, manifest_path)) =
        find_nearest_manifest(ws, rel_path, &["Rakefile", "rakefile"])?
    else {
        return Ok(None);
    };
    let text = std::fs::read_to_string(dir.join(
        manifest_path
            .rsplit('/')
            .next()
            .unwrap_or("Rakefile"),
    ))
    .with_context(|| format!("read {manifest_path}"))?;
    let is_rails = dir.join("config/application.rb").is_file()
        || text.contains("Rails.application.load_tasks")
        || text.contains("rails/tasks");
    let build_tool = if is_rails { "rails" } else { "rake" };
    let name = if is_rails { "Rails app" } else { "Rakefile" };
    let mut tasks = parse_rake_tasks(&text);
    if tasks.is_empty() {
        tasks.extend(default_rake_tasks(is_rails));
    }
    Ok(Some(leaf_tree(
        ws,
        rel_path,
        build_tool,
        name,
        &manifest_path,
        tasks,
    )?))
}

fn default_rake_tasks(is_rails: bool) -> Vec<BuildTask> {
    if is_rails {
        vec![
            task("server", "bin/rails server", "application"),
            task("test", "bin/rails test", "verification"),
            task("console", "bin/rails console", "application"),
            task("db-migrate", "bin/rails db:migrate", "database"),
            task("routes", "bin/rails routes", "info"),
        ]
    } else {
        vec![
            task("list", "rake -T", "info"),
            task("default", "rake default", "lifecycle"),
            task("test", "rake test", "verification"),
        ]
    }
}

fn parse_rake_tasks(text: &str) -> Vec<BuildTask> {
    let mut out = Vec::new();
    let mut namespace: Option<String> = None;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if trimmed.starts_with("desc ") {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("namespace ") {
            namespace = parse_rake_symbol(rest.split_whitespace().next().unwrap_or(""));
            continue;
        }
        if trimmed == "end" {
            namespace = None;
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("task ") {
            if let Some(name) = parse_rake_task_name(rest) {
                let full = match &namespace {
                    Some(ns) => format!("{ns}:{name}"),
                    None => name,
                };
                let cmd = format!("rake {full}");
                out.push(task(
                    &full,
                    &cmd,
                    "tasks",
                ));
            }
        }
    }
    out
}

fn try_bundler_tree(ws: &Path, rel_path: &str) -> Result<Option<BuildTasksTree>> {
    let Some((dir, manifest_path)) = find_nearest_manifest(ws, rel_path, &["Gemfile"])? else {
        return Ok(None);
    };
    let text = std::fs::read_to_string(dir.join("Gemfile"))
        .with_context(|| format!("read {manifest_path}"))?;
    let is_rails = dir.join("config/application.rb").is_file()
        || dir.join("config/routes.rb").is_file()
        || text.contains("rails");
    let name = if is_rails { "Rails app" } else { "Ruby project" };
    let build_tool = if is_rails { "rails" } else { "ruby" };
    let mut tasks = vec![
        task("install", "bundle install", "lifecycle"),
        task("update", "bundle update", "lifecycle"),
        task("exec", "bundle exec ruby -v", "verification"),
    ];
    if is_rails {
        tasks.extend([
            task("server", "bin/rails server", "application"),
            task("test", "bin/rails test", "verification"),
            task("console", "bin/rails console", "application"),
        ]);
    }
    Ok(Some(leaf_tree(
        ws,
        rel_path,
        build_tool,
        name,
        &manifest_path,
        tasks,
    )?))
}

fn try_go_tree(ws: &Path, rel_path: &str) -> Result<Option<BuildTasksTree>> {
    let Some((dir, manifest_path)) = find_nearest_manifest(ws, rel_path, &["go.mod"])? else {
        return Ok(None);
    };
    let text = std::fs::read_to_string(dir.join("go.mod"))
        .with_context(|| format!("read {manifest_path}"))?;
    let name = text
        .lines()
        .find_map(|line| line.trim().strip_prefix("module "))
        .unwrap_or("module")
        .trim()
        .to_string();
    let go = go_program_shell();
    let mut tasks = vec![
        task("build", &format!("{go} build ./..."), "lifecycle"),
        task("test", &format!("{go} test ./..."), "verification"),
        task("run", &format!("{go} run ."), "application"),
        task("vet", &format!("{go} vet ./..."), "verification"),
        task("fmt", &format!("{go} fmt ./..."), "lifecycle"),
        task("tidy", &format!("{go} mod tidy"), "lifecycle"),
    ];
    tasks.extend(collect_go_cmd_tasks(&dir, &go));
    Ok(Some(leaf_tree(
        ws,
        rel_path,
        "go",
        &name,
        &manifest_path,
        tasks,
    )?))
}

pub fn go_program_shell() -> String {
    match crate::toolchain::resolve_program("go") {
        Some(path) => {
            let s = path.to_string_lossy();
            if s.contains([' ', '\'', '"']) {
                shell_quote_path(&s)
            } else {
                s.into_owned()
            }
        }
        None => "go".into(),
    }
}

pub fn go_module_root(ws: &Path, rel_path: &str) -> Result<Option<PathBuf>> {
    Ok(find_nearest_manifest(ws, rel_path, &["go.mod"])?.map(|(dir, _)| dir))
}

pub fn go_run_file_command(rel_path: &str) -> String {
    format!("{} run {}", go_program_shell(), shell_quote_path(rel_path))
}

pub fn go_test_command(ws: &Path, rel_path: &str) -> Result<String> {
    let go = go_program_shell();
    let Some((root, _)) = find_nearest_manifest(ws, rel_path, &["go.mod"])? else {
        return Ok(format!("{go} test -v"));
    };
    let abs = ws.join(rel_path);
    let pkg = abs
        .parent()
        .and_then(|parent| parent.strip_prefix(&root).ok())
        .map(|p| {
            let rel = p.to_string_lossy().replace('\\', "/");
            if rel.is_empty() {
                ".".into()
            } else {
                format!("./{rel}")
            }
        })
        .unwrap_or_else(|| ".".into());
    Ok(format!("{go} test {pkg} -v"))
}

pub fn is_cpp_source_path(rel_path: &str) -> bool {
    let lower = rel_path.to_lowercase();
    lower.ends_with(".cpp")
        || lower.ends_with(".cc")
        || lower.ends_with(".cxx")
        || lower.ends_with(".c++")
}

pub fn is_c_source_path(rel_path: &str) -> bool {
    rel_path.to_lowercase().ends_with(".c")
}

pub fn is_native_source_path(rel_path: &str) -> bool {
    is_c_source_path(rel_path) || is_cpp_source_path(rel_path)
}

fn toolchain_program_shell(path: &Path) -> String {
    let s = path.to_string_lossy();
    if s.contains([' ', '\'', '"']) {
        shell_quote_path(&s)
    } else {
        s.into_owned()
    }
}

pub fn native_compiler_shell(is_cpp: bool) -> String {
    if is_cpp {
        if let Some(clang) = crate::toolchain::resolve_program("clang") {
            let clangxx = clang.parent().map(|p| p.join("clang++"));
            if clangxx.as_ref().is_some_and(|p| p.is_file()) {
                return toolchain_program_shell(&clangxx.unwrap());
            }
            return toolchain_program_shell(&clang);
        }
        if let Some(gcc) = crate::toolchain::resolve_program("gcc") {
            let gxx = gcc.parent().map(|p| p.join("g++"));
            if gxx.as_ref().is_some_and(|p| p.is_file()) {
                return toolchain_program_shell(&gxx.unwrap());
            }
        }
        return "clang++".into();
    }
    crate::toolchain::resolve_program("clang")
        .or_else(|| crate::toolchain::resolve_program("gcc"))
        .map(|p| toolchain_program_shell(&p))
        .unwrap_or_else(|| "clang".into())
}

pub fn native_project_root(ws: &Path, rel_path: &str) -> Result<Option<(PathBuf, String)>> {
    if let Some((dir, _)) = find_nearest_manifest(ws, rel_path, &["CMakeLists.txt"])? {
        return Ok(Some((dir, "cmake".into())));
    }
    for file in ["Makefile", "makefile", "GNUmakefile"] {
        if let Some((dir, _)) = find_nearest_manifest(ws, rel_path, &[file])? {
            return Ok(Some((dir, "make".into())));
        }
    }
    if let Some((dir, _)) = find_nearest_manifest(ws, rel_path, &["meson.build"])? {
        return Ok(Some((dir, "meson".into())));
    }
    Ok(None)
}

pub fn native_run_command(ws: &Path, rel_path: &str, is_cpp: bool) -> String {
    if let Ok(Some(cmd)) = native_cmake_run_command(ws, rel_path) {
        return cmd;
    }
    native_single_file_run_command(rel_path, is_cpp)
}

/// True when `native_run_command` would delegate to CMake for this source file.
pub fn native_run_uses_cmake(ws: &Path, rel_path: &str) -> bool {
    native_cmake_run_command(ws, rel_path)
        .ok()
        .flatten()
        .is_some()
}

pub fn native_cmake_run_command(ws: &Path, rel_path: &str) -> Result<Option<String>> {
    let Some((dir, _manifest)) = find_nearest_manifest(ws, rel_path, &["CMakeLists.txt"])? else {
        return Ok(None);
    };
    let cmake_text = std::fs::read_to_string(dir.join("CMakeLists.txt"))
        .with_context(|| format!("read {}", dir.join("CMakeLists.txt").display()))?;
    let Some(target) = cmake_executable_for_source(&cmake_text, rel_path) else {
        return Ok(None);
    };
    let project_rel = gradle::rel_path_for(ws, &dir)?;
    let (build_dir, source_dir) = if project_rel.is_empty() {
        ("build".to_string(), ".".to_string())
    } else {
        (format!("{project_rel}/build"), project_rel)
    };
    Ok(Some(format!(
        "cmake -B {build_dir} -S {source_dir} && cmake --build {build_dir} --target {target} && ./{build_dir}/{target}"
    )))
}

pub fn cmake_executable_for_source(cmake_text: &str, source_rel: &str) -> Option<String> {
    let source = source_rel.replace('\\', "/");
    let mut buf = String::new();
    let mut in_exec = false;
    for line in cmake_text.lines() {
        let trimmed = line.split('#').next().unwrap_or(line).trim();
        if trimmed.is_empty() {
            continue;
        }
        if !in_exec {
            if let Some(rest) = trimmed.strip_prefix("add_executable(") {
                in_exec = true;
                buf.push_str(rest);
            }
        } else {
            if !buf.is_empty() {
                buf.push(' ');
            }
            buf.push_str(trimmed);
        }
        if in_exec {
            let depth = buf.chars().filter(|&c| c == '(').count();
            let closes = buf.chars().filter(|&c| c == ')').count();
            if closes > depth {
                let inner = buf.trim().trim_end_matches(')');
                if let Some(target) = parse_add_executable_args(inner, &source) {
                    return Some(target);
                }
                in_exec = false;
                buf.clear();
            }
        }
    }
    None
}

fn parse_add_executable_args(inner: &str, source_rel: &str) -> Option<String> {
    let mut args = split_cmake_list_args(inner);
    let target = args.next()?.trim().to_string();
    if target.is_empty() {
        return None;
    }
    if args.any(|a| cmake_source_matches(a.trim(), source_rel)) {
        Some(target)
    } else {
        None
    }
}

fn cmake_source_matches(arg: &str, source_rel: &str) -> bool {
    let arg = arg.trim().trim_matches('"').replace('\\', "/");
    let source = source_rel.replace('\\', "/");
    arg == source
        || source.ends_with(&format!("/{arg}"))
        || arg.ends_with(&format!("/{source}"))
}

fn split_cmake_list_args(inner: &str) -> impl Iterator<Item = &str> {
    inner.split_whitespace()
}

fn native_single_file_run_command(rel_path: &str, is_cpp: bool) -> String {
    let compiler = native_compiler_shell(is_cpp);
    let quoted = shell_quote_path(rel_path);
    let out = ".reaper/native-out";
    let std_flag = if is_cpp { "-std=c++17" } else { "-std=c17" };
    let lang_flag = if is_cpp && !compiler.contains("++") {
        " -x c++"
    } else {
        ""
    };
    format!(
        "mkdir -p .reaper && {compiler} {std_flag}{lang_flag} -o {out} {quoted} && ./{out}"
    )
}

pub fn native_gtest_command(rel_path: &str, is_cpp: bool) -> String {
    let compiler = native_compiler_shell(is_cpp);
    let quoted = shell_quote_path(rel_path);
    let out = ".reaper/native-test-out";
    let std_flag = if is_cpp { "-std=c++17" } else { "-std=c11" };
    let lang_flag = if is_cpp && !compiler.contains("++") {
        " -x c++"
    } else {
        ""
    };
    format!(
        "mkdir -p .reaper && {compiler} {std_flag}{lang_flag} {quoted} -lgtest -lgtest_main -pthread -o {out} && ./{out}"
    )
}

pub fn native_catch2_command(rel_path: &str) -> String {
    let compiler = native_compiler_shell(true);
    let quoted = shell_quote_path(rel_path);
    let out = ".reaper/native-test-out";
    format!(
        "mkdir -p .reaper && {compiler} -std=c++17 -DCATCH_CONFIG_MAIN {quoted} -o {out} && ./{out}"
    )
}

pub fn native_cmake_test_command() -> String {
    "cmake -B build -S . && cmake --build build && ctest --test-dir build --output-on-failure".into()
}

pub fn native_make_test_command() -> String {
    "make test".into()
}

pub fn native_meson_test_command() -> String {
    "meson test -C build".into()
}

fn collect_go_cmd_tasks(dir: &Path, go: &str) -> Vec<BuildTask> {
    let cmd_dir = dir.join("cmd");
    if !cmd_dir.is_dir() {
        return Vec::new();
    }
    let Ok(entries) = std::fs::read_dir(&cmd_dir) else {
        return Vec::new();
    };
    let mut tasks = Vec::new();
    for entry in entries.flatten() {
        if entry.file_type().is_ok_and(|t| t.is_dir()) {
            let name = entry.file_name().to_string_lossy().into_owned();
            tasks.push(task(
                &format!("run-{name}"),
                &format!("{go} run ./cmd/{name}"),
                "application",
            ));
        }
    }
    tasks
}

fn try_cmake_tree(ws: &Path, rel_path: &str) -> Result<Option<BuildTasksTree>> {
    let Some((dir, manifest_path)) = find_nearest_manifest(ws, rel_path, &["CMakeLists.txt"])? else {
        return Ok(None);
    };
    let name = manifest_path
        .rsplit('/')
        .nth(1)
        .unwrap_or("project")
        .to_string();
    let mut tasks = Vec::new();
    if dir.join("vcpkg.json").is_file() {
        tasks.push(task("vcpkg-install", "vcpkg install", "setup"));
    }
    if dir.join("conanfile.txt").is_file() || dir.join("conanfile.py").is_file() {
        tasks.push(task("conan-install", "conan install .", "setup"));
    }
    tasks.extend([
        task("configure", "cmake -B build -S .", "setup"),
        task("build", "cmake --build build", "lifecycle"),
        task("test", "ctest --test-dir build --output-on-failure", "verification"),
        task("clean", "cmake --build build --target clean", "lifecycle"),
    ]);
    Ok(Some(leaf_tree(
        ws,
        rel_path,
        "cmake",
        &name,
        &manifest_path,
        tasks,
    )?))
}

pub fn is_makefile_manifest_path(rel_path: &str) -> bool {
    let base = rel_path
        .replace('\\', "/")
        .rsplit('/')
        .next()
        .unwrap_or(rel_path)
        .to_ascii_lowercase();
    matches!(base.as_str(), "makefile" | "gnumakefile")
}

pub fn try_make_tree(ws: &Path, rel_path: &str) -> Result<Option<BuildTasksTree>> {
    for file in ["Makefile", "makefile", "GNUmakefile"] {
        if let Some((_dir, manifest_path)) = find_nearest_manifest(ws, rel_path, &[file])? {
            let text = std::fs::read_to_string(ws.join(&manifest_path))
                .with_context(|| format!("read {manifest_path}"))?;
            let name = manifest_path
                .rsplit('/')
                .nth(1)
                .unwrap_or("project")
                .to_string();
            let mut tasks = parse_make_targets(&text);
            if tasks.is_empty() {
                tasks.extend([
                    task("all", "make", "lifecycle"),
                    task("clean", "make clean", "lifecycle"),
                    task("test", "make test", "verification"),
                ]);
            }
            return Ok(Some(leaf_tree(
                ws,
                rel_path,
                "make",
                &name,
                &manifest_path,
                tasks,
            )?));
        }
    }
    Ok(None)
}

pub fn is_runnable_make_target(name: &str) -> bool {
    !name.is_empty()
        && !name.contains('$')
        && !name.contains('%')
        && !name.contains(' ')
        && !name.starts_with('=')
}

fn parse_make_targets(text: &str) -> Vec<BuildTask> {
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
                let group = if phony.contains(name) {
                    "tasks"
                } else {
                    "targets"
                };
                targets.push(task(name, &format!("make {name}"), group));
            }
        }
    }
    targets
}

fn try_meson_tree(ws: &Path, rel_path: &str) -> Result<Option<BuildTasksTree>> {
    let Some((_dir, manifest_path)) = find_nearest_manifest(ws, rel_path, &["meson.build"])? else {
        return Ok(None);
    };
    let name = manifest_path
        .rsplit('/')
        .nth(1)
        .unwrap_or("project")
        .to_string();
    let tasks = vec![
        task("setup", "meson setup build", "setup"),
        task("compile", "meson compile -C build", "lifecycle"),
        task("test", "meson test -C build", "verification"),
        task("install", "meson install -C build", "lifecycle"),
    ];
    Ok(Some(leaf_tree(
        ws,
        rel_path,
        "meson",
        &name,
        &manifest_path,
        tasks,
    )?))
}

fn try_vcpkg_tree(ws: &Path, rel_path: &str) -> Result<Option<BuildTasksTree>> {
    let Some((dir, manifest_path)) = find_nearest_manifest(ws, rel_path, &["vcpkg.json"])? else {
        return Ok(None);
    };
    let name = manifest_path
        .rsplit('/')
        .nth(1)
        .unwrap_or("project")
        .to_string();
    let mut tasks = vec![
        task("vcpkg-install", "vcpkg install", "setup"),
    ];
    if dir.join("CMakeLists.txt").is_file() {
        tasks.extend([
            task("configure", "cmake -B build -S .", "setup"),
            task("build", "cmake --build build", "lifecycle"),
            task("test", "ctest --test-dir build --output-on-failure", "verification"),
        ]);
    }
    Ok(Some(leaf_tree(
        ws,
        rel_path,
        "vcpkg",
        &name,
        &manifest_path,
        tasks,
    )?))
}

fn try_conan_tree(ws: &Path, rel_path: &str) -> Result<Option<BuildTasksTree>> {
    for file in ["conanfile.txt", "conanfile.py"] {
        if let Some((dir, manifest_path)) = find_nearest_manifest(ws, rel_path, &[file])? {
            let name = manifest_path
                .rsplit('/')
                .nth(1)
                .unwrap_or("project")
                .to_string();
            let mut tasks = vec![task("conan-install", "conan install .", "setup")];
            if dir.join("CMakeLists.txt").is_file() {
                tasks.extend([
                    task("configure", "cmake -B build -S .", "setup"),
                    task("build", "cmake --build build", "lifecycle"),
                    task("test", "ctest --test-dir build --output-on-failure", "verification"),
                ]);
            }
            return Ok(Some(leaf_tree(
                ws,
                rel_path,
                "conan",
                &name,
                &manifest_path,
                tasks,
            )?));
        }
    }
    Ok(None)
}

fn extract_quoted(s: &str) -> Option<String> {
    let s = s.trim();
    if (s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')) {
        return Some(s[1..s.len() - 1].to_string());
    }
    None
}

fn parse_rake_symbol(token: &str) -> Option<String> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn npm_tree_reads_scripts() {
        let tmp = std::env::temp_dir().join(format!("reaper-npm-tasks-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        fs::write(
            tmp.join("package.json"),
            r#"{"name":"demo","scripts":{"build":"tsc","test":"jest"}}"#,
        )
        .unwrap();
        let tree = try_npm_tree(&tmp, "package.json").unwrap().expect("tree");
        assert_eq!(tree.build_tool, "npm");
        assert!(tree.tree.tasks.iter().any(|t| t.command == "npm run build"));
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn cargo_tree_has_build_and_test() {
        let tmp = std::env::temp_dir().join(format!("reaper-cargo-tasks-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        fs::write(
            tmp.join("Cargo.toml"),
            r#"[package]
name = "demo"
version = "0.1.0"
"#,
        )
        .unwrap();
        let tree = try_cargo_tree(&tmp, "Cargo.toml").unwrap().expect("tree");
        assert_eq!(tree.build_tool, "cargo");
        assert_eq!(tree.root_name, "demo");
        assert!(tree.tree.tasks.iter().any(|t| t.command == "cargo build"));
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn pyproject_tree_reads_scripts() {
        let tmp = std::env::temp_dir().join(format!("reaper-py-tasks-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        fs::write(
            tmp.join("pyproject.toml"),
            r#"[project]
name = "demo"
scripts = { serve = "demo.cli:main", test = "pytest:main" }

[tool.taskipy.tasks]
lint = "ruff check ."
"#,
        )
        .unwrap();
        let tree = try_pyproject_tree(&tmp, "pyproject.toml")
            .unwrap()
            .expect("tree");
        assert_eq!(tree.build_tool, "pip");
        assert_eq!(tree.root_name, "demo");
        assert!(tree.tree.tasks.iter().any(|t| t.id == "serve"));
        assert!(tree.tree.tasks.iter().any(|t| t.command == "ruff check ."));
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn rake_tree_parses_tasks() {
        let tmp = std::env::temp_dir().join(format!("reaper-rake-tasks-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        fs::write(
            tmp.join("Rakefile"),
            "task :default do\nend\ntask :test do\nend\n",
        )
        .unwrap();
        let tree = try_rake_tree(&tmp, "Rakefile").unwrap().expect("tree");
        assert_eq!(tree.build_tool, "rake");
        assert!(tree.tree.tasks.iter().any(|t| t.command == "rake test"));
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn docker_compose_tree_includes_services_and_logs() {
        let tmp = std::env::temp_dir().join(format!("reaper-docker-tasks-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        fs::write(
            tmp.join("docker-compose.yml"),
            "name: demo-stack\nservices:\n  web:\n    image: nginx\n  db:\n    image: postgres\n",
        )
        .unwrap();
        let tree = try_docker_compose_tree(&tmp, "docker-compose.yml", None)
            .unwrap()
            .expect("tree");
        assert_eq!(tree.build_tool, "docker");
        assert_eq!(tree.root_name, "demo-stack");
        assert!(tree
            .tree
            .tasks
            .iter()
            .any(|t| t.id == "logs-follow" && t.label == "docker compose logs -f"));
        assert!(tree.tree.tasks.iter().any(|t| t.id == "logs-web" && t.label.contains("docker compose logs")));
        assert!(tree.tree.tasks.iter().any(|t| t.id == "up-db" && t.command.contains("--force-recreate")));
        assert!(tree.tree.tasks.iter().any(|t| t.id == "up" && t.command.contains("docker compose")));
        assert!(tree.tree.tasks.iter().any(|t| t.id == "ps" && t.command == "docker ps"));
        assert!(tree.tree.tasks.iter().any(|t| t.id == "apply"));
        assert!(tree.tree.tasks.iter().any(|t| t.id == "kill-all"));
        assert!(tree.tree.tasks.iter().any(|t| t.id == "prune"));
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn docker_compose_tree_uses_editor_content_overlay() {
        let tmp = std::env::temp_dir().join(format!("reaper-docker-overlay-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        fs::write(
            tmp.join("docker-compose.yml"),
            "services:\n  web:\n    image: nginx\n",
        )
        .unwrap();
        let edited = "name: edited-stack\nservices:\n  web:\n    image: nginx\n  cache:\n    image: redis\n";
        let tree = try_docker_compose_tree(&tmp, "docker-compose.yml", Some(edited))
            .unwrap()
            .expect("tree");
        assert_eq!(tree.root_name, "edited-stack");
        assert!(tree.tree.tasks.iter().any(|t| t.id == "logs-cache"));
        assert!(!tree.tree.tasks.iter().any(|t| t.id == "logs-db"));
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn make_tree_prefers_makefile_over_compose() {
        let tmp = std::env::temp_dir().join(format!("reaper-make-compose-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        fs::write(
            tmp.join("docker-compose.yml"),
            "services:\n  postgres:\n    image: postgres:16-alpine\n",
        )
        .unwrap();
        fs::write(
            tmp.join("Makefile"),
            ".PHONY: init up\ninit:\n\t./scripts/init-db.sh\nup:\n\tdocker compose up -d\n",
        )
        .unwrap();
        let tree = super::super::build_tasks::build_tasks_tree(&tmp, "Makefile", None)
            .unwrap();
        assert_eq!(tree.build_tool, "make");
        assert!(tree.tree.tasks.iter().any(|t| t.id == "init"));
        assert!(tree.tree.tasks.iter().any(|t| t.id == "up"));
        assert!(!tree.tree.tasks.iter().any(|t| t.command.contains("docker compose up -d --force-recreate")));
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn cmake_run_command_builds_matching_executable() {
        let cmake = r#"cmake_minimum_required(VERSION 3.16)
project(cpp_proj VERSION 0.1.0 LANGUAGES CXX)
add_library(cpp_proj_lib src/greeter.cpp)
target_include_directories(cpp_proj_lib PUBLIC include)
add_executable(cpp_proj src/main.cpp)
target_link_libraries(cpp_proj PRIVATE cpp_proj_lib)
"#;
        assert_eq!(
            cmake_executable_for_source(cmake, "src/main.cpp").as_deref(),
            Some("cpp_proj")
        );
        assert_eq!(
            cmake_executable_for_source(cmake, "tests/greeter_test.cpp").as_deref(),
            None
        );

        let tmp = std::env::temp_dir().join(format!("reaper-cmake-run-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        fs::write(tmp.join("CMakeLists.txt"), cmake).unwrap();
        let cmd = native_cmake_run_command(&tmp, "src/main.cpp")
            .unwrap()
            .expect("cmake run");
        assert!(cmd.contains("--target cpp_proj"));
        assert!(cmd.contains("./build/cpp_proj"));
        let _ = fs::remove_dir_all(&tmp);
    }
}
