# 07 — Tutor Contract

Scope: `tutor-contract` + `adaptation` crates. Lesson-result schema, card mutations, card types, quality rules, adaptive rules.
Depends on: [03-domain-model.md](03-domain-model.md), [04-database.md](04-database.md), [05-review-engine.md](05-review-engine.md), [06-language-boundary.md](06-language-boundary.md).

## 1. Tutor role

Tutor is primary author + maintainer of cards. Learner never required to: create cards, pick phrases, copy to another tool, maintain tags/scheduling, dedupe, redesign. Learner may edit/suspend/restore/delete via UI — optional controls only.

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

Validation is server-side, schema- and size-bounded. Schema v2 makes lesson measurement part of completion rather than an optional annotation. Every selected evidence/listening probe must carry observed `assistance_level`, `response_millis`, `probe_phase`, and an explicit context (`context` for evidence, `context_tag` for listening); lesson completion requires at least one measured `diagnostic` baseline and at least two independent measured probes overall. `diagnostic`, `delayed_retrieval`, and `transfer` count toward independent measurement coverage; `practice` does not. If a probe cannot be measured reliably, run another measurable probe instead of inventing a value. Productive probes additionally record `pronunciation_intelligible` and `pronunciation_repeat_required`; concrete recurring pronunciation problems remain normal `error_occurrences`. Existing items use `learning_item_id`; same-result items may use `client_item_key`. Transfer additionally requires a stable tutor-assigned semantic `context_family`; the runtime stores and compares the opaque identifier but does not own a vocabulary or whitelist. `assistance_level=0` means explicitly unaided; assisted success is `prompted_production`, never `spontaneous_production` or `transfer`.

`interventions` records only pedagogically significant methods actually applied in the lesson. A planned `task_candidate` is not an intervention. Each intervention targets one item and capability and may point at one explicit immediate evidence/listening outcome. The referenced immediate outcome must be post-intervention and therefore cannot have `probe_phase=diagnostic`. Diagnostic observations remain baseline candidates only when they precede the referenced outcome in the same ordered evidence/listening array. Import flow: validate → preview → apply ([08-local-rpc.md](08-local-rpc.md) §2).

### Communicative task success rubric

When `task_attempt_count > 0`, `task_success_rubric` is mandatory. The canonical
planner metric is `goal_completed_independently_v2`.

One task attempt is one explicitly assigned communicative goal with an observable outcome
(for example: obtain the bill, clarify a destination, request an item). Under v2, count
success only when the learner completes that **same original goal** without the tutor
supplying/modeling the target utterance **and without a lexical/grammatical cue that supplies
the target language needed for completion**, and without relaxing, replacing, or materially
simplifying the goal. Natural self-correction, requests for repetition/clarification, and
interlocutor repair remain valid when the learner still constructs the target-language
solution independently.

`goal_completed_without_target_model_v1` remains accepted for historical/backward
compatibility, but planner aggregation does not mix v1 counts with the stronger v2
construct. Speaking time, grammatical perfection, confidence, or repetition after a model
do not by themselves count as communicative-task success.

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

Mutation applies automatically only when ALL hold: non-destructive; lesson completion explicitly submitted by tutor; schema + quality rules pass; daily activation limit respected; no duplicate conflict. Suspension, merge, semantic reset, retirement stay visible in lesson-result preview / MCP write confirmation.

## 4. Card quality rules

Every card must: test one primary recall target; have answerable unambiguous prompt; prefer situational prompt over literal translation; obey tutor-provided naturalness + regional rules for the profile target locale; include accepted alternatives when genuinely valid; avoid multiple unrelated unknowns; carry enough context for register; record `creation_reason`; link to source evidence; avoid duplicate skill testing unless evidence shows modality gap; contain plain text or strictly sanitized markup. A phrase appearing in a lesson is NOT sufficient reason for a card.

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
| Explicitly unaided transfer in at least 3 distinct semantic `context_family` values overall, with at least 2 distinct successful families from at least 2 independent prior lesson episodes inside the 84-day revalidation window plus a fresh unaided `probe_phase=transfer` success with non-empty `context_family` now, and no unresolved fresh spontaneous-production failure | eligible for full mastery |
| Failed 1/3/7-day recheck | target back to active practice |
| Persistent `Hard` with high response time | redesign before frequency increase |
| High review success but missing/failed spontaneous evidence | diagnose or return to unaided contextual production; absence of conversation evidence never implies low utility |

## 8. Adaptive lesson planner

Before each lesson the runtime derives a language-neutral plan from persisted evidence rather than copying the previous tutor goal.

Inputs:
- review pressure: due and significantly overdue review items;
- unresolved errors, due 1/3/7-day rechecks, and method-change targets;
- recent bounded lesson signals from the last 28 days: speaking ratio and confidence as diagnostic context, unaided-vs-aided recall, and v2 independent communicative-task attempts/success; decision rates are balanced per lesson so one massed lesson cannot dominate another independent episode;
- open adaptive recommendations;
- learner preferences.

Profile-level planning deliberately excludes aggregate listening score and aggregate answer latency from decision authority. Listening weakness and retrieval automaticity are selected from item-level measured evidence in `tutor_tasks`, where assistance, accuracy, context variation, evidence strength, and latency can be interpreted together.

The deterministic profile-level order is: required method change → due error rechecks → sufficiently sampled independent communicative-task failure → prompt dependence → sparsely sampled communicative-task failure → recurring errors → significant review backlog → open evidence-based recommendations → due retrieval → new high-utility material. A v2 task result becomes the stronger profile-level signal after at least three sampled attempts; one or two attempts remain evidence but do not override an established prompt-dependence signal.

