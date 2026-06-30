Reaper 0.1.2 — macOS arm64 (UI build 297)

**Install:** download the **DMG** below. Ignore GitHub's automatic "Source code (zip)" and "Source code (tar.gz)" links — those archives are empty placeholders and are not distributable builds.

Drag Reaper.app to Applications, then launch.

### What's new (build 297)
- **Java diagnostics false positives** — suppress squiggles for SLF4J, Mockito, JUnit, and Lombok `@Slf4j` when build files declare them but Reaper's offline classpath is still catching up
- **Project class references** — hide `cannot find symbol` for classes that exist elsewhere in the workspace sources
- **Per-file test classpath** — test sources merge test-scoped dependency trees for javac diagnostics
- **Gradle/Maven source roots** — recognize Kotlin, integrationTest, testFixtures, and generated layouts; color main/test/generated dirs in the file tree

### Prior 0.1.2 highlights (build 296)
- **Annotation indexing** — `@RestController`, custom `@interface` types, and library annotations detected via classfile flags and shown in `@` completions
- **Generated AP sources** — more Gradle/Maven generated output layouts indexed (MapStruct headers, annotation processor dirs)
- **Gradle wrapper in nested modules** — walks up to repo root `./gradlew` and runs `-p <module>` instead of falling back to Settings/PATH Gradle

**Tip:** opening the .dmg repeatedly mounts a new Finder volume each time — eject old Reaper drives when done.
