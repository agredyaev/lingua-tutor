# 07 — Tutor Contract

Scope: `tutor-contract` + `adaptation` crates. Lesson-result schema, card mutations, card types, quality rules, adaptive rules.
Depends on: [03-domain-model.md](03-domain-model.md), [04-database.md](04-database.md), [05-review-engine.md](05-review-engine.md), [06-language-packs.md](06-language-packs.md).

## 1. Tutor role

Tutor is primary author + maintainer of cards. Learner never required to: create cards, pick phrases, copy to another tool, maintain tags/scheduling, dedupe, redesign. Learner may edit/suspend/restore/delete via UI — optional controls only.

## 2. Lesson result

Versioned block returned by tutor (paste-import or MCP). Applied atomically with idempotency key.

```json
{
  "schema_version": "1",
  "client_operation_id": "op_01J...",
  "language_profile_id": "llp_01J...",
  "language_pack_id": "es-ES",
  "language_pack_version": "1.0.0",
  "lesson": {
    "lesson_id": "les_01J...",
    "scenario": "pharmacy",
    "duration_seconds": 1260,
    "tutor_summary": "…",
    "next_goal": "…"
  },
  "metrics": { "speaking_seconds": 540, "unaided_recall_count": 6,
               "aided_recall_count": 3, "listening_score": 0.7,
               "avg_answer_millis": 2100, "long_pause_count": 2, "confidence": 0.6 },
  "new_learning_items": [ { "item_type": "phrase", "target_text": "…",
                            "prompt_text": "…", "prompt_language": "ru" } ],
  "evidence": [ { "learning_item_id": "phr_01J...", "kind": "spontaneous_production",
                  "success": true, "detail": null } ],
  "error_occurrences": [ { "error_target_id": null,
                           "core_category": "verb_form", "pack_error_code": "es.verb.ser_estar",
                           "original_production": "…", "corrected_production": "…",
                           "explanation": "…" } ],
  "listening_attempts": [ { "learning_item_id": "phr_01J...", "success": false,
                            "source_reference": "lesson-audio-3", "transcript": "…" } ],
  "mastery_changes": [ { "learning_item_id": "phr_01J...", "new_status": "provisionally_mastered" } ],
  "card_mutations": [ { "operation": "create", "client_operation_id": "op_01J...x", "card": { } } ]
}
```

Validation server-side, schema- and size-bounded. Import flow: validate → preview → apply ([08-http-api.md](08-http-api.md) §7).

## 3. Card mutations

Operations: `create`, `update`, `add_accepted_answer`, `remove_accepted_answer`, `add_listening_variant`, `split`, `merge`, `suspend`, `restore`, `retire`, `link` (to phrase / error target / listening item / scenario).

Create payload:

```json
{
  "operation": "create",
  "client_operation_id": "op_...",
  "card": {
    "intent": "production",
    "prompt": "You need to ask whether there is an appointment available this week.",
    "answer": "¿Hay alguna cita disponible para esta semana?",
    "accepted_answers": ["¿Hay alguna cita libre para esta semana?"],
    "prompt_language": "ru",
    "explanation": "Use hay alguna + noun + disponible/libre.",
    "scenario_tags": ["medical", "appointment"],
    "skill_tags": ["question", "availability"],
    "source_learning_item_id": "phr_...",
    "source_error_target_id": null,
    "creation_reason": "Failed unaided production twice",
    "priority_score": 0.91
  }
}
```

`prompt_language` comes from learner language profile (es-ES default: `ru`). Every attempted mutation → `card_mutations` audit row, applied or not.

### Auto-apply rules

Mutation applies automatically only when ALL hold: non-destructive; lesson completion explicitly submitted by tutor; schema + quality rules pass; daily activation limit respected; no duplicate conflict. Suspension, merge, semantic reset, retirement stay visible in lesson-result preview / MCP write confirmation.

## 4. Card quality rules

