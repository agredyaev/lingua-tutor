# 07 — Tutor Contract

Scope: `tutor-contract` + `adaptation` crates. Lesson-result schema, card mutations, card types, validation rules, recommendation rules.
Depends on: [03-domain-model.md](03-domain-model.md), [04-database.md](04-database.md), [05-review-engine.md](05-review-engine.md), [06-language-packs.md](06-language-packs.md).

## 1. Tutor role

The tutor creates and maintains cards.
The learner is not required to create cards, select phrases, copy data to another tool, maintain tags or schedules, deduplicate cards, or redesign cards.
The UI exposes edit, suspend, restore, and delete controls.

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
  "new_learning_items": [ { "client_item_key": "new-1", "item_type": "phrase",
                            "target_text": "…", "prompt_text": "…", "prompt_language": "ru" } ],
  "evidence": [ { "client_item_key": "new-1", "kind": "spontaneous_production",
                  "success": true, "context": "pharmacy:ask-for-remedy",
                  "context_family": "service.ask_help",
                  "assistance_level": 0, "response_millis": 1800,
                  "probe_phase": "delayed_retrieval", "detail": null } ],
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

The server validates schema and size limits.
Existing items use `learning_item_id`. An item introduced in the same result can use `client_item_key`.
Productive evidence requires `context`. Transfer evidence also requires `context_family`.
`assistance_level = 0` means unaided production. Missing assistance does not prove mastery.
Assisted success is `prompted_production`; it is not `spontaneous_production` or `transfer`.
Do not infer `response_millis` or `probe_phase` when they were not observed.
Import flow: validate → preview → apply ([08-local-rpc.md](08-local-rpc.md) §2).

## 3. Card mutations

Supported operations:

- `create`
- `update`
- `add_accepted_answer`
- `remove_accepted_answer`
- `add_listening_variant`
- `split`
- `merge`
- `suspend`
- `restore`
- `retire`
- `link`

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

Automatic application requires all conditions:

- the mutation is non-destructive;
- the tutor submitted lesson completion;
- schema and validation rules pass;
- the daily activation limit is not exceeded;
- duplicate detection reports no conflict.

Suspension, merge, semantic reset, and retirement require lesson-result preview or MCP write confirmation.

## 4. Card quality rules

Each card must:

- test one recall target;
- use a prompt with one expected task;
- use a situational prompt when the target depends on a situation;
- pass language-pack answer and regional-usage rules;
- include an accepted alternative only when pack answer evaluation accepts it;
- avoid testing unrelated unknown targets in one card;
- include scenario or register context when omission changes the expected answer;
- include `creation_reason`;
- link to source evidence when source evidence exists;
- duplicate an existing skill only when evidence shows a modality gap;
- contain plain text or sanitized markup.

Lesson appearance alone does not satisfy `creation_reason`.

### Evidence sources

Evidence sources:

- phrase selected by the lesson planner for introduction;
- failed retrieval;
- recurring error;
- text recognition with failed listening evidence;
- comprehension without production;
- production with assistance but no unaided production;
- scenario listed in learner priorities;
- correction of a form rejected by language-pack evaluation;
- learned phrase without transfer evidence.

### Candidate scoring

`priority_score` is an optional tutor-provided value in `0.0..=1.0`.
The runtime stores the value but does not use an unspecified qualitative factor to bypass duplicate checks or the activation budget.

### Activation budget

The lesson-result schema permits up to the configured payload limit of candidate mutations.
The runtime activates at most the remaining daily new-card budget (default 8/day).
Cards above the budget remain `candidate`.

### Duplicate prevention

Before create, compare normalized answer, intent, linked phrase, linked error target, and accepted answers.
An exact retry with the same `client_operation_id` is idempotent.
A duplicate conflict returns a warning or merge recommendation; it does not create another card.

### Edit classification

| Class | Definition | Schedule effect |
|---|---|---|
| cosmetic | punctuation, typo, formatting | preserved |
| clarifying | prompt changed; tested answer unchanged | preserved; version recorded |
| semantic | changed recall target or answer meaning | new card or explicit tutor-confirmed reset |
| split | suspend original, create smaller cards | lineage via `parent_card_id` |
| merge | one survivor, source histories preserved | lineage recorded |

## 5. Card types

| Intent | Front | Back |
|---|---|---|
| `production` | Russian/situational prompt; optional context; optional hidden first-word hint | preferred answer; accepted alternatives; short explanation; pronunciation note |
| `listening` | audio reference or tutor-spoken prompt; no transcript | transcript; meaning; expected response; pronunciation/linking note |
| `contrast` | situation; competing incorrect form from learner evidence; request corrected form | correct form; one-sentence rule; two short contrast examples |
| `transfer` | known phrase, new scenario | adapted usage |

Listening stores a source reference and text; it does not store audio files.
Transfer cards require a new scenario or semantic context family; lexical-only variation does not qualify.

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
| Error occurrences = 3 | activate mini-target; add contrast only when error evidence contains a competing form |
| Unresolved error occurrences = 5 | require method change |
| Explicitly unaided transfer in 3 distinct semantic `context_family` values | eligible for full mastery |
| Failed 1/3/7-day recheck | target back to active practice |
| Persistent `Hard` | redesign before frequency increase |
| High review success + low real-life utility | consider retiring card |

## 8. Adaptive lesson planner

Before each lesson the runtime derives a language-neutral plan from persisted evidence rather than copying the previous tutor goal.

