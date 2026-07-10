use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

use super::types::DebugCapabilities;
use crate::config;
use crate::workspace::jdtls;
use crate::workspace::run_project::{JavaRunTarget, RunContext};
use crate::workspace::gradle;
use crate::workspace::maven;
use crate::workspace::{self, native_build_tasks};

#[derive(Clone, Debug)]
pub struct AdapterSpec {
    pub label: String,
    pub command: String,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
    pub cwd: Option<PathBuf>,
}

#[derive(Clone, Debug)]
pub struct LaunchPlan {
    pub language: String,
    pub adapter: AdapterSpec,
    pub pre_commands: Vec<String>,
    /// Working directory for `pre_commands` (defaults to workspace root).
    pub prebuild_cwd: Option<PathBuf>,
    pub launch: Value,
    /// Connect to Java DAP over TCP via jdtls instead of spawning a stdio adapter.
    pub use_jdtls_java: bool,
}

pub fn debug_capabilities(ws: &Path, rel_path: &str, line: u32) -> Result<DebugCapabilities> {
    let rel_path = workspace::normalize_workspace_source_path(rel_path);
    let ctx = workspace::run_context(ws, &rel_path, None, line.max(1), None, None)?;
    let target = ctx.target.as_ref();
    let language = debug_language_label(target);
    let Some(target) = target else {
        return Ok(DebugCapabilities {
            supported: false,
            language,
            adapter: None,
            reason: Some("no runnable target for this file".into()),
        });
    };
    if !target.runnable {
        return Ok(DebugCapabilities {
            supported: false,
            language,
            adapter: None,
            reason: target
                .reason
                .clone()
                .or_else(|| Some("file is not debuggable in the current context".into())),
        });
    }
    // Lightweight check only — do not resolve classpaths via jdtls here (too slow and
    // fails while import is still running, which kept the debug button disabled).
    match build_launch_plan(ws, &rel_path, &ctx, Some(target), &[], false) {
        Ok(plan) => Ok(DebugCapabilities {
            supported: true,
            language,
            adapter: Some(plan.adapter.label),
            reason: None,
        }),
        Err(e) => Ok(DebugCapabilities {
            supported: false,
            language,
            adapter: None,
            reason: Some(format!("{e:#}")),
        }),
    }
}

