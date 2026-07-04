//! Host platform helpers (macOS arch detection for bundled runtimes).

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
