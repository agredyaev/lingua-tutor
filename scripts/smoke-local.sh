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
rm -rf .wrangler/state # fresh local D1 every run — smoke is self-contained
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
 "error_occurrences":[{"core_category":"grammar","pack_error_code":"es.verb.ser_estar","original_production":"estoy médico","corrected_production":"soy médico"}],
 "card_mutations":[
   {"operation":"create","client_operation_id":"op-smoke-card-1","card":{
     "intent":"production","prompt":"Спросите, есть ли что-нибудь от кашля",
     "answer":"¿Tiene algo para la tos?","accepted_answers":["¿Hay algo para la tos?"],
     "prompt_language":"ru","creation_reason":"new high-value phrase"}},
   {"operation":"create","client_operation_id":"op-smoke-card-dup","card":{
     "intent":"production","prompt":"Спросите, есть ли что-нибудь от кашля",
     "answer":"¿Tiene algo para la tos?","accepted_answers":["¿Hay algo para la tos?"],
     "prompt_language":"ru","creation_reason":"duplicate on purpose"}}]}
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

# ---- M2: cards + review loop ----
echo "$APPLY" | grep -q '"verdict":"applied"' || { echo "FAIL card create: $APPLY" >&2; exit 1; }
echo "$APPLY" | grep -q '"verdict":"rejected"' || { echo "FAIL duplicate card not rejected: $APPLY" >&2; exit 1; }
CARDS=$(curl -sf "$BASE/cards?language_profile_id=$LLP")
CARD=$(echo "$CARDS" | sed -n 's/.*"id":"\(crd_[^"]*\)".*/\1/p' | head -1)
[ -n "$CARD" ] || { echo "FAIL cards list: $CARDS" >&2; exit 1; }
echo "9 tutor card created + duplicate rejected OK"

RS=$(curl -sf -X POST "$BASE/review-sessions" -H 'content-type: application/json' \
  -d "{\"language_profile_id\":\"$LLP\"}" | sed -n 's/.*"id":"\([^"]*\)".*/\1/p')
QUEUE=$(curl -sf "$BASE/review-sessions/$RS/queue")
echo "$QUEUE" | grep -q "$CARD" || { echo "FAIL queue: $QUEUE" >&2; exit 1; }
HASH=$(echo "$QUEUE" | sed -n 's/.*"prior_state_hash":"\([^"]*\)".*/\1/p' | head -1)
echo "10 review queue OK"

EVAL=$(curl -sf -X POST "$BASE/review/evaluate" -H 'content-type: application/json' \
  -d "{\"card_id\":\"$CARD\",\"learner_answer\":\"¿tiene algo para la tos?\"}")
echo "$EVAL" | grep -q '"correct"' || { echo "FAIL evaluate: $EVAL" >&2; exit 1; }
echo "11 pack evaluation OK"

EVENT=$(curl -sf -X POST "$BASE/review-sessions/$RS/events" -H 'content-type: application/json' \
  -d "{\"client_operation_id\":\"op-grade-1\",\"card_id\":\"$CARD\",\"grade\":3,
       \"presented_at\":\"2026-07-12T10:00:00Z\",\"answered_at\":\"2026-07-12T10:00:05Z\",
       \"response_millis\":5000,\"answer_revealed\":true,\"prior_state_hash\":\"$HASH\"}")
echo "$EVENT" | grep -q '"event_id"' || { echo "FAIL grade: $EVENT" >&2; exit 1; }
echo "$EVENT" | grep -q '"card_status":"learning"' || { echo "FAIL status transition: $EVENT" >&2; exit 1; }
echo "12 grade submit OK"

STALE=$(curl -s -X POST "$BASE/review-sessions/$RS/events" -H 'content-type: application/json' \
  -d "{\"client_operation_id\":\"op-grade-2\",\"card_id\":\"$CARD\",\"grade\":3,
       \"presented_at\":\"2026-07-12T10:00:00Z\",\"answered_at\":\"2026-07-12T10:00:05Z\",
       \"response_millis\":5000,\"answer_revealed\":true,\"prior_state_hash\":\"$HASH\"}")
