#!/usr/bin/env bash
# Full pre-build check: editor regression + Rust tests.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

echo "== Editor regression =="
"$ROOT/scripts/test-editor-regression.sh"

echo ""
echo "== Rust tests =="
cargo test --manifest-path "$ROOT/Cargo.toml"

echo ""
echo "All checks passed."
