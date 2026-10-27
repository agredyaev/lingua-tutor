# ADR-0001: Integrated review subsystem, no Anki

Date: 2026-07-12. Status: accepted.

## Context

Spaced repetition is required. Options were Anki as an external engine (AnkiConnect/sync bridge) or an owned review subsystem inside the application.

## Decision

Own `review-engine` behind the application-owned `ReviewScheduler` trait. Use the pinned FSRS-compatible implementation after the compatibility/determinism spike recorded in ADR-0004; ship the deterministic fallback scheduler regardless. Anki is forbidden as a runtime dependency.

## Consequences

- The app owns cards, review events, and schedule state in SQLite: one system of record, immutable history, and a direct tutor feedback loop.
- No sync bridge or desktop dependency is required.
- Scheduler correctness is owned here: golden/property tests and persisted algorithm/version are mandatory.
- The scheduler runs in the native runtime under ADR-0006; browser WASM does not depend on `review-engine`.

## Superseded architecture note

The historical `ReviewScheduler` trait described above has been removed. Current runtime architecture uses explicit integer-selected review processing passes as defined by the DOD/PWP architecture gates and current specs.
