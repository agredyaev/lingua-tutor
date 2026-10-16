# ADR-0005: Offline event queue in localStorage, not IndexedDB

Date: 2026-07-12. Status: accepted. The active frontend spec matches this decision.

## Context

The offline review queue is hard-bounded to the review batch size and contains only a small ordered event list plus the prefetched queue snapshot. IndexedDB would add async schema/versioning machinery for a bounded key-value use case.

## Decision

Use `localStorage` with three keys: `lt-offline-events`, `lt-conflict-events`, and `lt-prefetch-queue`. Pure serialization/queue helpers are host-tested; browser storage IO remains a thin `web_sys` wrapper.

## Consequences

- Ordered durable replay across browser restarts.
- `client_operation_id` and `prior_state_hash` preserve idempotency/conflict checks.
- Synchronous storage is acceptable for the bounded payload.
- If payloads approach ~1 MB or require indexed lookup, migrate to IndexedDB and supersede this ADR.
