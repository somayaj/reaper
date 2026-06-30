Reaper 0.1.2 — macOS arm64 (UI build 290)

**Install:** download the **DMG** below. Ignore GitHub's automatic "Source code (zip)" and "Source code (tar.gz)" links — those archives are empty placeholders and are not distributable builds.

Drag Reaper.app to Applications, then launch.

### What's new (build 290)
- **Gradle/Maven compile classpath** — indexing and live diagnostics use the real build classpath (via Gradle `reaperPrintClasspath` / Maven `dependency:build-classpath`) instead of incomplete tree-walk guesses
- **Tooling-only background resolve** — Gradle/Maven classpath updates no longer wipe and restart a finished Java index
- **Clearer indexing phases** — status shows *Running Gradle (compiling…)*, *Running Gradle (classpath…)*, and Maven equivalents while build tools run
- **Stable classpath cache** — build-file changes invalidate tooling cache; index cache stamps use stable file mtimes

### Prior 0.1.2 highlights
- **AI inline completions** — context-aware ghost text for control-flow statements (`for`, `while`, `if`, blocks) across languages
- **Live compiler diagnostics** — errors and warnings show as red squiggles while you type
- **Cross-file Java diagnostics** — errors from other source files show in open tabs via javac overlays
- **AI quick fixes** — sparkling yellow lightbulb in the toolbar and status bar; pick a fix with Ctrl+. when errors are present
- **Java member autocomplete** — `System.out.`, instance fields, and indexed methods for Maven, Gradle, and plain Java projects
- **Auto project reload** — Maven/Gradle classpath and index refresh when `.java`, `pom.xml`, or Gradle files change
- **Integrated terminal** — xterm.js PTY in the IDE
- **Gemini chat** — in-app AI assistance

**Tip:** opening the .dmg repeatedly mounts a new Finder volume each time — eject old Reaper drives when done.
