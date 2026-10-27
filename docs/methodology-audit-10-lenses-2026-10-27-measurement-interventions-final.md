# Methodology audit — measurement + actual interventions — 10 isolated lenses — FINAL

Date: 2026-10-27
Branch: `refactor/core-data-path`
Scope: mandatory lesson measurements, pronunciation signals, actual intervention attribution, delayed calibration, listening persistence.
Method: ten isolated validation passes over the same implementation snapshot, each with its own acceptance criteria. Findings were synthesized only after all passes completed. A first pass exposed one real standalone-listening measurement loophole; it was remediated and all ten lenses were rerun.

## Acceptance rule

PASS requires that the scoped change cannot silently produce unmeasured lesson evidence, cannot attribute a planned method as if it had been applied, preserves the project's DOD/PWP constraints, and is covered by runtime/spec/migration tests.

## First-pass finding and remediation

- Lens 05 found that the standalone `create_listening_attempt` write path could still persist a listening observation without assistance level, response latency, probe phase, or context.
- Remediation: the standalone path now requires `context_tag`, `assistance_level`, `response_millis`, and `probe_phase`; it rejects assistance > 3, latency > 600000 ms, and empty context.
- Lens 04 initially reported a failed textual criterion because the code uses `minimum_gap_seconds` rather than the validator's searched name. Inspection confirmed the delayed comparison already existed; the validator criterion was corrected. This was not a code defect.

## Final ten-lens result

### 1 — Measurement integrity — PASS
- Lesson-result schema is v2.
- Selected evidence probes require observed assistance level, response latency, probe phase, and explicit context.
- Lesson completion requires at least two measured diagnostic probes.
- Missing measurements are rejected rather than silently treated as unknown.
- Regression tests cover mandatory measurements and diagnostic coverage.

### 2 — Pronunciation — PASS
- Productive probes carry `pronunciation_intelligible` and `pronunciation_repeat_required`.
- Both fields are mandatory for production/transfer probes.
- Contradictory values are rejected.
- Pronunciation signals are persisted and surfaced in planner history.
- Tutor instructions require these measurements and avoid a noisy synthetic pronunciation score.

### 3 — Actual interventions — PASS
- `LessonResult` carries a flat `interventions` collection.
- Each intervention records action, item reference, target capability, and optional immediate outcome reference.
- Same-lesson client item keys resolve to persisted item IDs before outcome persistence.
- Planner history stores actual interventions in `outcome_json`.
- Planned task candidates are not treated as applied interventions.

### 4 — Calibration causality — PASS
- Only measurement-valid lessons contribute interventions to calibration.
- Calibration compares before/after snapshots only after the configured minimum time gap.
- Actual intervention action + item + capability is the attribution key.
- Malformed or too-close snapshots are ignored.
- Regression tests reject calibration from planned-only and measurement-invalid lessons.

### 5 — Listening metrics — PASS
- SQLite v6 persists assistance level, response latency, and probe phase.
- `context_tag` remains the explicit listening context.
- Capability projection reads persisted assistance and latency.
- Standalone listening writes now require the same core measurement semantics instead of bypassing lesson constraints.
- Migration v5→v6 is explicitly tested.

### 6 — DOD / programming-without-pointers — PASS
- Calibration remains column-oriented with dense integer keys and bounded vectors.
- No new Box/Arc/Mutex object graph was introduced in the scoped validation/calibration path.
- No service/factory/trait-object layer was introduced.
- Architecture tests guard the columnar processing shape.

### 7 — Persistence / migration — PASS
- Database schema version is 6.
- v5→v6 migration is explicit and tested.
- New listening measurements are flat scalar columns.
- Actual interventions reuse the existing planner decision outcome JSON; no new intervention table/object graph is introduced.

### 8 — Contract / tutor operability — PASS
- MCP, runtime lesson-result template, spec, and Spanish tutor instructions all use schema v2 semantics.
- Tutor instructions explicitly require at least two measured diagnostic probes.
- Tutor instructions distinguish actual applied interventions from planned tutor tasks.
- Unreliable values must be re-probed rather than invented.

### 9 — Metric signal-to-noise — PASS
- Mandatory metrics are observable signals: success, assistance, latency, phase, context, and productive pronunciation intelligibility/repeat.
- Subjective `confidence` remains optional.
- No generic pronunciation score was added.
- The design avoids forcing weak/noisy metrics merely for completeness.

### 10 — Runtime / spec / architecture consistency — PASS
- Contract regression tests, causal calibration tests, migration tests, standalone listening measurement tests, and planner-outcome tests are present.
- Legacy planned-candidate calibration source is absent.
- Full DOD architecture suite passes.
- Runtime/spec terminology is aligned around actual interventions and measured probes.

## Build and regression gates

- `cargo test --workspace --locked`: PASS.
- DOD architecture suite: 29/29 PASS.
- Tutor-contract tests: 18/18 PASS.
- Adaptation tests: 25/25 PASS.
- `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`: PASS. macOS sandbox emits `xcrun_db` linker-cache warnings; Clippy explicitly marks these as outside `-D warnings`.
- `cargo fmt --all -- --check`: PASS.
- `git diff --check`: PASS.
- `cargo check -p web --target wasm32-unknown-unknown --locked`: PASS.
- `cargo deny --locked check`: PASS.

## Metric coverage after remediation

Required for every selected lesson evidence/listening probe:
- `success`;
- `assistance_level`;
- `response_millis`;
- `probe_phase`;
- explicit context (`context` or `context_tag`).

Additionally required for production/transfer:
- `pronunciation_intelligible`;
- `pronunciation_repeat_required`.

Lesson-level calibration eligibility additionally requires:
- at least 2 measured `diagnostic` probes.

Intentionally not mandatory:
- `confidence` (subjective, weak diagnostic signal);
- aggregate speaking duration when it was not reliably measured;
- a synthetic pronunciation score.

## Consensus

All ten final isolated lenses pass after remediation.

**Scoped methodology gate: APPROVED.**

The remaining work is empirical optimization of thresholds and intervention effectiveness from longitudinal learner data, not a correctness repair in this scoped patch.
