# 08 — HTTP API

Scope: all `/api/v1` endpoints served by `apps/local`. Shared DTOs live in `shared-schema`.
Depends on: [04-database.md](04-database.md) (names), [05-review-engine.md](05-review-engine.md), [07-tutor-contract.md](07-tutor-contract.md).

## 1. Conventions

- Base path `/api/v1`. JSON bodies. Runtime binds to loopback and validates host/origin ([10-security.md](10-security.md)).
- Error shape, all endpoints:

```json
{ "error": { "code": "conflict_stale_state", "message": "…", "details": {} } }
```

- Write endpoints accept `client_operation_id` (or `idempotency_key` for lessons); duplicate → original result, HTTP 200, no second write.
- List endpoints: bounded pages (`limit` capped server-side, `cursor` opaque). No unbounded responses.
- All learning resources scoped by `language_profile_id` query/body param.

## 2. Language packs and profiles

```text
GET   /api/v1/language-packs
GET   /api/v1/language-packs/{pack_id}
GET   /api/v1/language-profiles
POST  /api/v1/language-profiles
PATCH /api/v1/language-profiles/{profile_id}
POST  /api/v1/language-profiles/{profile_id}/activate
```

`POST /language-profiles` body: `{ "language_pack_id": "es-ES", "source_language": "ru" }`. Activate: only built-in contract-compatible packs; incompatible → `409 pack_incompatible`.

## 3. Learning state

```text
GET /api/v1/learning-state?language_profile_id={id}
GET /api/v1/next-lesson?language_profile_id={id}
GET /api/v1/dashboard?language_profile_id={id}&range=7d
```

`dashboard` response includes: sessions today, speaking minutes, reviews due/completed, estimated review time, new-card budget remaining, error rechecks due, current lesson target, recurring-error targets, 7d/30d trends, actionable exceptions.

## 4. Lessons

```text
POST /api/v1/lessons/start
POST /api/v1/lessons/{lesson_id}/complete
POST /api/v1/lessons/{lesson_id}/correct
GET  /api/v1/lessons?language_profile_id={id}
GET  /api/v1/lessons/{lesson_id}
```

`complete` body = lesson-result block ([07-tutor-contract.md](07-tutor-contract.md) §2), incl. `card_mutations`; applied atomically; idempotent via `client_operation_id`. `correct`: compensating correction to a completed lesson; never rewrites history.

## 5. Cards

```text
GET   /api/v1/cards?language_profile_id={id}&status=…&intent=…&q=…
GET   /api/v1/cards/{card_id}
POST  /api/v1/cards
PATCH /api/v1/cards/{card_id}
POST  /api/v1/cards/{card_id}/suspend
POST  /api/v1/cards/{card_id}/restore
POST  /api/v1/cards/{card_id}/retire
POST  /api/v1/cards/{card_id}/split
POST  /api/v1/cards/merge
GET   /api/v1/cards/redesign-candidates?language_profile_id={id}
```

`PATCH` requires `edit_class` (`cosmetic|clarifying|semantic`); server enforces schedule effect per class. `split` body: `{ "client_operation_id": "…", "children": [ { card }, … ] }` — suspends parent, records lineage. `merge` body: `{ "client_operation_id": "…", "surviving_card_id": "…", "merged_card_ids": [ "…" ] }`.

## 6. Review

```text
POST /api/v1/review-sessions
GET  /api/v1/review-sessions/{session_id}/queue
POST /api/v1/review-sessions/{session_id}/events
POST /api/v1/review-sessions/{session_id}/undo-last
POST /api/v1/review-sessions/{session_id}/complete
GET  /api/v1/review/summary?language_profile_id={id}
GET  /api/v1/review/forecast?language_profile_id={id}&days=7
```

`POST events` body:

```json
{
  "client_operation_id": "op_01J...",
  "card_id": "crd_01J...",
  "grade": 3,
  "presented_at": "2026-07-12T08:00:01Z",
  "answered_at": "2026-07-12T08:00:07Z",
  "response_millis": 5800,
  "answer_revealed": true,
  "prior_state_hash": "h_…",
  "source": "online"
}
```

Response: `{ "event_id": "…", "new_state": { …schedule… }, "next": { …next queue item or null… } }`.
Stale `prior_state_hash` → `409 conflict_stale_state`. Duplicate `client_operation_id` → original result. `undo-last`: compensating event for the immediately previous grade only.

## 7. Import and export

```text
GET  /api/v1/export/full
GET  /api/v1/export/tutor-context
GET  /api/v1/export/cards.csv
POST /api/v1/import/lesson-result/validate
POST /api/v1/import/lesson-result/apply
POST /api/v1/import/full/validate
POST /api/v1/import/full/apply
```

- `export/full`: complete versioned learning data (JSON, schema-versioned).
- `export/tutor-context`: compact adaptive context for ChatGPT — active pack metadata, learner preferences, ranked adaptive lesson plan, recent learning signals, due errors/rechecks, review summary, redesign evidence, curriculum candidates, and the lesson-result contract. Versioned.
- Validate → `{ "validation_token": "vt_…", "expires_at": "…", "preview": { …diff… }, "warnings": [] }`. Apply requires unexpired `validation_token` from the matching validate call; mismatch/expired → `409`.
- Additional exports via `export/full` sections: cards, review history, error history, lesson history, Markdown summary.

## 8. Errors and listening

Resource endpoints (same conventions): error targets, error occurrences, error rechecks (complete/pass/fail), listening attempts.

```text
GET  /api/v1/error-targets?language_profile_id={id}&status=…
GET  /api/v1/error-targets/{id}
POST /api/v1/error-targets/{id}/rechecks/{recheck_id}/complete
POST /api/v1/listening-attempts
GET  /api/v1/listening-attempts?language_profile_id={id}
```

## 9. System

```text
GET /api/v1/health
```

Reports: local runtime status, SQLite connectivity, schema version, scheduler algorithm/version, current parameter set, latest migration, application version ([11-infrastructure.md](11-infrastructure.md) §6).
