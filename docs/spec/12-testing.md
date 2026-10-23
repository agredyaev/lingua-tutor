# 12 — Testing

Scope: test suites per subsystem, acceptance commands, performance targets.
Depends on: all component files 03–09.

## 1. Scheduler (`review-engine`)

- Deterministic golden vectors per grade (Again/Hard/Good/Easy).
- Initial scheduling; lapse + relearning transitions; overdue + early review; timezone + day-boundary cases.
- Algorithm-version persistence; parameter-set changes; state-hash reproducibility.
- Fallback scheduler passes the same suite.
- Property tests: no invalid, negative, or non-finite intervals.

## 2. Review events

Test cases:

- duplicate submission and idempotent result;
- offline replay;
- immediate undo through a compensating event;
- stale prior-state hash → 409;
- concurrent submission;
- event/state atomicity;
- immutable event history;
- corrected-event recalculation.

## 3. Tutor cards

Test cases:

- idempotent create;
- exact duplicate detection;
- near-duplicate warning;
- cosmetic edit preserves schedule;
- semantic edit resets or replaces schedule state;
- split and merge lineage;
- activation budget enforcement;
- ambiguous prompt rejection;
- missing `creation_reason` rejection;
- confirmation for destructive mutation.

## 4. Adaptive rules (`adaptation`)

Table-driven cases cover:

- unaided recall below threshold;
- overdue review backlog;
- repeated listening failure;
- prompted/spontaneous mismatch;
- 3-occurrence mini-target;
- 5-occurrence method change;
- redesign thresholds;
- mastery evidence.

## 5. Pack conformance (shared suite, `test-support`)

Every pack test suite checks:

- manifest validity;
- contract-version compatibility;
- deterministic normalization;
- bounded answer evaluation;
- stable error codes;
- card-template validation;
- no secret or network access;
- asset schema validation;
- historical version attribution.

Core CI runs the suite against `synthetic-test-pack` — proves core independence from Spanish.

## 6. Migrations

Apply all migrations to scratch SQLite in CI; schema matches [04-database.md](04-database.md); foreign keys + unique constraints verified.

## 7. End-to-end (Playwright)

Playwright covers:

- lesson-result import and card creation;
- MCP-equivalent lesson completion through the shared handler path;
- daily review;
- offline review sync;
- dashboard update;
- card suspend and restore;
- full export and restore.

## 8. Performance targets (one learner)

- Dashboard operation target: < 500 ms. A performance result is invalid unless it records the dataset size and query parameters.
- Review operation target: < 300 ms. A performance result is invalid unless it records the dataset size and operation parameters.
- Next-card transition reads from the prefetched client queue; it must not wait for a network round-trip.
- Offline review independent of server latency.
- Due-card query uses `ix_schedule_due`.
- Bounded review batches.
- Capacity claims require a recorded event count, card count, and measured operation latencies.
- Compressed frontend payload ≤ 3 MB (CI-reported).

## 9. Acceptance commands

```bash
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
cd apps/web && trunk build --release --cargo-profile wasm-release
cd ../..
cargo build -p lingua-tutor-local --bin lingua-tutor --release
```

## 10. Architecture acceptance checks

- No dependency from generic core crates to `language-pack-es-es`.
- No `spanish_*` fields or types in core.
- `review-engine` tests pass with synthetic pack or no pack.
- es-ES pack passes conformance suite.
- Incompatible pack contract version fails explicitly at startup.
- No empty architectural directories.