Inputs:
- review pressure: due items, significantly overdue mature reviews, effective new-card budget;
- unresolved errors, due 1/3/7-day rechecks, and method-change targets;
- recent lesson metrics over a bounded window: speaking ratio, unaided-vs-aided recall, listening score, answer latency, confidence;
- open adaptive recommendations;
- learner preferences.

Planner priorities are numeric:

| Condition | Action | Score |
|---|---|---:|
| `method_change_errors > 0` | `change_method` | 100 |
| `due_error_rechecks > 0` | `run_error_rechecks` | 96 |
| `unaided_recall_rate < 0.60` | `reduce_prompt_dependence` | 92 |
| `listening_score < 0.70` | `strengthen_listening` | 88 |
| `avg_answer_millis > 5000` | `build_automaticity` | 84 |
| `confidence < 0.55` | `stabilize_confidence` | 82 |
| `speaking_ratio < 0.45` | `increase_speaking_share` | 80 |
| `active_errors > 0` | `repair_recurring_errors` | 78 |
| `overdue_reviews > 0` | `consolidate_overdue_material` | 76 |
| `open_recommendations > 0` | `apply_adaptive_recommendations` | 72 |
| `due_reviews > 0` | `retrieve_due_material` | 68 |
| `max_new_items > 0` | `introduce_high_utility_material` | 55 |
| no condition above matched | `diagnose_and_expand` | 50 |

The planner returns:
- primary objective;
- supported/balanced/stretch difficulty;
- maximum new items for this lesson;
- ranked actions with evidence-based rationales;
- diagnostic probes;
- success criteria;
- recent signal summary;
- priority scenarios.

`max_new_items` is a ceiling, not a quota.
Any overdue review, required method change, or zero effective new-card budget sets `max_new_items = 0`.
When lesson evidence contradicts the pre-lesson plan, the tutor can choose another action and must record the resulting evidence and summary.

Recognized optional profile preferences:
- `max_new_items_per_lesson`: integer cap;
- `preferred_difficulty`: `supported`, `balanced`, or `stretch`;
- `priority_scenarios`: ordered list of scenario IDs.

`estimated_level` is context only, not the planning authority. Actual production/listening/transfer evidence wins.


## 9. Item capability projection and concrete task selection

The runtime derives an item-level learner model from immutable evidence. It is a projection, not a persisted second source of truth.

For each learning item it maintains separate estimates for:
- recognition;
- listening;
- prompted production;
- spontaneous production;
- transfer.

Each estimate uses bounded per-item history and calendar-time decay.

Tracked fields:

- weighted success rate;
- unaided success rate;
- response latency;
- evidence strength;
- qualified-success streak;
- contexts;
- context families.

Evidence weight decays with age.
Missing assistance does not count as unaided evidence.
`listening_attempts` is the listening source used by the projection.

The planner then ranks concrete `TaskCandidate` actions such as:
- `diagnostic_spontaneous`;
- `fade_prompts`;
- `spontaneous_retrieval`;
- `listening_probe`;
- `transfer_practice`;
- `automaticity_drill`.

`TaskCandidate` and tutor-focus state are planner inputs.
The runtime merges them into one bounded `tutor_tasks` queue.
Each task contains action, capability, priority, due/stage, context policy, rationale, and success criterion.
The planner uses deterministic Rust rules; it does not use embeddings or a reinforcement-learning policy.

## 10. Spaced tutor-focus threads

Problematic capabilities remain in a derived tutor-focus queue until consolidated. The queue applies Anki-like spacing to live tutoring without duplicating the card scheduler.

Spacing stages use consecutive **qualified** successes after the last failure.
Prompted production requires `assistance_level > 0`.
Spontaneous production requires `assistance_level = 0`.
Transfer requires `assistance_level = 0` and `context_family`:

| Stage | Next tutor probe |
|---|---:|
| 0 | immediate / current lesson |
| 1 | +1 day |
| 2 | +3 days |
| 3 | +7 days |
| 4 | +14 days |

A failure or unqualified latest attempt resets the capability to stage 0.
`candidate` learning items do not enter tutor focus.
A thread is consolidated at these thresholds:

- listening: weighted success ≥ 0.85 and evidence strength ≥ 0.65;
- spontaneous production: weighted success ≥ 0.85, unaided success ≥ 0.80, evidence strength ≥ 0.70, qualified-success streak ≥ 2;
- transfer: weighted success ≥ 0.80, unaided success ≥ 0.80, at least 3 successful context families, evidence strength ≥ 0.65, qualified-success streak ≥ 2.

Focus threads expose successful contexts, context families, and next-context policy.
The runtime merges focus threads and candidate scores into `tutor_tasks`.
A task with `due_now = true` is scheduled before new material, after method-change and error-recheck priorities.

## 11. Planner history and calibration

At actual lesson start, the runtime persists one `planner_decisions` snapshot containing:
- planner version;
- planner input signals;
- adaptive plan;
- item capability projection;
- ranked task candidates;
- tutor-focus threads.

Reading tutor context does not write state.
Lesson completion writes the observed outcome to the existing planner row: lesson metrics, evidence, errors, mastery proposals, scenario, and next goal.

Tutor context includes bounded `planner_history` and `intervention_effectiveness`.
`intervention_effectiveness` compares consecutive snapshots only after the configured minimum delay and only when the target item has newer evidence.
It reports sample count, newly observed count, improved/stable/declined counts, and mean success delta by action and capability.
It does not change thresholds.

The calibration chain is: `decision before lesson → intervention → delayed observable learner-state change`.
