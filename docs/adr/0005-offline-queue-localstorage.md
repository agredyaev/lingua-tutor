# ADR-0005: Offline event queue in localStorage, not IndexedDB

Date: 2026-07-12. Status: accepted. Deviation from 09-frontend.md §7 wording.

## Context

Spec names IndexedDB for the offline review-event queue. The queue is hard-bounded: ≤ `max_review_batch` (50) events plus the prefetched queue snapshot — ≈ 15–60 KB of JSON, orders of magnitude under localStorage limits. IndexedDB from Rust/WASM means async schema/versioning machinery or a new wrapper dependency — heavy for a bounded key-value need (16-rust-guidelines.md §1: no speculative machinery; anti-pattern review requirement).

## Decision

localStorage with three keys: `lt-offline-events` (ordered pending events), `lt-conflict-events` (409-rejected events kept for audit), `lt-prefetch-queue` (last due batch + dashboard snapshot). Serialization helpers are pure Rust, unit-tested on host; storage IO is a thin `web_sys` wrapper.

## Consequences

- Same functional guarantees the spec wants: durable across restarts, strictly ordered, bounded, idempotent replay (`client_operation_id` + `prior_state_hash` travel with each event).
- localStorage is synchronous — irrelevant at ≤ 60 KB, single write per grade.
- Revisit trigger: if any offline payload class outgrows ~1 MB or needs indexed lookup, migrate to IndexedDB and retire this ADR.