pub fn build_launch_plan(
    ws: &Path,
    rel_path: &str,
    ctx: &RunContext,
    target: Option<&JavaRunTarget>,
    breakpoints: &[(String, u32)],
    resolve_java_launch: bool,
) -> Result<LaunchPlan> {
    let target = target.context("no runnable target for this file")?;
    if !target.runnable {
        bail!("file is not debuggable in the current context");
    }
    let abs_path = ws.join(rel_path);
    let class_type = target.class_type.as_str();
    let mode = target.mode.as_str();

    if mode == "python" || class_type == "python-script" {
        let adapter = find_debugpy_adapter()?;
        let py = native_build_tasks::python_interpreter_for_project(
            native_build_tasks::python_package_manager_at(ws, rel_path)?
                .0
                .as_deref(),
        );
        return Ok(LaunchPlan {
            language: "Python".into(),
            adapter,
            pre_commands: Vec::new(),
            prebuild_cwd: None,
            launch: json!({
                "type": "debugpy",
                "request": "launch",
                "program": abs_path.display().to_string(),
                "python": py,
                "cwd": ws.display().to_string(),
                "justMyCode": false,
                "stopOnEntry": breakpoints.is_empty(),
            }),
            use_jdtls_java: false,
        });
    }

    if mode == "go" || class_type.contains("go") {
        let adapter = find_delve_adapter()?;
        return Ok(LaunchPlan {
            language: "Go".into(),
            adapter,
            pre_commands: Vec::new(),
            prebuild_cwd: None,
            launch: json!({
                "type": "go",
                "request": "launch",
                "mode": "debug",
                "program": abs_path.display().to_string(),
                "cwd": ws.display().to_string(),
                "stopOnEntry": breakpoints.is_empty(),
            }),
            use_jdtls_java: false,
        });
    }

    if mode == "js" || class_type == "js-script" || class_type == "ts-script" {
        let adapter = find_js_debug_adapter(ws)?;
        return Ok(LaunchPlan {
            language: if class_type == "ts-script" {
                "TypeScript".into()
            } else {
                "JavaScript".into()
            },
            adapter,
            pre_commands: Vec::new(),
            prebuild_cwd: None,
            launch: json!({
                "type": "node",
                "request": "launch",
                "program": abs_path.display().to_string(),
                "cwd": ws.display().to_string(),
                "stopOnEntry": breakpoints.is_empty(),
                "console": "integratedTerminal",
            }),
            use_jdtls_java: false,
        });
    }

    if mode == "rust" || class_type.starts_with("rust") || class_type.starts_with("cargo") {
        let adapter = find_lldb_adapter()?;
        let is_test = mode == "rust-test" || class_type.contains("test");
        let pre = if is_test {
            format!("cargo test --no-run")
        } else {
            format!("cargo build")
        };
        let binary = guess_rust_binary(ws, rel_path, is_test)?;
        return Ok(LaunchPlan {
            language: "Rust".into(),
            adapter,
            pre_commands: vec![pre],
            prebuild_cwd: None,
            launch: json!({
                "type": "lldb",
                "request": "launch",
                "program": binary.display().to_string(),
                "cwd": ws.display().to_string(),
                "stopOnEntry": breakpoints.is_empty(),
            }),
            use_jdtls_java: false,
        });
    }

    if mode == "native"
        || mode == "native-test"
        || class_type.contains("native")
        || class_type == "gtest"
        || class_type == "catch2"
        || native_build_tasks::is_cpp_source_path(rel_path)
        || native_build_tasks::is_c_source_path(rel_path)
    {
        let adapter = find_lldb_adapter()?;
        let is_cpp = native_build_tasks::is_cpp_source_path(rel_path);
        let out = ws.join(".reaper/native-debug-out");
        let compiler = native_build_tasks::native_compiler_shell(is_cpp);
        let std_flag = if is_cpp { "-std=c++17" } else { "-std=c17" };
        let lang_flag = if is_cpp && !compiler.contains("++") {
            " -x c++"
        } else {
            ""
        };
        let quoted = shell_quote(rel_path);
        let pre = if class_type == "gtest" {
            format!(
                "mkdir -p .reaper && {compiler} {std_flag}{lang_flag} {quoted} -g -O0 -lgtest -lgtest_main -pthread -o .reaper/native-debug-out"
            )
        } else if class_type == "catch2" {
            format!(
                "mkdir -p .reaper && {compiler} -std=c++17 -g -O0 -DCATCH_CONFIG_MAIN {quoted} -o .reaper/native-debug-out"
            )
        } else {
            format!(
                "mkdir -p .reaper && {compiler} {std_flag}{lang_flag} -g -O0 -o .reaper/native-debug-out {quoted}"
            )
        };
        return Ok(LaunchPlan {
            language: if is_cpp { "C++".into() } else { "C".into() },
            adapter,
            pre_commands: vec![pre],
            prebuild_cwd: None,
            launch: json!({
                "type": "lldb",
                "request": "launch",
                "program": out.display().to_string(),
                "cwd": ws.display().to_string(),
                "stopOnEntry": breakpoints.is_empty(),
            }),
            use_jdtls_java: false,
        });
    }

    if mode == "spring-boot" || class_type == "spring-boot-app" {
        let (adapter, use_jdtls_java) = resolve_java_debug_backend()?;
        let main_class = target
            .qualified_name
            .clone()
            .filter(|s| !s.is_empty())
            .context("could not resolve Spring Boot main class")?;
        let mut plan = java_launch_plan(
            "Spring Boot",
            adapter,
            use_jdtls_java,
            main_class,
            &ctx.project.project_root,
            ws,
            rel_path,
            true,
            resolve_java_launch,
        )?;
        if let Some((cwd, cmd)) = java_compile_prebuild(ws, rel_path, &ctx.project) {
            plan.pre_commands.push(cmd);
            plan.prebuild_cwd = Some(cwd);
        }
        return Ok(plan);
    }

    if mode == "main" || class_type.contains("java") || rel_path.ends_with(".java") {
        let (adapter, use_jdtls_java) = resolve_java_debug_backend()?;
        let main_class = target
            .qualified_name
            .clone()
            .filter(|s| !s.is_empty())
            .context("could not resolve Java main class")?;
        let mut plan = java_launch_plan(
            "Java",
            adapter,
            use_jdtls_java,
            main_class,
            &ctx.project.project_root,
            ws,
            rel_path,
            true,
            resolve_java_launch,
        )?;
        if let Some((cwd, cmd)) = java_compile_prebuild(ws, rel_path, &ctx.project) {
            plan.pre_commands.push(cmd);
            plan.prebuild_cwd = Some(cwd);
        }
        return Ok(plan);
    }

    if mode == "kotlin" || rel_path.ends_with(".kt") || rel_path.ends_with(".kts") {
        let (adapter, use_jdtls_java) = resolve_java_debug_backend()?;
        let main_class = target
            .qualified_name
            .clone()
            .filter(|s| !s.is_empty())
            .context("could not resolve Kotlin entry point")?;
        return Ok(java_launch_plan(
            "Kotlin",
            adapter,
            use_jdtls_java,
            main_class,
            &ctx.project.project_root,
            ws,
            rel_path,
            true,
            resolve_java_launch,
        )?);
    }

    bail!(
        "debugging is not available for {} ({})",
        debug_language_label(Some(target)),
        class_type
    );
}

