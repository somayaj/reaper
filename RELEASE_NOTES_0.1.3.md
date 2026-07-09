Reaper 0.1.3 (build 426) — macOS split release.

**Install:** download the **DMG for your Mac** below. Ignore GitHub's automatic "Source code (zip)" and "Source code (tar.gz)" links — those archives are empty placeholders and are not distributable builds.

| Mac | Download |
|-----|----------|
| Apple Silicon (M1/M2/M3/M4) | `reaper-0.1.3-macos-arm64.dmg` |
| Intel (2015–2020 MacBook Pro, iMac, etc.) | `reaper-0.1.3-macos-x86_64.dmg` |

Drag Reaper.app to Applications, then launch.

### What's new (build 426)

- **Java go-to-definition:** Reliable navigation in Gradle multi-module projects (sibling modules like `libs:common`), JDK types (`String`, `RuntimeException`), and imported project types. Classpath/index runs first; jdtls supplements library lookups.
- **Navigation stability:** Failed go-to-definition no longer poisons later requests; jdtls sessions reset on timeout; definition cache evicts jar/build-output stubs.
- **Java language level:** Settings → Compiler Java release dropdown (`--release` for javac and inline AI context).
- **Gradle classpath:** Multi-module `CLASSES:` / `SRCROOT:` from sibling modules; improved annotation completion and missing-import diagnostics.
- **Regression tests:** 1319 editor tests.

### Earlier in 0.1.3 (build 425)

- **Empty-line inline AI:** Tabnine-style ghost text on blank lines — Cursor agent → Gemini → Claude, then LSP/index fallbacks.
- **Cursor agent chat:** Auto-retries with a fresh session after bridge restarts.
- **Rename / Find Usages / Format:** Cross-language refactoring with Java file rename sync.

**Tip:** opening the .dmg repeatedly mounts a new Finder volume each time — eject old Reaper drives or run `scripts/eject-reaper-dmgs.sh`.
