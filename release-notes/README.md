# Release notes

macOS release notes for [Reaper](https://github.com/reaper-org/reaper). Published DMGs and checksums live in [reaper-org/releases](https://github.com/reaper-org/releases).

| Version | Notes |
|---------|--------|
| [0.1.22](v0.1.22.md) | Smooth editing + notarized macOS bundle (505) |
| [0.1.21](v0.1.21.md) | Window open + import performance (504) |
| [0.1.20](v0.1.20.md) | Cursor agent status UI (503) |
| [0.1.19](v0.1.19.md) | Cursor agent chat reliability (502) |
| [0.1.18](v0.1.18.md) | Cursor bridge hotfix (501) |

Older versions: `v0.1.*.md` in this directory.

When cutting a release, add `release-notes/vX.Y.Z.md` and run `./scripts/release-macos.sh` (auto-discovery for notes).

## macOS signing (Developer ID)

Release builds use **Developer ID Application** when available:

1. Copy `packaging/macos/signing.env.example` → `packaging/macos/signing.env` (gitignored), or rely on Keychain auto-detect.
2. `./scripts/release-macos.sh` — builds, runs **`sign-macos-app.sh`** (warms bundled **jdtls** OSGi config, signs nested Mach-O, signs app + DMG) on every cut.
3. Optional Gatekeeper notarization: `REAPER_NOTARIZE=1` and `REAPER_NOTARY_KEYCHAIN_PROFILE` in `signing.env` after `xcrun notarytool store-credentials`.

**Every release (repeatable):**

- `copy-bundled-jdtls.sh` copies plugins/bin/config templates only (drops stale `configuration/`).
- `sign-macos-app.sh` always runs `warm-bundled-jdtls-config.sh` before codesign so Java LSP works and notarization passes.
- Set `REAPER_MACOS_ARCH=x86_64` when building the Intel DMG on Apple Silicon.

The download site ([reaper-web](https://github.com/somayaj/reaper-web)) points at GitHub release assets; upload DMGs with `release-macos.sh` per architecture (`REAPER_MACOS_ARCH=arm64` / `x86_64`).
