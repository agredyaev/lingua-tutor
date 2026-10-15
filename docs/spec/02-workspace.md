# 02 — Workspace and Dependency Boundaries

## 1. Repository shape

```text
apps/
├── local/                    # native runtime: HTTP/API, in-process MCP tunnel, SQLite
└── web/                      # Leptos PWA

crates/
├── application/              # pure orchestration/planning
├── learning-core/            # domain types/invariants
├── review-engine/            # scheduling
├── adaptation/               # adaptive rules
├── language-pack-api/        # stable plugin contract
├── language-pack-registry/   # registration/selection
├── shared-schema/            # HTTP/UI DTOs
└── tutor-contract/           # tutor import/export contracts

language-packs/
├── es-es/
└── synthetic-test-pack/

migrations/                   # SQLite migrations
```

## 2. Ownership

| Component | Owns | Must not own |
|---|---|---|
| `learning-core` | language-neutral domain types and invariants | storage, HTTP, concrete packs |
| `application` | use-case planning/orchestration | SQL, runtime transport |
| `review-engine` | scheduling and workload calculations | language rules, persistence |
| `language-pack-api` | stable typed pack contract | concrete language behavior |
| concrete language pack | normalization, answer matching, taxonomy, tutor assets | SQLite, network, scheduling state |
| `apps/local` | composition root, HTTP, MCP, repositories, SQLite lifecycle | duplicated domain rules |
| `apps/web` | UI and local API client | direct SQLite or secrets |

## 3. Dependency direction

```text
apps/local ─→ application ─→ learning-core
     │              └──────→ review-engine
     ├─────────────→ language-pack-registry ─→ language-pack-api
     └─────────────→ shared-schema / tutor-contract

apps/web ─→ shared-schema
```

Core crates do not depend on concrete language packs or runtime infrastructure.

## 4. Runtime targets

| Concern | Decision |
|---|---|
| UI | Leptos CSR + Trunk |
| Application runtime | native Rust |
| Database | SQLite |
| HTTP | loopback only |
| MCP | one in-process JSON-RPC dispatcher through the built-in secure tunnel |
| Remote MCP bridge | built-in outbound OpenAI Secure MCP Tunnel protocol adapted from Rivet |
| Service management | foreground `lingua-tutor run`; current directory selects workspace |
