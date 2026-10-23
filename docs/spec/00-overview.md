# 00 — Overview

Lingua Tutor is a local language-learning application.

## Product goal

Support spoken-language practice while storing learner state, review history, tutor evidence, and scheduling state in one local database.

## Runtime stack

- Rust workspace.
- Leptos CSR PWA.
- Native `lingua-tutor` CLI/runtime.
- One SQLite database.
- FSRS-compatible review scheduler behind an application-owned interface.
- Versioned language packs.
- ChatGPT integration through bounded MCP tools and an in-process OpenAI Secure MCP Tunnel transport adapted from Rivet.

## Topology

One runtime serves the PWA and JSON-RPC on `127.0.0.1:8787`.
The runtime dispatches MCP in-process through OpenAI Secure MCP Tunnel.
The runtime accesses the same SQLite database.
The application has no remote deployment or secondary database.

## Core invariants

- SQLite is the sole application system of record.
- Review events are immutable; current schedule state is derived/updated atomically.
- Language-specific behavior lives in language packs, not core.
- Tutor and UI use the same application rules and persistence.
- No raw SQL or arbitrary code is exposed through MCP.
- Local backup and integrity checking are mandatory operations.

## v1 surface

- Dashboard and lesson history.
- Learner state, evidence, and errors.
- Tutor-generated cards and review sessions.
- Deterministic spaced repetition and review history.
- Recommendations derived from evidence.
- PWA installation and bounded offline behavior.
- JSON/CSV export.
- Localhost UI and MCP tutor integration.