echo "$STALE" | grep -q 'conflict_stale_state' || { echo "FAIL stale hash: $STALE" >&2; exit 1; }
DUP_EV=$(curl -sf -X POST "$BASE/review-sessions/$RS/events" -H 'content-type: application/json' \
  -d "{\"client_operation_id\":\"op-grade-1\",\"card_id\":\"$CARD\",\"grade\":1,
       \"presented_at\":\"x\",\"answered_at\":\"x\",\"response_millis\":1,
       \"answer_revealed\":false,\"prior_state_hash\":\"whatever\"}")
echo "$DUP_EV" | grep -q '"duplicate":true' || { echo "FAIL event idempotency: $DUP_EV" >&2; exit 1; }
echo "13 stale-hash 409 + event idempotency OK"

UNDO=$(curl -sf -X POST "$BASE/review-sessions/$RS/undo-last")
echo "$UNDO" | grep -q '"compensating_event_id"' || { echo "FAIL undo: $UNDO" >&2; exit 1; }
SUMMARY=$(curl -sf "$BASE/review/summary?language_profile_id=$LLP")
echo "$SUMMARY" | grep -q '"reviews_today"' || { echo "FAIL summary: $SUMMARY" >&2; exit 1; }
FORECAST=$(curl -sf "$BASE/review/forecast?language_profile_id=$LLP&days=7")
echo "$FORECAST" | grep -q '"due_per_day"' || { echo "FAIL forecast: $FORECAST" >&2; exit 1; }
echo "14 undo + summary + forecast OK"

# ---- M4: adaptive loop ----
HASH2=$(echo "$UNDO" | sed -n 's/.*"state_hash":"\([^"]*\)".*/\1/p')
for i in 2 3 4; do
  EV=$(curl -s -X POST "$BASE/review-sessions/$RS/events" -H 'content-type: application/json' \
    -d "{\"client_operation_id\":\"op-again-$i\",\"card_id\":\"$CARD\",\"grade\":1,
         \"presented_at\":\"2026-07-12T10:0$i:00Z\",\"answered_at\":\"2026-07-12T10:0$i:05Z\",
         \"response_millis\":4000,\"answer_revealed\":true,\"prior_state_hash\":\"$HASH2\"}")
  HASH2=$(echo "$EV" | sed -n 's/.*"state_hash":"\([^"]*\)".*/\1/p')
  [ -n "$HASH2" ] || { echo "FAIL again-grade $i: $EV" >&2; exit 1; }
done
COMPLETE=$(curl -sf -X POST "$BASE/review-sessions/$RS/complete" -H 'content-type: application/json' \
  -d '{"card_count":4,"review_seconds":120}')
echo "$COMPLETE" | grep -qE '"recommendations_created":[1-9]' \
  || { echo "FAIL rules: $COMPLETE" >&2; exit 1; }
echo "15 adaptive rules fired on session complete OK"

RECS=$(curl -sf "$BASE/recommendations?language_profile_id=$LLP")
echo "$RECS" | grep -q 'redesign_card' || { echo "FAIL recs: $RECS" >&2; exit 1; }
REDESIGN=$(curl -sf "$BASE/cards/redesign-candidates?language_profile_id=$LLP")
echo "$REDESIGN" | grep -q "$CARD" || { echo "FAIL redesign list: $REDESIGN" >&2; exit 1; }
echo "16 recommendations + redesign candidates OK"

DASH=$(curl -sf "$BASE/dashboard?language_profile_id=$LLP")
echo "$DASH" | grep -q '"open_recommendations":\[' || { echo "FAIL dash recs: $DASH" >&2; exit 1; }
CTX=$(curl -sf "$BASE/export/tutor-context?language_profile_id=$LLP")
echo "$CTX" | grep -q '"redesign_candidates"' || { echo "FAIL ctx redesign: $CTX" >&2; exit 1; }
echo "$CTX" | grep -q '"review_summary_7d"' || { echo "FAIL ctx summary: $CTX" >&2; exit 1; }
REC_ID=$(echo "$RECS" | sed -n 's/.*"id":"\(rec_[^"]*\)".*/\1/p' | head -1)
curl -sf -X POST "$BASE/recommendations/$REC_ID/dismiss" >/dev/null
echo "17 dashboard + tutor-context surfacing + dismiss OK"

echo "smoke OK: full M1+M2+M4 loop green"
