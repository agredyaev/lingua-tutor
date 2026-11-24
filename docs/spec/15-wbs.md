# 15 — Implementation WBS: Local-First Product

## 1. Runtime foundation

- Desktop `lingua-tutor` executable and Android APK containing the Rust library.
- Leptos PWA served on loopback.
- One SQLite database per desktop workspace or Android installation.
- Browser RPC and in-process MCP share processing/persistence.
- CI covers native runtime, web UI, migrations and end-to-end paths.

## 2. Learning methodology

- language-neutral scalar contracts and explicit processing passes;
- profiles, lessons, evidence, errors and listening attempts;
- cards, scheduling state, immutable review events and idempotent undo;
- adaptive recommendations, tutor-context export and measured calibration;
- source/target language stored only as language-profile metadata.

## 3. Local operations

- desktop release binary or Cargo installation; Android signed APK;
- current directory selects desktop state; Android uses app-private files;
- verified SQLite backup with bounded retention;
- health/integrity checks;
- no remote application deployment layer.

## 4. ChatGPT integration

- in-process secure MCP tunnel protocol;
- session-only tunnel credentials;
- no duplicate persistence path.

## 5. Exit gate

Release requires the checks in [11-infrastructure.md](11-infrastructure.md) and [12-testing.md](12-testing.md) to pass. Live authorized MCP and physical-device acceptance require separate execution evidence.
