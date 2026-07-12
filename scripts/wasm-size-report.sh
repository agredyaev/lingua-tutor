#!/usr/bin/env sh
# Build web release + report compressed WASM size vs 3 MB budget (09-frontend.md §9).
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT/apps/web"
trunk build --release --cargo-profile wasm-release >/dev/null

WASM="$(find dist -name '*.wasm' | head -1)"
RAW=$(wc -c <"$WASM" | tr -d ' ')
GZ=$(gzip -9 -c "$WASM" | wc -c | tr -d ' ')
BUDGET=$((3 * 1024 * 1024))

echo "wasm raw:  $RAW bytes"
echo "wasm gzip: $GZ bytes (budget $BUDGET)"
[ "$GZ" -le "$BUDGET" ] || { echo "FAIL: over 3 MB compressed budget" >&2; exit 1; }
echo "size OK"
