use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};

use super::exec::try_stdin_command;

const GJF_VERSION: &str = "1.25.2";
const GJF_BIN_NAME: &str = "google-java-format";
const GJF_JAR_NAME: &str = "google-java-format-all-deps.jar";
const GJF_JVM_EXPORTS: &[&str] = &[
    "--add-exports=jdk.compiler/com.sun.tools.javac.api=ALL-UNNAMED",
    "--add-exports=jdk.compiler/com.sun.tools.javac.code=ALL-UNNAMED",
    "--add-exports=jdk.compiler/com.sun.tools.javac.file=ALL-UNNAMED",
    "--add-exports=jdk.compiler/com.sun.tools.javac.parser=ALL-UNNAMED",
    "--add-exports=jdk.compiler/com.sun.tools.javac.tree=ALL-UNNAMED",
    "--add-exports=jdk.compiler/com.sun.tools.javac.util=ALL-UNNAMED",
];

/// Format Java source with [Google Java Format](https://github.com/google/google-java-format).
pub fn format_java(ws: &Path, content: &str) -> Result<String> {
    let mut errors: Vec<String> = Vec::new();

    if let Some(bin) = bundled_google_java_format_bin() {
        match try_stdin_command(ws, bin.to_string_lossy().as_ref(), &["-"], content) {
            Ok(formatted) => return Ok(formatted),
            Err(e) => errors.push(e.to_string()),
        }
    }

    match super::exec::try_tool_stdin(ws, "google-java-format", &["-"], content) {
        Ok(formatted) => return Ok(formatted),
        Err(e) => errors.push(e.to_string()),
    }

    for bin in google_java_format_bins() {
        match try_stdin_command(ws, bin.to_string_lossy().as_ref(), &["-"], content) {
            Ok(formatted) => return Ok(formatted),
            Err(e) => errors.push(e.to_string()),
        }
    }

    if let Ok(formatted) = super::exec::try_shell_stdin_command(ws, "google-java-format", &["-"], content)
    {
        return Ok(formatted);
    }

    if let Some(jar) = bundled_google_java_format_jar() {
        match try_java_jar_stdin(ws, &jar, content) {
            Ok(formatted) => return Ok(formatted),
            Err(e) => errors.push(e.to_string()),
        }
    }

    let hint = errors
        .last()
        .map(|e| format!(" ({e})"))
        .unwrap_or_default();
    bail!(
        "Google Java Format is not available{hint}. Install with `brew install google-java-format`, \
         or rebuild Reaper.app to bundle the formatter."
    );
}

fn google_java_format_bins() -> Vec<PathBuf> {
    let mut out = Vec::new();
    for dir in ["/opt/homebrew/bin", "/usr/local/bin"] {
        let p = PathBuf::from(dir).join("google-java-format");
        if p.is_file() {
            out.push(p);
        }
    }
    if let Some(path_var) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path_var) {
            let p = dir.join("google-java-format");
            if p.is_file() && !out.iter().any(|x| x == &p) {
                out.push(p);
            }
        }
    }
    out
}

pub fn bundled_google_java_format_jar() -> Option<PathBuf> {
    bundled_google_java_format_resource(GJF_JAR_NAME)
}

fn bundled_google_java_format_bin() -> Option<PathBuf> {
    bundled_google_java_format_resource(GJF_BIN_NAME).filter(|p| p.is_file())
}

fn bundled_google_java_format_resource(name: &str) -> Option<PathBuf> {
    if name == GJF_BIN_NAME {
        if let Ok(raw) = std::env::var("REAPER_GOOGLE_JAVA_FORMAT") {
            let p = PathBuf::from(raw.trim());
            if p.is_file() {
                return Some(p);
            }
        }
    } else if let Ok(raw) = std::env::var("REAPER_GOOGLE_JAVA_FORMAT_JAR") {
        let p = PathBuf::from(raw.trim());
        if p.is_file() {
            return Some(p);
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(mac_os) = exe.parent() {
            let path = mac_os.join(format!("../Resources/google-java-format/{name}"));
            if path.is_file() {
                return path.canonicalize().ok();
            }
        }
    }
    let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("resources/google-java-format")
        .join(name);
    if dev.is_file() {
        return Some(dev);
    }
    None
}

fn java_home_for_formatter() -> Result<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        for version in ["17", "21", "11"] {
            if let Ok(out) = std::process::Command::new("/usr/libexec/java_home")
                .arg("-v")
                .arg(version)
                .output()
            {
                if out.status.success() {
                    let home = String::from_utf8_lossy(&out.stdout).trim().to_string();
                    if !home.is_empty() {
                        if let Ok(valid) = crate::jdk::validate_java_home(Path::new(&home)) {
                            return Ok(valid);
                        }
                    }
                }
            }
        }
    }
    crate::jdk::gradle_java_home_with_max(21)
        .or_else(|_| crate::jdk::gradle_java_home_with_max(17))
        .or_else(|_| crate::jdk::effective_java_home())
}

fn try_java_jar_stdin(cwd: &Path, jar: &Path, content: &str) -> Result<String> {
    let java_home = java_home_for_formatter()?;
    let java = crate::jdk::java_bin(&java_home);
    if !java.is_file() {
        bail!("JDK not found for Google Java Format (missing {})", java.display());
    }

    let jar_arg = jar
        .to_str()
        .context("google-java-format jar path is not valid UTF-8")?;

    let mut cmd = Command::new(&java);
    cmd.args(GJF_JVM_EXPORTS)
        .args(["-jar", jar_arg, "-"])
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    crate::jdk::apply_java_env(&mut cmd);

    let mut child = cmd
        .spawn()
        .with_context(|| format!("failed to run {} -jar {}", java.display(), jar.display()))?;

    if let Some(stdin) = child.stdin.as_mut() {
        stdin.write_all(content.as_bytes())?;
    }

    let out = child.wait_with_output()?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        bail!("google-java-format jar failed: {err}");
    }

    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

pub fn google_java_format_version() -> &'static str {
    GJF_VERSION
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn format_java_uses_google_style_when_available() {
        let ws = std::env::temp_dir().join(format!("reaper-gjf-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&ws);
        fs::create_dir_all(&ws).unwrap();
        let src = "public class Hello{public static void main(String[]args){System.out.println(\"hi\");}}";
        let formatted = match format_java(&ws, src) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("skip format_java_uses_google_style_when_available: {e}");
                let _ = fs::remove_dir_all(&ws);
                return;
            }
        };
        assert!(formatted.contains("public class Hello"));
        assert!(formatted.contains('\n'), "expected Google Java Format to add line breaks");
        let _ = fs::remove_dir_all(&ws);
    }
}
