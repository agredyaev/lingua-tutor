#!/usr/bin/env sh
# Local smoke: start wrangler dev, assert /api/v1/health, stop.
# ADR-0003: local-first — no Cloudflare account until WBS 10.
set -eu

export PATH="/opt/homebrew/bin:$PATH" # node >= 22 for wrangler (README)
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PORT="${SMOKE_PORT:-8787}"

cd "$ROOT/apps/worker"
npx wrangler dev --local --port "$PORT" >"$ROOT/target/wrangler-dev.log" 2>&1 &
WRANGLER_PID=$!
trap 'kill "$WRANGLER_PID" 2>/dev/null || true' EXIT

i=0
until curl -sf "http://localhost:$PORT/api/v1/health" >/tmp/health.json 2>/dev/null; do
  i=$((i + 1))
  if [ "$i" -gt 60 ]; then
    echo "smoke FAIL: health not reachable after 60s" >&2
    tail -20 "$ROOT/target/wrangler-dev.log" >&2
    exit 1
  fi
  sleep 1
done

grep -q '"status":"ok"' /tmp/health.json || {
  echo "smoke FAIL: unexpected body: $(cat /tmp/health.json)" >&2
  exit 1
}
echo "smoke OK: $(cat /tmp/health.json)"
