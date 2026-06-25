# Reaper

A local **developer git studio** built in Rust. Host bare repositories over HTTP, edit files in a Monaco-powered IDE, import private company repos with a PAT, and run git commands — all through a Tailwind CSS UI.

## Features

- **Host git repos** — bare repositories with smart HTTP (`clone`, `fetch`, `push`)
- **Visual editor** — file tree, syntax-highlighted Monaco editor, tabs, diff panel
- **Private remotes** — import from GitHub, GitLab, Bitbucket, Azure DevOps, or any HTTPS host using a PAT
- **Source control** — staged/unstaged changes, commit & push from the UI
- **Git terminal** — run whitelisted git commands against the workspace
- **Commit history** — browse recent commits per repository

## Requirements

- Rust 1.75+
- Git 2.x installed and on `PATH`

## Run

```bash
cargo run
```

Open [http://127.0.0.1:8080](http://127.0.0.1:8080).

### Environment variables

| Variable | Default | Description |
|----------|---------|-------------|
| `REAPER_HOST` | `127.0.0.1` | Bind address |
| `REAPER_PORT` | `8080` | HTTP port |
| `REAPER_DATA_DIR` | `./data` | Root for repos, workspaces, and settings |
| `REAPER_PAT` | — | Default PAT for private HTTPS remotes |
| `REAPER_PAT_GITHUB_COM` | — | Host-specific PAT (dots → underscores, uppercased) |
| `REAPER_GIT_USERNAME` | `git` | Username for generic HTTPS git hosts |

## Clone a hosted repo

After creating a repo named `my-app` in the UI:

```bash
git clone http://127.0.0.1:8080/git/my-app.git
```

## Architecture

```
data/
  repos/        ← bare repos (hosted remotes)
  workspaces/   ← local clones for the visual editor
  metadata/     ← upstream remote info for imported repos
  settings.json ← PAT tokens per host (stored locally)
```

The UI edits files in `workspaces/`, commits locally, and pushes to the bare repo. Imported repos sync with private upstreams using your configured PAT.

## API

REST endpoints under `/api/repos` for repo CRUD, workspace file I/O, git status/diff/commit, PAT settings, and remote import.

Git smart HTTP:

- `GET /git/{name}.git/info/refs?service=git-upload-pack`
- `POST /git/{name}.git/git-upload-pack`
- `POST /git/{name}.git/git-receive-pack`
