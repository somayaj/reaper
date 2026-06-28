Reaper 0.1.2 — macOS arm64

**Install:** download the **DMG** below. Ignore GitHub's automatic "Source code (zip)" and "Source code (tar.gz)" links — those archives are empty placeholders and are not distributable builds.

Drag Reaper.app to Applications, then launch.

### What's new
- **AI inline completions** — context-aware ghost text for control-flow statements (`for`, `while`, `if`, blocks) across languages
- **Live compiler diagnostics** — errors and warnings show as red squiggles while you type
- **AI quick fixes** — sparkling yellow lightbulb in the toolbar and status bar; pick a fix with Ctrl+. when errors are present
- **Editor toolbar** — rollback (undo last edit), format, save, and run alongside the AI fix bulb
- **Cursor agent typography** — separate font and size settings for the agent panel
- **Language-aware AI** — inline and quick-fix suggestions respect project compiler and language versions

### Fixes
- Tab close when switching repository or workspace
- Quick-fix cache so AI fixes appear reliably in the lightbulb menu
- Java diagnostics pass through all `javac` errors with accurate underline spans

**Tip:** opening the .dmg repeatedly mounts a new Finder volume each time — eject old Reaper drives when done.
