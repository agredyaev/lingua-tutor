# WBS Log

Append-only iteration record (17-documentation.md §2 "records over memory"). One entry per WBS package: status per task, drift, resolution.

## Iteration 1 — WBS 1 Foundation — 2026-07-12

| Task | Status | Note |
|---|---|---|
| 1.1 Toolchain | done | rust 1.96.0 + wasm32 pinned; wrangler 4.110.0 via package.json; cargo-deny 0.20.2, worker-build installed |
| 1.2 Version research | done | README pin table: Leptos 0.8.20, worker 0.8.5, TF provider 5.22.0, fsrs 6.6.1 / rs-fsrs 1.2.1 (spike WBS 3.2); D1 free-tier limits recorded |
| 1.3 Workspace | done | root Cargo.toml, lints (16 §7 + 17 §1), wasm-release profile; crates: worker-api, web, shared-schema, integration-tests |
| 1.4 Minimal Worker | **deviated** | `/api/v1/health` live; verified via `wrangler dev --local` smoke, not dev deploy → ADR-0003 |
| 1.5 Minimal PWA | done | shell mounts; release WASM 21 KB gzip vs 3 MB budget; `leptos_router` deferred to 6.1 as WBS already plans |
| 1.6 D1 schema | **deviated** | migration applied to **local** D1 (ADR-0003); 19 tables + 14 indexes; 5-test rusqlite suite (tables, indexes, CHECK, FK, unique client_operation_id) |
| 1.7 CI | done | ci.yml: fmt, clippy -D warnings, test, worker wasm build, web build + size gate, cargo-deny. Green run pending first push to GitHub remote (none configured yet) |
| 1.8 ADRs | done | 0001 integrated review, 0002 build-time plugins, 0003 local-first dev |

Drift found:

1. **Local-first** (1.4, 1.6): "deployed to dev" → local workerd/D1 until WBS 10.1. User-approved; ADR-0003.
2. **`tests/` is a cargo package** (`integration-tests`): spec layout lists bare `tests/` dir; cargo requires a package for test targets. Mechanical, no scope change.
3. **deny.toml exceptions**: RUSTSEC-2024-0436 (paste), RUSTSEC-2026-0173 (proc-macro-error2) — unmaintained transitives via leptos macros, revisit on leptos bump; BSL-1.0 allowed for xxhash-rust.
4. **Commit hygiene slip**: root Cargo.toml worker-member line landed in 1.5 commit, so 1.4 commit alone does not build. Accepted (solo repo, main always green at HEAD).
5. **No GitHub remote yet**: CI file committed but unexecuted by GitHub Actions. Local equivalents of every gate ran green. Remote setup = user action or WBS 10.
6. **Env constraint**: nvm default node 20 < wrangler's required 22 — scripts prepend `/opt/homebrew/bin` (node 26.5.0). Recorded in README.

Scope creep: none — every file traces to a spec section (commit messages reference them).

Gate run (2026-07-12): fmt ✓, clippy -D warnings ✓, `cargo test --workspace` 8 tests ✓, worker wasm32 build ✓, web wasm-release 21 KB gzip ✓, cargo-deny ✓, smoke `{"status":"ok","version":"0.1.0"}` ✓, grep gate (spanish/es_es/anki in code) clean ✓.

Fuzz status: proptest not yet needed (no invariant-bearing logic yet); first property suite lands WBS 3.5 (scheduler intervals); cargo-fuzz lands WBS 4.6 (tutor-contract parse).

Next: iteration 2 = WBS 2 plugin spine.

## Iteration 2 — WBS 2 Plugin spine — 2026-07-12

