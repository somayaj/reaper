Reaper 0.1.3 (build 427) — macOS split release.

**Install:** download the **DMG for your Mac** below. Ignore GitHub's automatic "Source code (zip)" and "Source code (tar.gz)" links — those archives are empty placeholders and are not distributable builds.

| Mac | Download |
|-----|----------|
| Apple Silicon (M1/M2/M3/M4) | `reaper-0.1.3-macos-arm64.dmg` |
| Intel (2015–2020 MacBook Pro, iMac, etc.) | `reaper-0.1.3-macos-x86_64.dmg` |

Drag Reaper.app to Applications, then launch.

### What's new (build 427)

- **Project open:** Workspace open no longer blocks the server — heavy profile detection and indexing run on background threads so projects load reliably.
- **JDK navigation:** `java.lang.String`, `java.util.List`, and other JDK types resolve via extracted JDK sources; warm extract kicks off during splash for Maven and Gradle projects.
- **Paste:** Form fields use native WKWebView paste again — no floating Paste button on every input.
- **Regression tests:** Navigation discovery harness for Gradle/Maven siblings, JDK types, and Spring dependency sources.

### Earlier in 0.1.3 (build 426)

- **Java go-to-definition:** Reliable navigation in Gradle multi-module projects, JDK types, and imported project types.
- **Navigation stability:** Failed go-to-definition no longer poisons later requests; jdtls sessions reset on timeout.
- **Java language level:** Settings → Compiler Java release dropdown.
- **Gradle classpath:** Multi-module `CLASSES:` / `SRCROOT:` from sibling modules.

**Tip:** opening the .dmg repeatedly mounts a new Finder volume each time — eject old Reaper drives or run `scripts/eject-reaper-dmgs.sh`.
