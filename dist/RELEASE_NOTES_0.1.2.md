Reaper 0.1.2 — macOS (UI build 347)

**Install:** download the **DMG for your Mac** below. Ignore GitHub's automatic "Source code (zip)" and "Source code (tar.gz)" links — those archives are empty placeholders and are not distributable builds.

| Mac | Download |
|-----|----------|
| Apple Silicon (M1/M2/M3/M4) | `reaper-0.1.2-macos-arm64.dmg` |
| Intel (2015–2020 MacBook Pro, iMac, etc.) | `reaper-0.1.2-macos-x86_64.dmg` |

Requires **macOS 11 (Big Sur)** or later. Drag Reaper.app to Applications, then launch.

**First launch:** ad-hoc signed builds may require right-click → Open once, or allow in System Settings → Privacy & Security.

### What's new (build 347)

- **Run controls** — gutter play icons and toolbar Run scale with editor font size; flat theme-colored icons (no circular chrome)
- **C/C++ navigation** — go-to-definition for system/stdlib headers; external paths open read-only without breaking the tree
- **CMake native run** — C++ projects with `CMakeLists.txt` run via cmake build instead of single-file compile
- **Editor regression suite** — 197 automated language/UI tests run on every `cargo build` (`REAPER_SKIP_EDITOR_TESTS=1` to bypass)
- **Package manifest panel** — browse dependencies from Cargo, npm, Ruby, Go, and CMake manifests
- **Database viewer** — schema browser for project databases (dockable panel)
- **JaCoCo coverage** — ◔ widget on test files; resilient project indexing when Gradle source JARs are corrupt (build 312+)
- **Launch splash** — puzzle pieces snap together as logo quadrants (build 311)

**Tip:** Configure your Cursor API key in Settings → Cursor agent on each Mac. Install `cmake` and `llvm` (for `clangd`) via Homebrew for full C/C++ support.
