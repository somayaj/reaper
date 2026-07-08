Reaper 0.1.3 (build 424) — macOS split release.

**Install:** download the **DMG for your Mac** below. Ignore GitHub's automatic "Source code (zip)" and "Source code (tar.gz)" links — those archives are empty placeholders and are not distributable builds.

| Mac | Download |
|-----|----------|
| Apple Silicon (M1/M2/M3/M4) | `reaper-0.1.3-macos-arm64.dmg` |
| Intel (2015–2020 MacBook Pro, iMac, etc.) | `reaper-0.1.3-macos-x86_64.dmg` |

Drag Reaper.app to Applications, then launch.

### What's new (build 424)

- **Rename Symbol (F6):** Renames all occurrences across the project and renames the `.java` file when the class name matches the filename.
- **File tree rename:** Renaming a Java file updates the class name and all references across the repo.
- **Find Usages:** Faster text search (no jdtls wait when results exist), 12s timeout, results panel opens immediately with structured file/line rows.
- **Rename dialog:** In-app modal replaces blocked `window.prompt` in the WebView.
- **Editor regression:** Find usages/rename harness fixes and expanded refactor tests.

### Earlier in 0.1.3

- **Refactoring (all languages):** Find Usages, Rename Symbol, Change All Occurrences, and Format Document.
- **Compiler settings:** Maven and Gradle path overrides in Settings → Compiler.
- **Build task terminal:** Fixes for build output and spurious connection-failed toasts.
- **Cursor agent:** Model list filtered to models your API key supports.

**Tip:** opening the .dmg repeatedly mounts a new Finder volume each time — eject old Reaper drives or run `scripts/eject-reaper-dmgs.sh`.
