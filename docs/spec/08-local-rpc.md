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

Successful calls return JSON-RPC 2.0 `result`. Application failures return JSON-RPC errors whose `data` preserves the stable application error code, message, details, and HTTP-like status used by shared UI error handling.

Rules:

- loopback only; host and origin are validated;
- request body is bounded to 256 KiB;
- components name application operations; they do not encode use cases as REST URL paths;
- `apps/local/src/server.rs` is a transport boundary only and calls the same handlers used by MCP;
- read projections and write passes own application semantics; transport code does not own SQLite policy;
- write idempotency remains domain/application behavior through `client_operation_id`;
- no synthetic internal HTTP request/response round-trip is permitted.

## 2. Operation groups

The active browser RPC operations are grouped by capability.

### Language profiles

```text
list_language_profiles
create_language_profile
```

### Learning and lessons

```text
get_dashboard
get_learning_state
get_next_lesson
list_lessons
validate_lesson_result
apply_lesson_result
```

### Evidence and tutor state

```text
list_error_targets
complete_error_recheck
get_tutor_context
```

### Cards

```text
list_cards
get_card
set_card_status
```

### Review

```text
create_review_session
get_review_queue
review_card
undo_review
complete_review_session
get_review_summary
get_review_forecast
get_review_retention
```

### System

```text
health
```

The method list above is the active browser surface. Methods without a current PWA workflow are not retained speculatively.

## 3. File downloads

Files are the only remaining direct `/api/v1` routes because browser download semantics are not application command/query semantics:

```text
GET /api/v1/export/full?language_profile_id={id}
GET /api/v1/export/cards.csv?language_profile_id={id}
```

They return attachments. Tutor context is an RPC operation (`get_tutor_context`), not an HTTP export route.

## 4. Shared schemas

`shared-schema` contains transport-neutral UI/application DTOs and the stable error body used by the browser boundary. It does not define a REST resource model.

Domain types stay in domain crates. Single-consumer SQL stays with its owning read/write pass; only reused SQLite codecs and multi-consumer queries live in crate-private `apps/local::sqlite`. Transport envelopes stay at the transport boundary.

## 5. MCP relationship

MCP does not call the browser HTTP endpoint. `mcp_tools` invokes the same application handlers directly in-process, and the secure tunnel carries MCP JSON-RPC externally.

Therefore:

```text
PWA -> POST /rpc ----┐
                     ├-> read projections / write passes -> SQLite
MCP -> mcp_tools ----┘
```

There is one application execution path and one SQLite system of record.

For chat tutoring, `get_tutor_context` is the single authoritative pre-lesson read surface. Narrow inspection operations such as `get_learning_state`, `get_next_lesson`, `get_review_summary`, and redesign/error projections remain available to the browser through local RPC, but are deliberately not duplicated as MCP tools. The native tutor flow is:

```text
get_tutor_context
    -> choose scenario/goal from the returned state
    -> start_lesson
    -> use start_lesson.adaptive_lesson_plan + start_lesson.tutor_tasks
       as the final persisted snapshot for the lesson
    -> complete_lesson using lesson_result_contract
```

`start_lesson` recomputes and persists the final planner snapshot so a state change between the context read and lesson opening cannot make calibration refer to a different plan than the one returned to the chat model. Live MCP completion must pass that call's returned `id` explicitly as the top-level `lesson_id` to `complete_lesson`; the runtime never guesses a lesson from the latest open row. Due error-recheck outcomes are returned inside `lesson_result.error_rechecks` and recommendation dispositions inside `lesson_result.recommendation_resolutions`; both are applied in the same transaction as lesson completion, so canonical MCP does not need separate recheck or recommendation-maintenance write tools. The local browser RPC `complete_error_recheck` remains a direct compatibility/inspection write surface, but it enforces the same measurement contract: `assistance_level=0`, a concrete `context`, and after a failed recheck both `repair_applied=true` and a stable non-empty `repair_action` before a fresh cycle may be scheduled. A lesson-result document with `lesson.lesson_id = null` is reserved for the validated import flow that creates a completed lesson.