fn debug_language_label(target: Option<&JavaRunTarget>) -> String {
    let Some(t) = target else {
        return "Unknown".into();
    };
    match t.class_type.as_str() {
        "python-script" | "pytest" | "django-test" => "Python".into(),
        "go" | "go-test" => "Go".into(),
        "js-script" | "js-test" => "JavaScript".into(),
        "ts-script" => "TypeScript".into(),
        "rust-source" | "rust-program" | "cargo-run" | "cargo-test" | "rustc-test" => "Rust".into(),
        "c-source" | "cpp-source" | "native-test" | "gtest" | "catch2" => {
            if t.frameworks.iter().any(|f| f == "cpp") {
                "C++".into()
            } else {
                "C".into()
            }
        }
        _ if t.mode == "spring-boot" => "Spring Boot".into(),
        _ if t.class_type == "spring-boot-app" => "Spring Boot".into(),
        _ if t.mode == "main" => "Java".into(),
        _ if t.class_type.contains("java") => "Java".into(),
        _ if t.mode == "kotlin" => "Kotlin".into(),
        _ => t.class_type.replace('-', " "),
    }
}

fn adapter(label: &str, command: impl Into<String>, args: Vec<String>) -> AdapterSpec {
    AdapterSpec {
        label: label.into(),
        command: command.into(),
        args,
        env: HashMap::new(),
        cwd: None,
    }
}

fn find_debugpy_adapter() -> Result<AdapterSpec> {
    if let Some(dir) = config::bundled_debugpy_dir() {
        let py = python_for_debugpy();
        let mut spec = adapter("debugpy (bundled)", py, vec!["-m".into(), "debugpy.adapter".into()]);
        spec.env.insert("PYTHONPATH".into(), dir.display().to_string());
        return Ok(spec);
    }
    if command_ok("python3", &["-c", "import debugpy"]) {
        return Ok(adapter("debugpy", "python3", vec!["-m".into(), "debugpy.adapter".into()]));
    }
    if command_ok("python", &["-c", "import debugpy"]) {
        return Ok(adapter("debugpy", "python", vec!["-m".into(), "debugpy.adapter".into()]));
    }
    bail!("install debugpy: pip install debugpy")
}

fn python_for_debugpy() -> String {
    if command_ok("python3", &["-c", "import sys"]) {
        "python3".into()
    } else if command_ok("python", &["-c", "import sys"]) {
        "python".into()
    } else {
        "python3".into()
    }
}

fn find_delve_adapter() -> Result<AdapterSpec> {
    if let Some(path) = config::bundled_delve() {
        if path.is_file() {
            return Ok(adapter(
                "delve (bundled)",
                path.display().to_string(),
                vec!["dap".into()],
            ));
        }
    }
    if which("dlv") {
        return Ok(adapter("delve", "dlv", vec!["dap".into()]));
    }
    bail!("install Delve: brew install delve")
}

fn find_lldb_adapter() -> Result<AdapterSpec> {
    if let Some(path) = config::bundled_codelldb() {
        if path.is_file() {
            let mut spec = adapter(
                "codelldb (bundled)",
                path.display().to_string(),
                vec!["--stdio".into()],
            );
            spec.cwd = path.parent().map(|p| p.to_path_buf());
            return Ok(spec);
        }
    }
    if which("lldb-dap") {
        return Ok(adapter("lldb-dap", "lldb-dap", Vec::new()));
    }
    if which("codelldb") {
        return Ok(adapter("codelldb", "codelldb", vec!["--stdio".into()]));
    }
    if let Some(path) = find_vscode_adapter("vadimcn.vscode-lldb", "codelldb") {
        return Ok(adapter("codelldb", path, vec!["--stdio".into()]));
    }
    bail!("install a native debugger adapter: Xcode 16+ lldb-dap, or brew install codelldb")
}

