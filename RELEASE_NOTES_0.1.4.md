Reaper 0.1.4 — macOS split release.

**Install:** download the **DMG for your Mac** below. Ignore GitHub's automatic "Source code (zip)" and "Source code (tar.gz)" links — those archives are empty placeholders and are not distributable builds.

| Mac | Download |
|-----|----------|
| Apple Silicon (M1/M2/M3/M4) | `reaper-0.1.4-macos-arm64.dmg` |
| Intel (2015–2020 MacBook Pro, iMac, etc.) | `reaper-0.1.4-macos-x86_64.dmg` |

Drag Reaper.app to Applications, then launch.

### What's new

- **Docker Console:** View menu → Docker. Container list (`docker ps`), live logs, compose quick actions (Up / Down / Ps / Build / Follow), per-container Start / Stop / Restart / Logs, and a freeform `docker …` command bar (same idea as Git Console).
- **Docker logs fix (build 431–432):** Output pane keeps visible height; container/`docker ps` commands no longer depend on a compose project cwd; `docker logs` stderr is captured. Build **432** loads a finite log tail first, then follows via XHR streaming so WKWebView shows live lines.

### Also in recent 0.1.3 builds

- C/C++ and Java/Spring Boot debugger fixes, breakpoint gutter persistence, longer Gradle debug prebuild budget.

**Tip:** opening the .dmg repeatedly mounts a new Finder volume each time — eject old Reaper drives or run `scripts/eject-reaper-dmgs.sh`.
