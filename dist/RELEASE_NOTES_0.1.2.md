Reaper 0.1.2 — macOS arm64 (UI build 292)

**Install:** download the **DMG** below. Ignore GitHub's automatic "Source code (zip)" and "Source code (tar.gz)" links — those archives are empty placeholders and are not distributable builds.

Drag Reaper.app to Applications, then launch.

### What's new (build 292)
- **PAT detection fixes** — repo info no longer says "add a PAT" when a token is already saved
- **Host aliases** — `www.github.com` and port-suffixed hosts resolve to your `github.com` token; wildcard `*` still applies
- **Smarter clone errors** — "add a PAT" hint only when no token exists for that host (not on expired-token 401s)

### Prior 0.1.2 highlights (build 291)
- **Build-file + tooling classpath merge** — unions Gradle/Maven resolved classpath with transitive deps from build files (test scope)
- **Mockito & test libs** — `@Mock`, `@InjectMocks`, MockitoExtension recognized from dependency JARs
- **Gradle test classpath** — init script resolves `testCompileClasspath` / `testRuntimeClasspath`

**Tip:** opening the .dmg repeatedly mounts a new Finder volume each time — eject old Reaper drives when done.
