# 02 — Workspace and Dependency Boundaries

## 1. Repository shape

```text
apps/
├── local/                    # native runtime: RPC, MCP tunnel, SQLite, methodology assets
└── web/                      # Leptos PWA

crates/
├── adaptation/               # bounded adaptive processing passes
├── learning-core/            # scalar enums, pure rules and hashes
├── review-engine/            # review scheduling passes
├── shared-schema/            # transport DTOs
└── tutor-contract/           # structured tutor input/output contract

tests/
├── integration/              # cross-crate architecture, schema and regression gates
└── e2e/                      # browser end-to-end tests

fuzz/                         # separate cargo-fuzz workspace
```

There is no concrete-language source tree, language registry, plugin crate, or language-specific executable module.

## 2. Ownership

| Component | Owns | Must not own |
|---|---|---|
| `learning-core` | language-neutral scalar contracts and pure rules | retained entity graphs, persistence, linguistic interpretation |
| `review-engine` | scheduling calculations and replay | persistence, language rules, browser code |
| `adaptation` | bounded adaptive passes over indexed data | persistence, linguistic interpretation |
| `apps/local` | SQLite lifecycle/migrations, direct read/write passes, minimal SQLite codec/shared-query boundary, RPC/MCP boundaries, methodology assets | target-language correctness rules or alternate domain layers |
| `apps/web` | UI and local RPC client | direct SQLite, scheduling logic, secrets |
| `tutor-contract` | structured tutor result contract and validation | persistence or target-language implementation |
| `shared-schema` | transport data | domain behavior |
| `tests/integration` | cross-crate verification | production behavior |

## 3. Dependency direction

```text
apps/local ─→ learning-core
     ├──────→ review-engine
     ├──────→ adaptation
     ├──────→ tutor-contract
     └──────→ shared-schema

apps/web ─→ shared-schema
```

Core crates do not depend on a target language. The web crate does not own scheduling or persistence.

## 4. Runtime targets

| Concern | Decision |
|---|---|
| UI | Leptos CSR + Trunk |
| Application runtime | native Rust |
| Database | one project-local SQLite database |
| Browser boundary | loopback static assets + `POST /rpc` + downloads |
| MCP | one in-process dispatcher through the built-in secure tunnel |
| Language selection | scalar `source_language` + `target_locale` on a profile |
| Service management | foreground `lingua-tutor run`; current directory selects workspace |
