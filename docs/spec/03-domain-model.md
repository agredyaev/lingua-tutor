# 03 — Data Model and Processing Invariants

Scope: language-neutral data and processing rules. Persistence is defined by [04-database.md](04-database.md).

## 1. Data-oriented model

The runtime does not reconstruct retained domain objects. SQLite rows are projected directly into transport values or bounded processing columns. Hot processing state uses flat arrays/SoA layouts and dense `u32` indices; sparse relationships use indexed ranges/CSR-style offsets where multiplicity requires them.

External and persisted IDs are opaque strings at protocol/SQLite boundaries. Once data enters a processing pass, relationships use validated dense indices rather than object references or pointer-linked graphs.

Behavior is expressed as explicit functions over scalar values, arrays, and buffers. Storage structs do not own behavior.

## 2. Learner and language profile

`learner_profile` stores global policy such as timezone, session targets, card budget and desired retention.

`learner_language_profiles` scopes all learning state for one target locale and source/explanation language. Its language fields are:

- `source_language`: language used for prompts/explanations;
- `target_locale`: target BCP-47 locale;
- `estimated_level`: optional level label supplied/updated through the learning flow.

There is at most one active profile per `(learner_id, target_locale)`. A profile is the sole language identity for lessons, items, errors, cards, review state and recommendations.

## 3. Lessons and learning items

A lesson belongs to one profile and carries lifecycle state, scenario/goal, measurements and summary fields. A learning item belongs to one profile and stores the exact target text plus a technical canonical text used for duplicate lookup.

Technical canonicalization may trim/collapse whitespace. It must not implement target-language morphology, accent stripping, stemming, punctuation tolerance or grammatical rules.

Learning item types and statuses are closed enums in `learning-core`.

## 4. Evidence and mastery

Evidence belongs to a learning item and may optionally reference a lesson. Evidence kinds are closed and language-neutral: recognition, listening, prompted production, spontaneous production, transfer and correction.

`metadata_json` stores bounded measurement metadata such as assistance level and semantic context. Semantic identifiers such as `context_family` are tutor-provided opaque strings; core compares identity but does not interpret their linguistic meaning. Operational independence requires observed assistance metadata; unknown assistance is retained for audit but cannot increase independent-evidence strength. Item-level success/unaided rates are recency- and episode-weighted rather than lifetime raw ratios. Successful context/context-family diversity is also time-bounded, and the latest independent outcome for a given key supersedes older outcomes.

Mastery rules are deterministic methodology:

- card review success alone does not prove mastery;
- guided `probe_phase=practice` evidence is acquisition/repair performance and cannot grant provisional or full mastery;
- provisional mastery requires fresh successful unaided productive evidence from a mastery-eligible non-practice probe in the current lesson;
- full mastery requires at least two distinct successful transfer context families from at least two independent prior lesson episodes, at least three distinct successful semantic context families overall, and a fresh unaided `probe_phase=transfer` success with a non-empty `context_family` in the current lesson; an unresolved fresh spontaneous-production failure blocks full mastery even if transfer later succeeds, while a later qualifying spontaneous success may resolve that capability failure;
- prior transfer history used for full mastery is bounded to the current 84-day revalidation window, excludes `probe_phase=practice`, and uses the latest eligible transfer exit-observation per prior lesson episode; a later failed/unqualified transfer invalidates an earlier success from that episode, and older context diversity must be revalidated;
- multiple fresh context families inside one current lesson cannot replace distributed historical transfer evidence;
- a newer failed transfer blocks promotion until later qualifying evidence supersedes it.

## 5. Errors

An error target belongs to one profile. Core owns only the closed `CoreErrorCategory`; `error_code` is an optional tutor-provided detail code and is opaque to methodology.

Deterministic escalation rules use occurrence/evidence counts, not language-specific heuristics:

- three occurrences create focused recheck pressure;
- five unresolved occurrences require method change;
- rechecks are scheduled at +1, +3 and +7 days.

## 6. Cards and review

Cards belong to one profile and store prompt/answer content, accepted answers, intent, status and provenance. Scheduling state is separate from card content. Review events are immutable; undo/correction is represented by compensation rather than mutation of history.

Review and adaptation behavior is implemented by explicit processing passes over bounded indexed storage.

## 7. Recommendations

Recommendations target language-neutral subjects: learning item, error target, card, listening item, review load, or language profile. They are deterministic outputs and never directly execute content changes.
