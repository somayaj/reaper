#!/usr/bin/env bash
# Copy static/launch-logo.svg into index.html (between LAUNCH-LOGO markers).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
LOGO="$ROOT/static/launch-logo.svg"
TARGET="$ROOT/static/index.html"

python3 - "$LOGO" "$TARGET" <<'PY'
import pathlib
import sys

logo_path, html_path = map(pathlib.Path, sys.argv[1:3])
logo = logo_path.read_text(encoding="utf-8").strip()
html = html_path.read_text(encoding="utf-8")
start = "<!-- LAUNCH-LOGO:START -->"
end = "<!-- LAUNCH-LOGO:END -->"
if start not in html or end not in html:
    sys.exit("sync-launch-splash: logo markers missing in index.html")
before, rest = html.split(start, 1)
_, after = rest.split(end, 1)
html = before + start + "\n      <div class=\"ij-launch-logo-wrap\">\n" + logo + "\n      </div>\n      " + end + after
html_path.write_text(html, encoding="utf-8")
print(f"sync-launch-splash: updated {html_path}")
PY
