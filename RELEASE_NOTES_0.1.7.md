Reaper 0.1.7 (build 483) — macOS split release.

**Install:** download the **DMG for your Mac** below. Ignore GitHub's automatic "Source code (zip)" and "Source code (tar.gz)" links — those archives are empty placeholders and are not distributable builds.

| Mac | Download |
|-----|----------|
| Apple Silicon (M1/M2/M3/M4) | `reaper-0.1.7-macos-arm64.dmg` |
| Intel (2015–2020 MacBook Pro, iMac, etc.) | `reaper-0.1.7-macos-x86_64.dmg` |

Drag Reaper.app to Applications, then launch.

### What's new

- **Free typing after Escape / `var`:** Escape (and declaration lead-ins like `var` / `Type name`) latch free-typing on the line — suggest/AI/tab-complete stay off until `=` / `;`, leaving the line, or Ctrl+Space. Build **483**.
- **Escape frees the editor:** Escape dismisses suggest/AI ghosts, restores focus, and briefly suppresses reopen so you can keep typing. Build **482**.
- **Space never blocks typing:** Suggest/AI can still show, but Space/punctuation dismiss the popup instead of swallowing the key — fixes stuck caret after `var name` / `String value`. Build **481**.
- **Faster Cursor agent replies:** Chat no longer waits on session warm before streaming, and successful model checks are cached so each message skips a full model-list round-trip. Build **476**.
- **Modifiers no longer force autocorrect:** Typing `private` / `public` / `static` / other modifiers prefers the keyword over nearby identifiers. After a finished modifier (or modifier + space), suggest/index popups are dismissed so the next word types freely — no `PrivateKeyEntry` hijack. Applies across Java, Kotlin, Groovy, C#, C/C++, Swift, PHP, TypeScript/JavaScript, Rust, and Dart. Build **479**.
- **Clean Java declaration typing:** After `var` / primitives / `Type name`, Space stays a real space (no ghost accept inserting `.`), and `=` gets spaces (`name=` → `name = `). Build **480**.

### Also in 0.1.6

- Elide (`elide.pkl`) Build Tasks and Compiler path, Pkl highlighting, Structure/AST panel, Database SSH/SSL improvements.

**Tip:** opening the .dmg repeatedly mounts a new Finder volume each time — eject old Reaper drives or run `scripts/eject-reaper-dmgs.sh`.
