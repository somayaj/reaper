#!/usr/bin/env bash
# Sync static/reaper-logo.svg into logo.js (REAPER-LOGO-SVG markers).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SVG="$ROOT/static/reaper-logo.svg"
TARGET="$ROOT/static/logo.js"

python3 - "$SVG" "$TARGET" <<'PY'
import pathlib
import sys

svg_path, js_path = map(pathlib.Path, sys.argv[1:3])
svg = svg_path.read_text(encoding="utf-8").strip()
js = js_path.read_text(encoding="utf-8")
start = "  // REAPER-LOGO-SVG:START"
end = "  // REAPER-LOGO-SVG:END"
if start not in js or end not in js:
    sys.exit("sync-reaper-logo: markers missing in logo.js")
before, rest = js.split(start, 1)
_, after = rest.split(end, 1)
escaped = svg.replace("\\", "\\\\").replace("`", "\\`")
js = before + start + "\n  const SVG = `" + escaped + "`;\n  " + end + after
js_path.write_text(js, encoding="utf-8")
print(f"sync-reaper-logo: updated {js_path}")
PY
