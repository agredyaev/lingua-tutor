# 07 — Tutor Contract

Scope: `tutor-contract` + `adaptation` crates. Lesson-result schema, card mutations, card types, quality rules, adaptive rules.
Depends on: [03-domain-model.md](03-domain-model.md), [04-database.md](04-database.md), [05-review-engine.md](05-review-engine.md), [06-language-packs.md](06-language-packs.md).

## 1. Tutor role

Tutor is primary author + maintainer of cards. Learner never required to: create cards, pick phrases, copy to another tool, maintain tags/scheduling, dedupe, redesign. Learner may edit/suspend/restore/delete via UI — optional controls only.

## 2. Lesson result

Versioned block returned by tutor (paste-import or MCP). Applied atomically with idempotency key.

```json
{
  "schema_version": "2",
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
               "avg_answer_millis": 2100, "long_pause_count": 2,
               "task_attempt_count": 3, "task_success_count": 2,
               "task_success_rubric": "goal_completed_without_target_model_v1",
               "confidence": 0.6 },
  "new_learning_items": [ { "client_item_key": "new-1", "item_type": "phrase",
                            "target_text": "…", "prompt_text": "…", "prompt_language": "ru" } ],
  "evidence": [ { "client_item_key": "new-1", "kind": "spontaneous_production",
                  "success": true, "context": "pharmacy:ask-for-remedy",
                  "context_family": "service.ask_help",
                  "assistance_level": 0, "response_millis": 1800,
                  "probe_phase": "diagnostic",
                  "pronunciation_intelligible": true,
                  "pronunciation_repeat_required": false, "detail": null } ],
  "interventions": [ { "action": "fade_prompts", "client_item_key": "new-1",
                       "target_capability": "spontaneous_production",
                       "evidence_index": 0 } ],
  "error_occurrences": [ { "error_target_id": null,
                           "core_category": "grammar", "communicative_impact": "medium",
                           "pack_error_code": "es.verb.ser_estar",
                           "original_production": "…", "corrected_production": "…",
                           "explanation": "…" } ],
  "listening_attempts": [ { "learning_item_id": "phr_01J...", "success": false,
                            "assistance_level": 0, "response_millis": 1500,
                            "probe_phase": "diagnostic",
                            "source_reference": "lesson-audio-3", "source_variant_id": "v3",
                            "talker_key": "voice-b", "context_tag": "pharmacy",
                            "transcript": "…" } ],
  "mastery_changes": [ { "learning_item_id": "phr_01J...", "new_status": "provisionally_mastered" } ],
  "card_mutations": [ { "operation": "create", "client_operation_id": "op_01J...x", "card": { } } ]
}
```

Validation is server-side, schema- and size-bounded. Schema v2 makes lesson measurement part of completion rather than an optional annotation. Every selected evidence/listening probe must carry observed `assistance_level`, `response_millis`, `probe_phase`, and an explicit context (`context` for evidence, `context_tag` for listening); lesson completion requires at least two measured `diagnostic` probes. If a probe cannot be measured reliably, run another measurable probe instead of inventing a value. Productive probes additionally record `pronunciation_intelligible` and `pronunciation_repeat_required`; concrete recurring pronunciation problems remain normal `error_occurrences`. Existing items use `learning_item_id`; same-result items may use `client_item_key`. Transfer additionally requires a pack-owned canonical `context_family`. `assistance_level=0` means explicitly unaided; assisted success is `prompted_production`, never `spontaneous_production` or `transfer`.

`interventions` records only pedagogically significant methods actually applied in the lesson. A planned `task_candidate` is not an intervention. Each intervention targets one item and capability and may point at one immediate evidence/listening outcome. Import flow: validate → preview → apply ([08-local-rpc.md](08-local-rpc.md) §2).

### Communicative task success rubric

When `task_attempt_count > 0`, `task_success_rubric` is mandatory and must be
`goal_completed_without_target_model_v1`.

One task attempt is one explicitly assigned communicative goal with an observable outcome
(for example: obtain the bill, clarify a destination, request an item). Count success only
when the learner completes that **same original goal** without the tutor supplying/modeling
the target utterance before completion and without relaxing, replacing, or materially
simplifying the goal. Natural self-correction, clarification, and interlocutor repair are
allowed if the learner still achieves the original goal.

Speaking time, grammatical perfection, confidence, or repetition after a model do not by
themselves count as communicative-task success. Planner aggregation ignores task counts
from rows that do not carry this exact rubric ID.

Within one lesson result, evidence array order is observation order. For transfer mastery,
the latest transfer attempt is decisive: a later failed/unqualified transfer blocks mastery,
while a later unaided successful transfer supersedes an earlier failure. Persisted evidence
stores `observation_order` so attempts sharing the same timestamp retain this semantics.

For listening variability, `source_reference` is only a content reference and never proves
a distinct acoustic/context variant. Variation is counted only from explicit
`source_variant_id`, `talker_key`, and/or `context_tag` metadata.

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
| Explicitly unaided transfer in 3 distinct canonical semantic `context_family` values, and the latest observed transfer is not a failure (or is superseded by a new unaided successful transfer) | eligible for full mastery |
| Failed 1/3/7-day recheck | target back to active practice |
| Persistent `Hard` | redesign before frequency increase |
| High review success + low real-life utility | consider retiring card |

