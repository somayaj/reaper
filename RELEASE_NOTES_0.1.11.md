Reaper 0.1.11 (build 494) — macOS.

**Install:** download the **DMG for your Mac** below. Ignore GitHub's automatic "Source code (zip)" and "Source code (tar.gz)" links — those archives are empty placeholders and are not distributable builds.

| Mac | Download |
|-----|----------|
| Apple Silicon (M1/M2/M3/M4) | `reaper-0.1.11-macos-arm64.dmg` |
| Intel (2015–2020 MacBook Pro, iMac, etc.) | `reaper-0.1.11-macos-x86_64.dmg` |

Drag Reaper.app to Applications, then launch. If macOS blocks the app after download, right-click **Open** once or run `xattr -cr /Applications/Reaper.app`.

### What's new

- **Agent chat status in the thread:** The assistant bubble shows live progress (`Connecting…`, `Waiting for reply…`, `Thinking…`, tool activity) until the first reply text arrives — not only the status bar above the input.

**Tip:** opening the .dmg repeatedly mounts a new Finder volume each time — eject old Reaper drives or run `scripts/eject-reaper-dmgs.sh`.
