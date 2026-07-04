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
REPO="${REAPER_CAPTURE_REPO:-Spring-gradle-complicated}"
JAVA="${REAPER_CAPTURE_JAVA:-services/order-service/src/main/java/com/example/order/web/OrderController.java}"

if [[ ! -x "$CHROME" ]]; then
  echo "Google Chrome not found at $CHROME" >&2
  exit 1
fi

mkdir -p "$OUT"

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

enc() { python3 -c "import urllib.parse,sys; print(urllib.parse.quote(sys.argv[1], safe=''))" "$1"; }
JAVA_Q="$(enc "$JAVA")"

echo "Capturing feature screenshots (repo=$REPO)…"

capture "$BASE/?norepo=1&showcase=0" "welcome-home.png" 14000
capture "$BASE/?repo=$REPO&capture=file&path=$JAVA_Q" "editor-java.png" 45000
capture "$BASE/?repo=$REPO&capture=panel&panel=git" "git-commit.png" 35000
capture "$BASE/?repo=$REPO&capture=panel&panel=history" "git-history.png" 35000
capture "$BASE/?repo=$REPO&capture=panel&panel=terminal" "terminal.png" 30000
capture "$BASE/?repo=$REPO&capture=panel&panel=agent" "agent.png" 30000
capture "$BASE/?repo=$REPO&capture=feature&feature=build-tasks" "build-tasks.png" 50000
capture "$BASE/?repo=$REPO&capture=feature&feature=search&q=Order" "search.png" 35000
capture "$BASE/?repo=$REPO&capture=feature&feature=go-to-class&q=Order" "go-to-class.png" 35000

rm -f "$OUT"/_*.png
echo "Done — $(ls -1 "$OUT"/*.png | wc -l | tr -d ' ') screenshots in $OUT"
