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
ARCH="$(uname -m)"
TAG="v${VERSION}"
DMG="$ROOT/dist/Reaper-${VERSION}-macos-${ARCH}.dmg"
DMG_NAME="$(basename "$DMG")"
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
  "$ROOT/scripts/build-macos-dmg.sh"
fi

if [[ -n "${REAPER_RELEASES_DIR:-}" && -d "$REAPER_RELEASES_DIR/.git" ]]; then
  ARTIFACT_DIR="$REAPER_RELEASES_DIR/macos/${ARCH}/v${VERSION}"
  mkdir -p "$ARTIFACT_DIR"
  cp "$DMG" "$ARTIFACT_DIR/$DMG_NAME"
  (cd "$ARTIFACT_DIR" && shasum -a 256 "$DMG_NAME" > SHA256SUMS)
  echo "Synced DMG to ${ARTIFACT_DIR}"

  if [[ -z "${REAPER_RELEASE_NOTES:-}" && -f "$ARTIFACT_DIR/RELEASE_NOTES.md" ]]; then
    REAPER_RELEASE_NOTES="$ARTIFACT_DIR/RELEASE_NOTES.md"
  fi

  if ! git -C "$REAPER_RELEASES_DIR" diff --quiet -- "$ARTIFACT_DIR" \
    || [[ -n "$(git -C "$REAPER_RELEASES_DIR" status --porcelain -- "$ARTIFACT_DIR")" ]]; then
    git -C "$REAPER_RELEASES_DIR" add "$ARTIFACT_DIR/$DMG_NAME" "$ARTIFACT_DIR/SHA256SUMS"
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
else
  NOTES_FILE="$(mktemp)"
  cat >"$NOTES_FILE" <<EOF
macOS ${ARCH} build.

**Install:** download the **DMG** below. Ignore GitHub's automatic "Source code (zip)" and "Source code (tar.gz)" links — those archives are empty placeholders (see \`.gitattributes\` export-ignore) and are not distributable builds.

Drag Reaper.app to Applications, then launch.

**Tip:** opening the .dmg repeatedly mounts a new Finder volume each time — eject old Reaper drives or run scripts/eject-reaper-dmgs.sh.
EOF
fi

SUMS_FILE="$(mktemp)"
if [[ -n "${REAPER_RELEASES_DIR:-}" && -f "$REAPER_RELEASES_DIR/macos/${ARCH}/v${VERSION}/SHA256SUMS" ]]; then
  cp "$REAPER_RELEASES_DIR/macos/${ARCH}/v${VERSION}/SHA256SUMS" "$SUMS_FILE"
else
  (cd "$(dirname "$DMG")" && shasum -a 256 "$DMG_NAME" > "$SUMS_FILE")
fi

if gh release view "$TAG" --repo "$GH_REPO" >/dev/null 2>&1; then
  echo "Uploading assets to existing release ${TAG} on ${GH_REPO}…"
  gh release upload "$TAG" "$DMG" "$SUMS_FILE" --repo "$GH_REPO" --clobber
  gh release edit "$TAG" --repo "$GH_REPO" --title "$TITLE" --notes-file "$NOTES_FILE"
else
  echo "Creating release ${TAG} on ${GH_REPO}…"
  gh release create "$TAG" "$DMG" "$SUMS_FILE" --repo "$GH_REPO" --title "$TITLE" --notes-file "$NOTES_FILE"
fi

rm -f "$SUMS_FILE"
if [[ -z "${REAPER_RELEASE_NOTES:-}" ]]; then
  rm -f "$NOTES_FILE"
fi

echo ""
echo "Published: $(gh release view "$TAG" --repo "$GH_REPO" --json url -q .url)"
echo "  Asset: ${DMG_NAME}"
echo "  SHA256: $(shasum -a 256 "$DMG" | awk '{print $1}')"
