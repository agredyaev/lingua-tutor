# ADR-0004: FSRS spike — GO on fsrs-rs 6.6.1

Date: 2026-07-12. Status: accepted; architecture wording updated 2026-10-23 after DOD/PWP migration. WASM-specific consequence superseded by ADR-0006.

## Evidence

Candidates: `fsrs` 6.6.1 vs `rs-fsrs` 1.2.1. The spike established deterministic scheduler behavior and acceptable dependency weight.

| Gate | Result |
|---|---|
| wasm32 compile (historical topology) | Passed; no longer an active scheduler runtime requirement |
| Binary size (historical spike) | 38 KB raw / 12.5 KB gzip for scheduler-only use |
| Determinism | Identical outputs across runs for recorded vectors |

## Decision

Pin `fsrs = "6.6.1"` and use FSRS-6 defaults for v1.

The original scheduler-trait implementation was removed by the DOD/PWP refactor. The decision's algorithmic content remains, but dispatch is now explicit:

- `FSRS_ID` / `FALLBACK_ID` are stable request-local integer selectors;
- `initial_state`, `apply_review`, `predicted_retrievability`, and `replay` are free processing passes;
- FSRS scratch exists only for the duration of a processing stream;
- third-party FSRS types never leave `review-engine`;
- persisted state records `algorithm` and `algorithm_version = "fsrs-rs-6.6.1"`.

The deterministic fallback remains available without a polymorphic scheduler interface.

## Parameter policy

v1 uses `fsrs::DEFAULT_PARAMETERS`. The database can retain parameter-set metadata for a future optimizer, but no optimized set is active until a separate methodological acceptance gate is implemented.

The learner's persisted `desired_retention` is runtime policy and is passed into FSRS on grading and replay.

## Consequences

- Golden vectors live in `crates/review-engine/tests/golden.rs`; dependency/parameter changes that alter them are explicit algorithm migrations.
- Runtime RNG is not used on the scheduling path.
- Review queue ordering uses predicted retrievability for due mature cards.
- Under ADR-0006, `review-engine` runs only in the native runtime.
