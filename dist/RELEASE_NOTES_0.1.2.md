Reaper 0.1.2 — macOS arm64 (UI build 289)

**Install:** download the **DMG** below. Ignore GitHub's automatic "Source code (zip)" and "Source code (tar.gz)" links — those archives are empty placeholders and are not distributable builds.

Drag Reaper.app to Applications, then launch.

### What's new
- **AI inline completions** — context-aware ghost text for control-flow statements (`for`, `while`, `if`, blocks) across languages
- **Live compiler diagnostics** — errors and warnings show as red squiggles while you type
- **Cross-file Java diagnostics** — errors from other source files show in open tabs via javac overlays
- **AI quick fixes** — sparkling yellow lightbulb in the toolbar and status bar; pick a fix with Ctrl+. when errors are present
- **Editor toolbar** — rollback (undo last edit), format, save, and run alongside the AI fix bulb
- **Google Java Format** — format `.java` files from the editor toolbar
- **Java member autocomplete** — `System.out.`, instance fields, and indexed methods for Maven, Gradle, and plain Java projects
- **Auto project reload** — Maven/Gradle classpath and index refresh when `.java`, `pom.xml`, or Gradle files change
- **Integrated terminal** — xterm.js PTY in the IDE
- **Vendored Monaco + xterm** — same-origin bundles for reliable WKWebView editing
- **Gemini chat** — in-app AI assistance

### Fixes
- Java `for(` type and iterable completions
- File open race that could show empty editor content (e.g. `App.java`)
- WKWebView language bootstrap (`reaper-lang-core`) when the main editor bundle is slow to load
- Inline suggestions restored after a JavaScript parse error in `monaco-languages.js`
- Tab close when switching repository or workspace
- Quick-fix cache so AI fixes appear reliably in the lightbulb menu
- Java diagnostics pass through all `javac` errors with accurate underline spans
- Space and punctuation work after any identifier while completions are active
- Completion popup no longer traps the cursor; arrow keys and click reposition freely
- Classpath resolved from Maven/Gradle dependency trees (not incomplete cache)
- Indexing no longer stalls parsing dependency *-sources* JARs
- Incremental symbol index updates on file save
- Multi-module source roots for diagnostics and completions
- False-positive filters for Spring Data `PageRequest` and repository `save()` when Gradle tests pass
- Removed completion debug toasts and status-bar spam (debug opt-in via `localStorage reaper-complete-debug=1`)

**Tip:** opening the .dmg repeatedly mounts a new Finder volume each time — eject old Reaper drives when done.
