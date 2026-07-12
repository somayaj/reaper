Reaper 0.1.4 (build 454) — macOS split release.

**Install:** download the **DMG for your Mac** below. Ignore GitHub's automatic "Source code (zip)" and "Source code (tar.gz)" links — those archives are empty placeholders and are not distributable builds.

| Mac | Download |
|-----|----------|
| Apple Silicon (M1/M2/M3/M4) | `reaper-0.1.4-macos-arm64.dmg` |
| Intel (2015–2020 MacBook Pro, iMac, etc.) | `reaper-0.1.4-macos-x86_64.dmg` |

Drag Reaper.app to Applications, then launch.

### What's new (build 454)

- **Claude + Amazon Bedrock:** Settings → AI — Anthropic API or Bedrock (Mantle key or AWS IAM). Claude agent chat, inline completions, and AI quick fixes.
- **Faster AI quick fixes:** Cursor and Gemini race in parallel (first useful edit wins), then Claude API, then Bedrock. Java jdtls overlaps with AI instead of blocking it.
- **Honest empty-state toast:** If Cursor/Gemini are configured but return nothing, Reaper says so — it no longer claims you need to configure Settings.

### Also in 0.1.4

- **Docker Console:** View menu → Docker. Container list, live logs, compose quick actions, per-container Start/Stop/Restart/Logs, and a freeform `docker …` bar.
- **Plain Java debug:** Debug a `public static void main` class without Maven/Gradle — `javac -g` into `.reaper/java-out` and the bundled Java Debug adapter.

**Tip:** opening the .dmg repeatedly mounts a new Finder volume each time — eject old Reaper drives or run `scripts/eject-reaper-dmgs.sh`.
