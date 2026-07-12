#!/usr/bin/env sh
# Local smoke: wrangler dev + full M1 loop — health, packs, profile,
# tutor-context, import validate→apply, learning-state, idempotent re-apply.
# ADR-0003: local-first — no Cloudflare account until WBS 10.
set -eu

export PATH="/opt/homebrew/bin:$PATH" # node >= 22 for wrangler (README)
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PORT="${SMOKE_PORT:-8787}"
BASE="http://localhost:$PORT/api/v1"

cd "$ROOT/apps/worker"
npx wrangler d1 migrations apply DB --local >/dev/null
npx wrangler dev --local --port "$PORT" >"$ROOT/target/wrangler-dev.log" 2>&1 &
WRANGLER_PID=$!
trap 'kill "$WRANGLER_PID" 2>/dev/null || true' EXIT

i=0
until curl -sf "$BASE/health" >/tmp/health.json 2>/dev/null; do
  i=$((i + 1))
  if [ "$i" -gt 90 ]; then
    echo "smoke FAIL: health not reachable" >&2
    tail -20 "$ROOT/target/wrangler-dev.log" >&2
    exit 1
  fi
  sleep 1
done
grep -q '"status":"ok"' /tmp/health.json || { echo "FAIL health: $(cat /tmp/health.json)" >&2; exit 1; }
echo "1 health OK"

curl -sf "$BASE/language-packs" | grep -q '"es-ES"' || { echo "FAIL packs" >&2; exit 1; }
echo "2 packs OK"

PROFILE=$(curl -sf -X POST "$BASE/language-profiles" \
  -H 'content-type: application/json' \
  -d '{"language_pack_id":"es-ES","source_language":"ru"}')
LLP=$(echo "$PROFILE" | sed -n 's/.*"id":"\([^"]*\)".*/\1/p')
[ -n "$LLP" ] || { echo "FAIL profile create: $PROFILE" >&2; exit 1; }
echo "3 profile OK ($LLP)"

curl -sf "$BASE/export/tutor-context?language_profile_id=$LLP" | grep -q '"next_lesson_number":1' \
  || { echo "FAIL tutor-context" >&2; exit 1; }
echo "4 tutor-context OK"

BODY=$(cat <<EOF
{"schema_version":"1","client_operation_id":"op-smoke-1",
 "language_profile_id":"$LLP","language_pack_id":"es-ES","language_pack_version":"0.1.0",
 "lesson":{"scenario":"pharmacy","duration_seconds":900,"tutor_summary":"ok","next_goal":"dosage"},
 "metrics":{"speaking_seconds":300,"unaided_recall_count":2,"aided_recall_count":1,"long_pause_count":0},
 "new_learning_items":[{"item_type":"phrase","target_text":"¿Tiene algo para la tos?","prompt_text":"Спросите что-нибудь от кашля","prompt_language":"ru"}],
 "error_occurrences":[{"core_category":"grammar","pack_error_code":"es.verb.ser_estar","original_production":"estoy médico","corrected_production":"soy médico"}]}
EOF
)
VALIDATE=$(curl -sf -X POST "$BASE/import/lesson-result/validate" -H 'content-type: application/json' -d "$BODY")
TOKEN=$(echo "$VALIDATE" | sed -n 's/.*"validation_token":"\([^"]*\)".*/\1/p')
[ -n "$TOKEN" ] || { echo "FAIL validate: $VALIDATE" >&2; exit 1; }
echo "5 validate OK"

APPLY=$(curl -sf -X POST "$BASE/import/lesson-result/apply?validation_token=$TOKEN" \
  -H 'content-type: application/json' -d "$BODY")
echo "$APPLY" | grep -q '"lesson_id"' || { echo "FAIL apply: $APPLY" >&2; exit 1; }
echo "6 apply OK"

STATE=$(curl -sf "$BASE/learning-state?language_profile_id=$LLP")
echo "$STATE" | grep -q '"introduced":1' || { echo "FAIL state: $STATE" >&2; exit 1; }
echo "$STATE" | grep -q '"active_error_targets":1' || { echo "FAIL errors: $STATE" >&2; exit 1; }
echo "7 learning-state OK"

TOKEN2=$(curl -sf -X POST "$BASE/import/lesson-result/validate" -H 'content-type: application/json' -d "$BODY" | sed -n 's/.*"validation_token":"\([^"]*\)".*/\1/p')
DUP=$(curl -sf -X POST "$BASE/import/lesson-result/apply?validation_token=$TOKEN2" \
  -H 'content-type: application/json' -d "$BODY")
echo "$DUP" | grep -q '"duplicate"' || { echo "FAIL idempotency: $DUP" >&2; exit 1; }
echo "8 idempotent re-apply OK"

echo "smoke OK: full M1 loop green"
