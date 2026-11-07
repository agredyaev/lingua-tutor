# DOD/PWP preflight — current architecture (2026-11-06)

Current structural baseline: working tree on `audit/repository-cleanup`.
This document is the as-is DOD/PWP review index. Historical release evidence remains in
`docs/dod-release-gate.md`; this file describes the current architecture only.

## L1 — system context

```mermaid
flowchart LR
  Learner["Learner"] -->|browser interaction| Browser["Lingua Tutor PWA"]
  Learner -->|lesson conversation| ChatGPT["ChatGPT"]
  ChatGPT <-->|MCP tunnel| OpenAI["OpenAI Secure MCP Tunnel (external)"]
  Browser -->|loopback JSON-RPC / downloads| Runtime["lingua-tutor native runtime"]
  Runtime <-->|outbound tunnel protocol| OpenAI
  Runtime -->|read/write| SQLite[("Project-local SQLite")]
  Browser -->|bounded offline review queue/cache| LocalStorage[("Browser localStorage")]
  Runtime -->|verified backup| Backup[("Project-local backup files")]
```

Trust/ownership boundaries:
- browser and MCP/tunnel input are untrusted protocol input;
- SQLite is the sole persistent learning-state authority;
- browser localStorage is bounded non-authoritative replay/cache state;
- tunnel credentials live only in process memory;
- backup files are recovery artifacts, not a second application datastore.

## L2 — deployables, stores, and first-party compute

```mermaid
flowchart LR
  subgraph Browser["Browser / WASM"]
    WebMain["main.rs"]
    WebApp["app.rs + pages/*"]
    RpcClient["api_client.rs"]
    Offline["offline.rs"]
    WebMain --> WebApp
    WebApp --> RpcClient
    WebApp --> Offline
  end

  subgraph Native["Native process / apps/local"]
    Main["main.rs composition root"]
    Server["server.rs /rpc + downloads"]
    Tunnel["tunnel.rs"]
    MCP["mcp.rs + mcp_tools.rs\n4 canonical tools"]
    Reads["handlers/*\nread/wire projections"]
    Passes["operations/*\nexplicit processing/write passes"]
    SqliteBoundary["sqlite.rs\nused codecs + 5 multi-consumer queries"]
    Storage["storage.rs + assets.rs"]
    Main --> Server
    Main --> Tunnel
    Tunnel --> MCP
    Server --> Reads
    Server --> Passes
    MCP --> Reads
    MCP --> Passes
    Reads --> SqliteBoundary
    Passes --> SqliteBoundary
    Reads --> SQLite
    Passes --> SQLite
    Storage --> SQLite
  end

  subgraph Compute["First-party computation crates"]
    Adapt["adaptation"]
    Review["review-engine"]
    Contract["tutor-contract"]
    Core["learning-core"]
    Shared["shared-schema\nwire DTO boundary"]
  end

  SQLite[("SQLite .lingua-tutor/lingua.sqlite3")]
  Backup[("SQLite backups")]
  LocalStorage[("browser localStorage")]
  OpenAI["OpenAI Secure MCP Tunnel"]

  RpcClient -->|POST /rpc| Server
  Offline --> LocalStorage
  Tunnel <-->|HTTPS via system curl| OpenAI
  Passes --> Adapt
  Passes --> Review
  Passes --> Contract
  Passes --> Core
  Browser -. compile-time .-> Shared
  Native -. compile-time .-> Shared
  SQLite --> Backup
```

First-party deployables are the native binary and browser WASM bundle. `fuzz/` and GitHub Actions
are verification infrastructure, not runtime layers.

## L3 — native directed flow

```mermaid
flowchart TD
  Main["main.rs\nstartup + project path + secrets"]
  Env["runtime.rs\nEnv(db_path, signing_key)"]
  Server["server.rs\ndecode/admit local JSON-RPC"]
  Tunnel["tunnel.rs\npoll/post + bounded concurrency"]
  MCP["mcp.rs / mcp_tools.rs\ndecode/admit MCP"]
  Reads["handlers/*\ndirect read/wire projections"]
  Passes["operations/*\nexplicit processing/write passes"]
  Sqlite["sqlite.rs\nclosed-value/time codecs + shared SQL only"]
  DB[("SQLite")]
  Review["review-engine\nscalar scheduling/replay"]
  Adapt["adaptation\ncolumn/index planning/calibration"]

  Main -. constructs .-> Env
  Main --> Server
  Main --> Tunnel
  Tunnel --> MCP
  Server --> Reads
  Server --> Passes
  MCP --> Reads
  MCP --> Passes
  Reads --> DB
  Passes --> DB
  Reads -. only shared codecs/queries .-> Sqlite
  Passes -. only shared codecs/queries .-> Sqlite
  Passes --> Review
  Passes --> Adapt
```

