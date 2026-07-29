//! Host platform helpers (macOS arch detection, Windows console-less child processes).

#[cfg(target_os = "macos")]
use std::sync::OnceLock;

/// macOS host CPU arch: `arm64` or `x86_64`. Falls back to compile-time arch elsewhere.
pub fn macos_host_arch() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        static ARCH: OnceLock<&'static str> = OnceLock::new();
        return ARCH.get_or_init(|| {
            if macos_uname_is_arm64() {
                "arm64"
            } else {
                "x86_64"
            }
        });
    }
    #[cfg(not(target_os = "macos"))]
    {
        std::env::consts::ARCH
    }
}

#[cfg(target_os = "macos")]
fn macos_uname_is_arm64() -> bool {
    std::process::Command::new("uname")
        .arg("-m")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .is_some_and(|o| String::from_utf8_lossy(&o.stdout).trim() == "arm64")
}

/// CREATE_NO_WINDOW — console subsystem children (node, git, where, etc.) stay invisible.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// DETACHED_PROCESS — child does not inherit or allocate a visible console.
#[cfg(windows)]
const DETACHED_PROCESS: u32 = 0x0000_0008;

#[cfg(windows)]
fn is_windows_script(program: &std::path::Path) -> bool {
    program
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("bat") || ext.eq_ignore_ascii_case("cmd"))
}

#[cfg(windows)]
pub(crate) fn windows_console_creation_flags() -> u32 {
    CREATE_NO_WINDOW | DETACHED_PROCESS
}

#[cfg(windows)]
pub(crate) fn windows_user_process_creation_flags() -> u32 {
    CREATE_NO_WINDOW
}

/// Spawn helper — always hides the console on Windows.
pub fn command(program: impl AsRef<std::ffi::OsStr>) -> std::process::Command {
    let mut cmd = std::process::Command::new(program);
    hide_console_window(&mut cmd);
    cmd
}

/// Like [`command`], but avoids `DETACHED_PROCESS` so GUI apps (Swing/JavaFX) can open windows.
pub fn command_user_process(program: impl AsRef<std::ffi::OsStr>) -> std::process::Command {
    let mut cmd = std::process::Command::new(program);
    hide_console_window_user(&mut cmd);
    cmd
}

/// Like [`command`], but runs `.bat`/`.cmd` via `cmd /C` so wrapper scripts never flash a window.
pub fn command_path(program: &std::path::Path) -> std::process::Command {
    #[cfg(windows)]
    {
        if is_windows_script(program) {
            let mut cmd = command("cmd");
            cmd.arg("/C").arg(program);
            return cmd;
        }
    }
    command(program)
}

/// Async spawn helper — always hides the console on Windows.
pub fn async_command(program: impl AsRef<std::ffi::OsStr>) -> tokio::process::Command {
    let mut cmd = tokio::process::Command::new(program);
    hide_console_window_async(&mut cmd);
    cmd
}

/// Like [`async_command`], but runs `.bat`/`.cmd` via `cmd /C` on Windows.
pub fn async_command_path(program: &std::path::Path) -> tokio::process::Command {
    #[cfg(windows)]
    {
        if is_windows_script(program) {
            let mut cmd = async_command("cmd");
            cmd.arg("/C").arg(program);
            return cmd;
        }
    }
    async_command(program)
}

/// User profile directory (`USERPROFILE` on Windows, `HOME` elsewhere).
pub fn user_home_dir() -> Option<std::path::PathBuf> {
    #[cfg(windows)]
    {
        return std::env::var_os("USERPROFILE").map(std::path::PathBuf::from);
    }
    #[cfg(not(windows))]
    {
        std::env::var_os("HOME").map(std::path::PathBuf::from)
    }
}

/// Cursor / VS Code extension install roots.
pub fn editor_extension_roots() -> Vec<std::path::PathBuf> {
    let Some(home) = user_home_dir() else {
        return Vec::new();
    };
    [
        home.join(".cursor").join("extensions"),
        home.join(".vscode").join("extensions"),
    ]
    .into_iter()
    .filter(|p| p.is_dir())
    .collect()
}

/// Interactive shell for PTY / one-shot `-lc` wrappers.
pub fn login_shell() -> String {
    #[cfg(windows)]
    {
        std::env::var("COMSPEC").unwrap_or_else(|_| r"C:\Windows\System32\cmd.exe".into())
    }
    #[cfg(not(windows))]
    {
        std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".into())
    }
}

