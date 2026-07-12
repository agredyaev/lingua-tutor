# 15 — Implementation WBS: Zero to Production

Scope: work breakdown structure, 0 → production v1. Solo developer. Effort in focused days (d), rough. Dependencies explicit. Production v1 = M0–M5; M6–M8 optional post-prod.
Depends on: [13-roadmap.md](13-roadmap.md) milestones; all component specs.

Critical path: **1 → 2 → 3 → 4 → 5 → 6 → 8 → 10 → 11**. Packages 7, 9 parallelizable.

## WBS 1 — Foundation (M0) — ~5d

| # | Task | Deliverable | Depends | Est |
|---|---|---|---|---|
| 1.1 | Toolchain: install/pin Rust stable, Trunk, Wrangler, Terraform; `rust-toolchain.toml` | committed pins | — | 0.5d |
| 1.2 | Version research: current Leptos, workers-rs, D1 limits, TF provider v5, FSRS crates; pin exact versions | pin table in README | 1.1 | 0.5d |
| 1.3 | Workspace: root `Cargo.toml`, crates per [02-workspace.md](02-workspace.md) §1–2, no placeholders — each crate lands with first real type | building workspace | 1.1 | 0.5d |
| 1.4 | Minimal Worker: `workers-rs` hello + router + `/api/v1/health` stub | deployed to dev | 1.3 | 0.5d |
| 1.5 | Minimal Leptos PWA: Trunk build, one route, `wasm-release` profile, size report | loads in browser | 1.3 | 0.5d |
| 1.6 | D1: dev database, binding, `migrations/0001_init.sql` from [04-database.md](04-database.md) §2, apply | schema live in dev | 1.4 | 1d |
| 1.7 | CI skeleton: fmt, clippy `-D warnings`, test, WASM build, worker build, `cargo-deny` | green pipeline | 1.3 | 1d |
| 1.8 | ADRs: integrated review subsystem; plugin model | 2 ADR files | — | 0.5d |

## WBS 2 — Plugin spine (M0) — ~4d

| # | Task | Deliverable | Depends | Est |
|---|---|---|---|---|
| 2.1 | `language-pack-api`: `LanguagePack` trait, manifest types, all boundary request/response types ([06-language-packs.md](06-language-packs.md) §2–3) | crate + unit tests | 1.3 | 1d |
| 2.2 | `language-pack-registry`: registration, ID lookup, contract-version check, clear startup failure | crate + tests | 2.1 | 0.5d |
| 2.3 | `synthetic-test-pack`: full contract, artificial rules | pack passes contract | 2.1 | 1d |
| 2.4 | `language-pack-es-es` stub: manifest + normalization only | registered, selectable | 2.1 | 0.5d |
| 2.5 | Shared conformance suite in `test-support` ([12-testing.md](12-testing.md) §5); wire to CI vs synthetic pack | suite green both packs | 2.3, 2.4 | 1d |
| 2.6 | CI dependency-direction check: core ↛ concrete pack | failing test proves rule | 2.5 | 0.5d |

## WBS 3 — Scheduler spike + review-engine core (M0) — ~4d

| # | Task | Deliverable | Depends | Est |
|---|---|---|---|---|
| 3.1 | `ReviewScheduler` trait + `ScheduledState`, `SchedulerSettings`, state-hash fn | crate compiles | 1.3 | 0.5d |
| 3.2 | FSRS crate spike: wasm32/Worker compile, binary size, determinism. GO/NO-GO decision recorded | spike note + decision | 3.1, 1.2 | 1d |
| 3.3 | FSRS adapter behind trait (GO) — types never leak | adapter + tests | 3.2 | 1d |
| 3.4 | Fallback deterministic scheduler (built regardless of 3.2) | passes same suite | 3.1 | 1d |
| 3.5 | Golden tests: 4 grades, initial schedule, lapse; property test: no invalid intervals | suite green | 3.3/3.4 | 0.5d |

**M0 exit gate:** builds pass; pack registered + selected; conformance green on synthetic; one review event persists next due date; fallback documented; no Spanish field in core.

## WBS 4 — Core records (M1) — ~7d

