# ADR-0005: Offline event queue in localStorage, not IndexedDB

Date: 2026-07-12. Status: accepted. The active frontend spec matches this decision.

## Context

The offline review queue is bounded by the review batch size.
The queue stores an ordered event list and the prefetched queue snapshot.
IndexedDB would add asynchronous schema and version management for this bounded key-value state.

## Decision

Use `localStorage` with three keys: `lt-offline-events`, `lt-conflict-events`, and `lt-prefetch-queue`.
Host tests cover serialization and queue logic.
Browser storage I/O is isolated in the `web_sys` wrapper.

## Consequences

- Ordered durable replay across browser restarts.
- `client_operation_id` and `prior_state_hash` preserve idempotency/conflict checks.
- Synchronous storage is bounded by the review batch limit.
- If payloads approach ~1 MB or require indexed lookup, migrate to IndexedDB and supersede this ADR.
