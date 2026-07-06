#!/usr/bin/env bash
# Keep launch splash logo wrap in index.html (logo injected from logo.js at runtime).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TARGET="$ROOT/static/index.html"

python3 - "$TARGET" <<'PY'
import pathlib
import sys

html_path = pathlib.Path(sys.argv[1])
html = html_path.read_text(encoding="utf-8")
start = "<!-- LAUNCH-LOGO:START -->"
end = "<!-- LAUNCH-LOGO:END -->"
if start not in html or end not in html:
    sys.exit("sync-launch-splash: logo markers missing in index.html")
before, rest = html.split(start, 1)
_, after = rest.split(end, 1)
html = before + start + "\n      <div class=\"ij-launch-logo-wrap\"></div>\n      " + end + after
html_path.write_text(html, encoding="utf-8")
print(f"sync-launch-splash: updated {html_path}")
PY