| # | Task | Deliverable | Depends | Est |
|---|---|---|---|---|
| 4.1 | `learning-core` domain types ([03-domain-model.md](03-domain-model.md)): profiles, lesson, item, evidence, errors, enums, ID newtypes | crate + unit tests | 1.3 | 1d |
| 4.2 | Worker repositories: learner_profile, language_profiles, language_packs metadata | integration tests vs scratch D1 | 4.1, 1.6 | 1d |
| 4.3 | Repositories: lessons, lesson_metrics, learning_items, evidence, error_*, listening_attempts | integration tests | 4.2 | 1.5d |
| 4.4 | `application` services: profile CRUD, lesson start/complete (no cards yet), evidence + error recording, idempotency | service tests | 4.3 | 1.5d |
| 4.5 | HTTP endpoints: packs, profiles, learning-state, next-lesson, lessons, errors, listening ([08-http-api.md](08-http-api.md) §2–4, §8) | Worker handlers + tests | 4.4 | 1d |
| 4.6 | `tutor-contract` crate: lesson-result + card-mutation schemas, validation ([07-tutor-contract.md](07-tutor-contract.md) §2–3) | crate + fixture tests | 4.1 | 1d |
| 4.7 | Tutor-context export + lesson-result validate/import (validate → token → apply) | endpoints + tests | 4.6, 4.5 | 1d |
| 4.8 | README: exact local commands (build, test, migrate, run) | README | 4.5 | 0.5d |

## WBS 5 — Cards + review MVP (M2) — ~8d

| # | Task | Deliverable | Depends | Est |
|---|---|---|---|---|
| 5.1 | Card model + repository + `card_tags`, `content_hash` | tests | 4.3 | 1d |
| 5.2 | Card mutation workflow: ops, quality rules, duplicate detection, activation budget, edit classes, `card_mutations` audit | service + tests ([12-testing.md](12-testing.md) §3) | 5.1, 4.6 | 2d |
| 5.3 | Lesson completion applies card mutations atomically | e2e service test | 5.2, 4.4 | 0.5d |
| 5.4 | Review sessions + events: queue build ([05-review-engine.md](05-review-engine.md) §8), grade submit txn (event + state), idempotency, stale-hash 409, undo | service + tests §2 | 3.5, 5.1 | 2d |
| 5.5 | Backlog protection + budget throttling rules | `adaptation` tests | 5.4 | 0.5d |
| 5.6 | Review HTTP endpoints ([08-http-api.md](08-http-api.md) §6) + card endpoints (§5) | handlers + tests | 5.4 | 1d |
| 5.7 | Pack-driven answer evaluation wired into review reveal/compare | tests via synthetic pack | 5.4, 2.5 | 0.5d |
| 5.8 | Basic stats: review summary, forecast | endpoints | 5.4 | 0.5d |

## WBS 6 — Frontend core (M1+M2 UI) — ~8d

| # | Task | Deliverable | Depends | Est |
|---|---|---|---|---|
| 6.1 | App shell: `leptos_router` routes ([09-frontend.md](09-frontend.md) §3), context (profile, settings), `api_client` over `shared-schema`, `<ErrorBoundary>` per page | shell navigates | 1.5 | 1d |
| 6.2 | Dashboard page: `LocalResource` + `<Suspense>`, exceptions first | live data | 6.1, 4.5 | 1d |
| 6.3 | Lessons + import/export pages: copy tutor context, paste → validate → preview → apply | full no-MCP loop works | 6.1, 4.7 | 1.5d |
| 6.4 | Review screen: one card, reveal, 4 grades with definitions, `Action` submit (pending = double-submit guard), undo, keyboard 1–4/space, `<Transition>` between cards, summary | daily review usable | 6.1, 5.6 | 2d |
| 6.5 | Card management: list, filters, detail, suspend/restore, mutation history, redesign warnings | pages | 6.1, 5.6 | 1.5d |
| 6.6 | Learning items + errors + listening pages | pages | 6.1, 4.5 | 1d |

**M2 exit gate:** tutor creates cards via lesson completion; daily review end-to-end in UI; restart loses nothing; duplicates rejected.

## WBS 7 — Spanish pack (M3) — ~6d, parallel with 5/6 after 2.4

| # | Task | Deliverable | Depends | Est |
|---|---|---|---|---|
| 7.1 | Normalization + accepted-answer matching (case, diacritics, ¿¡, contractions, regional alternatives) | code + tests | 2.4 | 1.5d |
| 7.2 | Error taxonomy assets + mapping to core categories | `error-taxonomy/` | 2.4 | 1d |
| 7.3 | Card templates (4 intents) | `card-templates/` | 2.4 | 0.5d |
| 7.4 | Tutor instruction bundle, Russian prompts, correction conventions | `tutor-instructions/` | 2.4 | 1d |
| 7.5 | Scenario catalog + CEFR curriculum | `curriculum/` | 2.4 | 1d |
| 7.6 | Phrase seeds for initial curriculum | `phrase-seeds/` | 7.5 | 0.5d |
| 7.7 | Pack conformance green + asset schema validation | CI | 7.1–7.6 | 0.5d |

