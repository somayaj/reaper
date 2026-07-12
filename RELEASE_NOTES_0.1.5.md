Reaper 0.1.5 (build 456) — macOS split release.

**Install:** download the **DMG for your Mac** below. Ignore GitHub's automatic "Source code (zip)" and "Source code (tar.gz)" links — those archives are empty placeholders and are not distributable builds.

| Mac | Download |
|-----|----------|
| Apple Silicon (M1/M2/M3/M4) | `reaper-0.1.5-macos-arm64.dmg` |
| Intel (2015–2020 MacBook Pro, iMac, etc.) | `reaper-0.1.5-macos-x86_64.dmg` |

Drag Reaper.app to Applications, then launch.

### What's new

- **Bedrock agent tab:** Separate from Claude — Cursor · Gemini · Claude · Bedrock in the agent picker. Settings → AI has distinct Claude (Anthropic API) and Amazon Bedrock panels.
- **Live Bedrock model catalog:** With AWS credentials, Reaper lists authorized text/chat foundation models and inference profiles in your region (Nova, Llama, Mistral, Claude, …). Mantle-only still shows Claude models. Refresh list in Settings → AI → Bedrock.
- **Bedrock Converse:** IAM chat uses the Bedrock Converse API so non-Claude models work (not Anthropic Messages-only).
- **Faster AI quick fixes:** Cursor and Gemini race in parallel; Java jdtls overlaps with AI. Honest toasts when providers are configured but return nothing.

### Also in 0.1.4

- Docker Console, plain Java debug, Claude/Bedrock settings foundation.

**Tip:** opening the .dmg repeatedly mounts a new Finder volume each time — eject old Reaper drives or run `scripts/eject-reaper-dmgs.sh`.