Every card must: test one primary recall target; have answerable unambiguous prompt; prefer situational prompt over literal translation; obey pack naturalness + regional rules; include accepted alternatives when genuinely valid; avoid multiple unrelated unknowns; carry enough context for register; record `creation_reason`; link to source evidence; avoid duplicate skill testing unless evidence shows modality gap; contain plain text or strictly sanitized markup. A phrase appearing in a lesson is NOT sufficient reason for a card.

### Evidence sources

New high-value phrase; failed retrieval; recurring error; text-recognized-but-audio-missed; understood-but-not-produced; produced-only-with-hint; expected real scenario; useful correction replacing unnatural form; learned phrase needing transfer.

### Candidate scoring

Factors: real-life utility; frequency in expected situations; failure recurrence; communicative impact; transfer value; current priority; novelty; overlap with existing cards; cognitive load; review backlog. `priority_score` stored; deterministic rules control activation.

### Activation budget

Tutor may propose unlimited candidates; app activates ≤ daily new-card budget (default 8/day). Over budget or backlog limits → cards stay `candidate`.

### Duplicate prevention

Compare before create: normalized answer; communicative function; intent; linked phrase; linked error target; active accepted answers. Exact retry (same `client_operation_id`) → idempotent no-op. Near-duplicate → tutor warning or merge recommendation, never silent create.

### Edit classification

| Class | Definition | Schedule effect |
|---|---|---|
| cosmetic | punctuation, typo, formatting | preserved |
| clarifying | better prompt, same tested answer | preserved; version recorded |
| semantic | changed recall target or answer meaning | new card or explicit tutor-confirmed reset |
| split | suspend original, create smaller cards | lineage via `parent_card_id` |
| merge | one survivor, source histories preserved | lineage recorded |

## 5. Card types

| Intent | Front | Back |
|---|---|---|
| `production` | Russian/situational prompt; optional context; optional hidden first-word hint | preferred answer; accepted alternatives; short explanation; pronunciation note |
| `listening` | audio reference or tutor-spoken prompt; no transcript | transcript; meaning; expected response; pronunciation/linking note |
| `contrast` | situation; learner's typical incorrect form when useful; request corrected form | correct form; one-sentence rule; two short contrast examples |
| `transfer` | known phrase, new scenario | adapted usage |

Listening: MVP uses browser speech or tutor audio in-lesson; stores source reference + text, never audio files. Transfer: sparing; no trivial variations the learner already generates.

Intent separation: recognition ≠ production. Multiple cards per phrase only on evidenced modality gap (text-ok/listen-fail; listen-ok/produce-fail; prompted-ok/spontaneous-fail).

Default mix (guidance, evidence overrides): 60–75% production, 15–25% listening, 10–20% contrast/correction.

## 6. Redesign loop

1. System marks redesign candidate ([05-review-engine.md](05-review-engine.md) §10).
2. Evidence enters next lesson tutor context.
3. Tutor proposes clarification / split / modality change / suspension.
4. App validates, shows lineage.
5. Schedule preserved or reset per edit class.

## 7. Adaptive rules

Explicit Rust in `adaptation`, unit-tested, table-driven. No interpreted rule language. Rules emit recommendations; content mutation only via §3 auto-apply gate.

| Trigger | Action |
|---|---|
| Failed production review | phrase into next speaking session |
| Card review success + spontaneous speaking failure | schedule role-play transfer, not more reviews |
| Repeated listening failure on known phrase | create/activate listening card |
| Repeated prompted success + spontaneous failure | create transfer target |
| Error occurrences = 3 | activate mini-target; contrast card when appropriate |
| Unresolved error occurrences = 5 | require method change |
| Success in 3 distinct contexts | eligible for provisional mastery |
| Failed 1/3/7-day recheck | target back to active practice |
| Persistent `Hard` | redesign before frequency increase |
| High review success + low real-life utility | consider retiring card |
