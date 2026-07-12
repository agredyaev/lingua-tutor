# ADR-0004: FSRS spike — GO on fsrs-rs 6.6.1

Date: 2026-07-12. Status: accepted. WBS 3.2 gate (05-review-engine.md §2).

## Evidence

Candidates: `fsrs` 6.6.1 (open-spaced-repetition, active, optimizer+scheduler) vs `rs-fsrs` 1.2.1 (scheduler-only, last release 2024-10 → abandoned by dep policy).

Spike on `fsrs` 6.6.1 (scratch crate, wasm-release-equivalent profile):

| Gate | Result |
|---|---|
| wasm32-unknown-unknown compile | ✅ needs `getrandom` `wasm_js` feature + `--cfg getrandom_backend="wasm_js"` (committed in `.cargo/config.toml`) |
| Binary size | ✅ 38 KB raw / 12.5 KB gzip — burn training code dead-code-eliminated when only `next_states`/`current_retrievability` used; irrelevant vs Worker 1 MB and PWA 3 MB budgets |
| Determinism | ✅ identical outputs across runs (initial Good: s=2.3065 d=2.118104; lapse after +2d: s=0.6075801) |

## Decision

GO: pin `fsrs = "6.6.1"`, FSRS-6 `DEFAULT_PARAMETERS`, adapter `FsrsScheduler` behind `ReviewScheduler`; crate types never leave `review-engine`. `algorithm_version = "fsrs-rs-6.6.1"` persisted per state.

`FallbackScheduler` (deterministic multiplicative, integer arithmetic) ships regardless, same trait — scheduler dependency can be dropped without data loss.

## Consequences

- Golden vectors recorded in `crates/review-engine/tests/golden.rs`; crate bump changing them = conscious algorithm migration, not routine update.
- Runtime RNG never used on the scheduling path; `wasm_js` backend is satisfied by WebCrypto in both Workers and browsers.
- Optimizer API available later for M8 personalization without new deps.
