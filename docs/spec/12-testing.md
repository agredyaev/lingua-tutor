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

Duplicate submission (idempotent); offline replay; immediate undo (compensating event); stale prior-state hash → 409; concurrent submission; event/state atomicity; history immutability; corrected-event recalculation.

## 3. Tutor cards

Idempotent create; exact duplicate detection; near-duplicate warning; cosmetic edit preserves schedule; semantic edit resets/replaces; split lineage; merge lineage; activation budget enforcement; invalid ambiguous prompt rejected; missing `creation_reason` rejected; destructive mutation requires confirmation.

## 4. Adaptive rules (`adaptation`)

Table-driven: low recall; backlog; repeated listening failure; prompted/spontaneous mismatch; 3-occurrence mini-target; 5-occurrence method change; redesign candidates; mastery evidence.

## 5. Pack conformance (shared suite, `test-support`)

Every pack: manifest validity; contract-version compatibility; deterministic normalization; bounded answer evaluation; stable error codes; card-template validation; no secret/network access; asset schema validation; historical version attribution.

Core CI runs the suite against `synthetic-test-pack` — proves core independence from Spanish.

## 6. Migrations

Apply all migrations to scratch SQLite in CI; schema matches [04-database.md](04-database.md); foreign keys + unique constraints verified.

## 7. End-to-end (Playwright)

Lesson-result import creates cards; MCP-equivalent lesson completion (service-layer path); daily review; offline review sync; dashboard update; card suspend + restore; full export + restore.

## 8. Performance targets (one learner)

- Dashboard API < 500 ms normal load.
- Review event response < 300 ms online.
- Next-card transition feels immediate (queue prefetched client-side).
- Offline review independent of server latency.
- Due-card query uses `ix_schedule_due`.
- Bounded review batches.
- Capacity: tens of thousands of review events, several thousand cards, no redesign.
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
