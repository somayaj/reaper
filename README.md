# Reaper

A local **developer git studio** built in Rust. Host bare repositories over HTTP, edit files in a Monaco-powered IDE, import private company repos with a PAT, and run git commands — all through a Tailwind CSS UI.

## Features

- **Host git repos** — bare repositories with smart HTTP (`clone`, `fetch`, `push`)
- **Visual editor** — file tree, syntax-highlighted Monaco editor, tabs, diff panel
- **Private remotes** — import from GitHub, GitLab, Bitbucket, Azure DevOps, or any HTTPS host using a PAT
- **Source control** — staged/unstaged changes, commit & push from the UI
- **Git terminal** — run whitelisted git commands against the workspace
- **Commit history** — browse recent commits per repository
- **Build & run** — Gradle, Maven, Spring Boot, and native C/C++ (CMake) from toolbar or gutter
- **C/C++ & languages** — Monaco editor with clangd navigation, 25+ languages, editor regression suite
- **Package manifest** — dockable panel for Cargo, npm, Ruby, Go, and CMake dependencies
- **Database viewer** — schema browser for project databases
- **Test coverage** — JaCoCo widgets on Java test files

## Requirements

- Rust 1.75+
- Git 2.x installed and on `PATH`

## Run

```bash
cargo run
```

Open the URL printed at startup (for example `http://127.0.0.1:54321`). Reaper picks a random available port each run so it does not collide with other local servers. The chosen port is also written to `~/reaper/reaper.port`.

### Environment variables

| Variable | Default | Description |
|----------|---------|-------------|
| `REAPER_HOST` | `127.0.0.1` | Bind address |
| `REAPER_PORT` | random | HTTP port (`0` or unset = random available port; set to pin e.g. `8765`) |
| `REAPER_DATA_DIR` | `~/reaper` | Root for repos, workspaces, metadata, and settings |
| `REAPER_PAT` | — | Default PAT for private HTTPS remotes |
| `REAPER_PAT_GITHUB_COM` | — | Host-specific PAT (dots → underscores, uppercased) |
| `REAPER_GIT_USERNAME` | `git` | Username for generic HTTPS git hosts |

## Clone a hosted repo

After creating a repo named `my-app` in the UI (check the startup log or `~/reaper/reaper.port` for the port):

```bash
git clone http://127.0.0.1:<port>/git/my-app.git
```

## Architecture

```
~/reaper/
  repos/        ← bare repos (hosted remotes)
  workspaces/   ← local clones for the visual editor
  metadata/     ← upstream remote info for imported repos
  settings.json ← PAT tokens per host (stored locally)
```

Override with `REAPER_DATA_DIR`.

The UI edits files in `workspaces/`, commits locally, and pushes to the bare repo. Imported repos sync with private upstreams using your configured PAT.

## API

REST endpoints under `/api/repos` for repo CRUD, workspace file I/O, git status/diff/commit, PAT settings, and remote import.

Git smart HTTP:

- `GET /git/{name}.git/info/refs?service=git-upload-pack`
- `POST /git/{name}.git/git-upload-pack`
- `POST /git/{name}.git/git-receive-pack`

## Releases

macOS DMGs (Apple Silicon and Intel) are published at [reaper-org/releases](https://github.com/reaper-org/releases/releases). Build locally:

- **Split release (recommended):** `./scripts/build-macos-split-dmgs.sh` — arm64 + x86_64 DMGs (~half the size each vs universal)
- **This Mac only:** `./scripts/build-macos-dmg.sh` (arm64 on Apple Silicon) or `./scripts/build-macos-intel-dmg.sh` (x86_64)
- **Universal (both archs in one app):** `./scripts/build-macos-universal-dmg.sh`

Publish: `./scripts/release-macos-split.sh` or per-arch `./scripts/release-macos.sh`

Editor regression tests run automatically on `cargo build` (skip with `REAPER_SKIP_EDITOR_TESTS=1`).

## License

MIT License. Copyright (c) 2026 Asha Somayajula. See [LICENSE](LICENSE).