/// Quote a token for one-shot shell scripts (`cmd /C` on Windows, `bash -lc` on Unix).
pub fn shell_quote_for_script(s: &str) -> String {
    #[cfg(windows)]
    {
        if s.contains(' ') || s.contains('"') || s.contains('&') || s.contains('|') {
            format!("\"{}\"", s.replace('"', "\"\""))
        } else {
            s.to_string()
        }
    }
    #[cfg(not(windows))]
    {
        if s.contains(' ') || s.contains('\'') {
            format!("'{}'", s.replace('\'', "'\\''"))
        } else {
            s.to_string()
        }
    }
}

/// Configure a one-shot shell command (`bash -lc` on Unix, `cmd /C` on Windows).
pub fn configure_shell_script(cmd: &mut std::process::Command, script: &str) {
    #[cfg(windows)]
    {
        cmd.arg("/C").arg(script);
    }
    #[cfg(not(windows))]
    {
        cmd.args(["-lc", script]);
    }
}

/// Prevent Windows from flashing a console window for background child processes.
pub fn hide_console_window(cmd: &mut std::process::Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(windows_console_creation_flags());
    }
    let _ = cmd;
}

/// Hide the console for user-facing programs that may still open GUI windows.
pub fn hide_console_window_user(cmd: &mut std::process::Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(windows_user_process_creation_flags());
    }
    let _ = cmd;
}

/// Same as [`hide_console_window`] for `tokio::process::Command`.
pub fn hide_console_window_async(cmd: &mut tokio::process::Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(windows_console_creation_flags());
    }
    let _ = cmd;
}

/// Drop any inherited/attached console so GUI launches never leave a cmd window up.
#[cfg(windows)]
pub fn hide_attached_console() {
    #[link(name = "kernel32")]
    extern "system" {
        fn GetConsoleWindow() -> *mut core::ffi::c_void;
        fn FreeConsole() -> i32;
    }
    #[link(name = "user32")]
    extern "system" {
        fn ShowWindow(hwnd: *mut core::ffi::c_void, n_cmd_show: i32) -> i32;
    }
    const SW_HIDE: i32 = 0;
    unsafe {
        let hwnd = GetConsoleWindow();
        if !hwnd.is_null() {
            ShowWindow(hwnd, SW_HIDE);
        }
        let _ = FreeConsole();
    }
}

/// Alias for [`hide_attached_console`].
#[cfg(windows)]
pub fn free_console() {
    hide_attached_console();
}

/// Load the embedded 32×32 PNG generated from logo-icon.svg at build time.
#[cfg(target_os = "windows")]
pub fn app_window_icon() -> Option<tao::window::Icon> {
    use std::io::BufReader;

    const ICON_PNG: &[u8] = include_bytes!("../packaging/windows/icon-32.png");
    let decoder = png::Decoder::new(BufReader::new(ICON_PNG));
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;
    let bytes = &buf[..info.buffer_size()];
    tao::window::Icon::from_rgba(bytes.to_vec(), info.width, info.height).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    #[cfg(windows)]
    fn command_path_wraps_batch_and_cmd_scripts() {
        for script in ["C:\\repo\\gradlew.bat", "C:\\repo\\mvnw.cmd"] {
            let path = PathBuf::from(script);
            let cmd = command_path(&path);
            assert_eq!(cmd.get_program(), "cmd");
            let args: Vec<_> = cmd.get_args().collect();
            assert_eq!(args.len(), 2);
            assert_eq!(args[0], "/C");
            assert_eq!(args[1], path.as_os_str());
        }
    }

    #[test]
    #[cfg(windows)]
    fn command_path_passes_through_non_script_executables() {
        let java = PathBuf::from(r"C:\Program Files\Microsoft\jdk-17\bin\java.exe");
        let cmd = command_path(&java);
        assert_eq!(cmd.get_program(), java.as_os_str());
        assert!(cmd.get_args().len() == 0);
    }

    #[test]
    #[cfg(not(windows))]
    fn command_path_is_direct_on_unix() {
        let mvnw = PathBuf::from("./mvnw");
        let cmd = command_path(&mvnw);
        assert_eq!(cmd.get_program(), mvnw.as_os_str());
    }

    #[test]
    #[cfg(windows)]
    fn hide_console_window_uses_detached_no_window_flags() {
        assert_eq!(
            windows_console_creation_flags(),
            CREATE_NO_WINDOW | DETACHED_PROCESS
        );
    }

    #[test]
    #[cfg(windows)]
    fn login_shell_defaults_to_comspec() {
        let shell = login_shell();
        assert!(
            shell.to_ascii_lowercase().ends_with("cmd.exe"),
            "expected cmd.exe shell, got {shell}"
        );
    }

    #[test]
    #[cfg(windows)]
    fn app_window_icon_loads_embedded_png() {
        assert!(app_window_icon().is_some());
    }
}
