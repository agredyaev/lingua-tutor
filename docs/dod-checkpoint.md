> **Superseded:** this 2026-10-18 partial checkpoint is historical. The final 2026-10-23 structural decision is recorded in [dod-release-gate.md](dod-release-gate.md) and [dod-preflight.md](dod-preflight.md).

# DOD/PWP checkpoint — 2026-10-18

**Milestone:** verified partial migration on `refactor/core-data-path`; **not** whole-repository architectural acceptance, merge approval, or release.

## Completed at this checkpoint

- Native SQLite-backed lesson/card/review operations progressively replaced object reconstruction with bounded indexed processing and direct SQL projections.
- `LessonPlanState`, `LessonPlan`, `CardPlanState`, `CardPlan`, adaptive-selection columns, evidence buffers and review queue now use explicit functions over bounded indexed arrays.
- `adaptation::calibration` uses column-oriented buffers; legacy independently tested oracles remain in test-only paths.
- `review-engine` uses integer FSRS/fallback selectors and explicit processing passes; replay and state hashing are functions, without the old `ReviewScheduler` trait.
- Language-pack behavior trait removed; API is data-only and pack behavior is exposed as explicit free processing passes.
- Registry stores aligned manifest/pack arrays, resolves external IDs to dense `u32` indices, and has no virtual pack dispatch.
- Spanish and synthetic packs plus conformance tests use explicit processing passes.
- Regression tests include architectural drift guards, SQLite spellings, deterministic scheduler replay and parity against independent legacy behavior.

## Verification to reproduce

Run at the commit in this branch:

```sh
cargo test --workspace --locked --quiet
cargo fmt --all --check
cargo check -p web --target wasm32-unknown-unknown --locked
cargo clippy -p review-engine --all-targets -- -D warnings
git diff --check
```

These tests/checks passed at the checkpoint except that their current execution can emit macOS `xcrun_db` sandbox-cache warnings. The full-workspace strict gate `cargo clippy --workspace --all-targets --all-features -- -D warnings` **fails** with numerous warnings/errors in test/legacy and native runtime code. Do not disable lints to bypass it.

## Explicit non-acceptance / next stages

1. Audit remaining retained references/IDs, SoA and CSR relationships, storage lifetime and PWA state. Enforce across the full repository rather than only migrated functions.
2. Make strict all-target workspace Clippy pass; production local library already passes `-D warnings`, while remaining failures are test-target hygiene., then production Trunk build, Playwright E2E, real MCP↔SQLite and recovery checks, and comparative benchmarks.

**Policy:** The checkpoint may be committed to an isolated work branch as an incomplete engineering snapshot; it must not be merged or represented as fully DOD/PWP compliant until all mandatory gates pass. See `docs/dod-release-gate.md` and `docs/dod-preflight.md`.

## Legacy implementation retirement — 2026-10-22

The duplicate `apps/local/src/old` implementation and obsolete `crates/application` use-case layer were removed after the new processing passes passed the existing parity suite. Parity checks were converted to independent wire/database fixtures and direct business invariants before deletion. Test-only repository `Row -> Domain` adapters were also removed. Architecture tests reject reintroduction of the old module, application crate, legacy helpers, and repository domain adapters.
