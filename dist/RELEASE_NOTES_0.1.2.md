Reaper 0.1.2 — macOS arm64 (UI build 291)

**Install:** download the **DMG** below. Ignore GitHub's automatic "Source code (zip)" and "Source code (tar.gz)" links — those archives are empty placeholders and are not distributable builds.

Drag Reaper.app to Applications, then launch.

### What's new (build 291)
- **Build-file + tooling classpath merge** — always unions Gradle/Maven resolved classpath with transitive deps from `pom.xml` / `build.gradle` (including test scope)
- **Mockito & test libs** — `@Mock`, `@InjectMocks`, MockitoExtension recognized; indexes `org.mockito`, SLF4J, Lombok from dependency JARs
- **Gradle test classpath** — init script resolves `testCompileClasspath` / `testRuntimeClasspath` after `compileTestJava`

### Prior 0.1.2 highlights (build 290)
- **Gradle/Maven compile classpath** — real build classpath via Gradle `reaperPrintClasspath` / Maven `dependency:build-classpath`
- **Tooling-only background resolve** — classpath updates without wiping a finished Java index
- **Clearer indexing phases** — *Running Gradle (compiling…)* / *Running Gradle (classpath…)* while build tools run
- **AI inline completions**, **live javac diagnostics**, **Java member autocomplete**, **Gemini chat**

**Tip:** opening the .dmg repeatedly mounts a new Finder volume each time — eject old Reaper drives when done.
