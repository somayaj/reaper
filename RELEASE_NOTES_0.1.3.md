Reaper 0.1.3 (build 428) — macOS split release.

**Install:** download the **DMG for your Mac** below. Ignore GitHub's automatic "Source code (zip)" and "Source code (tar.gz)" links — those archives are empty placeholders and are not distributable builds.

| Mac | Download |
|-----|----------|
| Apple Silicon (M1/M2/M3/M4) | `reaper-0.1.3-macos-arm64.dmg` |
| Intel (2015–2020 MacBook Pro, iMac, etc.) | `reaper-0.1.3-macos-x86_64.dmg` |

Drag Reaper.app to Applications, then launch.

### What's new (build 428)

- **C/C++ debugger:** CodeLLDB 1.12 handshake fixed (stdio default); DAP `launch` completes after `configurationDone` so sessions no longer stick on Starting…. CMake Debug prebuild resolves includes and linked sources (e.g. `greeter.hpp`).
- **Java / Spring Boot debugger:** Maven and Gradle prebuild + jdtls classpath launch; multi-module `projectName` inference; classpath merges resolved jars so SLF4J and Spring Cloud types resolve at debug time.
- **Breakpoints:** Red gutter dots persist across file tabs (no longer cleared by editor remounts).
- **Run:** Opening the terminal for Run no longer remounts xterm on every click (first-click no-op race fixed).
- **Maven:** Nested BOM import expansion and wrapper/`-pl` handling for multi-module projects.

### Earlier in 0.1.3 (build 427)

- **Project open:** Workspace open no longer blocks the server — heavy profile detection and indexing run on background threads so projects load reliably.
- **JDK navigation:** `java.lang.String`, `java.util.List`, and other JDK types resolve via extracted JDK sources; warm extract kicks off during splash for Maven and Gradle projects.
- **Paste:** Form fields use native WKWebView paste again — no floating Paste button on every input.

**Tip:** opening the .dmg repeatedly mounts a new Finder volume each time — eject old Reaper drives or run `scripts/eject-reaper-dmgs.sh`.