Structural result:
- no generic repository/service/application layer remains;
- `repositories/profiles|lessons|records|cards` SQL bags were removed;
- the remaining review/recommendation shared SQL and scalar DB conversions were collapsed into one
  crate-private `sqlite.rs`;
- single-consumer SQL is colocated with its owning read projection or write/processing pass;
- `handlers/*` are direct projections, not write services;
- business mutation is implemented by explicit `operations/*` passes;
- redundant recommendation operation modules were merged;
- standalone MCP maintenance write subsystems were removed;
- MCP exposes only `get_active_language_profile`, `get_tutor_context`, `start_lesson`,
  and `complete_lesson`;
- narrow inspection operations remain on the browser RPC compatibility surface, not duplicated as MCP tools;
- executable language packs/registry and scheduler/domain-object dispatch are absent;
- `Env`, tunnel/MCP, `shared-schema`, offline replay, and compute crates remain because they carry
  concrete lifetime, protocol, failure-mode, wire-contract, or algorithmic boundaries.

## L3 — browser directed flow

```mermaid
flowchart TD
  Main["main.rs mount"]
  App["app.rs router + active-profile UI state"]
  Pages["pages/*"]
  RPC["api_client.rs JSON-RPC"]
  Offline["offline.rs bounded replay/cache"]
  LS[("localStorage")]
  Server["native /rpc"]

  Main --> App
  App --> Pages
  Pages --> RPC
  Pages --> Offline
  RPC --> Server
  Offline --> LS
```

Offline pending events are bounded to 50 and replay in order. Browser state never becomes scheduling
or learning-state authority.

## L4 — operation closure

| Operation | Physical data path | Retained state | Decision |
| --- | --- | --- | --- |
| Browser read | wire request → `server::dispatch_rpc` → direct `handlers` projection → SQLite | request-local rows/columns | KEEP |
| Browser write | wire request → `server::dispatch_rpc` → one explicit `operations` pass → SQLite transaction | request-local buffers | KEEP |
| Tutor read | tunnel → MCP decode → `get_tutor_context` → direct projections/planner passes → SQLite | one authoritative context document | KEEP |
| Tutor lesson write | tunnel → `start_lesson` / `complete_lesson` → plan/apply/write passes → SQLite transaction | bounded SoA/index buffers | KEEP |
| Lesson completion | `LessonResult` → preload → `LessonPlanState` / card plan → bounded writes | dense indices + text arenas | DOD path |
| Review | persisted scalar schedule columns → `review-engine` → scalar result → SQLite | no scheduler object graph | DOD path |
| Adaptation | SQLite/context columns → indexed/column buffers → explicit rules | dense `u32` indices / CSR-style ranges | DOD path |
| Offline replay | bounded `PendingEvent` wire records → ordered RPC replay | max 50 records | boundary-only |
| SQLite sharing | owning pass contains single-consumer SQL; `sqlite.rs` contains only reused codecs/queries | no repository objects | minimal boundary |

## Data-layout and PWP status

SQLite/wire entity IDs remain strings at persistence/protocol boundaries. Retained relationships on the
audited planner/card/lesson/adaptation paths use dense `u32` indices or flat prefix/range layouts;
`usize` conversion is transient at slice access. Application/data paths contain no dynamic domain dispatch,
pointer-linked entity graph, `HashMap`/`BTreeMap` state store, service object, repository object, or manager
hierarchy. `Arc` and `Box<RawValue>` remain only in tunnel/wire infrastructure where concurrency/protocol
ownership requires them.

## Removed as unnecessary

- generic `repositories/` subsystem;
- profile/lesson/record/card SQL-bag modules;
- test-only/unused SQLite enum encode/decode functions;
- `recommendation_ops` split from recommendation writes;
- standalone `upsert_cards`, bulk `suspend_cards`, `log_error_occurrence`, and lesson-correction MCP paths;
- five partial MCP inspection tools duplicated by authoritative `get_tutor_context`;
- integration test API exposure used only to reach internal SQLite codecs.

## Current verification

Passed on this working tree:
- `cargo fmt --all -- --check`;
- `cargo check --workspace --locked`;
- `cargo test --workspace --locked`;
- `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`;
- `git diff --check`.

The separate fuzz workspace could not be recompiled in this Rivet sandbox because Cargo cannot create its
registry cache (`EPERM`), including with a separate `/tmp` Cargo home. This is an environment failure before
fuzz code compilation, not a green fuzz result. Fuzz compilation remains an external/CI gate.

## Acceptance

**Current structural state: DOD/PWP architecture accepted for the audited paths.**

No additional layer should be introduced unless it has a concrete ownership, correctness, protocol,
failure-mode, or measured performance requirement. Functional correctness alone does not justify
reintroducing removed repository/service/maintenance abstractions.
