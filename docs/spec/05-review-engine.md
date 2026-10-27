# 05 — Review Engine

Scope: `review-engine` plus the native review queue/write passes. Language-neutral: no Spanish, grammar, or vocabulary rules.
Depends on: [03-domain-model.md](03-domain-model.md), [04-database.md](04-database.md).

## 1. Processing model

Scheduling behavior is implemented as explicit functions over persisted scalar state. There is no scheduler behavior trait or retained scheduler object graph.

Stable request-local algorithm IDs:

- `FSRS_ID = 0`;
- `FALLBACK_ID = 1`.

Core passes:

```rust
initial_state(id, at)
apply_review(id, scratch, state, event, settings)
predicted_retrievability(id, state, at)
replay(id, activated_at, events, settings)
state_hash(state)
```

The FSRS crate is a request-local computation resource inside `review-engine`; its types never cross the crate boundary. Persisted state remains application-owned scalars.

## 2. Algorithm selection

- Preferred algorithm: FSRS-6 via pinned `fsrs = 6.6.1`.
- Persist `algorithm` and exact `algorithm_version`; unavailable persisted versions fail closed rather than silently changing schedules.
- Deterministic fallback remains available under its own stable integer ID.
- Invalid, non-finite, negative, or zero intervals are rejected/clamped by the shared interval pass.
- Golden/property/replay tests are the algorithm migration gate.

v1 uses the reviewed FSRS default parameter vector. `parameter_set_id` is retained in storage for future validated optimization, but v1 does **not** claim personalized FSRS parameters.

## 3. Card states

```text
candidate → new → learning → review ⇄ relearning
                        any → suspended → (restore)
                        any → retired
```

`candidate` is a tutor proposal awaiting quality/budget admission. A first grade moves `new → learning`; a passing learning/relearning grade moves to `review`; `Again` from review moves to `relearning`.

## 4. Grades

| Grade | Value | Meaning |
|---|---:|---|
| `Again` | 1 | Not recalled correctly before reveal |
| `Hard` | 2 | Correct, but substantial effort / hesitation / minor self-correction |
| `Good` | 3 | Correct with normal effort |
| `Easy` | 4 | Immediate, confident, fully correct |

`Hard` is a passing grade. Forgetting is `Again`.

## 5. Learning flow

1. Tutor introduces material and obtains an immediate retrieval attempt in the lesson.
2. Accepted card candidates are admitted only within the effective new-card budget.
3. A newly admitted card is due immediately for its first review opportunity.
4. After graded retrieval, FSRS owns the next due interval.
5. Failed mature review enters `relearning`.

There is no configurable multi-step learning/relearning subsystem. Short-term scheduling remains inside FSRS rather than duplicating Anki-style fixed steps.

## 6. Settings and profile policy

```rust
pub struct SchedulerSettings {
    pub desired_retention: f64,
    pub max_new_cards_per_day: u8,
    pub target_review_minutes: (u8, u8),
    pub max_review_batch: u16,
    pub max_interval_days: u32,
    pub timezone: String,
}
```

Runtime policy is loaded from the persisted learner profile for every grading/replay path:

- `desired_retention`: profile value, default 0.90; runtime accepts only 0.70–0.97 to prevent pathological workload/retention settings;
- `max_new_cards_per_day`: profile value, default 8;
- review-minute ceiling: profile value, default 12;
- timezone: profile value;
- `max_review_batch`: 50;
- `max_interval_days`: 36,500 (100 years).

Changing desired retention changes future FSRS intervals; historical review events are not rewritten.

## 7. Review events and replay

Review events are immutable. Grade submission inserts the event and updates `card_schedule_state` in one transaction.

`client_operation_id` is unique: duplicate submission returns the original result without another write.

`prior_state_hash` / `result_state_hash` provide deterministic conflict detection and audit. A stale prior hash is rejected.

Undo only compensates the immediately previous grade. It never deletes history: the effective event stream is replayed with the **current persisted profile scheduling policy**, and a compensating event records the lineage.

## 8. Due queue

Bounded queue: at most `max_review_batch`.

Priority:

1. due `relearning`;
2. due mature `review` cards ordered by **ascending predicted retrievability** (lowest recall probability / highest forgetting risk first);
3. due `learning`;
4. `new` cards within the remaining effective daily budget, oldest admitted first.

Due times still determine eligibility. Once mature cards are due, predicted forgetting risk is the ranking signal; an arbitrary separate “significantly overdue” tier is not used.

## 9. Backlog protection

The effective daily new-card budget is derived from real persisted history:

- overdue review/relearning count > 50 → pause new cards;
- review time > the profile's max_review_minutes on each of the two previous study days → halve;
- active-recall success < 70% on each of the two previous study days → halve;
- both pressure rules may stack;
- after throttling/pausing, recovery is gradual: at most yesterday's effective budget + 1.

Review time comes from completed `review_sessions`; recall comes from effective non-undone `review_events`. `review_daily_budget` persists one scalar effective budget per profile/local day solely to preserve recovery hysteresis. The remaining budget subtracts the appropriate same-day consumption counter.

## 10. Ineffective-card detection

A card becomes a redesign candidate on evidence such as repeated `Again`, repeated high-latency `Hard`, or strong review performance with real-life production failure.

Actions are recommendations, not silent semantic mutations: clarify prompt, add context/accepted answer, split, switch modality, suspend, or replace repeated card drilling with live transfer practice.

## 11. Parameter sets

`scheduler_parameter_sets` is a forward-compatible persistence boundary.

Current v1 behavior:

- FSRS uses `fsrs::DEFAULT_PARAMETERS`;
- schedule state normally has no active optimized `parameter_set_id`;
- no personalized-parameter claim is made.

A future optimizer must have its own acceptance gate: minimum history threshold, corrected/invalid event exclusion, off-request execution, dry-run comparison, retained previous set, explicit activation, rollback, and no automatic mass rescheduling.
