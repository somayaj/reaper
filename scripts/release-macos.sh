#!/usr/bin/env bash
# Build Reaper.app + DMG and publish to reaper-org/releases (DMG only).
#
# GitHub always shows automatic "Source code (zip)" and "Source code (tar.gz)"
# links on every release; those cannot be removed. The releases repo uses
# .gitattributes export-ignore so those archives are empty. Release notes warn
# users to download the DMG instead.
#
# Usage:
#   ./scripts/release-macos.sh              # build + publish
#   REAPER_SKIP_BUILD=1 ./scripts/release-macos.sh   # upload existing DMG
#   REAPER_RELEASE_NOTES=notes.md ./scripts/release-macos.sh
#   REAPER_RELEASES_DIR=/path/to/releases/checkout ./scripts/release-macos.sh
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -1)"
ARCH="${REAPER_MACOS_ARCH:-$(uname -m)}"
TAG="v${VERSION}"
DMG="$ROOT/dist/reaper-${VERSION}-macos-${ARCH}.dmg"
DMG_NAME="$(basename "$DMG")"
LEGACY_DMG_NAME="Reaper-${VERSION}-macos-${ARCH}.dmg"
LEGACY_DMG_LC="$(printf '%s' "$LEGACY_DMG_NAME" | tr '[:upper:]' '[:lower:]')"
DMG_NAME_LC="$(printf '%s' "$DMG_NAME" | tr '[:upper:]' '[:lower:]')"
TITLE="Reaper ${VERSION} (macOS ${ARCH})"
GH_REPO="${REAPER_GH_REPO:-reaper-org/releases}"

if [[ -z "$VERSION" ]]; then
  echo "Could not read version from Cargo.toml" >&2
  exit 1
fi

if ! command -v gh >/dev/null 2>&1; then
  echo "gh CLI is required (https://cli.github.com/)" >&2
  exit 1
fi

if [[ -z "${REAPER_RELEASES_DIR:-}" ]]; then
  for candidate in \
    "$ROOT/../releases/releases" \
    "$HOME/dev/reaper_org/releases/releases"; do
    if [[ -d "$candidate/.git" ]]; then
      REAPER_RELEASES_DIR="$candidate"
      break
    fi
  done
fi

if [[ "${REAPER_SKIP_BUILD:-}" == "1" ]]; then
  if [[ ! -f "$DMG" ]]; then
    echo "DMG not found at $DMG (run without REAPER_SKIP_BUILD=1 first)" >&2
    exit 1
  fi
else
  if [[ "$ARCH" == "x86_64" ]]; then
    "$ROOT/scripts/build-macos-intel-dmg.sh"
  else
    "$ROOT/scripts/build-macos-dmg.sh"
  fi
fi

if [[ -n "${REAPER_RELEASES_DIR:-}" && -d "$REAPER_RELEASES_DIR/.git" ]]; then
  ARTIFACT_DIR="$REAPER_RELEASES_DIR/macos/${ARCH}/v${VERSION}"
  mkdir -p "$ARTIFACT_DIR"
  cp "$DMG" "$ARTIFACT_DIR/$DMG_NAME"
  if [[ "$LEGACY_DMG_LC" != "$DMG_NAME_LC" && -f "$ARTIFACT_DIR/$LEGACY_DMG_NAME" ]]; then
    rm -f "$ARTIFACT_DIR/$LEGACY_DMG_NAME"
  fi
  (cd "$ARTIFACT_DIR" && shasum -a 256 "$DMG_NAME" > SHA256SUMS)
  echo "Synced DMG to ${ARTIFACT_DIR}"

  if [[ -z "${REAPER_RELEASE_NOTES:-}" && -f "$ARTIFACT_DIR/RELEASE_NOTES.md" ]]; then
    REAPER_RELEASE_NOTES="$ARTIFACT_DIR/RELEASE_NOTES.md"
  fi

  if ! git -C "$REAPER_RELEASES_DIR" diff --quiet -- "$ARTIFACT_DIR" \
    || [[ -n "$(git -C "$REAPER_RELEASES_DIR" status --porcelain -- "$ARTIFACT_DIR")" ]]; then
    git -C "$REAPER_RELEASES_DIR" add "$ARTIFACT_DIR/$DMG_NAME" "$ARTIFACT_DIR/SHA256SUMS"
    if [[ "$LEGACY_DMG_LC" != "$DMG_NAME_LC" ]] \
      && git -C "$REAPER_RELEASES_DIR" ls-files --error-unmatch "$ARTIFACT_DIR/$LEGACY_DMG_NAME" >/dev/null 2>&1; then
      git -C "$REAPER_RELEASES_DIR" rm -f "$ARTIFACT_DIR/$LEGACY_DMG_NAME"
    fi
    if [[ -f "$ARTIFACT_DIR/RELEASE_NOTES.md" ]]; then
      git -C "$REAPER_RELEASES_DIR" add "$ARTIFACT_DIR/RELEASE_NOTES.md"
    fi
    git -C "$REAPER_RELEASES_DIR" commit -m "$(cat <<EOF
chore: update macOS ${ARCH} ${VERSION} DMG