fn find_js_debug_adapter(ws: &Path) -> Result<AdapterSpec> {
    if let Some(path) = config::bundled_js_debug_dap() {
        if path.is_file() {
            return Ok(adapter(
                "js-debug (bundled)",
                node_command(),
                vec![path.display().to_string()],
            ));
        }
    }
    if let Some(path) = find_workspace_js_debug(ws) {
        return Ok(adapter("js-debug", node_command(), vec![path]));
    }
    if let Some(path) = find_vscode_adapter("ms-vscode.js-debug", "dapDebugServer.js") {
        return Ok(adapter("js-debug", node_command(), vec![path]));
    }
    if let Some(path) = find_vscode_adapter("ms-vscode.js-debug", "debugAdapter.js") {
        return Ok(adapter("js-debug", node_command(), vec![path]));
    }
    if which("npx") {
        return Ok(adapter(
            "js-debug",
            "npx",
            vec![
                "--yes".into(),
                "@vscode/js-debug".into(),
                "--stdio".into(),
            ],
        ));
    }
    bail!("install Node.js and js-debug (npm i -D @vscode/js-debug)")
}

fn java_compile_prebuild(
    ws: &Path,
    rel_path: &str,
    project: &crate::workspace::run_project::RunProjectInfo,
) -> Option<(PathBuf, String)> {
    if !project.has_project {
        return None;
    }
    match project.build_tool.as_str() {
        "gradle" => {
            let root = gradle::find_gradle_root(ws, rel_path).ok()??;
            let cwd = gradle::find_gradle_wrapper_root(&root);
            let module = gradle::find_gradle_module_for_source_file(ws, rel_path, &cwd)
                .ok()
                .and_then(|m| m)
                .unwrap_or_default();
            let gradlew = if cwd.join("gradlew").is_file() {
                "./gradlew"
            } else {
                "gradle"
            };
            let cmd = if module.is_empty() {
                format!("{gradlew} classes -x test")
            } else {
                format!(
                    "{gradlew} :{}:classes -x test",
                    module.replace('/', ":")
                )
            };
            Some((cwd, cmd))
        }
        "maven" => {
            let module_root = maven::find_maven_root(ws, rel_path).ok()??;
            let reactor = maven::find_maven_reactor_root(&module_root).unwrap_or_else(|| module_root.clone());
            let cwd = if reactor.join("mvnw").is_file() || reactor.join("mvnw.cmd").is_file() {
                reactor
            } else {
                module_root.clone()
            };
            let mvn = if cwd.join("mvnw").is_file() {
                "./mvnw"
            } else if cfg!(windows) && cwd.join("mvnw.cmd").is_file() {
                "./mvnw.cmd"
            } else {
                "mvn"
            };
            let pl_suffix = maven::maven_reactor_context(&module_root)
                .filter(|ctx| !ctx.module_pl.is_empty())
                .map(|ctx| format!(" -pl {} -am", ctx.module_pl))
                .unwrap_or_default();
            Some((cwd, format!("{mvn} -q -DskipTests{pl_suffix} compile")))
        }
        _ => None,
    }
}

fn java_launch_plan(
    language: &str,
    adapter: AdapterSpec,
    use_jdtls_java: bool,
    main_class: String,
    _project_name: &str,
    ws: &Path,
    rel_path: &str,
    stop_on_entry: bool,
    resolve_java_launch: bool,
) -> Result<LaunchPlan> {
    let launch = if use_jdtls_java {
        if resolve_java_launch {
            let args = jdtls::prepare_java_launch(ws, &main_class, Some(rel_path))?;
            let mut launch = json!({
                "type": "java",
                "request": "launch",
                "mainClass": args.main_class,
                "classPaths": args.class_paths,
                "modulePaths": args.module_paths,
                "cwd": args.cwd.display().to_string(),
                "stopOnEntry": stop_on_entry,
                "console": "internalConsole",
                "shortenCommandLine": "auto",
            });
            if let Some(name) = args.project_name.filter(|s| !s.is_empty()) {
                launch["projectName"] = json!(name);
            }
            if let Some(java_exec) = args.java_exec.filter(|s| !s.is_empty()) {
                launch["javaExec"] = json!(java_exec);
            }
            launch
        } else if !jdtls::workspace_ready(ws) {
            bail!("Java language server is still starting — wait for Maven/Gradle import to finish");
        } else {
            json!({
                "type": "java",
                "request": "launch",
                "mainClass": main_class,
                "cwd": ws.display().to_string(),
                "stopOnEntry": stop_on_entry,
                "console": "internalConsole",
            })
        }
    } else {
        json!({
            "type": "java",
            "request": "launch",
            "mainClass": main_class,
            "projectName": _project_name,
            "cwd": ws.display().to_string(),
            "stopOnEntry": stop_on_entry,
            "console": "internalConsole",
        })
    };
    Ok(LaunchPlan {
        language: language.into(),
        adapter,
        pre_commands: Vec::new(),
        prebuild_cwd: None,
        launch,
        use_jdtls_java,
    })
}

