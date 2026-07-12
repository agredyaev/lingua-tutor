# 02 — Workspace

Scope: repository layout, crate responsibilities, dependency direction, toolchain, dependency policy.
Depends on: [01-architecture.md](01-architecture.md).

## 1. Layout

```text
lingua-tutor/
├── Cargo.toml                 # workspace root
├── Cargo.lock                 # committed
├── rust-toolchain.toml        # pinned toolchain
├── README.md
├── LICENSE
├── apps/
│   ├── web/                   # Leptos CSR PWA
│   │   ├── Cargo.toml
│   │   ├── Trunk.toml
│   │   ├── index.html
│   │   └── src/
│   │       ├── app.rs
│   │       ├── routes.rs
│   │       ├── api_client.rs
│   │       ├── pages/
│   │       ├── components/
│   │       ├── review/
│   │       └── offline/
│   └── worker/                # Cloudflare Worker API
│       ├── Cargo.toml
│       ├── wrangler.jsonc
│       └── src/
│           ├── lib.rs
│           ├── router.rs
│           ├── auth/
│           ├── handlers/
│           └── repositories/
├── crates/
│   ├── learning-core/
│   ├── application/
│   ├── review-engine/
│   ├── adaptation/
│   ├── analytics/
│   ├── tutor-contract/
│   ├── language-pack-api/
│   ├── language-pack-registry/
│   ├── shared-schema/
│   └── test-support/
├── language-packs/
│   ├── synthetic-test-pack/
│   └── es-es/
│       ├── Cargo.toml
│       ├── manifest.json
│       ├── src/
│       └── assets/
│           ├── curriculum/
│           ├── phrase-seeds/
│           ├── card-templates/
│           ├── error-taxonomy/
│           └── tutor-instructions/
├── migrations/                # D1 SQL migrations, forward-only
├── infra/terraform/
├── scripts/
├── tests/                     # cross-crate integration + e2e
└── .github/workflows/
```

Empty architectural placeholders prohibited. Each crate has one concrete responsibility. Create a crate only when its first real code lands.

## 2. Crate responsibilities

| Crate | Owns | Must not |
|---|---|---|
| `learning-core` | Domain types, learner/language profiles, lesson lifecycle, learning items, evidence, errors, card model, mastery rules | Depend on a concrete pack; contain language-specific logic; do IO |
| `application` | Use-case services: lesson completion, card mutation workflow, review orchestration, import/export, recommendation assembly | Contain HTTP/Worker types |
| `review-engine` | `ReviewScheduler` trait, FSRS adapter, fallback scheduler, states, due queue, backlog protection, event semantics | Language logic; network; expose third-party scheduler types |
| `adaptation` | Deterministic adaptive rules ([07-tutor-contract.md](07-tutor-contract.md) §7) as explicit Rust + unit tests | Interpreted rule language |
| `analytics` | Metrics, trends, success indicators ([03-domain-model.md](03-domain-model.md) §8) | Write learning state |
| `tutor-contract` | Versioned lesson-result and card-mutation schemas, validation | Business logic |
| `language-pack-api` | `LanguagePack` trait, manifest types, all request/response types at plugin boundary | Depend on core internals beyond shared types |
| `language-pack-registry` | Registration, lookup by ID, contract-version compatibility check | Pack behavior |
| `shared-schema` | DTOs shared web↔worker (serde types mirroring API) | Domain logic |
| `test-support` | Fixtures, builders, golden-test helpers | Ship in production builds |
| `language-packs/es-es` | All Spanish-for-Spain behavior + assets | Access D1, network, secrets, scheduling state |
| `language-packs/synthetic-test-pack` | Minimal pack for core CI | Real-language content |
| `apps/web` | Leptos UI, api_client, offline queue | Direct D1 access; secrets |
| `apps/worker` | Routing, auth, handlers, D1 repositories | Domain rules (delegates to `application`) |

## 3. Dependency direction

```text
apps/worker ─→ application ─→ learning-core
apps/web    ─→ shared-schema
application ─→ review-engine, adaptation, analytics, tutor-contract,
               language-pack-registry ─→ language-pack-api
language-packs/* ─→ language-pack-api, shared-schema
```

Rules (CI-enforced, see [12-testing.md](12-testing.md)):

- No generic core crate depends on `language-pack-es-es` or any concrete pack.
- `review-engine` contains zero language-specific code; tests pass with synthetic pack or none.
- Third-party scheduler types stay inside `review-engine`.
- Concrete pack types never cross the plugin boundary.
- Core CI runs against `synthetic-test-pack`.

## 4. Technology baseline

| Area | Technology |
|---|---|
| Language | Rust stable, pinned via `rust-toolchain.toml` |
| Frontend | Leptos stable line |
| Web build | Trunk |
| Browser target | `wasm32-unknown-unknown` |
| Worker runtime | Cloudflare Workers + `workers-rs` |
| Database | Cloudflare D1 |
| Scheduling | pinned FSRS Rust crate after compat spike; fallback behind same trait |
| Serialization | Serde |
| Errors | `thiserror` |
| Logging | structured, Worker-runtime compatible |
| IaC | Terraform, Cloudflare Provider v5, pinned |
| Deploy | Wrangler, pinned |
| Browser tests | Playwright |
| Rust tests | `cargo test`; `cargo-nextest` where compatible |
| Supply chain | `cargo-deny` + dependency update automation |

## 5. Dependency policy

- Commit all lock files.
- Pin toolchain after compatibility validation.
- Pin scheduler crate; record its algorithm version.
- Reject abandoned/incompatible scheduler deps; fallback scheduler always available.
- Before pinning any version: inspect current official docs for Leptos, workers-rs, D1, Terraform provider, FSRS crate ([14-traceability.md](14-traceability.md) §2).
- Do not add abstractions not required by the two initial packs (synthetic + es-ES).