## 8. Adaptive lesson planner

Before each lesson the runtime derives a language-neutral plan from persisted evidence rather than copying the previous tutor goal.

Inputs:
- review pressure: due items, mature reviews ranked by predicted forgetting risk, effective new-card budget;
- unresolved errors, due 1/3/7-day rechecks, and method-change targets;
- recent lesson metrics over a bounded window: speaking ratio, unaided-vs-aided recall, listening score, answer latency, confidence;
- open adaptive recommendations;
- learner preferences.

The deterministic priority order is: required method change → due error rechecks → prompt dependence → listening weakness → slow retrieval → low confidence → insufficient learner speaking → recurring errors → significant review backlog → open evidence-based recommendations → due retrieval → new high-utility material.

The planner returns:
- primary objective;
- supported/balanced/stretch difficulty;
- maximum new items for this lesson;
- ranked actions with evidence-based rationales;
- diagnostic probes;
- success criteria;
- recent signal summary;
- priority scenarios.

The new-item count is a ceiling, never a quota. Backlog pressure is graded: small overdue sets reduce the ceiling; serious backlog or a required method change sets it to zero. The tutor may deviate from the plan when live evidence contradicts it, but the deviation must be reflected in lesson evidence and summary.

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

Each estimate uses a bounded per-item history and calendar-time decay. It tracks weighted success, explicitly unaided success, latency, evidence strength, qualified-success streak, concrete contexts, and semantic context families. Old evidence loses strength automatically; unknown assistance never counts as independence. `listening_attempts` is the canonical listening signal to prevent double counting.

The planner then ranks concrete `TaskCandidate` actions such as:
- `diagnostic_spontaneous`;
- `fade_prompts`;
- `spontaneous_retrieval`;
- `listening_probe`;
- `transfer_practice`;
- `automaticity_drill`.

`TaskCandidate` and tutor-focus state are internal planner inputs. They are merged into one bounded tutor-facing `tutor_tasks` queue containing action, capability, priority, due/stage, context policy, rationale, and success criterion. The tutor follows this single queue; no embeddings, opaque model score, or reinforcement-learning policy is required.

## 10. Spaced tutor-focus threads

Problematic capabilities remain in a derived tutor-focus queue until consolidated. The queue applies Anki-like spacing to live tutoring without duplicating the card scheduler.

Spacing stages use consecutive **qualified** successes after the last failure. Productive success qualifies only when assistance is explicitly appropriate; transfer additionally requires a semantic `context_family`:

| Stage | Next tutor probe |
|---|---:|
| 0 | immediate / current lesson |
| 1 | +1 day |
| 2 | +3 days |
| 3 | +7 days |
| 4 | +14 days |

A failure or unqualified latest attempt resets the capability to stage 0. `candidate` learning items never enter tutor focus. The thread remains active until evidence is strong enough for the capability:
- listening: strong repeated comprehension;
- spontaneous production: strong explicitly unaided production;
- transfer: strong explicitly unaided production in at least three distinct semantic context families.

Internal focus threads expose successful contexts/families and a next-context policy. They are merged with candidate scoring into `tutor_tasks`; `due_now=true` tasks are retrieval obligations before optional new material, subject to error-method-change priorities.

## 11. Planner history and calibration

At actual lesson start, the runtime persists one `planner_decisions` snapshot containing:
- planner version;
- planner input signals;
- adaptive plan;
- item capability projection;
- ranked task candidates;
- tutor-focus threads.

Reading tutor context alone has no side effects. On lesson completion the same row receives the observed outcome: lesson metrics, measured evidence/listening probes, actual interventions, errors, mastery proposals, scenario, and next goal.

The next tutor receives a compact bounded `planner_history`, plus `intervention_effectiveness`. Calibration reads actual interventions from the completed lesson outcome, never `task_candidates_json`. It compares consecutive snapshots only after a minimum delay and only when newer evidence exists for the targeted item. Lessons without the required diagnostic measurement coverage are excluded. The summary reports samples/newly-observed/improved/stable/declined and mean success delta by action/capability; it does not auto-tune thresholds.

The calibration chain is: `decision before lesson → actual intervention → delayed observable learner-state change`.


## 13. Methodology calibration boundaries

Exact capability half-lives, evidence-strength thresholds, tutor-focus intervals, error escalation counts, and the three-family mastery threshold are explicit v1 policy heuristics. They are deterministic and regression-tested, but are not universal scientific constants. Longitudinal calibration must compare these policy predictions against later unaided communicative-task and transfer outcomes before changing them.

`communicative_impact` makes recurring-error escalation task-sensitive: high-impact errors escalate earlier, medium-impact errors retain the 3/5 defaults, and low-impact errors require more recurrence. The 1/3/7-day error recheck cadence remains a v1 heuristic subject to calibration.

Lesson-level `confidence`, speaking share, and aggregate latency are diagnostic context. They do not outrank objective communicative-task success, item-level capability evidence, due rechecks, or recurring errors.

`intervention_effectiveness` is intentionally item/intervention-level evidence consumed together with `tutor_tasks`; it is not collapsed into the profile-level planner score, because doing so would discard the item/capability identity needed to decide which teaching method actually worked.