fn resolve_java_debug_backend() -> Result<(AdapterSpec, bool)> {
    if jdtls::java_debug_via_jdtls_available() {
        return Ok((
            adapter("java-debug (bundled)", String::new(), Vec::new()),
            true,
        ));
    }
    if let Some(path) = find_vscode_adapter("vscjava.vscode-java-debug", "debugAdapter.js") {
        return Ok((adapter("java-debug", node_command(), vec![path]), false));
    }
    bail!(
        "Java debugging requires bundled jdtls + java-debug (Reaper.app) or the VS Code Java Debug extension"
    )
}

fn find_workspace_js_debug(ws: &Path) -> Option<String> {
    for rel in [
        "node_modules/@vscode/js-debug/dist/debugAdapter.js",
        "node_modules/@vscode/js-debug/src/debugAdapter.js",
    ] {
        let p = ws.join(rel);
        if p.is_file() {
            return p.to_str().map(str::to_string);
        }
    }
    None
}

fn find_vscode_adapter(extension_prefix: &str, file_suffix: &str) -> Option<String> {
    let home = std::env::var("HOME").ok()?;
    for root in [
        format!("{home}/.cursor/extensions"),
        format!("{home}/.vscode/extensions"),
    ] {
        let dir = PathBuf::from(&root);
        if !dir.is_dir() {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.starts_with(extension_prefix) {
                continue;
            }
            if let Some(path) = find_file_named(entry.path(), file_suffix) {
                return path.to_str().map(str::to_string);
            }
        }
    }
    None
}

fn find_file_named(dir: PathBuf, name: &str) -> Option<PathBuf> {
    if !dir.is_dir() {
        return None;
    }
    let mut stack = vec![dir];
    while let Some(cur) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&cur) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && path.file_name().and_then(|n| n.to_str()) == Some(name) {
                return Some(path);
            }
            if path.is_dir() {
                stack.push(path);
            }
        }
    }
    None
}

fn guess_rust_binary(ws: &Path, rel_path: &str, is_test: bool) -> Result<PathBuf> {
    let name = Path::new(rel_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("main");
    let candidates = if is_test {
        vec![
            ws.join(format!("target/debug/deps/{name}")),
            ws.join("target/debug/deps"),
        ]
    } else {
        let pkg = ws
            .join("Cargo.toml")
            .is_file()
            .then(|| {
                std::fs::read_to_string(ws.join("Cargo.toml"))
                    .ok()
                    .and_then(|t| {
                        t.lines()
                            .find(|l| l.trim().starts_with("name ="))
                            .and_then(|l| l.split('"').nth(1).map(str::to_string))
                    })
            })
            .flatten()
            .unwrap_or_else(|| name.to_string());
        vec![
            ws.join(format!("target/debug/{pkg}")),
            ws.join(format!("target/debug/{name}")),
        ]
    };
    for c in candidates {
        if c.is_file() {
            return Ok(c);
        }
    }
    bail!("build Rust binary first (cargo build)")
}

fn node_command() -> String {
    if let Some(path) = config::bundled_node() {
        return path.display().to_string();
    }
    if which("node") {
        "node".into()
    } else {
        "nodejs".into()
    }
}

fn which(cmd: &str) -> bool {
    command_ok("which", &[cmd])
}

fn command_ok(cmd: &str, args: &[&str]) -> bool {
    Command::new(cmd)
        .args(args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn shell_quote(s: &str) -> String {
    if s.contains(' ') || s.contains('\'') {
        format!("'{}'", s.replace('\'', "'\\''"))
    } else {
        s.to_string()
    }
}
