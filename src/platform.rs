//! Host platform helpers (macOS arch detection, Windows console-less child processes).

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

/// Prevent Windows from flashing a console window for background child processes.
pub fn hide_console_window(cmd: &mut std::process::Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let _ = cmd;
}

/// Same as [`hide_console_window`] for `tokio::process::Command`.
pub fn hide_console_window_async(cmd: &mut tokio::process::Command) {
    #[cfg(windows)]
    {
        cmd.creation_flags(0x0800_0000);
    }
    let _ = cmd;
}
