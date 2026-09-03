# Reaper

A **local IDE** for **Rust developers** — Cargo, rustc, rustfmt, clippy, and CodeLLDB in one app, plus git, debug, and polyglot editing. Built in Rust. Runs on your machine. No cloud account.

Open a crate, run it, test it, debug it, commit it.

## Install

macOS builds are on [reaper-org/releases](https://github.com/reaper-org/releases/releases). Download the **DMG for your Mac** from the latest release. Ignore GitHub’s “Source code (zip)” and “Source code (tar.gz)” links — those archives are empty placeholders, not the app.

| Mac | File |
|-----|------|
| Apple Silicon (M1/M2/M3/M4) | `reaper-<version>-macos-arm64.dmg` |
| Intel (2015–2020 MacBook Pro, iMac, etc.) | `reaper-<version>-macos-x86_64.dmg` |

1. Open the DMG and drag **Reaper.app** into **Applications**.
2. Launch Reaper from Applications or Spotlight.
3. If macOS says the app is from an unidentified developer: right-click **Reaper.app** → **Open**, or use **System Settings → Privacy & Security → Open Anyway**.

Opening the same DMG more than once mounts another Finder volume each time. Eject old Reaper drives when you are done.

Need `rustc` / `cargo` on `PATH` (or set them under **Settings → Compiler**). Git 2.x on `PATH` for remotes.

To run Reaper from source instead, see [Run from source](#run-from-source).

## IDE for Rust developers

Reaper treats a crate as a **Cargo project**, not a folder of `.rs` files.

| You want | What Reaper does |
|----------|------------------|
| Build / check / lint | `cargo build`, `cargo check`, `cargo clippy` from the Cargo tasks and manifest panel |
| Run the crate | Toolbar / gutter **Run** → `cargo run` (or `--bin` / `--example` when that file is open) |
| Test | `cargo test`, or `cargo test --test <name>` for `tests/*.rs` |
| Single-file scratch | No `Cargo.toml`? Compile and run (or `--test`) with **rustc** |
| Errors in the editor | **rustc** diagnostics (codes like `E0412`) as squiggles |
| Format | **rustfmt** |
| Outline | Structure / AST via **tree-sitter Rust** |
| Jump to a type | Symbol index for `struct` / `enum` / `impl` / `fn` |
| Debug | Bundled **CodeLLDB**: breakpoints (F9), stack, locals, watches; `cargo build` or `cargo test --no-run` first |
| See the crate | Dockable **Cargo.toml** panel: name, edition, `rust-version`, workspace members, dependencies, features, bins |
| Pin a toolchain | **Settings → Compiler** → `rustc` and `cargo` (`REAPER_RUSTC` / `REAPER_CARGO`) |

Open `Cargo.toml` or any `.rs` file. Nested crates (`crates/foo/Cargo.toml`) and workspace members are detected the same way.

**Not rust-analyzer.** There is no rust-analyzer LSP. Reaper uses rustc, rustfmt, clippy, tree-sitter, and a symbol index. For RA-level inlay hints and trait navigation, keep rust-analyzer in another editor if you need it. Reaper’s loop is **run / test / debug / git**.

## Languages

Every language below gets Monaco highlighting and keyword completions. Run, debug, check, format, and outline are listed only where Reaper actually implements them. Pin tools under **Settings → Compiler** or `REAPER_*` env vars.

### Rust

`.rs` — see [IDE for Rust developers](#ide-for-rust-developers).

- **Run / tasks:** `cargo build` / `test` / `run` / `check` / `clippy`; `--bin` / `--example` / `--test`; standalone `rustc` / `rustc --test`
- **Debug:** CodeLLDB (bundled); prebuild `cargo build` or `cargo test --no-run`
- **Check:** rustc squiggles
- **Format:** rustfmt
- **Structure:** tree-sitter; symbol index (`struct` / `enum` / `impl` / `fn`)
- **Manifest:** `Cargo.toml` (workspace members, deps, features, bins, edition, `rust-version`)
- **Compiler:** `rustc`, `cargo` (`REAPER_RUSTC`, `REAPER_CARGO`)

### Java

`.java`

- **Run / tasks:** Gradle, Maven, Spring Boot from toolbar, gutter, or build file
- **Debug:** bundled jdtls + java-debug (or VS Code Java Debug)
- **Check:** javac diagnostics; jdtls go-to-def / completions
- **Format:** google-java-format (bundled on macOS DMG)
- **Structure:** tree-sitter; Go to Class; classpath index
- **Coverage:** JaCoCo widgets on test files
- **Compiler:** `JAVA_HOME`, `gradle`, `mvn`, `jdtls` (`REAPER_JAVA_HOME`, `REAPER_GRADLE`, `REAPER_MVN`, `REAPER_JDTLS`)

### Kotlin

`.kt`, `.kts`, `.gradle.kts`

- **Run / tasks:** Gradle / kotlinc
- **Debug:** Java debug stack (jdtls + java-debug)
- **Check:** kotlinc diagnostics
- **Format:** ktfmt or ktlint if on PATH
- **Structure:** keyword / symbol index
- **Compiler:** `kotlin` (`REAPER_KOTLINC`)

### Groovy

`.groovy`, `.gradle`

- **Run / tasks:** Gradle
- **Check:** groovyc diagnostics
- **Format:** prettier groovy parser if present; else trim whitespace
- **Structure:** keyword index
- **Compiler:** `groovy`, `gradle` (`REAPER_GROOVC`, `REAPER_GRADLE`)

### Python

`.py`, `.pyw`

- **Run / tasks:** `python3` on the file; `pyproject.toml` / `requirements.txt` / `Pipfile` / Django tasks when present
- **Debug:** debugpy (bundled; needs `python3` on PATH)
- **Check:** Python diagnostics
- **Format:** black or autopep8 if on PATH
- **Structure:** tree-sitter
- **Compiler:** `python` (`REAPER_PYTHON`)

### Go

`.go`, `go.mod`

- **Run / tasks:** `go build ./...`, `go test ./...`, `go run .`, `go mod tidy`
- **Debug:** delve (bundled)
- **Check:** go / compiler diagnostics
- **Format:** gofmt
- **Structure:** tree-sitter
- **Manifest:** `go.mod`
- **Compiler:** `go` (`REAPER_GO`)

### JavaScript and TypeScript

`.js`, `.mjs`, `.cjs`, `.jsx`, `.ts`, `.tsx`

- **Run / tasks:** Node on scripts; npm/yarn/pnpm tree from `package.json`; test files (`*.test.js` / `*.test.ts`)
- **Debug:** js-debug (bundled Node)
- **Check:** JavaScript / `tsc` diagnostics
- **Format:** prettier
- **Structure:** tree-sitter
- **Compiler:** `node`, `tsc` (`REAPER_NODEJS`, `REAPER_TSC`)

### C and C++

`.c`, `.h`, `.cpp`, `.cc`, `.cxx`, `.hpp`, `.hh`

- **Run / tasks:** clang / gcc; CMake, Make, Meson, vcpkg, Conan when those manifests exist
- **Debug:** CodeLLDB (bundled)
- **Check:** clang / gcc diagnostics; **clangd** go-to-def / navigation
- **Format:** clang-format
- **Structure:** tree-sitter
- **Compiler:** `clang`, `gcc`, `clangd` (`REAPER_CLANG`, `REAPER_GCC`, `REAPER_CLANGD`)

### Ruby

`.rb`, `Gemfile`, `Rakefile`

- **Run / tasks:** `ruby` / `bundle exec`; Rails (`bin/rails test`); RSpec; Rake task tree
- **Check:** Ruby diagnostics; Solargraph when configured
- **Format:** rufo if on PATH
- **Structure:** class / method index (models, controllers, helpers)
- **Manifest:** `Gemfile`
- **Compiler:** `ruby`, `bundle`, `rails` (`REAPER_RUBY`, `REAPER_BUNDLE`, `REAPER_RAILS`)

### PHP

`.php`

- **Run / tasks:** `php` on the file; Composer tree from `composer.json`
- **Check:** php diagnostics
- **Format:** php-cs-fixer if on PATH
- **Structure:** keyword index
- **Compiler:** `php` (`REAPER_PHP`)

### C#

`.cs`

- **Run / tasks:** `csc` when configured
- **Check:** csc diagnostics
- **Format:** csharpier if on PATH
- **Structure:** keyword index
- **Compiler:** `csc` (`REAPER_CSC`)

### Swift

`.swift`

- **Run / tasks:** `swiftc` when configured
- **Check:** swiftc diagnostics
- **Format:** swift-format if on PATH
- **Structure:** keyword index
- **Compiler:** `swiftc` (`REAPER_SWIFTC`)

### Dart and Flutter

`.dart`, `pubspec.yaml`

- **Run / tasks:** `dart` / Flutter; pubspec task panel
- **Check:** dart diagnostics
- **Format:** `dart format`
- **Structure:** keyword index
- **Compiler:** `dart` (`REAPER_DART`)

### Scala

`.scala` (and Scala run targets)

- **Run / tasks:** `scala` when configured
- **Format:** scalafmt if on PATH
- **Compiler:** `scala` (`REAPER_SCALA`)

### Clojure

- **Run / tasks:** `clojure` when configured
- **Compiler:** `clojure` (`REAPER_CLOJURE`)

### Lua

`.lua`

- **Run / tasks:** `luac` when configured
- **Check:** luac diagnostics
- **Format:** stylua if on PATH
- **Structure:** keyword index
- **Compiler:** `luac` (`REAPER_LUAC`)

### R

`.r`

- **Highlight:** yes
- **Check:** R diagnostics
- **Structure:** keyword index

### Shell

`.sh`, `.bash`, `.zsh`

- **Run / tasks:** bash / sh / zsh on the script
- **Check:** shellcheck-style diagnostics
- **Format:** shfmt if on PATH
- **Structure:** function index
- **Compiler:** `bash` (`REAPER_BASH`)

### SQL

`.sql`

- **Run / tasks:** execute against the Database viewer connection (SQLite, PostgreSQL, MySQL/MariaDB; optional SSL + SSH bastion)
- **Check:** SQL diagnostics
- **Format:** sqlfluff if on PATH
- **Structure:** table / object index
- **Tools:** `psql`, `sqlite3`, `sqlfluff`

### YAML

`.yml`, `.yaml`

- **Check:** yamllint; optional actionlint / kubeconform by content
- **Format:** yamlfmt, prettier, or built-in YAML format
- **Structure:** tree-sitter
- **Tools:** `yamllint`, `yamlfmt` (`REAPER_YAMLLINT`)

### JSON

`.json`, `.jsonc`

- **Check:** jsonlint / JSON parse (jsonc comments stripped)
- **Format:** prettier
- **Structure:** tree-sitter
- **Tools:** `jsonlint`, `ajv`

### TOML

`.toml` (including `Cargo.toml`)

- **Check:** TOML diagnostics
- **Format:** taplo if on PATH
- **Manifest:** Cargo / pyproject when those files are open

### Markdown

`.md`, `.mdx`

- **Check:** markdown diagnostics
- **Format:** prettier

### HTML, Vue, Svelte

`.html`, `.htm`, `.vue`, `.svelte`

- **Check:** XML/HTML or component-file diagnostics
- **Format:** prettier

### CSS, SCSS, Less

`.css`, `.scss`, `.less`

- **Check:** stylelint if on PATH
- **Format:** prettier

### XML

`.xml` (including `pom.xml`)

- **Check:** XML diagnostics
- **Format:** prettier
- **Manifest:** Maven when `pom.xml` is the project file

### Protobuf

`.proto`

- **Check:** protoc / protobuf diagnostics
- **Highlight:** yes

### GraphQL

`.graphql`, `.gql`

- **Check:** GraphQL diagnostics
- **Format:** prettier

### Dockerfile

`Dockerfile`, `Dockerfile.*`

- **Run / tasks:** Docker / Compose tree when compose files are present
- **Check:** Dockerfile diagnostics

### Makefile

`Makefile`, `GNUmakefile`

- **Run / tasks:** Make target tree (runnable targets)
- **Check:** Makefile diagnostics

### CMake

`CMakeLists.txt`

- **Run / tasks:** CMake configure / build tree
- **Check:** CMake diagnostics
- **Format:** cmake-format if on PATH

### Pkl / Elide

`.pkl`, `elide.pkl`

- **Run / tasks:** Elide lifecycle and script bridges (`cargo:`, Maven, Gradle, …)
- **Highlight:** Pkl
- **Compiler:** `elide` (`REAPER_ELIDE`)

### INI and properties

`.ini`, `.properties`, `.gradle.properties`

- **Check:** INI / properties diagnostics
- **Highlight:** yes
- **Also:** Spring property completions when those files are in a Java project

### Other manifests

Opened as task / dependency panels (not a full language IDE): `package.json`, `composer.json`, `pyproject.toml`, `requirements.txt`, `Pipfile`, `Gemfile`, `pubspec.yaml`, `vcpkg.json`, `conanfile.txt` / `conanfile.py`, `meson.build`.

## Debugger

Dockable panel and toolbar:

- **Breakpoints** — glyph margin or **F9**
- **Call stack** — click a frame to jump to source
- **Variables** — locals at the pause
- **Watch** — expressions while stopped
- **Controls** — **F6** start debug, **F5** continue, **F10** step over, **F11** step in, **⇧F11** step out

| Language | Adapter |
|----------|---------|
| **Rust** / C / C++ | `codelldb` (bundled in Reaper.app) |
| Python | `debugpy` (bundled; needs `python3` on PATH) |
| Go | `delve` (bundled) |
| JavaScript / TypeScript | `js-debug` (bundled; uses bundled Node) |
| Java / Kotlin | Bundled jdtls + java-debug (or VS Code Java Debug) |

macOS DMGs vendor adapters into `Reaper.app/Contents/Resources/debug-adapters/`. From source, once:

```bash
./scripts/vendor-debug-adapters-macos.sh
```

The debug button tooltips if the active file cannot be debugged.

## Editor

- Monaco tabs, file tree, diffs
- Inline **git blame**
- Go to symbol / class
- Completions (language keywords + workspace symbols)
- JaCoCo coverage widgets on Java tests
- Database viewer: SQLite, PostgreSQL, MySQL/MariaDB (optional SSL + SSH bastion)

## Git (in the same IDE)

- Import GitHub, GitLab, Bitbucket, Azure DevOps, or any HTTPS remote (PAT in Settings)
- Stage, commit, push, pull
- Interactive rebase, cherry-pick, merge abort
- Commit history and a git terminal (allowlisted commands)
- Optional: host a bare repo over HTTP on localhost

Data stays in `~/reaper` (override with `REAPER_DATA_DIR`). No account required.

## Run from source

```bash
cargo run
```

Open the URL printed at startup (for example `http://127.0.0.1:54321`). Reaper picks a free port and writes it to `~/reaper/reaper.port`.

**Requirements:** Rust 1.75+, Git 2.x on `PATH`.

### Environment variables

| Variable | Default | Description |
|----------|---------|-------------|
| `REAPER_HOST` | `127.0.0.1` | Bind address |
| `REAPER_PORT` | random | HTTP port (`0` or unset = random; pin e.g. `8765`) |
| `REAPER_DATA_DIR` | `~/reaper` | Repos, workspaces, metadata, settings |
| `REAPER_RUSTC` | `rustc` on PATH | rustc binary |
| `REAPER_CARGO` | `cargo` on PATH | cargo binary |
| `REAPER_PAT` | — | Default PAT for private HTTPS remotes |
| `REAPER_PAT_GITHUB_COM` | — | Host-specific PAT (dots → underscores, uppercased) |
| `REAPER_GIT_USERNAME` | `git` | Username for generic HTTPS git hosts |

### Clone a repo Reaper is hosting

After creating `my-app` in the UI:

```bash
git clone http://127.0.0.1:<port>/git/my-app.git
```

## Architecture

```
~/reaper/
  repos/        ← bare repos (hosted remotes)
  workspaces/   ← local clones the IDE edits
  metadata/     ← upstream remote info for imported repos
  settings.json ← PATs, compiler paths (local only)
```

The UI edits `workspaces/`, commits locally, and pushes to the bare repo. Imported remotes use your PAT.

## API

REST under `/api/repos`: repo CRUD, workspace file I/O, git status/diff/commit, PAT settings, remote import.

Git smart HTTP:

- `GET /git/{name}.git/info/refs?service=git-upload-pack`
- `POST /git/{name}.git/git-upload-pack`
- `POST /git/{name}.git/git-receive-pack`

## Releases

macOS DMGs (Apple Silicon and Intel): [reaper-org/releases](https://github.com/reaper-org/releases/releases).

Build locally:

- **Split (recommended):** `./scripts/build-macos-split-dmgs.sh`
- **This Mac:** `./scripts/build-macos-dmg.sh` or `./scripts/build-macos-intel-dmg.sh`
- **Universal:** `./scripts/build-macos-universal-dmg.sh`

Publish: `./scripts/release-macos-split.sh` or `./scripts/release-macos.sh`

Editor regression tests run on `cargo build` (skip with `REAPER_SKIP_EDITOR_TESTS=1`).

## License

MIT License. Copyright (c) 2026 Asha Somayajula. See [LICENSE](LICENSE).
