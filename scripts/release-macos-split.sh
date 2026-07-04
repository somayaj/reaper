#!/usr/bin/env bash
# Build and publish split macOS releases (arm64 + x86_64 DMGs on one GitHub release).
#
# Usage:
#   ./scripts/release-macos-split.sh
#   REAPER_SKIP_BUILD=1 ./scripts/release-macos-split.sh
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -1)"
TAG="v${VERSION}"
GH_REPO="${REAPER_GH_REPO:-reaper-org/releases}"
ARM_DMG="$ROOT/dist/reaper-${VERSION}-macos-arm64.dmg"
INTEL_DMG="$ROOT/dist/reaper-${VERSION}-macos-x86_64.dmg"
ARM_NAME="$(basename "$ARM_DMG")"
INTEL_NAME="$(basename "$INTEL_DMG")"
TITLE="Reaper ${VERSION} (macOS)"

if [[ -z "$VERSION" ]]; then
  echo "Could not read version from Cargo.toml" >&2
  exit 1
fi

if ! command -v gh >/dev/null 2>&1; then
  echo "gh CLI is required (https://cli.github.com/)" >&2
  exit 1
fi

if [[ "${REAPER_SKIP_BUILD:-}" != "1" ]]; then
  "$ROOT/scripts/build-macos-split-dmgs.sh"
fi

for dmg in "$ARM_DMG" "$INTEL_DMG"; do
  if [[ ! -f "$dmg" ]]; then
    echo "DMG not found: $dmg" >&2
    exit 1
  fi
done

NOTES_TMP_DIR="$(mktemp -d)"
NOTES_FILE="$NOTES_TMP_DIR/RELEASE_NOTES.md"
if [[ -n "${REAPER_RELEASE_NOTES:-}" && -f "$REAPER_RELEASE_NOTES" ]]; then
  cp "$REAPER_RELEASE_NOTES" "$NOTES_FILE"
else
  cat >"$NOTES_FILE" <<EOF
Reaper ${VERSION} — macOS split release.

**Install:** download the **DMG for your Mac** below. Ignore GitHub's automatic "Source code (zip)" and "Source code (tar.gz)" links — those archives are empty placeholders and are not distributable builds.

| Mac | Download |
|-----|----------|
| Apple Silicon (M1/M2/M3/M4) | \`${ARM_NAME}\` |
| Intel (2015–2020 MacBook Pro, iMac, etc.) | \`${INTEL_NAME}\` |

Drag Reaper.app to Applications, then launch.

**Tip:** opening the .dmg repeatedly mounts a new Finder volume each time — eject old Reaper drives or run scripts/eject-reaper-dmgs.sh.
EOF
fi

ARM_SHA="$(shasum -a 256 "$ARM_DMG" | awk '{print $1}')"
INTEL_SHA="$(shasum -a 256 "$INTEL_DMG" | awk '{print $1}')"
{
  echo ""
  echo "SHA256 (arm64): \`${ARM_SHA}\`"
  echo ""
  echo "SHA256 (x86_64): \`${INTEL_SHA}\`"
} >>"$NOTES_FILE"

cleanup_stray_release_assets() {
  while IFS= read -r asset_name; do
    [[ -z "$asset_name" ]] && continue
    case "$asset_name" in
      reaper-*-macos-arm64.dmg|reaper-*-macos-x86_64.dmg) continue ;;
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
  echo "Uploading split DMGs to existing release ${TAG}…"
  cleanup_stray_release_assets
  remove_release_asset_if_present "$ARM_NAME"
  remove_release_asset_if_present "$INTEL_NAME"
  gh release upload "$TAG" "$ARM_DMG" "$INTEL_DMG" --repo "$GH_REPO" --clobber
  gh release edit "$TAG" --repo "$GH_REPO" --title "$TITLE" --notes-file "$NOTES_FILE"
  cleanup_stray_release_assets
else
  echo "Creating release ${TAG} with split DMGs…"
  gh release create "$TAG" "$ARM_DMG" "$INTEL_DMG" --repo "$GH_REPO" --title "$TITLE" --notes-file "$NOTES_FILE"
  cleanup_stray_release_assets
fi

rm -rf "$NOTES_TMP_DIR"

echo ""
echo "Published: $(gh release view "$TAG" --repo "$GH_REPO" --json url -q .url)"
echo "  ${ARM_NAME}  ($(du -h "$ARM_DMG" | awk '{print $1}'))"
echo "  ${INTEL_NAME}  ($(du -h "$INTEL_DMG" | awk '{print $1}'))"
