Reaper 0.1.2 — macOS arm64 (UI build 293)

**Install:** download the **DMG** below. Ignore GitHub's automatic "Source code (zip)" and "Source code (tar.gz)" links — those archives are empty placeholders and are not distributable builds.

Drag Reaper.app to Applications, then launch.

### What's new (build 293)
- **Gradle version picker** — Settings → Compiler lists installed Gradle versions (wrapper cache, Homebrew, PATH) and lets you pick one for build runs
- **Build-run override** — configured Gradle is used when a project has no `gradlew` (project wrapper still wins when present)
- **Manual path** — enter a `gradle` binary or Gradle home directory; `REAPER_GRADLE` env override still works

### Prior 0.1.2 highlights (build 292)
- **PAT detection fixes** — repo info no longer says "add a PAT" when a token is already saved
- **Host aliases** — `www.github.com` and port-suffixed hosts resolve to your `github.com` token; wildcard `*` still applies
- **Smarter clone errors** — "add a PAT" hint only when no token exists for that host (not on expired-token 401s)

**Tip:** opening the .dmg repeatedly mounts a new Finder volume each time — eject old Reaper drives when done.
