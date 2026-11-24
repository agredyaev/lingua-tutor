# 00 — Overview

Lingua Tutor is a local-first language-learning application.

## Product goal

Record tutor-led lessons, schedule retrieval practice and adapt subsequent lessons using persisted learner evidence.

## Runtime stack

- Rust workspace.
- Leptos CSR PWA.
- Native Rust runtime: desktop CLI or Android APK with WebView.
- One SQLite database.
- FSRS review scheduling through explicit processing passes; no scheduler registry, fallback implementation, or scheduler behavior interface.
- Language-neutral methodology with scalar target/source language profile metadata.
- ChatGPT integration through bounded MCP tools and an in-process OpenAI Secure MCP Tunnel transport adapted from Rivet.

## Topology

One local runtime serves the PWA and JSON-RPC on loopback, dispatches MCP in-process through the built-in secure tunnel, and accesses one SQLite database. Desktop defaults to port 8787; Android uses an OS-assigned port and a private session cookie. There is no remote application deployment, secondary database, language-pack registry or concrete-language runtime module.

## Core invariants

- SQLite is the sole application system of record.
- Review events are immutable; current schedule state is derived/updated atomically.
- Deterministic methodology stays language-neutral.
- Linguistic interpretation belongs to the tutor contract boundary, not concrete-language core heuristics.
- Tutor and UI use the same application rules and persistence.
- No raw SQL or arbitrary code is exposed through MCP.
- Local backup and integrity checking are mandatory operations.

## v1 surface

Dashboard; lesson history and lifecycle; learner state; tutor-generated cards; card lifecycle; review sessions; deterministic spaced repetition; review history; listening/speaking/error evidence; adaptive recommendations; desktop browser and Android WebView; bounded offline review; JSON/CSV export; localhost UI; MCP tutor integration.