The planner returns:
- primary objective;
- supported/balanced/stretch difficulty;
- maximum new items for this lesson;
- ranked actions with evidence-based rationales;
- diagnostic probes;
- success criteria;
- recent signal summary.

The planner does not choose curriculum topics or scenario priorities. The tutor chooses the current direction from the live conversation plus persisted learner history, then records the observed scenario, next goal, evidence, errors, learning items/cards, and interventions. Those persisted records are the context for later lessons.

The new-item count is a ceiling, never a quota. Backlog pressure is graded: small overdue sets reduce the ceiling; serious backlog or a required method change sets it to zero. The tutor may deviate from the plan when live evidence contradicts it, but the deviation must be reflected in lesson evidence and summary.

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

The planner then ranks concrete `TaskCandidate` actions such as:
- `diagnostic_spontaneous`;
- `fade_prompts`;
- `spontaneous_retrieval`;
- `listening_probe`;
- `listening_repair`;
- `transfer_practice`;
- `automaticity_drill`.

`TaskCandidate` and tutor-focus state are internal planner inputs. They are merged into one bounded tutor-facing `tutor_tasks` queue containing action, capability, priority, due/stage, context policy, rationale, and success criterion. Before global top-K competition, alternative actions are reduced to one best candidate per item+capability so one bottleneck cannot consume several scarce candidate slots. Capacity pressure must not change decision semantics: when focus state still requires a spontaneous diagnostic, a missing candidate row falls back to `diagnostic_spontaneous`, not retrieval practice. `probe_prompt_independence` explicitly boosts that matching diagnostic task. Live tutor context and lesson-start planner snapshots use the same canonical selection capacities so persisted history is reproducible. The tutor follows this single queue; no embeddings, opaque model score, or reinforcement-learning policy is required.

## 10. Spaced tutor-focus threads

Problematic capabilities remain in a derived tutor-focus queue until consolidated. The queue applies Anki-like spacing to live tutoring without duplicating the card scheduler.

Spacing stages use consecutive **qualified, distributed** successes after the last failure. Known lesson IDs define practice episodes: multiple qualified attempts inside one lesson can stabilize performance but advance at most one spacing stage. Different lesson episodes may advance separate stages even on the same day. Legacy evidence without episode identity uses a 12-hour anti-massing fallback. Productive/listening success qualifies only when explicitly unaided where independence is required; transfer additionally requires a semantic `context_family`:

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
- tutor-focus threads.

Reading tutor context alone has no side effects. On lesson completion the same row receives the observed outcome: lesson metrics, measured evidence/listening probes, actual interventions, errors, mastery proposals, scenario, and next goal.

The next tutor receives a compact bounded `planner_history`, plus `intervention_effectiveness`. Calibration reads actual interventions from the completed lesson outcome, never `task_candidates_json`. It compares consecutive snapshots only after a minimum delay and only when newer evidence exists for the targeted item. Lessons without the required diagnostic measurement coverage are excluded. Each intervention is calibration-eligible only when a qualified diagnostic baseline for the same learning item and target capability occurs strictly before its explicit non-diagnostic immediate outcome and the persisted outcome records `calibration_eligible=true`; legacy outcomes missing that field fail closed. Recognition/listening/spontaneous/transfer baselines require `assistance_level=0`; prompted-production baselines require observed assistance `>0`. Unrelated, assisted-independent-capability, or post-outcome diagnostics cannot confer causal credit. If multiple different interventions target the same item/capability in one lesson, that outcome is excluded from per-action effectiveness attribution as confounded. The summary reports samples/newly-observed/improved/stable/declined and mean success delta by action/capability; it does not auto-tune thresholds. One delayed sample is preliminary observational evidence only; the tutor should not switch method solely from one sample. The v1 policy requires at least two independent delayed samples with a consistently weak/negative direction before treating an action as probably ineffective, unless live learner evidence in the current lesson already demands an immediate method change.

The calibration chain is: `decision before lesson → qualified same-target diagnostic → actual intervention → explicit post-intervention outcome → delayed observable learner-state change`. The numeric before-state remains the pre-lesson capability snapshot, while the same-lesson diagnostic establishes temporal/target eligibility rather than replacing that aggregate state with a noisy single probe. This is observational longitudinal evidence, not a causal experiment. One comparable sample is a hypothesis-generating signal only; repeated comparable samples can be used as a tie-breaker between methods. Immediate live failure and an explicit `method_change_required` remain sufficient reasons to change method without waiting for repeated calibration samples.


## 13. Methodology calibration boundaries

Exact capability half-lives, evidence-strength thresholds, tutor-focus intervals, error escalation counts, and the three-family mastery threshold are explicit v1 policy heuristics. They are deterministic and regression-tested, but are not universal scientific constants. Longitudinal calibration must compare these policy predictions against later unaided communicative-task and transfer outcomes before changing them.

`communicative_impact` makes recurring-error escalation task-sensitive: high-impact errors escalate earlier, medium-impact errors retain the 3/5 defaults, and low-impact errors require more recurrence. The 1/3/7-day error recheck cadence remains a v1 heuristic subject to calibration.

Lesson-level `confidence` and speaking share are diagnostic context only. Raw lesson-level listening score and average answer latency remain persisted telemetry but are intentionally omitted from adaptive `recent_signals`. Listening decisions use item-level independent evidence; response latency is interpreted only at item/capability level and can trigger automaticity work only after independent accuracy (≥80%), evidence strength (≥0.65), and a distributed qualified-success streak are already sufficient. The exact cutoffs remain v1 policy heuristics.

`intervention_effectiveness` is intentionally item/intervention-level observational evidence consumed together with `tutor_tasks`; it is not collapsed into the profile-level planner score, because doing so would discard the item/capability identity and overstate weak causal evidence.
