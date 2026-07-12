# ADR-0001: Integrated review subsystem, no Anki

Date: 2026-07-12. Status: accepted.

## Context

Spaced repetition needed. Options: Anki as external engine (AnkiConnect/sync bridge) vs own review subsystem inside app.

## Decision

Own `review-engine` crate behind app-owned `ReviewScheduler` trait. FSRS-compatible pinned crate if wasm32/size spike passes (WBS 3.2); deterministic fallback scheduler ships regardless. Anki forbidden as runtime dependency.

## Consequences

- App owns cards, review events, schedule state in D1 — single source of truth, immutable history, tutor feedback loop possible.
- No sync bridge, no desktop dependency, works on iPhone PWA.
- Cost: we maintain scheduler correctness ourselves → golden + property tests (12-testing.md §2), versioned algorithm + parameter sets.
- Spec: docs/spec/01-architecture.md AD-4, 05-review-engine.md, spec.md §6.5.