## WBS 8 — Adaptive loop (M4) — ~4d

| # | Task | Deliverable | Depends | Est |
|---|---|---|---|---|
| 8.1 | Adaptive rules ([07-tutor-contract.md](07-tutor-contract.md) §7) table-driven in `adaptation` | tests §4 | 5.4 | 1.5d |
| 8.2 | Recommendations: persistence, endpoints, dashboard surfacing | feature | 8.1 | 0.5d |
| 8.3 | Redesign candidates: detection ([05-review-engine.md](05-review-engine.md) §10) + endpoint + tutor-context inclusion | feature | 8.1 | 1d |
| 8.4 | Review evidence + redesign evidence in tutor context; lesson planning from evidence | richer context export | 8.3, 4.7 | 0.5d |
| 8.5 | 1/3/7-day recheck scheduling + escalation (3 → mini-target, 5 → method change) | tests | 8.1 | 0.5d |

## WBS 9 — Offline (M2 tail) — ~3d, parallel after 6.4

| # | Task | Deliverable | Depends | Est |
|---|---|---|---|---|
| 9.1 | Service worker: asset cache + dashboard summary cache | PWA offline shell | 6.2 | 0.5d |
| 9.2 | Due-batch prefetch + IndexedDB event queue | offline review works | 6.4 | 1.5d |
| 9.3 | Replay: ordered, idempotent, 409 reconciliation; reject offline semantic edits | sync tests | 9.2 | 1d |

## WBS 10 — Production hardening (M5) — ~6d

| # | Task | Deliverable | Depends | Est |
|---|---|---|---|---|
| 10.1 | Terraform: dev+prod D1, Worker config, Access app/policy, `prevent_destroy`, replacement gate | `infra/terraform/` applied | 1.6 | 1.5d |
| 10.2 | Cloudflare Access + Worker JWT verification | auth enforced | 10.1 | 1d |
| 10.3 | CD pipeline ([11-infrastructure.md](11-infrastructure.md) §4): approval, backup, plan gate, migrate, deploy, smoke | green prod deploy | 10.1, 1.7 | 1d |
| 10.4 | Backups: automated export, restore command, dry-run restore test | documented + tested | 10.3 | 1d |
| 10.5 | Observability: structured events + full `/health` | live | 4.5 | 0.5d |
| 10.6 | Playwright e2e suite ([12-testing.md](12-testing.md) §7) in CI | green | 6.x, 9.x | 1d |
| 10.7 | PWA install (iPhone), accessibility pass, performance validation vs targets §8 | checklist done | 6.x | 0.5d |
| 10.8 | Security review vs [10-security.md](10-security.md) §2 checklist | signed-off doc | all | 0.5d |

**Production v1 gate = DoD ([00-overview.md](00-overview.md) §6).**

## WBS 11 — Launch — ~1d

| # | Task | Deliverable |
|---|---|---|
| 11.1 | Prod deploy via CD; smoke; release metadata | live app |
| 11.2 | First real lesson via copy/import loop; first review session | validated in anger |
| 11.3 | Fix-forward list from first week | backlog |

## WBS 12 — Post-prod, optional (M6–M8)

- 12.1 MCP: feasibility validation → OAuth 2.1 → read tools → `complete_lesson` write → mobile check ([13-roadmap.md](13-roadmap.md) §3). ~5d, gated.
- 12.2 Second-pack validation: small pack, zero core changes proven. ~2d.
- 12.3 Personalization: FSRS optimization, forecasting, retention charts. ~4d, needs review-history volume.

## Totals

| Phase | Est |
|---|---|
| M0 (WBS 1–3) | ~13d |
| M1 (WBS 4) | ~7d |
| M2 (WBS 5, 6, 9) | ~19d |
| M3 (WBS 7, parallel) | ~6d |
| M4 (WBS 8) | ~4d |
| M5 + launch (WBS 10–11) | ~7d |
| **0 → prod (serial worst case)** | **~50 focused days** |

Parallelism (7 alongside 5/6) → realistic ~44–46 focused days solo.

## Rules

- All code follows [16-rust-guidelines.md](16-rust-guidelines.md) — clippy lint config lands in WBS 1.7.
- Never skip a milestone exit gate.
- Spike 3.2 decides FSRS vs fallback — decision recorded, either way 3.4 ships.
- No task starts before its `Depends` column is done.
- Scope creep = reject: anything not traceable to a spec file does not enter WBS.
