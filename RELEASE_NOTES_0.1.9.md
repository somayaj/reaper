Reaper 0.1.9 (build 492) — macOS.

**Install:** download the **DMG for your Mac** below. Ignore GitHub's automatic "Source code (zip)" and "Source code (tar.gz)" links — those archives are empty placeholders and are not distributable builds.

| Mac | Download |
|-----|----------|
| Apple Silicon (M1/M2/M3/M4) | `reaper-0.1.9-macos-arm64.dmg` |
| Intel (2015–2020 MacBook Pro, iMac, etc.) | `reaper-0.1.9-macos-x86_64.dmg` |

Drag Reaper.app to Applications, then launch.

### What's new

- **Faster Cursor follow-ups:** Each agent message clears a leftover local run before it starts, so the second prompt does not wait on the first.

**Tip:** opening the .dmg repeatedly mounts a new Finder volume each time — eject old Reaper drives or run `scripts/eject-reaper-dmgs.sh`.
