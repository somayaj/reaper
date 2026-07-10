Reaper 0.1.3 (build 429) — macOS split release.

**Install:** download the **DMG for your Mac** below. Ignore GitHub's automatic "Source code (zip)" and "Source code (tar.gz)" links — those archives are empty placeholders and are not distributable builds.

| Mac | Download |
|-----|----------|
| Apple Silicon (M1/M2/M3/M4) | `reaper-0.1.3-macos-arm64.dmg` |
| Intel (2015–2020 MacBook Pro, iMac, etc.) | `reaper-0.1.3-macos-x86_64.dmg` |

Drag Reaper.app to Applications, then launch.

### What's new (build 429)

- **Debug prebuild:** Gradle no longer forces `--rerun-tasks` on every start (incremental `classes` with debug symbols). Prebuild timeout raised to 500s; UI `/debug/start` budget raised to 540s so multi-module Spring Boot projects can finish compiling.

### Earlier in 0.1.3 (build 428)

- **C/C++ debugger:** CodeLLDB 1.12 handshake fixed; DAP `launch` completes after `configurationDone`. CMake Debug prebuild resolves includes and linked sources.
- **Java / Spring Boot debugger:** Maven and Gradle prebuild + jdtls classpath launch; multi-module `projectName` inference.
- **Breakpoints:** Red gutter dots persist across file tabs.
- **Run:** Terminal no longer remounts xterm on every Run click.

**Tip:** opening the .dmg repeatedly mounts a new Finder volume each time — eject old Reaper drives or run `scripts/eject-reaper-dmgs.sh`.
