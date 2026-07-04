#!/usr/bin/env bash
# Capture Reaper UI screenshots for the welcome home page.
# Requires Reaper running locally and Google Chrome.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="$ROOT/static/screenshots"
CHROME="/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
PORT="${REAPER_PORT:-$(cat "$HOME/reaper/reaper.port" 2>/dev/null || true)}"
PORT="${PORT:-63319}"
BASE="http://127.0.0.1:${PORT}"

if [[ ! -x "$CHROME" ]]; then
  echo "Google Chrome not found at $CHROME" >&2
  exit 1
fi

mkdir -p "$OUT"

# Sync static assets into the running app bundle when present.
BUNDLE_STATIC="$ROOT/dist/Reaper.app/Contents/Resources/static"
if [[ -d "$BUNDLE_STATIC" ]]; then
  rsync -a "$ROOT/static/" "$BUNDLE_STATIC/"
fi

capture() {
  local url="$1"
  local out="$2"
  local budget="${3:-15000}"
  "$CHROME" \
    --headless=new \
    --disable-gpu \
    --hide-scrollbars \
    --window-size=1440,920 \
    --virtual-time-budget="$budget" \
    --run-all-compositor-stages-before-draw \
    --screenshot="$OUT/$out" \
    "$url" \
    2>/dev/null
  echo "wrote $OUT/$out"
}

curl -sf "$BASE/" >/dev/null || {
  echo "Reaper not reachable at $BASE — start the app first." >&2
  exit 1
}

capture "$BASE/?norepo=1&showcase=0" "welcome-home.png" 12000
capture "$BASE/?repo=Spring-gradle-complicated" "editor-java.png" 25000
capture "$BASE/?repo=Spring-gradle-complicated&panel=git" "git-commit.png" 45000

rm -f "$OUT"/_*.png
