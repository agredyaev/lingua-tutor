# 05 — Review Engine

Scope: `review-engine` crate. Scheduler trait, algorithm selection, states, grades, queue, events, backlog protection. Language-neutral: no knowledge of Spanish, grammar, or vocabulary.
Depends on: [03-domain-model.md](03-domain-model.md), [04-database.md](04-database.md).

## 1. Scheduler trait

Application-owned. Third-party algorithm crate = implementation detail behind this trait; its types never leave `review-engine`.

```rust
pub trait ReviewScheduler {
    fn initial_state(&self, input: InitialReviewInput) -> Result<ScheduledState, ScheduleError>;

    fn apply_review(
        &self,
        current: &ScheduledState,
        event: &ReviewEventInput,
        settings: &SchedulerSettings,
    ) -> Result<ScheduledState, ScheduleError>;

    fn predicted_retrievability(
        &self,
        state: &ScheduledState,
        at: Timestamp,
    ) -> Result<f64, ScheduleError>;
}
```

## 2. Algorithm selection

- Primary scheduler: FSRS-compatible, with difficulty, stability, and retrievability state.
- Adoption requires deterministic outputs, persisted algorithm/version state, and passing golden/property tests.
- ADR-0004 retains the historical WASM spike. `review-engine` is not a browser dependency.
- If the FSRS gate fails, use `FallbackScheduler` behind the same trait. Preserve immutable review data.
- Persist per state: `algorithm`, `algorithm_version`, `parameter_set_id`. Never invalid, negative, or non-finite intervals.

## 3. Card states

```text
candidate → new → learning → review ⇄ relearning
                        any → suspended → (restore)
                        any → retired
```

`candidate`: tutor proposal, not introduced (budget, duplication, or quality validation pending). Full transition rules: tutor mutations ([07-tutor-contract.md](07-tutor-contract.md)) + review results.

## 4. Grades

| Grade | Value | Meaning |
|---|---|---|
| `Again` | 1 | Not recalled correctly before reveal |
| `Hard` | 2 | Correct after hesitation or self-correction |
| `Good` | 3 | Correct after at most a short pause, without answer reveal |
| `Easy` | 4 | Immediate correct answer without self-correction |

UI must state: `Hard` is a passing grade; after forgetting, use `Again` ([09-frontend.md](09-frontend.md) §4).

## 5. Learning flow

1. Tutor introduces phrase in lesson.
2. Learner performs immediate retrieval in lesson.
3. Card due in second daily session or next practical window.
4. After first delayed successful recall, scheduler owns due dates.
5. Failed review → `relearning`.

No configurable learning-step subsystem.

## 6. Settings and defaults

```rust
pub struct SchedulerSettings {
    pub desired_retention: f64,        // 0.90
    pub max_new_cards_per_day: u8,     // 8
    pub target_review_minutes: (u8, u8), // (8, 12)
    pub max_review_batch: u16,         // 50
    pub max_interval_days: u32,        // conservative, several years
    pub timezone: Tz,                  // Europe/Madrid default
}
```

Raising desired retention raises workload; UI must show the consequence, not encourage increases.

## 7. Review events

`review_events` rows are immutable.
Each grade writes the event and `card_schedule_state` in one transaction.
`client_operation_id` is unique. A duplicate submission returns the original result and writes no row.

`prior_state_hash` and `result_state_hash` are deterministic hashes of `ScheduledState`.
They support offline conflict detection, reproducibility tests, and audit.
A stale `prior_state_hash` returns a conflict error; the client refetches state.

Undo: only the immediately previous grade; compensating event with `compensates_event_id` set + recalculated state. Never deletes.

## 8. Due queue

Order:

1. relearning cards;
2. significantly overdue review cards;
3. due review cards by predicted forgetting risk (descending);
4. due same-day learning cards;
5. new cards within remaining daily budget.

Batch size is at most `max_review_batch`.
If another due card is available, do not place near-duplicates consecutively.
The due-card query must use `ix_schedule_due`.

## 9. Backlog protection

- Daily review time > 12 min for 2 consecutive study days → reduce new-card budget.
- Overdue count > configured threshold → pause new cards.
- Active recall < 70% for 2 consecutive study days → new cards −50%.
- Backlog cleared → restore budget gradually, not instantly.

## 10. Ineffective-card detection

A card becomes a redesign candidate when any configured condition matches: repeated `Again` grades in the rolling window; response time above the redesign threshold with repeated `Hard` grades; repeated failure while related cards succeed; learner-reported ambiguous prompt; conflict with another card; or review success with repeated conversational failure.

The detector emits recommendations; it does not apply mutations.
Actions: clarify prompt; add context; add accepted answer; split; switch modality; add contrast; suspend; replace with role-play practice.

## 11. Parameter sets

- v1 ships reviewed defaults in `scheduler_parameter_sets` (`origin='default'`).
- Constraints when implemented: documented minimum sample threshold; ignore invalid/corrected events; run off request path; retain previous set; dry-run comparison; no mass rescheduling by default; rollback supported.
