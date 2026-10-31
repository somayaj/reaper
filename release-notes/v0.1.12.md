Reaper 0.1.12 (build 495) — macOS.

**Install:** download the **DMG for your Mac** below. Ignore GitHub's automatic "Source code (zip)" and "Source code (tar.gz)" links — those archives are empty placeholders and are not distributable builds.

| Mac | Download |
|-----|----------|
| Apple Silicon (M1/M2/M3/M4) | `reaper-0.1.12-macos-arm64.dmg` |
| Intel (2015–2020 MacBook Pro, iMac, etc.) | `reaper-0.1.12-macos-x86_64.dmg` |

Drag Reaper.app to Applications, then launch. If macOS blocks the app after download, right-click **Open** once or run `xattr -cr /Applications/Reaper.app`.

### What's new

- **Native window drag:** Custom title bar drags the IDE window again on macOS (WKWebView uses native `drag_window` instead of `-webkit-app-region` alone).
- **Open Windows menu:** **Window → Show Open Windows…** (in-app menu and macOS menu bar) lists every Reaper IDE window; pick one to focus it. Integration tests cover IPC and the picker UI.

**Tip:** opening the .dmg repeatedly mounts a new Finder volume each time — eject old Reaper drives or run `scripts/eject-reaper-dmgs.sh`.
