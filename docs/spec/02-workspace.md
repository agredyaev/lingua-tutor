# 02 — Workspace and Dependency Boundaries

## 1. Repository shape

```text
apps/
├── local/                    # native runtime: /rpc, downloads, MCP tunnel, SQLite
│   ├── src/
│   └── migrations/
└── web/                      # Leptos PWA

crates/
├── application/              # pure orchestration/planning
├── learning-core/            # domain types/invariants
├── review-engine/            # native scheduling
├── adaptation/               # adaptive rules
├── language-pack-api/        # stable plugin contract
├── language-pack-registry/   # registration/selection
├── shared-schema/            # transport-neutral UI/application DTOs
└── tutor-contract/           # tutor import/export contracts

language-packs/
├── es-es/
└── synthetic-test-pack/

tests/
├── integration/              # conventional Rust cross-crate test crate
│   ├── src/
│   └── tests/
└── e2e/                      # Playwright package/config/specs

fuzz/                         # separate cargo-fuzz workspace
```

The repository root is a virtual Cargo workspace, so it intentionally has no root `src/`. Production Rust sources live under their owning package's `src/` directory.

## 2. Ownership

| Component | Owns | Must not own |
|---|---|---|
| `learning-core` | language-neutral domain types and invariants | storage, transport, concrete packs |
| `application` | use-case planning/orchestration | SQL, runtime transport |
| `review-engine` | native scheduling and workload calculations | language rules, persistence, browser code |
| `language-pack-api` | stable typed pack contract | concrete language behavior |
| concrete language pack | normalization, answer matching, taxonomy, tutor assets | SQLite, network, scheduling state |
| `apps/local` | composition root, browser RPC/download boundary, MCP, repositories, SQLite lifecycle/migrations | duplicated domain rules |
| `apps/web` | UI and local RPC client | direct SQLite, scheduler, secrets |
| `tests/integration` | cross-crate verification | production code |
| `tests/e2e` | browser journey verification | application logic |

## 3. Dependency direction

```text
apps/local ─→ application ─→ learning-core
     │              └──────→ review-engine
     ├─────────────→ language-pack-registry ─→ language-pack-api
     └─────────────→ shared-schema / tutor-contract

apps/web ─→ shared-schema
```

Core crates do not depend on concrete language packs or runtime infrastructure. The web crate does not depend on `review-engine`.

## 4. Runtime targets

| Concern | Decision |
|---|---|
| UI | Leptos CSR + Trunk |
| Application runtime | native Rust |
| Database | SQLite |
| Browser boundary | loopback static assets + `POST /rpc` + file downloads |
| MCP | one in-process tool dispatcher through the built-in secure tunnel |
| Remote MCP bridge | outbound OpenAI Secure MCP Tunnel protocol adapted from Rivet |
| Service management | foreground `lingua-tutor run`; current directory selects workspace |
