# 00 — Overview

Lingua Tutor is a local-first language-learning application.

## Product goal

Help a learner start speaking quickly while preserving durable learner state, review history, tutor evidence, and adaptive scheduling in one local system of record.

## Runtime stack

- Rust workspace.
- Leptos CSR PWA.
- Native `lingua-tutor` CLI/runtime.
- One SQLite database.
- FSRS-compatible review scheduling through explicit integer-selected processing passes; no scheduler behavior interface.
- Versioned language packs.
- ChatGPT integration through bounded MCP tools and an in-process OpenAI Secure MCP Tunnel transport adapted from Rivet.

## Topology

One local runtime serves the PWA and browser JSON-RPC boundary on `127.0.0.1:8787`, dispatches MCP in-process through the built-in secure tunnel, and accesses the same SQLite database. There is no remote application deployment or secondary database.

## Core invariants

- SQLite is the sole application system of record.
- Review events are immutable; current schedule state is derived/updated atomically.
- Language-specific behavior lives in language packs, not core.
- Tutor and UI use the same application rules and persistence.
- No raw SQL or arbitrary code is exposed through MCP.
- Local backup and integrity checking are mandatory operations.

## v1 surface

Dashboard; lesson history and lifecycle; learner state; tutor-generated cards; card lifecycle; review sessions; deterministic spaced repetition; review history; listening/speaking/error evidence; adaptive recommendations; PWA installation; bounded offline behavior; JSON/CSV export; localhost UI; MCP tutor integration.