Refresh the ${VERSION} ${ARCH} build from release-macos.sh.
EOF
)"
    git -C "$REAPER_RELEASES_DIR" push origin main
    echo "Pushed releases repo commit."
  else
    echo "Releases repo artifacts unchanged; skipping git commit."
  fi
else
  echo "Warning: releases git checkout not found (set REAPER_RELEASES_DIR)." >&2
fi

if [[ -n "${REAPER_RELEASE_NOTES:-}" ]]; then
  NOTES_FILE="$REAPER_RELEASE_NOTES"
  if [[ ! -f "$NOTES_FILE" ]]; then
    echo "Release notes file not found: $NOTES_FILE" >&2
    exit 1
  fi
  NOTES_TMP=0
else
  NOTES_TMP_DIR="$(mktemp -d)"
  NOTES_FILE="$NOTES_TMP_DIR/RELEASE_NOTES.md"
  NOTES_TMP=1
  cat >"$NOTES_FILE" <<EOF
macOS ${ARCH} build.

**Install:** download the **DMG** below. Ignore GitHub's automatic "Source code (zip)" and "Source code (tar.gz)" links — those archives are empty placeholders (see \`.gitattributes\` export-ignore) and are not distributable builds.

Drag Reaper.app to Applications, then launch.

**Tip:** opening the .dmg repeatedly mounts a new Finder volume each time — eject old Reaper drives or run scripts/eject-reaper-dmgs.sh.
EOF
fi

SUMS_TMP_DIR="$(mktemp -d)"
SUMS_FILE="$SUMS_TMP_DIR/SHA256SUMS"
if [[ -n "${REAPER_RELEASES_DIR:-}" && -f "$REAPER_RELEASES_DIR/macos/${ARCH}/v${VERSION}/SHA256SUMS" ]]; then
  cp "$REAPER_RELEASES_DIR/macos/${ARCH}/v${VERSION}/SHA256SUMS" "$SUMS_FILE"
else
  (cd "$(dirname "$DMG")" && shasum -a 256 "$DMG_NAME" > "$SUMS_FILE")
fi

DMG_SHA256="$(awk '{print $1}' "$SUMS_FILE")"

append_checksum_to_notes() {
  local notes="$1"
  if grep -q '^SHA256:' "$notes"; then
    return 0
  fi
  printf '\n\nSHA256: `%s`\n' "$DMG_SHA256" >>"$notes"
}

if [[ "${NOTES_TMP:-0}" == "1" ]]; then
  append_checksum_to_notes "$NOTES_FILE"
elif [[ -n "${REAPER_RELEASE_NOTES:-}" ]]; then
  NOTES_WITH_SHA="$(mktemp -d)"
  cp "$NOTES_FILE" "$NOTES_WITH_SHA/RELEASE_NOTES.md"
  append_checksum_to_notes "$NOTES_WITH_SHA/RELEASE_NOTES.md"
  NOTES_FILE="$NOTES_WITH_SHA/RELEASE_NOTES.md"
  NOTES_TMP=1
  NOTES_TMP_DIR="$NOTES_WITH_SHA"
fi

cleanup_stray_release_assets() {
  while IFS= read -r asset_name; do
    [[ -z "$asset_name" ]] && continue
    case "$asset_name" in
      reaper-*-macos-arm64.dmg|reaper-*-macos-x86_64.dmg) continue ;;
    esac
    echo "Removing stray release asset: ${asset_name}"
    gh release delete-asset "$TAG" "$asset_name" --repo "$GH_REPO" --yes
  done < <(gh release view "$TAG" --repo "$GH_REPO" --json assets -q '.assets[].name')
}

remove_release_asset_if_present() {
  local name="$1"
  if gh release view "$TAG" --repo "$GH_REPO" --json assets -q '.assets[].name' | grep -Fxq "$name"; then
    echo "Removing existing asset for replacement: ${name}"
    gh release delete-asset "$TAG" "$name" --repo "$GH_REPO" --yes
  fi
}

upload_release_dmg() {
  cleanup_stray_release_assets
  remove_release_asset_if_present "$DMG_NAME"
  gh release upload "$TAG" "$DMG" --repo "$GH_REPO" --clobber
  cleanup_stray_release_assets
}

if gh release view "$TAG" --repo "$GH_REPO" >/dev/null 2>&1; then
  echo "Uploading DMG to existing release ${TAG} on ${GH_REPO}…"
  upload_release_dmg
  gh release edit "$TAG" --repo "$GH_REPO" --title "$TITLE" --notes-file "$NOTES_FILE"
else
  echo "Creating release ${TAG} on ${GH_REPO}…"
  gh release create "$TAG" "$DMG" --repo "$GH_REPO" --title "$TITLE" --notes-file "$NOTES_FILE"
  cleanup_stray_release_assets
fi

rm -rf "$SUMS_TMP_DIR"
if [[ "${NOTES_TMP:-0}" == "1" ]]; then
  rm -rf "$NOTES_TMP_DIR"
fi

echo ""
echo "Published: $(gh release view "$TAG" --repo "$GH_REPO" --json url -q .url)"
echo "  Asset: ${DMG_NAME}"
echo "  SHA256: ${DMG_SHA256}"
