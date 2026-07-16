#!/usr/bin/env bash
# Build and publish macOS DMGs + Windows exe on one GitHub release (release push).
#
# Usage:
#   ./scripts/release-macos-and-windows.sh
#   REAPER_SKIP_BUILD=1 ./scripts/release-macos-and-windows.sh
#   REAPER_SKIP_MACOS=1 ./scripts/release-macos-and-windows.sh   # Windows only
#   REAPER_SKIP_WINDOWS=1 ./scripts/release-macos-and-windows.sh # macOS only
#
# Env:
#   REAPER_GH_REPO          default reaper-org/releases
#   REAPER_RELEASE_NOTES    optional notes file (SHA256 lines still appended)
#   REAPER_SKIP_BUILD=1     skip compile; upload existing dist/ artifacts
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -1)"
TAG="v${VERSION}"
GH_REPO="${REAPER_GH_REPO:-reaper-org/releases}"

ARM_DMG="$ROOT/dist/reaper-${VERSION}-macos-arm64.dmg"
INTEL_DMG="$ROOT/dist/reaper-${VERSION}-macos-x86_64.dmg"
WIN_EXE="$ROOT/dist/reaper-${VERSION}-windows-x64.exe"

ARM_NAME="$(basename "$ARM_DMG")"
INTEL_NAME="$(basename "$INTEL_DMG")"
WIN_NAME="$(basename "$WIN_EXE")"
TITLE="Reaper ${VERSION} (macOS + Windows)"

SKIP_MACOS="${REAPER_SKIP_MACOS:-0}"
SKIP_WINDOWS="${REAPER_SKIP_WINDOWS:-0}"

if [[ -z "$VERSION" ]]; then
  echo "Could not read version from Cargo.toml" >&2
  exit 1
fi

if ! command -v gh >/dev/null 2>&1; then
  echo "gh CLI is required (https://cli.github.com/)" >&2
  exit 1
fi

mkdir -p "$ROOT/dist"

if [[ "${REAPER_SKIP_BUILD:-}" != "1" ]]; then
  if [[ "$SKIP_MACOS" != "1" ]]; then
    echo "== Building macOS split DMGs =="
    "$ROOT/scripts/build-macos-split-dmgs.sh"
  fi
  if [[ "$SKIP_WINDOWS" != "1" ]]; then
    echo "== Building Windows exe =="
    "$ROOT/scripts/build-windows-exe.sh"
  fi
fi

UPLOAD_ASSETS=()
if [[ "$SKIP_MACOS" != "1" ]]; then
  for dmg in "$ARM_DMG" "$INTEL_DMG"; do
    if [[ ! -f "$dmg" ]]; then
      echo "DMG not found: $dmg" >&2
      exit 1
    fi
    UPLOAD_ASSETS+=("$dmg")
  done
fi
if [[ "$SKIP_WINDOWS" != "1" ]]; then
  if [[ ! -f "$WIN_EXE" ]]; then
    echo "Windows exe not found: $WIN_EXE" >&2
    exit 1
  fi
  UPLOAD_ASSETS+=("$WIN_EXE")
fi

if [[ ${#UPLOAD_ASSETS[@]} -eq 0 ]]; then
  echo "Nothing to upload (both macOS and Windows skipped?)" >&2
  exit 1
fi

NOTES_TMP_DIR="$(mktemp -d)"
NOTES_FILE="$NOTES_TMP_DIR/RELEASE_NOTES.md"
if [[ -n "${REAPER_RELEASE_NOTES:-}" && -f "$REAPER_RELEASE_NOTES" ]]; then
  cp "$REAPER_RELEASE_NOTES" "$NOTES_FILE"
else
  cat >"$NOTES_FILE" <<EOF
Reaper ${VERSION} — macOS + Windows release.

**Install:** download the asset for your platform below. Ignore GitHub's automatic "Source code (zip)" and "Source code (tar.gz)" links — those archives are empty placeholders and are not distributable builds.

| Platform | Download |
|----------|----------|
| Apple Silicon (M1/M2/M3/M4) | \`${ARM_NAME}\` |
| Intel Mac (2015–2020) | \`${INTEL_NAME}\` |
| Windows x64 | \`${WIN_NAME}\` |

**macOS:** open the DMG, drag Reaper.app to Applications, then launch.

**Windows:** run \`${WIN_NAME}\` (or \`reaper.exe --server\`). The IDE UI opens in your browser at the printed local URL. Native Windows desktop GUI is not included in this build yet.

**Tip (macOS):** opening the .dmg repeatedly mounts a new Finder volume each time — eject old Reaper drives or run scripts/eject-reaper-dmgs.sh.
EOF
fi

{
  echo ""
  if [[ -f "$ARM_DMG" ]]; then
    echo "SHA256 (macos-arm64): \`$(shasum -a 256 "$ARM_DMG" | awk '{print $1}')\`"
    echo ""
  fi
  if [[ -f "$INTEL_DMG" ]]; then
    echo "SHA256 (macos-x86_64): \`$(shasum -a 256 "$INTEL_DMG" | awk '{print $1}')\`"
    echo ""
  fi
  if [[ -f "$WIN_EXE" ]]; then
    echo "SHA256 (windows-x64): \`$(shasum -a 256 "$WIN_EXE" | awk '{print $1}')\`"
    echo ""
  fi
} >>"$NOTES_FILE"

cleanup_stray_release_assets() {
  while IFS= read -r asset_name; do
    [[ -z "$asset_name" ]] && continue
    case "$asset_name" in
      reaper-*-macos-arm64.dmg|reaper-*-macos-x86_64.dmg|reaper-*-windows-x64.exe) continue ;;
    esac
    echo "Removing stray release asset: ${asset_name}"
    gh release delete-asset "$TAG" "$asset_name" --repo "$GH_REPO" --yes
  done < <(gh release view "$TAG" --repo "$GH_REPO" --json assets -q '.assets[].name' 2>/dev/null || true)
}

remove_release_asset_if_present() {
  local name="$1"
  if gh release view "$TAG" --repo "$GH_REPO" --json assets -q '.assets[].name' 2>/dev/null | grep -Fxq "$name"; then
    echo "Removing existing asset for replacement: ${name}"
    gh release delete-asset "$TAG" "$name" --repo "$GH_REPO" --yes
  fi
}

if gh release view "$TAG" --repo "$GH_REPO" >/dev/null 2>&1; then
  echo "Uploading assets to existing release ${TAG}…"
  cleanup_stray_release_assets
  for asset in "${UPLOAD_ASSETS[@]}"; do
    remove_release_asset_if_present "$(basename "$asset")"
  done
  gh release upload "$TAG" "${UPLOAD_ASSETS[@]}" --repo "$GH_REPO" --clobber
  gh release edit "$TAG" --repo "$GH_REPO" --title "$TITLE" --notes-file "$NOTES_FILE"
  cleanup_stray_release_assets
else
  echo "Creating release ${TAG}…"
  gh release create "$TAG" "${UPLOAD_ASSETS[@]}" --repo "$GH_REPO" --title "$TITLE" --notes-file "$NOTES_FILE"
  cleanup_stray_release_assets
fi

rm -rf "$NOTES_TMP_DIR"

echo ""
echo "Published: $(gh release view "$TAG" --repo "$GH_REPO" --json url -q .url)"
for asset in "${UPLOAD_ASSETS[@]}"; do
  echo "  $(basename "$asset")  ($(du -h "$asset" | awk '{print $1}'))"
done
