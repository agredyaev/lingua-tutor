# 08 — Local Operation Boundary

Scope: browser JSON-RPC, direct download endpoints, and transport-to-operation mapping in `apps/local`.
Depends on: [04-database.md](04-database.md), [05-review-engine.md](05-review-engine.md), [07-tutor-contract.md](07-tutor-contract.md).

## 1. Binding

The PWA has one command/query transport:

```text
POST /rpc
Content-Type: application/json

{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "get_dashboard",
  "params": { "language_profile_id": "llp_..." }
}
```

Successful calls return JSON-RPC 2.0 `result`.
Application failures return JSON-RPC errors.
The error `data` contains the application error code, message, details, and status consumed by shared UI error handling.

Rules:

- loopback only; host and origin are validated;
- request body is bounded to 256 KiB;
- components name application operations; they do not encode use cases as REST URL paths;
- `apps/local/src/server.rs` is a transport boundary only and calls the same handlers used by MCP;
- handlers own application-facing orchestration and repository calls, not HTTP concerns;
- write idempotency remains domain/application behavior through `client_operation_id`;
- no synthetic internal HTTP request/response round-trip is permitted.

## 2. Operation groups

The active browser RPC operations are grouped by capability.

### Profiles and packs

```text
list_language_profiles
list_language_packs
get_language_pack
create_language_profile
update_profile
```

### Learning and lessons

```text
get_dashboard
get_learning_state
get_next_lesson
start_lesson
list_lessons
get_lesson
complete_lesson
validate_lesson_result
apply_lesson_result
```

### Evidence and tutor state

```text
list_error_targets
get_error_target
complete_error_recheck
create_listening_attempt
get_tutor_context
```

### Cards and recommendations

```text
list_cards
get_card
patch_card
set_card_status
list_recommendations
resolve_recommendation
get_card_redesign_candidates
```

### Review

```text
create_review_session
get_review_queue
review_card
undo_review
complete_review_session
evaluate_review_answer
get_review_summary
get_review_forecast
get_review_retention
```

### System

```text
health
```

The method list above is a compatibility surface. Renaming/removing methods requires a conscious contract change and matching PWA/e2e updates.

## 3. File downloads

Files are the only remaining direct `/api/v1` routes because browser download semantics are not application command/query semantics:

```text
GET /api/v1/export/full?language_profile_id={id}
GET /api/v1/export/cards.csv?language_profile_id={id}
```

The two download routes return attachments.
Tutor context uses the `get_tutor_context` RPC operation, not an HTTP export route.

## 4. Shared schemas

`shared-schema` contains transport-neutral UI/application DTOs and the stable error body used by the browser boundary. It does not define a REST resource model.

Domain types stay in domain crates; persistence rows stay in `apps/local::repositories`; transport envelopes stay at the transport boundary.

## 5. MCP relationship

MCP does not call the browser HTTP endpoint.
`mcp_tools` invokes the same application handlers in-process.
OpenAI Secure MCP Tunnel carries MCP JSON-RPC between the local runtime and OpenAI.

Therefore:

```text
PWA -> POST /rpc ----┐
                     ├-> handlers -> repositories -> SQLite
MCP -> mcp_tools ----┘
```

There is one application execution path and one SQLite system of record.
