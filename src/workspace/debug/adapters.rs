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

pub fn debug_capabilities(
    ws: &Path,
    rel_path: &str,
    line: u32,
    content: Option<&str>,
) -> Result<DebugCapabilities> {
    let rel_path = workspace::normalize_workspace_source_path(rel_path);
    let ctx = workspace::run_context(ws, &rel_path, content, line.max(1), None, None, None)?;
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

    if mode == "python"
        || mode == "python-test"
        || class_type == "python-script"
        || class_type == "pytest"
        || class_type == "django-test"
    {
        let adapter = find_debugpy_adapter()?;
        let py = native_build_tasks::python_interpreter_for_project(
            native_build_tasks::python_package_manager_at(ws, rel_path)?
                .0
                .as_deref(),
        );
        let is_pytest = class_type == "pytest" || (mode == "python-test" && class_type != "django-test");
        let launch = if is_pytest {
            json!({
                "type": "debugpy",
                "request": "launch",
                "module": "pytest",
                "args": [abs_path.display().to_string()],
                "python": py,
                "cwd": ws.display().to_string(),
                "justMyCode": false,
                "stopOnEntry": breakpoints.is_empty(),
            })
        } else if class_type == "django-test" {
            // manage.py test <label> — debug the manage.py entry with args.
            let manage = native_build_tasks::python_package_manager_at(ws, rel_path)?
                .0
                .map(|root| root.join("manage.py"))
                .filter(|p| p.is_file())
                .unwrap_or_else(|| ws.join("manage.py"));
            json!({
                "type": "debugpy",
                "request": "launch",
                "program": manage.display().to_string(),
                "args": ["test", rel_path.replace('\\', "/")],
                "python": py,
                "cwd": ws.display().to_string(),
                "justMyCode": false,
                "stopOnEntry": breakpoints.is_empty(),
            })
        } else {
            json!({
                "type": "debugpy",
                "request": "launch",
                "program": abs_path.display().to_string(),
                "python": py,
                "cwd": ws.display().to_string(),
                "justMyCode": false,
                "stopOnEntry": breakpoints.is_empty(),
            })
        };
        return Ok(LaunchPlan {
            language: "Python".into(),
            adapter,
            pre_commands: Vec::new(),
            prebuild_cwd: None,
            launch,
            use_jdtls_java: false,
        });
    }

    if mode == "go" || mode == "go-test" || class_type.contains("go") {
        let adapter = find_delve_adapter()?;
        let is_test = mode == "go-test" || class_type == "go-test" || rel_path.ends_with("_test.go");
        let program = if is_test {
            abs_path
                .parent()
                .unwrap_or(ws)
                .display()
                .to_string()
        } else {
            abs_path.display().to_string()
        };
        return Ok(LaunchPlan {
            language: "Go".into(),
            adapter,
            pre_commands: Vec::new(),
            prebuild_cwd: None,
            launch: json!({
                "type": "go",
                "request": "launch",
                "mode": if is_test { "test" } else { "debug" },
                "program": program,
                "cwd": ws.display().to_string(),
                "stopOnEntry": breakpoints.is_empty(),
            }),
            use_jdtls_java: false,
        });
    }

    if mode == "js"
        || mode == "js-test"
        || class_type == "js-script"
        || class_type == "ts-script"
        || class_type == "js-test"
    {
        let adapter = find_js_debug_adapter(ws)?;
        let is_ts = class_type == "ts-script"
            || rel_path.ends_with(".ts")
            || rel_path.ends_with(".tsx");
        let mut launch = json!({
            "type": "node",
            "request": "launch",
            "program": abs_path.display().to_string(),
            "cwd": ws.display().to_string(),
            "stopOnEntry": breakpoints.is_empty(),
            "console": "internalConsole",
        });
        if is_ts {
            if let Some(runtime) = ts_debug_runtime(ws, rel_path) {
                launch["runtimeExecutable"] = json!(runtime);
            }
        }
        return Ok(LaunchPlan {
            language: if is_ts {
                "TypeScript".into()
            } else {
                "JavaScript".into()
            },
            adapter,
            pre_commands: Vec::new(),
            prebuild_cwd: None,
            launch,
            use_jdtls_java: false,
        });
    }

    if mode == "rust"
        || mode == "rust-test"
        || class_type.starts_with("rust")
        || class_type.starts_with("cargo")
    {
        let adapter = find_lldb_adapter()?;
        let is_test = mode == "rust-test" || class_type.contains("test");
        let pre = if is_test {
            "cargo test --no-run".to_string()
        } else {
            "cargo build".to_string()
        };
        let cargo_cwd = native_build_tasks::cargo_manifest_root(ws, rel_path)?
            .unwrap_or_else(|| ws.to_path_buf());
        let binary = guess_rust_binary(&cargo_cwd, rel_path, is_test)?;
        return Ok(LaunchPlan {
            language: "Rust".into(),
            adapter,
            pre_commands: vec![pre],
            prebuild_cwd: Some(cargo_cwd.clone()),
            launch: json!({
                "type": "lldb",
                "request": "launch",
                "program": binary.display().to_string(),
                "cwd": cargo_cwd.display().to_string(),
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
        || class_type == "c-source"
        || class_type == "cpp-source"
        || native_build_tasks::is_cpp_source_path(rel_path)
        || native_build_tasks::is_c_source_path(rel_path)
    {
        let adapter = find_lldb_adapter().map_err(|e| {
            anyhow::anyhow!(
                "{e:#} — C/C++ debugging needs CodeLLDB (bundled in Reaper.app) or `lldb-dap`"
            )
        })?;
        let is_cpp = native_build_tasks::is_cpp_source_path(rel_path);

        // Prefer CMake Debug build so include dirs + linked sources (e.g. greeter.cpp) resolve.
        if let Some(cmake) = native_build_tasks::native_cmake_debug_launch(ws, rel_path)? {
            let program = ws.join(&cmake.program_rel);
            return Ok(LaunchPlan {
                language: if is_cpp { "C++".into() } else { "C".into() },
                adapter,
                pre_commands: vec![cmake.prebuild],
                prebuild_cwd: Some(cmake.cwd),
                launch: json!({
                    "type": "lldb",
                    "request": "launch",
                    "program": program.display().to_string(),
                    "cwd": ws.display().to_string(),
                    "stopOnEntry": breakpoints.is_empty(),
                }),
                use_jdtls_java: false,
            });
        }

        let out = ws.join(".reaper/native-debug-out");
        let compiler = native_build_tasks::native_compiler_shell(is_cpp);
        let std_flag = if is_cpp { "-std=c++17" } else { "-std=c17" };
        let lang_flag = if is_cpp && !compiler.contains("++") {
            " -x c++"
        } else {
            ""
        };
        let quoted = shell_quote(rel_path);
        let includes = native_debug_include_flags(ws);
        let pre = if class_type == "gtest" {
            format!(
                "mkdir -p .reaper && {compiler} {std_flag}{lang_flag} {includes} {quoted} -g -O0 -lgtest -lgtest_main -pthread -o .reaper/native-debug-out"
            )
        } else if class_type == "catch2" {
            format!(
                "mkdir -p .reaper && {compiler} -std=c++17 {includes} -g -O0 -DCATCH_CONFIG_MAIN {quoted} -o .reaper/native-debug-out"
            )
        } else {
            format!(
                "mkdir -p .reaper && {compiler} {std_flag}{lang_flag} {includes} -g -O0 -o .reaper/native-debug-out {quoted}"
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
            .context(
                "could not resolve Spring Boot main class — ensure the file has a package \
                 declaration or lives under src/main/java",
            )?;
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

    if mode == "main"
        || mode == "test"
        || class_type.contains("java")
        || class_type == "plain-main"
        || class_type == "junit-test"
        || class_type == "spring-boot-test"
        || rel_path.ends_with(".java")
    {
        let (adapter, use_jdtls_java) = resolve_java_debug_backend()?;
        let main_class = target
            .qualified_name
            .clone()
            .filter(|s| !s.is_empty())
            .context("could not resolve Java main class")?;
        let mut plan = java_launch_plan(
            if mode == "test" || class_type.contains("test") {
                "Java Test"
            } else {
                "Java"
            },
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

    if mode == "kotlin"
        || mode == "kotlin-test"
        || class_type.starts_with("kotlin")
        || rel_path.ends_with(".kt")
        || rel_path.ends_with(".kts")
    {
        let (adapter, use_jdtls_java) = resolve_java_debug_backend()?;
        let main_class = target
            .qualified_name
            .clone()
            .filter(|s| !s.is_empty())
            .context("could not resolve Kotlin entry point")?;
        let mut plan = java_launch_plan(
            "Kotlin",
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
    // CodeLLDB 1.12+ uses stdio by default and rejects the old `--stdio` flag.
    if let Some(path) = config::bundled_codelldb() {
        if path.is_file() {
            let mut spec = adapter(
                "codelldb (bundled)",
                path.display().to_string(),
                Vec::new(),
            );
            spec.cwd = path.parent().map(|p| p.to_path_buf());
            return Ok(spec);
        }
    }
    if which("lldb-dap") {
        return Ok(adapter("lldb-dap", "lldb-dap", Vec::new()));
    }
    if which("codelldb") {
        return Ok(adapter("codelldb", "codelldb", Vec::new()));
    }
    if let Some(path) = find_vscode_adapter("vadimcn.vscode-lldb", "codelldb") {
        return Ok(adapter("codelldb", path, Vec::new()));
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

fn gradle_debug_classes_cmd(gradlew: &str, module: &str) -> String {
    // Incremental compile with debug symbols. Avoid --rerun-tasks: multi-module
    // Spring projects routinely exceed a minute on a forced full rebuild.
    let debug_flags = "-Dorg.gradle.java.compile.options.debug=true";
    if module.is_empty() {
        format!("{gradlew} classes -x test {debug_flags}")
    } else {
        format!(
            "{gradlew} :{}:classes -x test {debug_flags}",
            module.replace('/', ":")
        )
    }
}

fn maven_debug_compile_cmd(mvn: &str, pl_suffix: &str) -> String {
    // lines,vars,source — without vars, stepping often skips and hover/locals are empty.
    // Prefer compile (not clean) so debug start stays fast.
    format!(
        "{mvn} -q -DskipTests -Dmaven.compiler.debug=true -Dmaven.compiler.debuglevel=lines,vars,source{pl_suffix} compile"
    )
}

fn java_compile_prebuild(
    ws: &Path,
    rel_path: &str,
    project: &crate::workspace::run_project::RunProjectInfo,
) -> Option<(PathBuf, String)> {
    if !project.has_project {
        return plain_java_javac_prebuild(ws, rel_path);
    }
    match project.build_tool.as_str() {
        "gradle" => {
            let root = gradle::find_gradle_root(ws, rel_path).ok()??;
            let cmd = gradle::resolve_gradle_command(&root).ok()?;
            let module = gradle::find_gradle_module_for_source_file(ws, rel_path, &cmd.cwd)
                .ok()
                .and_then(|m| m)
                .unwrap_or_default();
            let program = cmd.program.to_string_lossy().into_owned();
            Some((cmd.cwd, gradle_debug_classes_cmd(&program, &module)))
        }
        "maven" => {
            let module_root = maven::find_maven_root(ws, rel_path).ok()??;
            let cmd = maven::resolve_maven_command(&module_root);
            let program = cmd.program.to_string_lossy().into_owned();
            let pl_suffix = if cmd.project_args.is_empty() {
                String::new()
            } else {
                format!(" {}", cmd.project_args.join(" "))
            };
            Some((cmd.cwd, maven_debug_compile_cmd(&program, &pl_suffix)))
        }
        // Unknown build tool — still try single-file javac so Debug isn't empty.
        _ => plain_java_javac_prebuild(ws, rel_path),
    }
}

/// Compile a plain `.java` file (no Maven/Gradle) into `.reaper/java-out` with debug symbols.
/// Mirrors Run's `javac -d .reaper/java-out` path so Debug has classes on the classpath.
fn plain_java_javac_prebuild(ws: &Path, rel_path: &str) -> Option<(PathBuf, String)> {
    if !rel_path.ends_with(".java") {
        return None;
    }
    let rel = rel_path.replace('\\', "/");
    let javac = crate::jdk::javac_path()
        .ok()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "javac".into());
    // -g: lines,vars,source so locals/watch work (same idea as Maven debuglevel).
    let cmd = format!(
        "mkdir -p .reaper/java-out && {} -g -d .reaper/java-out -encoding UTF-8 {}",
        shell_quote(&javac),
        shell_quote(&rel)
    );
    Some((ws.to_path_buf(), cmd))
}

/// DAP launch when jdtls classpath resolve fails but we already compiled to `.reaper/java-out`.
pub fn plain_java_launch_fallback(
    ws: &Path,
    main_class: &str,
    rel_path: &str,
    stop_on_entry: bool,
) -> Option<Value> {
    if gradle::find_gradle_root(ws, rel_path)
        .ok()
        .flatten()
        .is_some()
        || maven::find_maven_root(ws, rel_path).ok().flatten().is_some()
    {
        return None;
    }
    let out = ws.join(".reaper/java-out");
    if !out.is_dir() {
        return None;
    }
    let project_name = ws
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("app")
        .to_string();
    let mut launch = json!({
        "type": "java",
        "request": "launch",
        "mainClass": main_class,
        "classPaths": [out.display().to_string()],
        "modulePaths": [],
        "cwd": ws.display().to_string(),
        "projectName": project_name,
        "stopOnEntry": stop_on_entry,
        "console": "internalConsole",
        "shortenCommandLine": "auto",
    });
    if let Ok(home) = crate::jdk::effective_java_home() {
        let java = home.join("bin/java");
        if java.is_file() {
            launch["javaExec"] = json!(java.display().to_string());
        }
    }
    Some(launch)
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
            java_launch_from_jdtls(ws, &main_class, rel_path, stop_on_entry)?
        } else {
            // Stub — `finalize_java_launch_plan` fills classpaths after prebuild.
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

fn java_launch_from_jdtls(
    ws: &Path,
    main_class: &str,
    rel_path: &str,
    stop_on_entry: bool,
) -> Result<Value> {
    let args = jdtls::prepare_java_launch(ws, main_class, Some(rel_path))?;
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
    // Required by Java Debug Server for evaluate/hover expressions.
    let project_name = args
        .project_name
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            args.cwd
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("app")
                .to_string()
        });
    launch["projectName"] = json!(project_name);
    if let Some(java_exec) = args.java_exec.filter(|s| !s.is_empty()) {
        launch["javaExec"] = json!(java_exec);
    }
    Ok(launch)
}

/// Resolve classpaths via jdtls after Maven/Gradle (or plain javac) prebuild.
pub fn finalize_java_launch_plan(plan: &mut LaunchPlan, ws: &Path, rel_path: &str) -> Result<()> {
    if !plan.use_jdtls_java {
        return Ok(());
    }
    let main_class = plan
        .launch
        .get("mainClass")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .context("Java launch missing mainClass")?;
    let stop_on_entry = plan
        .launch
        .get("stopOnEntry")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    match java_launch_from_jdtls(ws, &main_class, rel_path, stop_on_entry) {
        Ok(launch) => {
            plan.launch = launch;
            Ok(())
        }
        Err(e) => {
            if let Some(fallback) =
                plain_java_launch_fallback(ws, &main_class, rel_path, stop_on_entry)
            {
                tracing::warn!(
                    "java launch resolve failed for plain project; using .reaper/java-out: {e:#}"
                );
                plan.launch = fallback;
                return Ok(());
            }
            // Large Spring Boot projects often need a beat after `classes`/`compile`
            // before jdtls can resolve the classpath.
            tracing::warn!("java launch resolve failed, retrying once: {e:#}");
            std::thread::sleep(std::time::Duration::from_millis(1500));
            match java_launch_from_jdtls(ws, &main_class, rel_path, stop_on_entry) {
                Ok(launch) => {
                    plan.launch = launch;
                    Ok(())
                }
                Err(e2) => {
                    if let Some(fallback) =
                        plain_java_launch_fallback(ws, &main_class, rel_path, stop_on_entry)
                    {
                        tracing::warn!(
                            "java launch retry failed; using .reaper/java-out: {e2:#}"
                        );
                        plan.launch = fallback;
                        Ok(())
                    } else {
                        Err(e2).with_context(|| format!("after prebuild: {e:#}"))
                    }
                }
            }
        }
    }
}

/// Refresh Rust/C++ program paths that only exist after prebuild.
pub fn resolve_launch_program_after_prebuild(
    plan: &mut LaunchPlan,
    ws: &Path,
    rel_path: &str,
) -> Result<()> {
    if plan.language == "Rust" {
        let is_test = plan.pre_commands.iter().any(|c| c.contains("test --no-run"));
        let cwd = plan
            .prebuild_cwd
            .clone()
            .unwrap_or_else(|| ws.to_path_buf());
        let binary = guess_rust_binary(&cwd, rel_path, is_test)?;
        if !binary.is_file() || binary.file_name().and_then(|s| s.to_str()) == Some("pending")
            || binary
                .file_name()
                .and_then(|s| s.to_str())
                .is_some_and(|n| n.ends_with("-pending"))
        {
            bail!(
                "Rust {} binary not found after prebuild under {}",
                if is_test { "test" } else { "debug" },
                cwd.join("target/debug").display()
            );
        }
        plan.launch["program"] = json!(binary.display().to_string());
    }
    Ok(())
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
    let sanitized = name.replace('-', "_");
    if is_test {
        let deps = ws.join("target/debug/deps");
        if deps.is_dir() {
            let mut matches: Vec<(std::time::SystemTime, PathBuf)> = Vec::new();
            if let Ok(entries) = std::fs::read_dir(&deps) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if !path.is_file() {
                        continue;
                    }
                    let fname = path
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("");
                    // cargo test --no-run emits `foo-hash` / `foo_hash` binaries (no extension on Unix).
                    if fname.contains('.') {
                        continue;
                    }
                    if fname.starts_with(name)
                        || fname.starts_with(&sanitized)
                        || fname.starts_with(&format!("{name}-"))
                        || fname.starts_with(&format!("{sanitized}-"))
                    {
                        let modified = entry
                            .metadata()
                            .and_then(|m| m.modified())
                            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                        matches.push((modified, path));
                    }
                }
            }
            matches.sort_by(|a, b| b.0.cmp(&a.0));
            if let Some((_, path)) = matches.into_iter().next() {
                return Ok(path);
            }
        }
        // Before `cargo test --no-run`, deps binaries do not exist yet.
        return Ok(deps.join(format!("{sanitized}-pending")));
    }

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
    for c in [
        ws.join(format!("target/debug/{pkg}")),
        ws.join(format!("target/debug/{name}")),
    ] {
        if c.is_file() {
            return Ok(c);
        }
    }
    // Provisional path — `resolve_launch_program_after_prebuild` re-checks after cargo build.
    Ok(ws.join(format!("target/debug/{pkg}")))
}

fn ts_debug_runtime(ws: &Path, rel_path: &str) -> Option<String> {
    let project = native_build_tasks::node_project_root(ws, rel_path)
        .ok()
        .flatten();
    let pkg = project
        .as_ref()
        .map(|p| p.join("package.json"))
        .and_then(|p| std::fs::read_to_string(p).ok())
        .unwrap_or_default();
    let prefer_tsx = pkg.contains("\"tsx\"") || !pkg.contains("\"ts-node\"");
    let candidates = if prefer_tsx {
        ["tsx", "ts-node"]
    } else {
        ["ts-node", "tsx"]
    };
    for name in candidates {
        if let Some(root) = project.as_ref() {
            let local = root.join("node_modules").join(".bin").join(name);
            if local.is_file() {
                return Some(local.display().to_string());
            }
        }
        if which(name) {
            return Some(name.into());
        }
    }
    None
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

/// `-I` flags for single-file C/C++ debug when CMake is not available.
fn native_debug_include_flags(ws: &Path) -> String {
    let mut flags = Vec::new();
    for dir in ["include", "src", "inc", "headers", "."] {
        if dir == "." || ws.join(dir).is_dir() {
            flags.push(format!("-I{dir}"));
        }
    }
    flags.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maven_debug_compile_includes_line_var_source_symbols() {
        let cmd = maven_debug_compile_cmd("mvn", "");
        assert!(cmd.contains("maven.compiler.debug=true"));
        assert!(cmd.contains("debuglevel=lines,vars,source"));
        assert!(cmd.ends_with(" compile"));
        assert!(
            !cmd.contains(" clean "),
            "clean makes first debug start feel hung"
        );
    }

    #[test]
    fn maven_debug_compile_keeps_reactor_pl_suffix() {
        let cmd = maven_debug_compile_cmd("./mvnw", " -pl :app -am");
        assert!(cmd.contains(" -pl :app -am "));
        assert!(cmd.contains("debuglevel=lines,vars,source"));
        assert!(cmd.starts_with("./mvnw "));
    }

    #[test]
    fn gradle_debug_prebuild_prefers_wrapper_program() {
        let cmd = gradle_debug_classes_cmd("./gradlew", "services/api");
        assert!(cmd.starts_with("./gradlew "));
        assert!(cmd.contains(":services:api:classes"));
    }

    #[test]
    fn gradle_debug_classes_keeps_debug_symbols_without_forced_rerun() {
        let cmd = gradle_debug_classes_cmd("./gradlew", "");
        assert!(!cmd.contains("--rerun-tasks"));
        assert!(!cmd.contains("org.gradle.caching=false"));
        assert!(cmd.contains("org.gradle.java.compile.options.debug=true"));
        assert!(cmd.contains("classes -x test"));
    }

    #[test]
    fn plain_java_javac_prebuild_uses_debug_symbols() {
        let dir = std::env::temp_dir().join(format!(
            "reaper-plain-java-debug-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src")).unwrap();
        let rel = "src/Hello.java";
        std::fs::write(
            dir.join(rel),
            "public class Hello { public static void main(String[] a) {} }\n",
        )
        .unwrap();
        let project = crate::workspace::run_project::RunProjectInfo::default();
        let (cwd, cmd) = java_compile_prebuild(&dir, rel, &project).expect("plain prebuild");
        assert_eq!(cwd, dir);
        assert!(cmd.contains(".reaper/java-out"), "{cmd}");
        assert!(cmd.contains(" -g "), "{cmd}");
        assert!(cmd.contains("src/Hello.java") || cmd.contains("'src/Hello.java'"), "{cmd}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn plain_java_launch_fallback_uses_java_out() {
        let dir = std::env::temp_dir().join(format!(
            "reaper-plain-java-fallback-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".reaper/java-out")).unwrap();
        let launch =
            plain_java_launch_fallback(&dir, "Hello", "src/Hello.java", true).expect("fallback");
        assert_eq!(launch["mainClass"], "Hello");
        assert_eq!(launch["stopOnEntry"], true);
        let cps = launch["classPaths"].as_array().unwrap();
        assert!(
            cps.iter()
                .any(|v| v.as_str().is_some_and(|s| s.contains(".reaper/java-out"))),
            "{launch}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn plain_java_launch_fallback_skips_maven_projects() {
        let dir = std::env::temp_dir().join(format!(
            "reaper-plain-java-skip-maven-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src/main/java")).unwrap();
        std::fs::create_dir_all(dir.join(".reaper/java-out")).unwrap();
        std::fs::write(
            dir.join("pom.xml"),
            r#"<project><modelVersion>4.0.0</modelVersion>
            <groupId>t</groupId><artifactId>t</artifactId><version>1</version></project>"#,
        )
        .unwrap();
        assert!(
            plain_java_launch_fallback(&dir, "t.App", "src/main/java/App.java", true).is_none()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn python_pytest_mode_is_accepted_by_language_label() {
        let t = JavaRunTarget {
            mode: "python-test".into(),
            class_type: "pytest".into(),
            runnable: true,
            ..Default::default()
        };
        assert_eq!(debug_language_label(Some(&t)), "Python");
    }

    #[test]
    fn go_test_language_label() {
        let t = JavaRunTarget {
            mode: "go-test".into(),
            class_type: "go-test".into(),
            runnable: true,
            ..Default::default()
        };
        assert_eq!(debug_language_label(Some(&t)), "Go");
    }

    #[test]
    fn spring_boot_language_label() {
        let t = JavaRunTarget {
            mode: "spring-boot".into(),
            class_type: "spring-boot-app".into(),
            runnable: true,
            qualified_name: Some("com.example.App".into()),
            ..Default::default()
        };
        assert_eq!(debug_language_label(Some(&t)), "Spring Boot");
    }

    #[test]
    fn rust_pending_test_binary_path_is_provisional() {
        let dir = std::env::temp_dir().join(format!(
            "reaper-rust-debug-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = guess_rust_binary(&dir, "src/lib.rs", true).unwrap();
        assert!(
            path.file_name()
                .and_then(|s| s.to_str())
                .is_some_and(|n| n.ends_with("-pending")),
            "got {}",
            path.display()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
