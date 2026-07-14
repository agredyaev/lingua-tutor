#!/usr/bin/env sh
# MCP smoke: full OAuth 2.1 PKCE flow + JSON-RPC transport against
# wrangler dev --local (WBS 12.1h, ADR-0006). Self-contained; no account.
set -eu

export PATH="/opt/homebrew/bin:$PATH"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PORT="${SMOKE_PORT:-8789}"
BASE="http://localhost:$PORT"

cd "$ROOT/apps/worker"
rm -rf .wrangler/state
npx wrangler d1 migrations apply DB --local >/dev/null
npx wrangler dev --local --port "$PORT" --var MCP_ENABLED:true \
  >"$ROOT/target/wrangler-mcp.log" 2>&1 &
WRANGLER_PID=$!
trap 'kill "$WRANGLER_PID" 2>/dev/null || true' EXIT

i=0
until curl -sf "$BASE/.well-known/oauth-authorization-server" >/tmp/asm.json 2>/dev/null; do
  i=$((i + 1))
  [ "$i" -gt 90 ] && { echo "FAIL: dev server not reachable" >&2; tail -20 "$ROOT/target/wrangler-mcp.log" >&2; exit 1; }
  sleep 1
done
grep -q '"code_challenge_methods_supported":\["S256"\]' /tmp/asm.json || { echo "FAIL AS metadata" >&2; exit 1; }
echo "1 AS metadata OK"

curl -sf "$BASE/.well-known/oauth-protected-resource" | grep -q '"authorization_servers"' \
  || { echo "FAIL PRM" >&2; exit 1; }
echo "2 protected-resource metadata OK"

# 3: DCR (localhost redirect allowed because AUTH_ENFORCED=false locally)
CLIENT_ID=$(curl -sf -X POST "$BASE/oauth/register" -H 'Content-Type: application/json' \
  -d '{"client_name":"smoke","redirect_uris":["http://localhost:9999/cb"]}' \
  | sed -n 's/.*"client_id":"\([^"]*\)".*/\1/p')
[ -n "$CLIENT_ID" ] || { echo "FAIL DCR" >&2; exit 1; }
echo "3 DCR OK ($CLIENT_ID)"

# 4: PKCE pair + authorize (dev: unenforced auth = owner) → code
VERIFIER="dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"
CHALLENGE="E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
LOCATION=$(curl -sf -X POST "$BASE/oauth/authorize" \
  -H 'Content-Type: application/x-www-form-urlencoded' \
  --data-urlencode "client_id=$CLIENT_ID" \
  --data-urlencode "redirect_uri=http://localhost:9999/cb" \
  --data-urlencode "response_type=code" \
  --data-urlencode "scope=" \
  --data-urlencode "state=xyz" \
  --data-urlencode "code_challenge=$CHALLENGE" \
  --data-urlencode "code_challenge_method=S256" \
  -o /dev/null -w '%{redirect_url}')
CODE=$(printf '%s' "$LOCATION" | sed -n 's/.*[?&]code=\([^&]*\).*/\1/p')
[ -n "$CODE" ] || { echo "FAIL authorize: $LOCATION" >&2; exit 1; }
printf '%s' "$LOCATION" | grep -q 'state=xyz' || { echo "FAIL state echo" >&2; exit 1; }
echo "4 authorize OK"

# 5: bad verifier must fail
curl -sf -X POST "$BASE/oauth/token" \
  -d "grant_type=authorization_code&code=$CODE&client_id=$CLIENT_ID&redirect_uri=http://localhost:9999/cb&code_verifier=wrong" \
  >/tmp/badpkce.json 2>/dev/null && { echo "FAIL: bad verifier accepted" >&2; exit 1; }
echo "5 bad PKCE rejected OK"

# 6: code is single-use — re-authorize for a fresh code, then exchange
LOCATION=$(curl -sf -X POST "$BASE/oauth/authorize" \
  -H 'Content-Type: application/x-www-form-urlencoded' \
  --data-urlencode "client_id=$CLIENT_ID" \
  --data-urlencode "redirect_uri=http://localhost:9999/cb" \
  --data-urlencode "response_type=code" \
  --data-urlencode "scope=" \
  --data-urlencode "state=abc" \
  --data-urlencode "code_challenge=$CHALLENGE" \
  --data-urlencode "code_challenge_method=S256" \
  -o /dev/null -w '%{redirect_url}')