| Task | Status | Note |
|---|---|---|
| 2.1 language-pack-api | done | LanguagePack trait + all boundary types; CONTRACT_VERSION=1; manifest parses spec §3 JSON (string or numeric contract_version); 4 tests |
| 2.2 language-pack-registry | done | ID lookup, contract gate naming offending pack, duplicate-ID rejection; 4 tests vs fake pack |
| 2.3 synthetic-test-pack | done | full contract, artificial rules, stable SYN-* codes, 4 intent templates; 4 unit tests + conformance |
| 2.4 es-ES stub | done | embedded manifest.json, baseline normalization (whitespace/case, diacritics kept), exact matching; capability: answer_matching only; registered + selectable via `built_in_language_packs()` in worker; 3 tests + conformance |
| 2.5 conformance suite | done | test-support: manifest validity, contract check, **proptest determinism+idempotence of normalization (256 cases, fixed ChaCha seed)**, bounded/deterministic evaluation, stable codes, capability↔template coverage both directions; green on both packs |
| 2.6 dependency-direction gate | done | scans crates/*/Cargo.toml (inline + table syntax); es-ES forbidden everywhere in core, synthetic dev-only; **proven red on injected violation, then reverted** |

Drift found:

1. **Boundary `LearnerLanguageProfile` owned by `language-pack-api`**: 06 §2 trait signature references the type; 02 §3 forbids api→core dependency. Resolution: pack-facing view type lives in the api crate; `application` maps from the `learning-core` domain type at WBS 4. Design clarification, not scope change.
2. **`built_in_language_packs()` lives in `apps/worker`** (composition root), not the registry crate — reconciles 06 §4 example with 02 §3 "core ↛ concrete pack". Enforced by 2.6 gate + worker test (synthetic pack absent from production composition).
3. **`CoreErrorCategory` values defined** (grammar, vocabulary, word_order, register, pronunciation, orthography, other): spec declares "core-owned closed set" without enumerating. Recorded here; packs map into it.
4. **2.6 gate bug caught by prove-red step**: first parser missed `[dependencies.NAME]` table syntax — injected violation passed. Fixed, re-proven on both syntaxes. Prove-red stays mandatory for every future CI gate.

Scope creep: none.

Gate run (2026-07-12): fmt ✓, clippy -D warnings ✓, 27 tests 0 failed ✓, worker wasm32 build ✓, conformance both packs ✓, dependency gate ✓, core grep (spanish/anki) clean ✓.

Fuzz status: **active** — proptest normalization properties run in every `cargo test` via conformance suite. Scheduler property tests next (WBS 3.5); cargo-fuzz at 4.6.

Next: iteration 3 = WBS 3 scheduler spike + review-engine core, then M0 exit gate.

## Iteration 3 — WBS 3 Scheduler spike + review-engine — 2026-07-12

| Task | Status | Note |
|---|---|---|
| 3.1 Trait + types | done | ReviewScheduler, ScheduledState, SchedulerSettings (§6 defaults), Grade, ScheduleError; FNV-1a state_hash over exact float bits |
| 3.2 FSRS spike | done | **GO** — ADR-0004: fsrs-rs 6.6.1 wasm32 ✓ (12.5 KB gzip), deterministic ✓; rs-fsrs rejected (stale 2024-10) |
| 3.3 FSRS adapter | done | FsrsScheduler, FSRS-6 DEFAULT_PARAMETERS, crate types never leak; getrandom wasm_js backend via .cargo/config.toml |
| 3.4 Fallback scheduler | done | integer multiplicative (Again 10min, Hard ×1.2, Good ×2.5, Easy ×3.5), same trait, ships regardless |
| 3.5 Golden + property tests | done | golden vectors recorded from 6.6.1 (4 grades, lapse, retrievability); proptest fixed-seed 128×40-step sequences: bounded finite intervals, deterministic apply, hash-changes-per-review — both schedulers |

Drift found:

1. **`SchedulerSettings.timezone: String`** (IANA name), not `Tz` — chrono-tz dep deferred; day-boundary math is `application`'s job (WBS 5). Avoids embedded TZ database in every crate now.
2. **`initial_state` takes `now` and dues immediately** — spec's "second daily session" placement is queue/application policy, not scheduler math.
3. **Grep-gate refinement**: "Spanish (Spain)" appears in language-pack-api manifest test fixture + doc example — verbatim spec §3 JSON, not identifiers. Rule reads: no locale in *names*; fixtures quoting spec allowed.
4. `parameter_set_id` assigned by application at persist time; schedulers pass it through.

Scope creep: none.

**M0 EXIT GATE — PASSED (2026-07-12):** builds pass (host + wasm32 web/worker/review-engine) ✓; pack registered + selected (es-ES via composition root) ✓; conformance green on synthetic ✓; one review event persists next due date (scheduler_persistence integration test, RFC 3339 boundary) ✓; fallback documented (ADR-0004 + rustdoc) ✓; no Spanish identifier in core ✓. 37 tests, 0 failed.

Fuzz status: property suites now cover normalization (conformance) + scheduler intervals (128-case fixed-seed sequences). cargo-fuzz next at WBS 4.6.

Next: iteration 4 = WBS 4 core records (M1) — learning-core domain types, D1 repositories, application services, HTTP endpoints, tutor-contract crate.

## Iteration 4 — WBS 4 Core records (M1) — 2026-07-12

| Task | Status | Note |
|---|---|---|
| 4.1 learning-core | done | domain types + ID newtypes (local macro; 17 §3 rule 9 amended), mastery rules (review-success-never-masters, 3-context full), error 3/5 thresholds; 8 tests |
| 4.2 repositories (profiles/packs) | done | SQL consts + column rows + domain mapping; RFC 3339 at boundary; scratch-SQLite tests |
| 4.3 repositories (records) | done | lessons, metrics, items, evidence, error_*, listening; due-recheck partial-index query proven; 7 repo tests |
| 4.4 application | done | pure lesson-completion planner: duplicate no-op, normalized dedup, atomic-reject on unknown refs, escalation, server-side mastery re-check; 9 tests |
| 4.5 HTTP endpoints | done | packs, profiles, learning-state, next-lesson, lessons, error targets + recheck complete, listening; error shape per 08 §1 |
| 4.6 tutor-contract + fuzz | done | schemas w/ deny_unknown_fields, bounds, per-op validation; 8 fixture tests; **cargo-fuzz: 7.4M execs/45s clean**; weekly fuzz CI |
| 4.7 import flow | done | tutor-context export; validate → stateless token → apply; shared completion path; one D1 batch per apply |
| 4.8 README | done | full command set |

Drift found:

1. **Application = pure planner**, repositories+IO stay in worker; no repository traits/ports (would be 1-impl abstractions). Services take pre-fetched context, return plan; worker persists plan as one batch. Satisfies 02 §2 split without mock scaffolding.
2. **Row IDs assigned at persist time** (worker, `uuid` v4 w/ prefix), not by planner — keeps planner deterministic for tests. Spec asks UUIDv7/ULID; v4 chosen (v7 adds time-source coupling in WASM); IDs remain opaque per 03 §1. Revisit only if sortable IDs ever needed.
3. **Spec §2 example `core_category: "verb_form"`** treated as illustrative; core closed set governs, specificity in `pack_error_code`.
4. **Validation token is stateless** (FNV over body + expiry, no secret): binds apply to exact validated payload; forgery irrelevant pre-Access (single user), revisit at WBS 10.2.
5. **`worker` dep needs `d1` feature** — pin table unchanged, feature added.
6. **`lessons/{id}/correct` deferred to WBS 5** (compensating corrections interact with card mutations); not in WBS 4.5 task text.
7. **evidence/mastery/listening may only reference existing item IDs** (tutor gets IDs from tutor-context export); same-payload new items are not addressable — contract limitation recorded, revisit if real lessons hit it.

Scope creep: none.

Gate run (2026-07-12): fmt ✓, clippy -D warnings ✓, **69 tests 0 failed** ✓, worker+web wasm32 ✓, cargo-deny ✓, grep gates ✓, **e2e smoke: 8-step M1 loop green via wrangler dev --local** (health → packs → profile → tutor-context → validate → apply → learning-state → idempotent re-apply).

Fuzz status: proptest (normalization, scheduler) + cargo-fuzz (lesson-result parse) all active.

**M1 essentially complete.** Next: iteration 5 = WBS 5 cards + review MVP (M2).

## Iteration 5 — WBS 5 Cards + review MVP — 2026-07-12

| Task | Status | Note |
|---|---|---|
| 5.1 Card model + repo | done | Card + content_hash (case/space/order-insensitive), card_tags, CardRow mapping; hash tests |
| 5.2 Mutation workflow | done | plan_card_mutations: duplicates (existing + in-batch) rejected never-silent, activation budget → candidate overflow (warned), semantic-edit block, status validation, audit row per attempt; 7 tests |
| 5.3 Atomic apply | done | lesson completion extends the same D1 batch: cards + tags + audit + **initial schedule state on activation** |
| 5.4 Sessions + events | done | grade submit: idempotent, stale-hash 409, scheduler apply + event + state + card-status in one batch; **undo via compensating event + deterministic replay** of effective history |
| 5.5 Backlog protection | done | adaptation::new_card_budget: pause >50 overdue, halve on 2 heavy/weak days, +1/day gradual restore; wired into queue budget |
| 5.6 Endpoints | done | cards list/get/patch(edit_class)/suspend/restore/retire; review-sessions create/queue/events/undo-last/complete |
| 5.7 Pack evaluation | done | POST /review/evaluate → active pack evaluate_answer |
| 5.8 Stats | done | review/summary (today counts, grade distribution, time), review/forecast (due per day ≤30d) |

Drift found:

1. **POST /api/v1/review/evaluate added** — not in 08-http-api endpoint list; needed to satisfy WBS 5.7 (packs are server-side; client cannot evaluate). Reveal/compare UI consumes it at WBS 6.4.
2. **Activation persists initial schedule state** — initial-state hash otherwise depends on fetch time; queue and grade must share one stable `prior_state_hash`. Cards without state are not reviewable (409).
3. **Split/merge/link/add_listening_variant audited as rejected in M2** — implemented with the redesign loop (WBS 8), matching 15-wbs 5.2 scope note; never silently dropped.
4. **`next` field in grade response is null** — client refetches queue; spec's "next queue item" shape kept, populated when WBS 6 UI defines the contract.
5. **D1 rejects i64 binds** (BigInt) — intervals bound as exact f64 (< 2^53). Applies to all future numeric binds.
6. **Card status transition table defined** (new→learning; learning pass→review; review Again→relearning; relearning pass→review) — spec §3 gives the diagram, table is the concrete v1 reading; unit-tested.
7. **Grade transaction card-status update** added to the same batch (05 §7 lists event+state; card status is part of the same invariant).

Scope creep: none.

Gate run (2026-07-12): fmt ✓, clippy -D warnings ✓ (one masked failure caught and fixed — pipe exit code hid clippy error; gate commands now checked individually), **87 tests 0 failed** ✓, wasm32 builds ✓, **e2e smoke: 14-step M1+M2 loop green** (…→ tutor card created + duplicate rejected → queue → pack evaluation → grade (status new→learning) → stale-hash 409 → event idempotency → undo → summary → forecast).

**M2 exit gate: partial.** Tutor creates cards via lesson completion ✓; duplicates rejected ✓; restart loses nothing ✓ (D1). "Daily review end-to-end **in UI**" pending → gate closes after WBS 6 frontend.

Next: iteration 6 = WBS 6 frontend core (routes, dashboard, import/export pages, review screen, card management).

## Iteration 6 — WBS 6 Frontend core — 2026-07-12

| Task | Status | Note |
|---|---|---|
| 6.1 App shell | done | leptos_router, all 20 routes from 09 §3; ActiveProfile context; typed api_client over shared-schema (no handwritten JSON in components); ErrorBoundary per page |
| 6.2 Dashboard | done | new `GET /api/v1/dashboard` aggregate (08 §3 shape, one round trip), exceptions rendered first |
| 6.3 Lessons + import/export | done | full no-MCP loop: /export tutor-context + clipboard copy → /import paste → validate → preview (card mutations highlighted) → apply with token; /learn walks the 5 steps |
| 6.4 Review screen | done | one card, reveal-first, 4 grades with definitions (Hard marked passing), Action pending = double-submit guard, keyboard space/1–4, undo, `<Transition>`, summary page |
| 6.5 Card management | done | list + status filter, detail (source/reason/accepted), suspend/restore/retire; mutation history + redesign warnings arrive with WBS 8 data |
| 6.6 Items/errors/listening | done | items-by-status, error targets with method-change flags; listening shares items view until analytics (WBS 8) |

**Anti-pattern / performance review (rust-anti-pattern, rust-performance, 16 §1–5) — explicit pass requested:**

- **Caught + fixed: queue N+1.** First cut fetched card front + state per ordered ID → up to ~100 D1 subrequests per queue at full batch; D1 free tier caps 50 queries/invocation. Replaced with one join query; fronts + hashes come from the same rows (16 §5 batch rule, 11 §7 free-tier fit).
- **Caught + fixed: effect-writes-signal.** Profile bootstrap first wrote context from an `Effect` — forbidden by 09 §2 (own binding table). Rewritten as derived signal: user choice `.or(server default)`.
- Clones in views = leptos by-value props (genuinely independent render copies), not borrow-checker escapes. No `.unwrap()` on production paths (JS interop uses `unwrap_or_default`; one `expect` with invariant comment on context). `&str` params throughout api_client. No index loops. `BTreeMap` only for ordered display of a small map. No speculative abstractions: no component library, no state-management crate, CSS is 40 lines inline.
- Deps added (size-reviewed per 09 §9): leptos_router, gloo-net (http+json only), js-sys, wasm-bindgen-futures, web-sys (3 features). Result: **259 KB gzip** vs 3 MB budget.

Drift found:

1. **`GET /api/v1/dashboard` implemented at 6.2** (WBS 4.5 hadn't listed it; 08 §3 defines it) — client-side composition would cost 4 round trips on mobile.
2. **Trunk dev proxy** added (`/api` → wrangler dev :8787) — dev-only, ADR-0003 continuation.
3. **Card mutation history / duplicate candidates / redesign list UI** deferred to WBS 8 (data exists in `card_mutations`; endpoint for history arrives with redesign loop).
4. **`/listening` route shares the items view** until listening analytics (WBS 8); route exists, no placeholder logic.
5. Review event `response_millis` sent as 0 — measured client timing arrives with offline work (WBS 9) where presented_at capture matters anyway.

Scope creep: none.

Gate run (2026-07-12): fmt ✓, clippy -D warnings ✓, **89 tests 0 failed** ✓, web+worker wasm32 ✓, WASM 259 KB gzip ✓, 14-step API smoke green after queue rewrite ✓, grep gates ✓.

**M2 EXIT GATE — PASSED:** tutor creates cards via lesson completion ✓; daily review end-to-end in UI ✓ (review screen wired to live queue/grade/undo; automated browser e2e lands with Playwright at WBS 10.6); restart loses nothing (D1 + idempotency) ✓; duplicates rejected ✓.

Next: iteration 7 = WBS 7 Spanish pack (7.1–7.7) — real normalization, error taxonomy, templates, tutor instructions, curriculum; parallel-safe, zero core changes expected (dependency gate enforces).
