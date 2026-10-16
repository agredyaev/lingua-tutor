# 15 — Implementation WBS: Local-First Product

The active implementation path is intentionally flat and local.

## 1. Runtime foundation

- Self-contained native `lingua-tutor` binary.
- Leptos PWA built with Trunk and served on loopback.
- One SQLite database initialized from `apps/local/migrations/0001_init.sql`.
- Browser JSON-RPC and in-process MCP dispatch share the same handlers and persistence.
- CI builds/tests native runtime, web UI, migrations, smoke and e2e paths.

## 2. Domain and review

- language-neutral domain types and language-pack contract.
- profiles, lessons, evidence, errors, listening attempts.
- cards, scheduling state, immutable review events, undo/idempotency.
- adaptive recommendations and tutor-context export.

## 3. Local operations

- verified release binaries plus direct Cargo installation; no installer wrapper.
- current directory selects persistent `.lingua-tutor/` workspace state.
- explicit SQLite backup with bounded retention.
- health/integrity checks.
- no remote application deployment infrastructure.

## 4. ChatGPT integration

- in-process OpenAI Secure MCP Tunnel protocol adapted from Rivet.
- tunnel ID/API key are hidden-input, session-only values; no credential file, public HTTP MCP endpoint, or duplicate persistence path.

## 5. Exit gate

The local-first cutover is complete only when:
1. repository search shows no obsolete remote-platform dependencies/configuration;
2. `cargo fmt`, `clippy`, tests, web build, local smoke, and MCP smoke pass;
3. a scratch SQLite database initializes and passes integrity/FK checks;
4. localhost UI loads and the review/card flow uses that database;
5. in-process MCP reads/writes the same database;
6. the built-in tunnel path connects when valid session-only tunnel credentials are supplied.