CODE=$(printf '%s' "$LOCATION" | sed -n 's/.*[?&]code=\([^&]*\).*/\1/p')
TOKENS=$(curl -sf -X POST "$BASE/oauth/token" \
  -d "grant_type=authorization_code&code=$CODE&client_id=$CLIENT_ID&redirect_uri=http://localhost:9999/cb&code_verifier=$VERIFIER")
AT=$(printf '%s' "$TOKENS" | sed -n 's/.*"access_token":"\([^"]*\)".*/\1/p')
RT=$(printf '%s' "$TOKENS" | sed -n 's/.*"refresh_token":"\([^"]*\)".*/\1/p')
[ -n "$AT" ] && [ -n "$RT" ] || { echo "FAIL token exchange: $TOKENS" >&2; exit 1; }
echo "6 token exchange OK"

# 7: replayed code must fail (single-use)
curl -sf -X POST "$BASE/oauth/token" \
  -d "grant_type=authorization_code&code=$CODE&client_id=$CLIENT_ID&redirect_uri=http://localhost:9999/cb&code_verifier=$VERIFIER" \
  >/dev/null 2>&1 && { echo "FAIL: code replay accepted" >&2; exit 1; }
echo "7 code replay rejected OK"

MCP() {
  curl -sf -X POST "$BASE/mcp" -H "Authorization: Bearer $AT" \
    -H 'Content-Type: application/json' -d "$1"
}

# 8: no bearer → 401 + WWW-Authenticate
HDRS=$(curl -s -o /dev/null -D - -X POST "$BASE/mcp" -d '{}')
printf '%s' "$HDRS" | grep -qi '^HTTP.* 401' || { echo "FAIL: /mcp without bearer not 401" >&2; exit 1; }
printf '%s' "$HDRS" | grep -qi 'www-authenticate:.*resource_metadata' || { echo "FAIL: WWW-Authenticate missing" >&2; exit 1; }
echo "8 unauthenticated 401 OK"

# 9: GET /mcp → 405
[ "$(curl -s -o /dev/null -w '%{http_code}' "$BASE/mcp")" = "405" ] || { echo "FAIL: GET /mcp not 405" >&2; exit 1; }
echo "9 GET 405 OK"

# 10: initialize + tools/list (all 14 roadmap tools)
MCP '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' | grep -q '"protocolVersion"' \
  || { echo "FAIL initialize" >&2; exit 1; }
TOOLS=$(MCP '{"jsonrpc":"2.0","id":2,"method":"tools/list"}')
for t in list_language_packs get_active_language_profile get_language_pack_context \
         get_learning_state get_next_lesson get_due_error_targets get_review_summary \
         get_card_redesign_candidates get_tutor_context start_lesson complete_lesson \
         upsert_cards suspend_cards log_error_occurrence correct_lesson_record; do
  printf '%s' "$TOOLS" | grep -q "\"$t\"" || { echo "FAIL tools/list missing $t" >&2; exit 1; }
done
echo "10 initialize + tools/list OK"

# 11: read tool end-to-end (create profile via API first — dev is unauthenticated)
PROFILE=$(curl -sf -X POST "$BASE/api/v1/language-profiles" -H 'Content-Type: application/json' \
  -d '{"language_pack_id":"es-ES","source_language":"ru","estimated_level":"A2"}' \
  | sed -n 's/.*"id":"\([^"]*\)".*/\1/p')
[ -n "$PROFILE" ] || { echo "FAIL profile create" >&2; exit 1; }
MCP "{\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"tools/call\",\"params\":{\"name\":\"get_learning_state\",\"arguments\":{\"language_profile_id\":\"$PROFILE\"}}}" \
  | grep -q '"isError":false' || { echo "FAIL get_learning_state" >&2; exit 1; }
echo "11 read tool OK"

