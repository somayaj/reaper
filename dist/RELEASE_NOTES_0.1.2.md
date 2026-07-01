Reaper 0.1.2 — macOS (UI build 301)

**Install:** download the **DMG for your Mac** below. Ignore GitHub's automatic "Source code (zip)" and "Source code (tar.gz)" links — those archives are empty placeholders and are not distributable builds.

| Mac | Download |
|-----|----------|
| Apple Silicon (M1/M2/M3/M4) | `reaper-0.1.2-macos-arm64.dmg` |
| Intel (2015–2020 MacBook Pro, iMac, etc.) | `reaper-0.1.2-macos-x86_64.dmg` |

Requires **macOS 11 (Big Sur)** or later. Drag Reaper.app to Applications, then launch.

**First launch:** ad-hoc signed builds may require right-click → Open once, or allow in System Settings → Privacy & Security.

### What's new (build 301)
- **Download links in app** — welcome screen and status bar link to Apple Silicon and Intel DMGs on GitHub
- **Cursor agent reliability** — replace orphan bridges, surface SDK errors, fix stale bridge URL after restart
- **Launch splash** — puzzle logo animation, harvest loop, clean black startup
- **JaCoCo coverage** — run tests with coverage, gutter highlights, status bar panel

### Prior 0.1.2 highlights
- Java diagnostics false-positive fixes, annotation indexing, Gradle wrapper in nested modules

**Tip:** opening the .dmg repeatedly mounts a new Finder volume each time — eject old Reaper drives when done.

SHA256 (arm64): `366883a2c317b2ae44042b24370455e15458a4469a06e0a08a0e95fbd0ea3bbb`

SHA256 (x86_64): `fcae139fe06cbcb77e2b198884117083857fd27c67fddb9ece09ce5ee35c487b`
