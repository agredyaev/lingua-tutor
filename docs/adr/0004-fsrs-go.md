# ADR-0004: FSRS spike — GO on fsrs-rs 6.6.1

Date: 2026-07-12. Status: accepted; WASM-specific consequence superseded by ADR-0006.

## Evidence

Candidates: `fsrs` 6.6.1 and `rs-fsrs` 1.2.1.
The spike recorded deterministic scheduler behavior and dependency size.

| Gate | Result |
|---|---|
| wasm32 compile (historical topology) | Passed with an explicit getrandom WASM backend; this is no longer an active runtime requirement |
| Binary size (historical spike) | 38 KB raw / 12.5 KB gzip for scheduler-only use |
| Determinism | Identical outputs across runs for the recorded vectors |

## Decision

Pin `fsrs = "6.6.1"` and use FSRS-6 defaults.
Keep `FsrsScheduler` behind `ReviewScheduler`.
Third-party FSRS types do not leave `review-engine`.
Persist `algorithm_version = "fsrs-rs-6.6.1"` with schedule state.

`FallbackScheduler` ships behind the same trait so the external scheduler dependency can be replaced without losing review history.

## Consequences

- Golden vectors live in `crates/review-engine/tests/golden.rs`; a dependency bump that changes them is an explicit algorithm migration.
- Runtime RNG is not used on the scheduling path.
- Under ADR-0006, `review-engine` runs only in the native runtime. The old workspace-wide `.cargo` WASM getrandom configuration is therefore removed.
