# ADR-0001: Integrated review subsystem, no Anki

Date: 2026-07-12. Status: accepted; implementation amended 2026-11-06 after DOD/PWP cleanup.

## Context

Spaced repetition is required. Options were Anki as an external engine (AnkiConnect/sync bridge) or an owned review subsystem inside the application.

## Decision

Own `review-engine` inside the application and use the pinned FSRS implementation selected by ADR-0004. Anki is forbidden as a runtime dependency.

The initial trait/fallback design was superseded by the data-oriented implementation: scheduling is now a single set of explicit FSRS processing passes over persisted scalar state. There is no scheduler trait, registry, fallback scheduler, or speculative parameter-set subsystem.

## Consequences

- The app owns cards, review events, and schedule state in SQLite: one system of record, immutable history, and a direct tutor feedback loop.
- No sync bridge or desktop dependency is required.
- Scheduler correctness is owned here: golden/property/replay tests and persisted algorithm/version validation are mandatory.
- The scheduler runs in the native runtime under ADR-0006; browser WASM does not depend on `review-engine`.
- Any future second scheduler or optimizer requires a new ADR and measurable need; it must not be added as dormant compatibility infrastructure.
