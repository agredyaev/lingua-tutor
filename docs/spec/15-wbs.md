# 15 — Implementation WBS: Local-First Product

## 1. Runtime foundation

- Self-contained native `lingua-tutor` binary.
- Leptos PWA served on loopback.
- One project-local SQLite database.
- Browser RPC and in-process MCP share processing/persistence.
- CI covers native runtime, web UI, migrations and end-to-end paths.

## 2. Learning methodology

- language-neutral scalar contracts and explicit processing passes;
- profiles, lessons, evidence, errors and listening attempts;
- cards, scheduling state, immutable review events and idempotent undo;
- adaptive recommendations, tutor-context export and measured calibration;
- source/target language stored only as language-profile metadata.

## 3. Local operations

- release binary or direct Cargo installation;
- current directory selects persistent `.lingua-tutor/` state;
- verified SQLite backup with bounded retention;
- health/integrity checks;
- no remote application deployment layer.

## 4. ChatGPT integration

- in-process secure MCP tunnel protocol;
- session-only tunnel credentials;
- no duplicate persistence path.

## 5. Exit gate

Cutover is complete only when formatting/lint/tests/web build/local smoke/MCP smoke pass, scratch SQLite initializes with FK/integrity checks, browser and MCP use the same database, and architecture gates reject target-language executable behavior or extra object/service layers.
