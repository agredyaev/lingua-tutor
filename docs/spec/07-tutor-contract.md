# 07 — Tutor Contract

Scope: `tutor-contract` + `adaptation` crates. Lesson-result schema, card mutations, card types, quality rules, adaptive rules.
Depends on: [03-domain-model.md](03-domain-model.md), [04-database.md](04-database.md), [05-review-engine.md](05-review-engine.md), [06-language-boundary.md](06-language-boundary.md).

## 1. Tutor role

Tutor is primary author + maintainer of cards. Learner never required to: create cards, pick phrases, copy to another tool, maintain tags/scheduling, dedupe, redesign. Learner may suspend/restore/retire cards via UI — optional controls only. Card-content changes stay Tutor-owned and use lesson card mutations.

## 2. Lesson result

Versioned block returned by tutor (paste-import or MCP). Applied atomically with idempotency key.

```json
{
  "schema_version": "2",
  "client_operation_id": "op_01J...",
  "language_profile_id": "llp_01J...",
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
               "task_success_rubric": "goal_completed_independently_v2",
               "confidence": 0.6 },
  "new_learning_items": [ { "client_item_key": "new-1", "item_type": "phrase",
                            "target_text": "…", "prompt_text": "…", "prompt_language": "<source_language>" } ],
  "evidence": [
    { "client_item_key": "new-1", "kind": "spontaneous_production",
      "success": true, "context": "pharmacy:ask-for-remedy",
      "context_family": "service.ask_help",
      "assistance_level": 0, "response_millis": 1800,
      "probe_phase": "diagnostic",
      "pronunciation_intelligible": true,
      "pronunciation_repeat_required": false, "detail": null },
    { "client_item_key": "new-1", "kind": "spontaneous_production",
      "success": true, "context": "pharmacy:post-fade",
      "assistance_level": 0, "response_millis": 1500,
      "probe_phase": "practice",
      "pronunciation_intelligible": true,
      "pronunciation_repeat_required": false, "detail": null }
  ],
  "interventions": [ { "action": "fade_prompts", "client_item_key": "new-1",
                       "target_capability": "spontaneous_production",
                       "evidence_index": 1 } ],
  "error_occurrences": [ { "error_target_id": null,
                           "core_category": "grammar", "communicative_impact": "medium",
                           "error_code": "verb.copula",
                           "original_production": "…", "corrected_production": "…",
                           "explanation": "…" } ],
  "error_rechecks": [ { "error_target_id": "err_01J...",
                         "recheck_id": "rc_01J...", "passed": true,
                         "assistance_level": 0,
                         "context": "pharmacy:recheck-copula",
                         "repair_applied": false,
                         "repair_action": null } ],
  "recommendation_resolutions": [
    { "recommendation_id": "rec_01J...", "status": "accepted" }
  ],
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

Validation is server-side, schema- and size-bounded. The LLM-facing `template` is a safe submit skeleton: optional write arrays are empty by default. Object shapes live separately in `shape_examples`; those rows are documentation only and must never be copied into a result unless the corresponding observation/action actually occurred. Schema v2 makes lesson measurement part of completion rather than an optional annotation. Every selected evidence/listening probe must carry observed `assistance_level`, `probe_phase`, and an explicit context (`context` for evidence, `context_tag` for listening). `response_millis` is optional retrieval-latency telemetry measured from probe readiness to response onset/first committed response, not total utterance or text-composition duration. It is supplied only when that latency was actually instrumented; missing or incompatible timing cannot be fabricated. Lesson completion does not require a fixed probe quota. Calibration eligibility requires at least one measured `diagnostic` baseline and at least two measurement-eligible probes overall. `diagnostic`, `delayed_retrieval`, and `transfer` count toward lesson-level measurement coverage; `practice` does not. This coverage count is not a count of independent learning episodes. A lesson with lower coverage is persisted with `calibration_valid=false` and is excluded from intervention-effectiveness attribution. Do not add a probe merely to satisfy the calibration threshold when it cannot affect the teaching decision; if an observation is needed but cannot be measured reliably, run another measurable probe instead of inventing a value. When productive speech is actually audible to the tutor, production/transfer probes additionally record `pronunciation_intelligible` and `pronunciation_repeat_required` as a pair. In text-only or otherwise non-audible interaction both stay absent; pronunciation must never be inferred from text. Concrete recurring pronunciation problems remain normal `error_occurrences`. Existing items use `learning_item_id`; same-result items may use `client_item_key`. Transfer additionally requires a stable tutor-assigned semantic `context_family`. It is an opaque language-neutral identifier, not a vocabulary entry: new values use canonical lowercase ASCII letters/digits plus `._-`, with an alphanumeric first and last character (for example `service.ask_help`). The runtime does not own a semantic whitelist, but non-canonical legacy values fail closed for transfer qualification/mastery so case or formatting drift cannot manufacture context diversity. `assistance_level=0` means explicitly unaided; assisted success is `prompted_production`, never `spontaneous_production` or `transfer`.

For recurring `error_occurrences`, the Tutor should reuse `error_target_id` from context. If that ID is accidentally omitted, the runtime may reuse an existing unresolved target only on an exact profile-local signature match `(core_category, error_code, trimmed original_production, trimmed corrected_production)`; there is no fuzzy merge and resolved targets are not silently reopened. An explicitly supplied stale ID that already points to a resolved target is rejected fail-closed; a later relapse starts a new target/history instead of inheriting the old lifetime occurrence count. This prevents LLM omission from splitting one active recurrence history while keeping relapse policy explicit.

`error_rechecks` records outcomes for due 1/3/7-day rechecks supplied in tutor context. Every scheduled recheck is an explicitly unaided measurement (`assistance_level=0`) in a concrete communicative `context`; the runtime verifies that the recheck belongs to the supplied error target and language profile and is due before accepting it. Each exposed due row also carries `last_repair_action` when a prior named repair exists, derived from persisted recheck or accepted recommendation audit history, so method-cycling protection survives direct compatibility writes without a planner snapshot. A failed recheck logically returns the target to active practice and closes the failed cycle. The tutor must then apply corrective repair in the same lesson, set `repair_applied=true`, and provide a stable short `repair_action` code naming the repair actually used; the runtime then starts a fresh 1/3/7-day series and persists the target back in `recheck` state, without leaving a duplicate `practice_again` recommendation. Once `method_change_required` becomes true for an unresolved target it remains sticky across later recurrences. That escalation supersedes weaker open repair recommendations such as `activate_mini_target`/legacy `practice_again`; while the sticky flag is set, only an actually accepted `require_method_change` repair may start the next retention series, and its canonical `repair_action` must differ from the most recent persisted named repair for that target. The sticky flag is a sticky repair constraint, not a command to reteach before each successful recheck: profile-level method-change pressure and generic recurring-error repair pressure are active only while the target is in an active repair state (`active`, with legacy `escalated` treated likewise); `recheck` state measures retention first and must not independently emit `repair_recurring_errors`. Only the next unfinished stage in a cycle is actionable. A later stage requires the previous stage to have passed and the corresponding elapsed day gap since that actual completion; if a stage is completed late, remaining due times shift forward rather than allowing catch-up attempts in the same session. Because a scheduled recheck is a retention measurement, global lesson difficulty is capped at `balanced` for this primary objective (`supported` remains allowed by learner preference); `stretch` must not confound the recheck with unrelated task complexity. A target becomes `resolved` only when all three spaced rechecks in one cycle pass; resolution then clears `method_change_required`. Recheck outcomes are applied atomically with the lesson result, so the canonical MCP flow does not need a separate maintenance write tool. A target resolved by the runtime to a scheduled `error_recheck` must not also appear as an ordinary `error_occurrence` in the same result: the two arrays have no shared temporal order, so such double-accounting is rejected fail-closed rather than guessing whether it was one event or a later recurrence.

Open recommendations form a bounded FIFO work queue: the oldest unresolved recommendations are exposed first so continual new recommendations cannot starve older work. `recommendation_resolutions` closes only recommendations explicitly addressed in the current lesson. `accepted` means the recommendation was actually acted on. The runtime requires same-item evidence/listening/intervention activity before accepting `practice_in_lesson`, and an Applied non-noop mutation of the same card before accepting `redesign_card`; an accepted error-target repair additionally requires a bounded non-empty `repair_action` naming the method actually applied. That action is retained both in the recommendation audit detail and in planner outcome history so later tutors can avoid method cycling. `dismissed` means it was explicitly evaluated and judged inapplicable. Recommendations merely present in tutor context remain `open`. Read-time planner/Tutor projections additionally fail closed on legacy stale rows: learning-item recommendations require an actionable item state, error-target repair recommendations require an active/escalated target, and card recommendations require an active review lifecycle state. The runtime verifies profile ownership and open state, applies resolution atomically with lesson completion, and rejects unknown/foreign/already-resolved IDs. `expired` is reserved for deterministic runtime cleanup when the underlying condition becomes stale or resolved (for example, a resolved error target, an open `redesign_card` recommendation whose card is suspended/retired, or `practice_in_lesson` for a suspended learning item); Tutor does not emit it. This keeps the canonical MCP flow closed without restoring a separate recommendation-maintenance write tool.

`mastery_changes` may move an existing learning item among active learning/mastery states or suspend it, but must not assign the creation-only `candidate` state; candidate items enter the system only through the new-item admission path. A lesson result may contain at most one mastery change for each `learning_item_id`; conflicting same-result transitions are rejected rather than resolved by JSON array order. `interventions` records only pedagogically significant methods actually applied in the lesson. A planned `task_candidate` is not an intervention. Each intervention targets one item and capability and may point at one explicit immediate evidence/listening outcome. The referenced immediate outcome must be post-intervention and therefore cannot have `probe_phase=diagnostic`. Diagnostic observations remain baseline candidates only when they precede the referenced outcome in the same ordered evidence/listening array. Import flow: validate → preview → apply ([08-local-rpc.md](08-local-rpc.md) §2).

### Communicative task success rubric

When `task_attempt_count > 0`, `task_success_rubric` is mandatory. The canonical
planner metric is `goal_completed_independently_v2`.

One task attempt is one explicitly assigned, meaning-focused communicative goal with an observable outcome
(for example: obtain the bill, clarify a destination, request an item). The outcome must be more than production of a predetermined form: `say/repeat X`, substitution drills, and other form-only exercises are practice/evidence but do not increment `task_attempt_count`. Under v2, count
success only when the learner completes that **same original goal** without the tutor
supplying/modeling the target utterance **and without a lexical/grammatical cue that supplies
the target language needed for completion**, and without relaxing, replacing, or materially
simplifying the goal. Natural self-correction, requests for repetition/clarification, and
interlocutor repair remain valid when the learner still constructs the target-language
solution independently. Source-language or gesture-only workarounds do not count as v2
success when they replace target-language material required by the assigned language task;
non-linguistic resources may supplement, but not substitute for, that required L2 solution.

`goal_completed_without_target_model_v1` remains accepted for historical/backward
compatibility, but planner aggregation does not mix v1 counts with the stronger v2
construct. Speaking time, grammatical perfection, confidence, or repetition after a model
do not by themselves count as communicative-task success.

Within one lesson result, evidence array order is observation order. For transfer mastery,
the latest transfer attempt is decisive: a later failed/unqualified transfer blocks mastery,
while a later unaided successful transfer supersedes an earlier failure. Persisted evidence
stores `observation_order` so attempts sharing the same timestamp retain this semantics.

Every `listening_attempt` is item-level evidence: it references exactly one existing or same-result learning item and carries a non-empty `source_reference` for the audio/live source actually presented. A live/TTS source may use a stable local marker when no external asset ID exists; text-only interaction must not create listening evidence. For listening variability, `source_reference` is only a content reference and never proves a distinct acoustic/context variant. Variation is counted only from explicit `source_variant_id`, `talker_key`, and/or `context_tag` metadata.

## 3. Card mutations

Operations: `create`, `update`, `add_accepted_answer`, `remove_accepted_answer`, `suspend`, `restore`, `retire`.

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
    "prompt_language": "<source_language>",
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

`prompt_language` comes from the learner language profile; there is no compiled language-specific default. Every attempted mutation → `card_mutations` audit row, applied or not.

### Auto-apply rules

Mutation applies automatically only when ALL hold: the requested operation is supported; lesson completion is explicitly submitted by the tutor; schema + quality rules pass; daily activation limit is respected; no duplicate conflict. Status changes and content edits stay visible in lesson-result preview / MCP write confirmation. Semantic target changes are rejected as `update`; express them as an explicit new card plus suspend/retire of the old card.

## 4. Card quality rules

Every card must: test one primary recall target; have answerable unambiguous prompt; prefer situational prompt over literal translation; obey tutor-provided naturalness + regional rules for the profile target locale; include accepted alternatives when genuinely valid; avoid multiple unrelated unknowns; carry enough context for register; record `creation_reason`; link to source evidence; avoid duplicate skill testing unless evidence shows modality gap; contain plain text or strictly sanitized markup. A phrase appearing in a lesson is NOT sufficient reason for a card.

### Evidence sources

New high-value phrase; failed retrieval; recurring error; text-recognized-but-audio-missed; understood-but-not-produced; produced-only-with-hint; expected real scenario; useful correction replacing unnatural form; learned phrase needing transfer.

### Candidate scoring

Factors: real-life utility; frequency in expected situations; failure recurrence; communicative impact; transfer value; current priority; novelty; overlap with existing cards; cognitive load; review backlog. `priority_score` stored; deterministic rules control activation.

### Activation budget

Tutor may propose unlimited candidates; app activates ≤ daily new-card budget (default 8/day). Over budget or backlog limits → cards stay `candidate`.

### Duplicate prevention

Compare before create: normalized answer; communicative function; intent; linked phrase; linked error target; active accepted answers. Exact retry (same `client_operation_id`) → idempotent no-op. Near-duplicate → tutor warning or redesign recommendation, never silent create.

### Edit classification

| Class | Definition | Schedule effect |
|---|---|---|
| cosmetic | punctuation, typo, formatting | preserved |
| clarifying | better prompt, same tested answer | preserved; version recorded |
| semantic | changed recall target or answer meaning | reject `update`; create a new card and explicitly suspend/retire the old one |

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
3. Tutor proposes clarification, smaller replacement cards, modality change, or suspension using supported mutations.
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
| High-impact error recurrence = 2 lesson episodes | activate mini-target; contrast card when appropriate |
| Medium-impact error recurrence = 3 lesson episodes | activate mini-target; contrast card when appropriate |
| Low-impact error recurrence = 4 lesson episodes | activate mini-target; contrast card when appropriate |
| Unresolved high-impact recurrence = 4 lesson episodes | require method change |
| Unresolved medium-impact recurrence = 5 lesson episodes | require method change |
| Unresolved low-impact recurrence = 6 lesson episodes | require method change |
| Explicitly unaided transfer in at least 3 distinct semantic `context_family` values overall, with at least 2 distinct successful families from at least 2 temporally distributed prior lesson episodes inside the 84-day revalidation window plus a fresh unaided `probe_phase=transfer` success with non-empty `context_family` now, and no unresolved fresh spontaneous-production failure; distinct lesson IDs inside the anti-massing gap do not count as separate mastery confirmations | eligible for full mastery |
| Failed 1/3/7-day recheck | target back to active practice |
| Persistent `Hard` with high response time | redesign before frequency increase |
| High review success but missing/failed spontaneous evidence | diagnose or return to unaided contextual production; absence of conversation evidence never implies low utility |

## 8. Adaptive lesson planner

Before each lesson the runtime derives a language-neutral plan from persisted evidence rather than copying the previous tutor goal.

Inputs:
- review pressure: due and significantly overdue review items;
- unresolved errors, due 1/3/7-day rechecks, and method-change targets;
- recent bounded lesson signals from the last 28 days, capped to the 5 most recent completed lessons: confidence as diagnostic context, episode-balanced explicitly unaided spontaneous-production success, and v2 independent communicative-task attempts/success; profile-wide spontaneous weakness can request a discriminating prompt-independence probe but cannot establish prompt dependence because prompted and spontaneous evidence may belong to different items; actual prompt dependence is a same-item `tutor_tasks` decision from paired capability evidence; transfer remains a separate item-level capability and cannot lower the profile spontaneous-production signal; speaking share remains persisted lesson telemetry but is intentionally excluded from adaptive `recent_signals`; decision rates are balanced per lesson so one massed lesson cannot dominate another independent episode; positive spontaneous-production and strong-rubric task history can authorize `stretch` only when each relevant history spans the current anti-massing gap—two high-scoring lesson IDs inside that gap remain diagnostic evidence but do not prove durable readiness;
- open adaptive recommendations;
- learner preferences.

Profile-level planning deliberately excludes aggregate listening score and aggregate answer latency from decision authority. Listening weakness and retrieval automaticity are selected from item-level measured evidence in `tutor_tasks`, where assistance, accuracy, context variation, evidence strength, and latency can be interpreted together.

The deterministic profile-level order is: sufficiently sampled independent communicative-task failure → required method change → due error rechecks → sufficiently established spontaneous-production weakness requiring diagnosis → sparsely sampled communicative-task failure → recurring errors → open evidence-based recommendations → sparse spontaneous-production diagnostic → new high-utility material. FSRS card backlog is not a standalone Tutor-lesson objective: the separate review workflow owns the concrete bounded review queue and grading path, while due/overdue review pressure remains a workload signal that throttles new-item capacity and can block `stretch`. Prompt dependence itself is intentionally absent from this profile-level order and is established only in same-item `tutor_tasks`. While communicative-task outcomes are stored only as lesson-level aggregates, raw within-lesson `task_attempt_count` is telemetry/rationale rather than evidence-strength authority. A v2 task result becomes the stronger profile-level signal only after at least three task-bearing lesson episodes; one or two such episodes remain evidence but do not override a sufficiently established spontaneous-production diagnostic. This avoids treating many same-lesson attempts as independent evidence; a future explicit task-attempt log may support finer-grained weighting.

Due error rechecks are bounded maintenance obligations, not an unbounded competing curriculum. Tutor context exposes the oldest actionable rechecks with a v1 workload cap: at most 1 when another objective is primary, and at most 3 when `run_error_rechecks` is the primary objective. For an active lesson, that cap is derived from the persisted lesson-start primary objective even if the live adaptive context later recomputes to a different objective; Tutor workload and completion validation therefore share one contract. Exposed rows are the maintenance obligations for that lesson; additional due rows remain persisted for later lessons rather than being catch-up tested in the same session. For a live lesson created by `start_lesson`, completion fails closed if any recheck that belonged to that bounded block at lesson start is omitted from `error_rechecks`. A recheck that only becomes due after the lesson has started does not retroactively become a completion requirement.

The planner returns:
- primary objective;
- supported/balanced/stretch difficulty;
- maximum new items for this lesson;
- ranked actions with evidence-based rationales;
- diagnostic probes;
- success criteria;
- recent signal summary.

`difficulty` controls scaffolding and extraneous task complexity, not the identity of the communicative goal or the success rubric. `supported` may reduce irrelevant complexity, slow the interaction, add non-answer context, or allow more repair after the initial unaided attempt; it must not pre-supply the target utterance, replace the original goal, or materially simplify the goal and then count that easier task as success. `stretch` may increase contextual/interactional complexity only when evidence is strong enough; it must not be used to contaminate a scheduled retention recheck.

The planner does not choose curriculum topics or scenario priorities. Before a canonical live lesson starts, the tutor must supply a non-empty `scenario` and `planned_goal`; the goal is fixed before teaching so later communicative-task outcomes are not defined post hoc. Lesson-level v2 task metrics without a persisted preplanned goal may be retained as telemetry/import history but have no strong adaptive task-outcome authority. The tutor chooses that direction from the live conversation plus persisted learner history, then records the observed scenario, next goal, evidence, errors, learning items/cards, and interventions. Those persisted records are the context for later lessons.

The new-item count is a ceiling, never a quota. For a lesson created through `start_lesson`, `complete_lesson` re-checks the persisted planner snapshot transaction-locally and rejects any result that would actually create more learning items than `max_new_items`; items deduplicated to existing material do not consume the cap. While that lesson remains active, a recomputed live tutor context may become stricter or change its diagnostic objective, but its exposed `max_new_items` is capped by the persisted start-of-lesson value so the read contract cannot authorize a write that completion will reject. Backlog pressure is graded: small overdue sets reduce the ceiling; serious backlog sets it to zero. A required method change sets the ceiling to zero. A sufficiently established communicative-task failure or sufficiently established spontaneous-production weakness suppresses optional expansion but retains at most one new-item slot so the tutor can teach one bottleneck-specific phrase/frame when a discriminating probe shows that missing L2 material is the actual cause; this repair slot must not be treated as `introduce_high_utility_material`. For established-bottleneck repair slots the runtime also requires each genuinely new item to be the target of an actually recorded intervention through the same `client_item_key`; unrelated optional expansion is rejected while deduplicated existing material remains allowed. Sparse diagnostic signals do not automatically zero the ceiling. Same-item prompt dependence from `tutor_tasks` deterministically suppresses optional expansion even though it is not represented as a profile-level scalar diagnosis: after item-level selection, `fade_prompts` reduces the effective `max_new_items` to 0 in both the persisted lesson snapshot and live tutor context. Optional expansion resumes only after live evidence shows that the bottleneck no longer constrains the task. The tutor may deviate from the plan when live evidence contradicts it, but the deviation must be reflected in lesson evidence and summary.

Recognized optional profile preferences:
- `max_new_items_per_lesson`: integer cap;
- `preferred_difficulty`: `supported`, `balanced`, or `stretch`.

Curriculum direction is not a profile preference or compiled list. It is tutor-owned and becomes durable only through persisted lesson/evidence state.

`estimated_level` is context only, not the planning authority. Actual production/listening/transfer evidence wins.


## 9. Item capability projection and concrete task selection

The runtime derives an item-level learner model from immutable evidence. It is a projection, not a persisted second source of truth.

For each learning item it maintains separate estimates for:
- recognition;
- listening;
- prompted production;
- spontaneous production;
- transfer.

Each estimate uses a bounded per-item history and calendar-time decay. It tracks weighted success, explicitly unaided success, retrieval latency, evidence strength, independent evidence episodes, qualified-success streak, concrete contexts, and semantic context families. Evidence strength, weighted success, and explicitly unaided success are derived from independent lesson/learning episodes with recency decay rather than lifetime raw counts; `probe_phase=practice` remains persisted acquisition/repair evidence but is excluded from operational capability confidence and profile-level unaided-recall authority. The storage scan first reduces each explicit lesson episode to its newest non-practice exit-observation and only then applies the bounded-history limit, so massed or guided-practice rows cannot evict independent episodes. Unknown assistance never counts as independence or evidence strength. Successful contexts/families are also time-bounded evidence: only fresh qualified independent outcomes count, and the newest independent outcome for a context/family is authoritative, so a later failed/unqualified observation invalidates an older success until a newer qualifying success restores it. Retrieval latency is aggregated only from qualified independent successes; failures or assisted successes cannot inflate the automaticity estimate. Automaticity work requires both recency-weighted accuracy and explicitly unaided accuracy of at least 80%, sufficient evidence strength, a distributed qualified-success streak, and retrieval latency measured in at least two independent qualified episodes. Measured listening evidence is canonical when it exists; legacy listening rows remain historical attempts but cannot manufacture unaided/context-diversity evidence. Retrieval-latency policy is language-neutral and must not lengthen the threshold merely because `target_text` contains more words. For spontaneous-production intervention, fewer than three independent episodes remain diagnostic rather than established state. The runtime also retains raw recency/order-weighted evidence mass, explicitly unaided evidence mass/episode count, and latency-measurement episode count. Before committing to a threshold-driven action, it checks decision fragility: when one fresh independent success or failure can move the relevant weighted rate across that action boundary, a discriminating probe has positive value of information and is preferred. Prompt dependence requires sufficiently established evidence on both the prompted and spontaneous sides. Listening uses the same distinction: uncertain or decision-fragile state emits `listening_probe`, while a sufficiently established independent listening gap emits `listening_repair` rather than repeated diagnosis. The 200-item projection reserves a bounded revalidation share for mastered items. Revalidation age is driven by the stalest relevant focus capability among listening, spontaneous production, and transfer; a fresh recognition/prompted observation cannot hide stale productive/listening evidence, and a missing relevant capability remains stale. Only unresolved failures in those focus capabilities receive scarce repair priority. Any such failure remains repair-prioritized until that same capability receives later qualifying evidence: success in a different capability cannot mask it, and assisted/unqualified success cannot clear an independent failure. The same unresolved-failure priority applies within the bounded active-learning share so an older failed item cannot be starved merely by newer item updates. Once all relevant focus capabilities have fresh qualified evidence, the mastered item rotates behind older revalidation candidates; unused reserve capacity falls back to active learning items.

Transfer admission uses one shared spontaneous-production readiness boundary for both `transfer_practice` candidates and the spaced transfer-focus thread. The tutor must not receive unscheduled transfer practice before the same state is strong enough to open transfer spacing/context tracking; the exact readiness cutoffs remain v1 policy heuristics.

The planner then ranks concrete `TaskCandidate` actions such as:
- `diagnostic_spontaneous`;
- `fade_prompts`;
- `spontaneous_retrieval`;
- `listening_probe`;
- `listening_repair`;
- `transfer_practice`;
- `automaticity_drill`.

`TaskCandidate` and tutor-focus state are internal planner inputs. They are merged into one bounded tutor-facing `tutor_tasks` queue containing action, capability, priority, due/stage, context policy, rationale, and success criterion. Before global top-K competition, alternative actions are reduced to one best candidate per item+capability so one bottleneck cannot consume several scarce candidate slots. A due item-level focus adds a bounded priority bonus and wins ties, but `due_now` is not an absolute veto over a materially higher-priority diagnosed repair; item-level revalidation/retrieval obligations outrank optional expansion, not an established bottleneck. Capacity pressure must not change decision semantics: when focus state still requires a spontaneous diagnostic, a missing candidate row falls back to `diagnostic_spontaneous`, not retrieval practice. `probe_prompt_independence` explicitly boosts that matching diagnostic task. Live tutor context and lesson-start planner snapshots use the same canonical selection capacities so persisted history is reproducible. The tutor follows this single queue; no embeddings, opaque model score, or reinforcement-learning policy is required.

## 10. Spaced tutor-focus threads

Problematic capabilities remain in a derived tutor-focus queue until consolidated. The queue applies Anki-like spacing to live tutoring without duplicating the card scheduler.

Spacing stages use consecutive **qualified, temporally distributed** successes after the last failure. Known lesson IDs define observational episodes: multiple qualified attempts inside one lesson can stabilize performance but advance at most one spacing stage. A different lesson ID does not by itself prove spacing; counted spacing stages must also be separated by the current 12-hour anti-massing gap. Distinct lesson episodes inside that gap may still contribute separately to episode-weighted accuracy/evidence, but cannot advance multiple spacing stages. The 12-hour gap is a v1 policy heuristic, not a universal learning constant. Productive/listening success qualifies only when explicitly unaided where independence is required; transfer additionally requires a semantic `context_family`:

| Stage | Next tutor probe |
|---|---:|
| 0 | immediate / current lesson |
| 1 | +1 day |
| 2 | +3 days |
| 3 | +7 days |
| 4 | +14 days |

A failure or unqualified latest attempt resets the capability to stage 0. `candidate` learning items never enter tutor focus. The thread remains active until evidence is strong enough for the capability:
- listening: strong repeated **independent** comprehension;
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
- tutor-focus threads;
- the final bounded `tutor_tasks` queue returned to the tutor. Repeated `start_lesson` calls for the same open lesson return this persisted snapshot rather than recomputing a new decision from fresher state; the response also repeats the persisted `scenario` and `planned_goal` so retry arguments cannot silently appear to replace the pre-teaching framing. `snapshot_complete=true` means the exact final `tutor_tasks` queue was persisted. A migrated legacy open lesson whose historical row predates that field returns `snapshot_complete=false`, preserves its persisted plan/framing, and returns `tutor_tasks=null` rather than fabricating an authoritative empty queue.

Reading tutor context alone has no side effects. On lesson completion the same row receives the observed outcome: lesson metrics, measured evidence/listening probes, actual interventions, errors, mastery proposals, scenario, and next goal.

The next tutor receives a compact bounded `planner_history`, plus `intervention_effectiveness`. Calibration reads actual interventions from the completed lesson outcome, never `task_candidates_json`. It compares consecutive snapshots only after the current 6-hour minimum-delay policy and only when newer evidence exists for the targeted item. Lessons without the required diagnostic measurement coverage are excluded. Each intervention is calibration-eligible only when a qualified diagnostic baseline for the same learning item and target capability occurs strictly before its explicit non-diagnostic immediate outcome and the persisted outcome records `calibration_eligible=true`; legacy outcomes missing that field fail closed. Recognition/listening/spontaneous/transfer baselines require `assistance_level=0`; prompted-production baselines require observed assistance `>0`. Listening intervention calibration additionally requires the referenced post-intervention listening outcome to be explicitly fresh relative to its qualifying diagnostic baseline by at least one of `source_variant_id`, `talker_key`, or `context_tag`; repetition of the same listening variant remains valid practice performance but receives no intervention-effectiveness credit. Transfer intervention calibration likewise requires the post-intervention transfer outcome to use a non-empty `context_family` different from the qualifying diagnostic transfer baseline; repetition inside the same semantic family is practice, not evidence that the intervention generalized. Unrelated, assisted-independent-capability, or post-outcome diagnostics cannot confer causal credit. If multiple different interventions target the same item/capability in one lesson, that outcome is excluded from per-action effectiveness attribution as confounded. The summary reports samples/newly-observed/improved/stable/declined and mean success delta by action/capability; it does not auto-tune thresholds. One delayed sample is preliminary observational evidence only; the tutor should not switch method solely from one sample. The v1 policy requires at least two independent delayed samples with a consistently weak/negative direction before treating an action as probably ineffective, unless live learner evidence in the current lesson already demands an immediate method change.

The calibration chain is: `decision before lesson → qualified same-target diagnostic → actual intervention → explicit post-intervention outcome → genuinely delayed observable learner-state change`. The numeric before-state remains the pre-lesson capability snapshot, while the same-lesson diagnostic establishes temporal/target eligibility rather than replacing that aggregate state with a noisy single probe. Persisted calibration outcomes carry the intervention-lesson completion timestamp. A same-lesson diagnostic or any observation earlier than the current minimum delay after completion cannot consume the pending effectiveness sample; the intervention remains pending until the first later non-practice observation of that item/capability qualifies. If another intervention targets the same item/capability before such an observation appears, the older pending attribution is superseded and receives no causal credit. Legacy outcomes without an explicit completion timestamp fail closed. This is observational longitudinal evidence, not a causal experiment. One comparable sample is a hypothesis-generating signal only; repeated comparable samples can be used as a tie-breaker between methods. Immediate live failure and an explicit `method_change_required` remain sufficient reasons to change method without waiting for repeated calibration samples.


## 13. Methodology calibration boundaries

Exact capability half-lives, the 28-day recent-signal window, its 5-lesson cap, the 84-day mastery revalidation window, the 6-hour intervention-calibration delay, the 10-decision bounded calibration-history window, minimum episode counts, evidence-strength thresholds, tutor-focus intervals, error escalation counts, due-recheck per-lesson workload caps, and the three-family mastery threshold are explicit v1 policy heuristics. They are deterministic and regression-tested, but are not universal scientific constants. Longitudinal calibration must compare these policy predictions against later unaided communicative-task and transfer outcomes before changing them. A policy-level A/B change additionally requires a predeclared primary outcome, control/baseline, unit of analysis, minimally important effect, adequate sample/power plan and stopping rule; acceptance considers effect size plus uncertainty/confidence interval, statistical significance and practical significance. Per-learner `intervention_effectiveness` is observational evidence and never substitutes for that experiment-level acceptance gate.

`communicative_impact` makes recurring-error escalation task-sensitive: high-impact errors escalate earlier, medium-impact errors retain the 3/5 defaults, and low-impact errors require more recurrence. Recurrence advances at most once per lesson episode for one error target; repeated raw sightings within the same lesson are retained as observations but do not manufacture extra recurrence. Crossing the escalation threshold opens an active repair recommendation but does not yet create a retention cycle. The initial 1/3/7-day series starts only when that repair recommendation is explicitly resolved as `accepted`, meaning the repair was actually applied. An ordinary recurrence during `recheck` cancels the pending cycle and returns the target to active repair before any fresh series can start. The 1/3/7-day error recheck cadence remains a v1 heuristic subject to calibration. Future interval calibration should be evaluated against the intended retention horizon and later unaided outcomes rather than searching for one universally optimal spacing interval.

Lesson-level `confidence` and speaking share are diagnostic context only. Raw lesson-level listening score and average answer latency remain persisted telemetry but are intentionally omitted from adaptive `recent_signals`. Listening decisions use item-level independent evidence; response latency is interpreted only at item/capability level and can trigger automaticity work only after independent accuracy (≥80%), evidence strength (≥0.65), and a distributed qualified-success streak are already sufficient. The exact cutoffs remain v1 policy heuristics.

`intervention_effectiveness` is intentionally item/intervention-level observational evidence consumed together with `tutor_tasks`; it is not collapsed into the profile-level planner score, because doing so would discard the item/capability identity and overstate weak causal evidence.