# 12: complete_lesson write + idempotent replay
LESSON_DOC=$(cat <<JSON
{"schema_version":"1","client_operation_id":"op_smoke_mcp_1","language_profile_id":"$PROFILE",
 "language_pack_id":"es-ES","language_pack_version":"1.0.0",
 "lesson":{"scenario":"smoke","duration_seconds":60,"tutor_summary":"s","next_goal":"g"},
 "metrics":{"speaking_seconds":30,"unaided_recall_count":1,"aided_recall_count":0,
            "listening_score":0.5,"avg_answer_millis":1000,"long_pause_count":0,"confidence":0.5},
 "new_learning_items":[],"evidence":[],"error_occurrences":[],"listening_attempts":[],
 "mastery_changes":[],"card_mutations":[]}
JSON
)
CALL=$(printf '{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"complete_lesson","arguments":{"lesson_result":%s}}}' "$LESSON_DOC")
MCP "$CALL" | grep -q '"isError":false' || { echo "FAIL complete_lesson" >&2; exit 1; }
MCP "$(printf '%s' "$CALL" | sed 's/"id":4/"id":5/')" | grep -q 'duplicate' \
  || { echo "FAIL idempotent replay" >&2; exit 1; }
echo "12 complete_lesson + idempotent replay OK"

# 13: scope enforcement — token restricted to learning.read cannot write
LOCATION=$(curl -sf -X POST "$BASE/oauth/authorize" \
  -H 'Content-Type: application/x-www-form-urlencoded' \
  --data-urlencode "client_id=$CLIENT_ID" \
  --data-urlencode "redirect_uri=http://localhost:9999/cb" \
  --data-urlencode "response_type=code" \
  --data-urlencode "scope=learning.read" \
  --data-urlencode "state=s" \
  --data-urlencode "code_challenge=$CHALLENGE" \
  --data-urlencode "code_challenge_method=S256" \
  -o /dev/null -w '%{redirect_url}')
CODE=$(printf '%s' "$LOCATION" | sed -n 's/.*[?&]code=\([^&]*\).*/\1/p')
RO_AT=$(curl -sf -X POST "$BASE/oauth/token" \
  -d "grant_type=authorization_code&code=$CODE&client_id=$CLIENT_ID&redirect_uri=http://localhost:9999/cb&code_verifier=$VERIFIER" \
  | sed -n 's/.*"access_token":"\([^"]*\)".*/\1/p')
curl -sf -X POST "$BASE/mcp" -H "Authorization: Bearer $RO_AT" -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"start_lesson","arguments":{"language_profile_id":"x","scenario":"y"}}}' \
  | grep -q 'insufficient_scope' || { echo "FAIL scope enforcement" >&2; exit 1; }
echo "13 scope enforcement OK"

# 14: refresh rotation — old refresh dies after use
NEW_TOKENS=$(curl -sf -X POST "$BASE/oauth/token" -d "grant_type=refresh_token&refresh_token=$RT&client_id=$CLIENT_ID")
printf '%s' "$NEW_TOKENS" | grep -q '"access_token"' || { echo "FAIL refresh grant" >&2; exit 1; }
curl -sf -X POST "$BASE/oauth/token" -d "grant_type=refresh_token&refresh_token=$RT&client_id=$CLIENT_ID" \
  >/dev/null 2>&1 && { echo "FAIL: rotated refresh token still valid" >&2; exit 1; }
echo "14 refresh rotation OK"

# 15: MCP_ENABLED off → 404 (restart without the var)
kill "$WRANGLER_PID" 2>/dev/null || true
sleep 1
npx wrangler dev --local --port "$PORT" >"$ROOT/target/wrangler-mcp.log" 2>&1 &
WRANGLER_PID=$!
i=0
until curl -sf "$BASE/api/v1/health" >/dev/null 2>&1; do
  i=$((i + 1)); [ "$i" -gt 90 ] && { echo "FAIL: restart" >&2; exit 1; }; sleep 1
done
[ "$(curl -s -o /dev/null -w '%{http_code}' "$BASE/.well-known/oauth-authorization-server")" = "404" ] \
  || { echo "FAIL: MCP paths visible while disabled" >&2; exit 1; }
echo "15 disabled gate 404 OK"

echo "MCP smoke: 15/15 OK"
