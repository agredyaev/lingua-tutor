# ADR-0006: Native local runtime with one operation boundary

Date: 2026-10-16. Status: accepted.

## Context

The original design carried assumptions from a remote/Worker-style deployment and exposed most application use cases as REST `/api/v1` resources. After the local-first cutover, the product is one native Rust binary, one directory-local SQLite database, one Leptos PWA, and one in-process MCP implementation.

Keeping REST-shaped internal calls, Worker/WASM scheduler constraints, credential files, or a second application transport would duplicate representations and contradict the single-process architecture.

## Decision

- The native `lingua-tutor` binary is the only application runtime.
- The PWA uses JSON-RPC 2.0 over `POST /rpc`.
- MCP calls the same application handlers directly in-process; it does not make synthetic HTTP requests.
- Only file downloads remain direct `/api/v1` routes.
- SQLite migrations belong to `apps/local/migrations`.
- `review-engine` is native-only. The PWA does not depend on it, so workspace-wide WASM `getrandom` configuration is removed.
- Tunnel ID and Runtime API key are prompted securely for each run and remain process-memory-only.
- Build-time language packs remain unchanged.

## Consequences

- One operation model is shared by PWA and MCP.
- No service layer, REST controller layer, secondary MCP process, or remote deployment runtime is required.
- Browser WASM contains UI logic only; scheduler and persistence stay native.
- Specs describing Worker execution, persisted tunnel credentials, or REST application resources are superseded by this ADR and [08-local-rpc.md](../spec/08-local-rpc.md).
- Direct download URLs remain intentionally separate because their response is a file rather than an application result.
