# 00 — Overview

Scope: entry point for the implementation spec suite. Index, hard constraints, scope, naming, definition of done.
Baseline: `docs/spec.md` v2.0. On conflict, spec.md governs; reconcile the delta, do not fork.

## 1. Product

Lingua Tutor: language-agnostic personal learning platform. One learner (owner). Tutor lessons run in ChatGPT; Lingua Tutor stores state, schedules reviews, adapts. Target-language behavior lives in versioned language-pack plugins. First pack: `es-ES` (Spanish for Spain, Russian-speaking learner).

Stack: Rust workspace. Leptos CSR PWA (WASM). Cloudflare Worker (`workers-rs`). Cloudflare D1. Terraform + Wrangler. FSRS-compatible review scheduler behind app-owned trait.

## 2. File index

| File | Covers |
|---|---|
| [00-overview.md](00-overview.md) | Constraints, scope, naming, DoD |
| [01-architecture.md](01-architecture.md) | System context, trust boundaries, architecture decisions |
| [02-workspace.md](02-workspace.md) | Repo layout, crates, dependency rules, toolchain |
| [03-domain-model.md](03-domain-model.md) | Domain types, statuses, mastery rules |
| [04-database.md](04-database.md) | Full DDL, indexes, migration policy |
| [05-review-engine.md](05-review-engine.md) | Scheduler trait, FSRS, states, queue, events |
| [06-language-packs.md](06-language-packs.md) | Plugin contract, manifest, registry, es-ES pack |
| [07-tutor-contract.md](07-tutor-contract.md) | Lesson result, card mutations, card types, adaptive rules |
| [08-http-api.md](08-http-api.md) | All endpoints, schemas, idempotency, import/export |
| [09-frontend.md](09-frontend.md) | Leptos app, routes, review UX, offline, Leptos best practices |
| [10-security.md](10-security.md) | Threat model, auth, validation, privacy |
| [11-infrastructure.md](11-infrastructure.md) | Terraform/Wrangler split, CI/CD, backup, observability |
| [12-testing.md](12-testing.md) | Test suites, acceptance commands, performance targets |
| [13-roadmap.md](13-roadmap.md) | Milestones M0–M8, MCP spec (deferred), risks |
| [14-traceability.md](14-traceability.md) | spec.md section → file map, external assumptions |
| [15-wbs.md](15-wbs.md) | Work breakdown structure, 0 → production, dependencies, estimates |
| [16-rust-guidelines.md](16-rust-guidelines.md) | Binding Rust coding rules, anti-overengineering, clippy enforcement |
| [17-documentation.md](17-documentation.md) | Rustdoc standards, NPR 7150.2D doc discipline, Power of Ten mapping |

Reading order = numeric order. Implementation order: 02 → 04 → 06 → 05 → 03 → 07 → 08 → 09, with 10–12 in parallel.

## 3. Hard constraints

Topology: one Worker deployment, one D1 database per environment, one private GitHub repo.

Forbidden (version 1):

- Anki Desktop, AnkiConnect, local sync bridge; Anki is not a runtime dependency
- Kubernetes, service mesh, message brokers, event streaming
- microservices, multiple databases
- runtime loading of arbitrary/untrusted plugin code
- interpreted generic rules engine; generic workflow engine
- separate backend language without documented necessity
- React; FastAPI; Axum as production server (test utilities only)
- OpenAI API as a runtime dependency — app works without ChatGPT/MCP
- raw audio storage (MVP stores metrics, text, timestamps, source refs only)
- direct model (ChatGPT) access to DB, SQL, secrets, or unbounded data
- advertising, analytics trackers, third-party behavioral SDKs
- empty architectural placeholder directories or placeholder business logic

Required:

- language-agnostic core: no `spanish_*`, `es_es`, locale-specific names in core crates, schema, API
- language packs = build-time registered plugins behind versioned contract
- app-owned learning state: cards, review history, scheduling — D1 is source of truth
- free-tier fit: Cloudflare Workers Free + D1 for normal personal use
- Rust-first. JS allowed only for: WASM loader, PWA service worker, isolated chart adapter if SVG insufficient, temporary MCP compat adapter

## 4. Scope

### In scope v1

Mobile-first dashboard; lesson history and lifecycle; learning-item inventory; tutor-generated cards; card editing/lifecycle; integrated review sessions; deterministic spaced repetition; immutable review history; active-recall status; listening attempts; speaking metrics; error tracking with 1/3/7-day rechecks; error escalation; adaptive recommendations; PWA install; bounded offline review; JSON+CSV export; Cloudflare deploy; Terraform; GitHub Actions CI/CD; optional MCP tool layer (feature-gated); fallback copy/import lesson flow.

### Out of scope v1

Anki compatibility/sync/APKG; third-party deck import; speech recognition; pronunciation scoring; audio hosting; multi-user; pack marketplace; third-party plugin binaries; public registration; subscriptions; classrooms; social; deck sharing; gamification economy; tutor video; autonomous lessons without user-initiated conversation.

## 5. Naming

| Concept | Name |
|---|---|
| Product | Lingua Tutor |
| Repository | `lingua-tutor` |
| Reusable platform logic | `learning-core` |
| Spaced-repetition subsystem | `review-engine` |
| Tutor-facing contracts | `tutor-contract` |
| Plugin registry | `language-pack-registry` |
| Plugin API crate | `language-pack-api` |
| Spanish pack | `language-pack-es-es` |
| Test pack | `synthetic-test-pack` |

Never use `Spanish`, `Anki`, or a locale in generic crate, table, endpoint, or domain-type names. Generic vocabulary: source language, target language, language-pack ID, target-language content, learner-facing prompt, evidence, error category, card intent, review state.

## 6. Definition of done

Feature complete when: behavior implemented; success+failure tests; migrations included; server-side validation; idempotency handled; security reviewed; docs updated; no placeholder logic; no unnecessary abstraction; core language-neutral; new language-specific behavior in a pack; pack conformance tests pass; all quality gates pass.

Production v1 complete when: lessons persist; tutor cards auto-created from lesson evidence; reviews run in-app; scheduling deterministic and versioned; review history feeds tutor lessons; recurring errors trigger targeted cards; workload bounded; PWA works on iPhone; export+restore works; core workflows work without MCP and without Anki; infra deploys via Terraform + Wrangler reproducibly.

## 7. Spec style

These files are contracts. Every sentence is a requirement, constraint, or definition. "Must" = binding. "Deferred" = designed but not built in the named milestone; see [13-roadmap.md](13-roadmap.md).
