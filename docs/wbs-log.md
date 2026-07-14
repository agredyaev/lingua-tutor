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

## Iteration 7 — WBS 7 Spanish pack (M3) — 2026-07-12

| Task | Status | Note |
|---|---|---|
| 7.1 Normalization + matching | done | canonical keeps case/diacritics/¿¡; comparison drops case + prosody punctuation, contracts `de el`→`del`/`a el`→`al`, **keeps diacritics** (tú≠tu); accent-only miss → Incorrect + "revisa las tildes" feedback; comma bug caught by regional-variant test |
| 7.2 Error taxonomy | done | `error-taxonomy/taxonomy.json`: 8 stable `es.*` codes → core categories, Russian guidance; classifier = 8 bounded first-match rules (ser/estar, por/para, gender, order, accent, vocab) |
| 7.3 Card templates | done | 4 intents, Russian prompt templates, `card-templates/templates.json` |
| 7.4 Tutor instructions | done | `tutor-instructions/*-ru.md`: Spain-Spanish conventions (vosotros/coger/vale), level-aware, correction protocol referencing error_target_id + es.* codes |
| 7.5 Curriculum | done | 12 CEFR scenarios A1–B1, life-in-Spain (empadronamiento, farmacia, alquiler…) |
| 7.6 Phrase seeds | done | 20 A1 seeds with Russian prompts, every seed cross-referenced to a scenario |
| 7.7 Conformance + asset validation | done | shared suite green; strict asset schemas (`deny_unknown_fields`), code uniqueness, seed→scenario referential checks |

**Anti-pattern / performance review (explicit pass):**

- No regex/ICU deps — plain char ops, single-pass canonicalization with `String::with_capacity` (09 §9 size rule; 16 §5). Pack adds ~0 measurable WASM weight.
- Fixed word sets = const arrays + linear scan, not HashMap (16 §5 small-set rule). All classifier rules bounded linear in input length; conformance re-proves determinism/idempotence via 256-case proptest.
- Taxonomy lookup = linear over 8 entries at classify time — measured set, no premature indexing.
- `expect` only on embedded-asset parse with invariant comment (tests + conformance make failure a build defect). Dead-code allows are annotated with the consuming milestone (seeds → WBS 8.4).

Drift found:

1. **Caught: `lessons/start` hardcoded pack version "0.1.0"** — invisible until the manifest bumped to 0.2.0. Now reads the registry manifest. Historical rows keep the version that created them (06 §5) — bug class to watch at every pack bump.
2. **Comparison drops internal commas** — policy addition (prosody, not meaning); without it accepted-variant matching failed on natural tutor punctuation.
3. Seeds asset validated but not yet surfaced (enters tutor context at WBS 8.4) — `cfg_attr` dead-code allows document that.
4. Core grep hits = test fixtures quoting spec §2 example + manifest fixture — allowed per iteration-3 refinement; zero identifiers.

Scope creep: none. Zero core-crate changes (dependency gate held; the one worker change was the version bug fix).

Gate run (2026-07-12): fmt ✓, clippy -D warnings ✓, **98 tests 0 failed** ✓, conformance both packs ✓, 14-step smoke green ✓, wasm32 builds ✓.

**M3 complete.** Next: iteration 8 = WBS 8 adaptive loop (M4) — table-driven rules in `adaptation`, recommendations persistence + endpoints + dashboard, redesign candidates, richer tutor context, recheck escalation wiring.

## Iteration 8 — WBS 8 Adaptive loop (M4) — 2026-07-12

| Task | Status | Note |
|---|---|---|
| 8.1 Rule table | done | 07 §7 as explicit Rust in `adaptation`: failed-review→practice, pass-but-spontaneous-fail→transfer (not more reviews), listening card, transfer target, retirement (volume+silence guards), escalation 3/5, recheck-failed; 13 unit tests + cross-crate threshold pin |
| 8.2 Recommendations | done | persistence w/ open-duplicate guard (no spam), GET /recommendations, accept/dismiss, dashboard top-5 in DTO + web view |
| 8.3 Redesign candidates | done | 05 §10 detectors: ≥3 Again in window; ≥3 Hard + ≥10s latency; review-pass-conversation-fail; GET /cards/redesign-candidates joins card fronts; in tutor context |
| 8.4 Tutor context evidence | done | review_summary_7d, redesign_candidates, open_recommendations, curriculum_scenarios added to export |
| 8.5 Recheck escalation | done | failed recheck → practice_again recommendation; import escalations (3/5) → mini-target/method-change recommendations |

**Anti-pattern / performance review (explicit pass):**

- Signals for a whole session come from **two fixed queries** (ROW_NUMBER window over last-10 effective events + one grouped evidence join) — no per-card N+1; compensated events excluded in SQL, not post-filtered.
- Rules are pure fns over bounded windows (≤ [`GRADE_WINDOW`]=10); no allocation beyond the output Vec; stable order; determinism unit-pinned.
- `adaptation` still has **zero deps** — thresholds mirrored, sync enforced by an integration test instead of a dependency edge (thinner than coupling the crates).
- No trait/dispatch for the rule table: one function, straight conditionals — closed set, enum-over-abstraction rule.
- Recommendation dedup is per-item SELECT (bounded by rule output, ≤ ~6/session); acceptable vs building a unique index migration for M4 — noted for revisit if volume grows.

Drift found:

1. **Recommendation endpoint paths defined here** (`/api/v1/recommendations`, `/accept`, `/dismiss`) — 08-http-api.md names dashboard surfacing but no resource routes; follows §1 conventions.
2. **05 §10 detectors subset**: "failure despite successful related cards" and "conflict with another card" need card-relation data that doesn't exist yet — deferred, documented; "learner-reported ambiguous prompt" arrives with a UI affordance later.
3. Rule "success in 3 distinct contexts" not re-emitted as recommendation — already auto-assessed server-side at mastery re-check (iteration 4); duplicate signal would be noise.
4. Tutor context now includes curriculum scenarios (pack catalog) — satisfies "lesson planning from evidence" without exposing pack seeds through the trait (contract unchanged).

Scope creep: none.

Gate run (2026-07-12): fmt ✓, clippy -D warnings ✓, **107 tests 0 failed** ✓, wasm32 builds ✓, **17-step smoke green** (…15 rules fire on session complete, 16 redesign candidate listed, 17 dashboard + context surfacing + dismiss) ✓.

**M4 complete.** Next: iteration 9 = WBS 9 offline (M2 tail) — service worker, due-batch prefetch + IndexedDB queue, ordered idempotent replay with 409 reconciliation.

## Iteration 9 — WBS 9 Offline (M2 tail) — 2026-07-12

| Task | Status | Note |
|---|---|---|
| 9.1 Service worker | done | sw.js: cache-first same-origin static assets, network-first `/api/v1/dashboard` snapshot, API otherwise network-only; PWA manifest + registration |
| 9.2 Prefetch + queue | done | queue cached on every online fetch (`lt-prefetch-queue`); grades queue to `lt-offline-events` on network failure with measured latency; **hard bound 50 — full queue stops the session, never drops history** |
| 9.3 Replay | done | on review entry: ordered replay; success/duplicate → dequeue; `conflict_stale_state` → conflict audit list (`lt-conflict-events`), local copy dropped, never guessed; network error → stop and stay offline; offline banner in UI |

**Anti-pattern / performance review (explicit pass):**

- **ADR-0005**: localStorage over IndexedDB for a ≤50-event (~15–60 KB) queue — async IDB schema/versioning machinery or a wrapper dep would be speculative for this size (16 §1). Functional guarantees identical (durable, ordered, bounded, idempotent). Revisit trigger written down (~1 MB or indexed lookup).
- Queue ops split pure/IO: parse/append/serialize are host-tested pure fns (corrupt storage → empty queue, never panic); Storage access is two 4-line wrappers. No mocking.
- `response_millis` now measured (reveal→grade instant via non-reactive `StoredValue` — no signal-from-effect) and clamped ≤ 10 min.
- No new crate deps for offline (web-sys `Storage` feature only). WASM 266 KB gzip (+7 KB).

Drift found:

1. **IndexedDB → localStorage** (ADR-0005) — the one deliberate spec deviation this iteration; functional requirements of 09 §7 all met.
2. **Repeat-grading the same card offline** produces `conflict_stale_state` for the second+ events on replay (hash advanced server-side) — those land in the conflict audit list exactly as spec prescribes ("reconcile safely, never guess"). Documented behavior, not a bug.
3. Offline semantic edits: no offline path exists for PATCH (network error surfaces to the user) — client-side rejection requirement satisfied structurally.
4. Browser-level offline e2e (airplane-mode round trip) deferred to Playwright (WBS 10.6); pure logic + server idempotency/409 paths are covered by tests + smoke today.

Scope creep: none.

Gate run (2026-07-12): fmt ✓, clippy -D warnings ✓, **110 tests 0 failed** ✓, wasm32 builds ✓, WASM 266 KB gzip ✓, 17-step smoke green ✓.

Next: iteration 10 = WBS 10 production hardening (M5) — Terraform (D1/Worker/Access, prevent_destroy), Access JWT verification, CD pipeline, backups + restore drill, full /health, Playwright e2e, PWA/perf/security checklists.

## Iteration 10 — WBS 10 Production hardening (M5) — 2026-07-13

| Task | Status | Note |
|---|---|---|
| 10.1 Terraform | done | account-scoped provider v5; D1 dev+prod (`prevent_destroy`), Access self-hosted app on workers.dev origin + standalone owner-email policy (v5 schema: policy attached via `policies=[{id,precedence}]`); outputs (aud, D1 ids) wired into `wrangler.jsonc`; state/lock gitignored |
| 10.2 Access JWT | done | worker-side RS256 verify of `Cf-Access-Jwt-Assertion` (pure-Rust `rsa`+`sha2` — no ring on wasm), JWKS from team domain, aud string-or-array, exp, owner-email claim; `AUTH_ENFORCED` off locally, on in `env.production` |
| 10.3 CD pipeline | done | `deploy-production`: dispatch-only + `production` environment approval → gates → pre-migration backup → terraform plan drift gate (`-detailed-exitcode`) → prod migrations → trunk build → wrangler deploy (PWA as Worker static assets, single origin) → runtime secret sync → prod smoke → release metadata |
| 10.4 Backups | done | daily `backup.yml`: D1 export artifact + `sqlite3` dry-run restore drill |
| 10.5 Full /health | done | version, D1 reachability, migration count, pack registry, auth mode |
| 10.6 Playwright e2e | done | core loop (profile → import → review → undo → dashboard) against `wrangler dev --local`; separate CI job; trace/screenshot retained on failure |
| 10.7 Checklists | done | `m5-security-review.md` 13/13 (HMAC-signed validation token fixed in-code, not risk-logged), `m5-launch-readiness.md`; owner one-command follow-ups: long-lived CI token before 2026-07-21, CF 2FA |
| 10.8 Secrets | done | GitHub env `production` (required reviewer = owner), `CLOUDFLARE_API_TOKEN`/`CLOUDFLARE_ACCOUNT_ID`/`TOKEN_SIGNING_KEY`; CF has no OIDC federation — env-protected secrets are the standard |

**Anti-pattern / performance review (explicit pass):**

- **e2e flake root-caused, not retried away**: undo replay seeded `initial_state(now)` — wall clock in a replay path. Zero surviving events → replayed `due_at` = undo second ≠ apply second → `prior_state_hash` mismatch → 409 on next grade (~1/5 locally, worse on CI). Fix: pure `review_engine::replay()` (clock access impossible by construction), undo seeds from `cards.created_at`; 3 regression tests pin zero-event identity, fold equality, purity. `retries: 0` stands — flaky test was a bug, per 16 §6.
- Same lens killed a second silent-wall-clock path: `submit_event` substituted `now` for unparsable `answered_at` (`unwrap_or_else` fallback) — would diverge from replay; now explicit 400 `invalid_timestamp`.
- Auth verify is per-request pure computation over a fetched JWKS; no ring dependency (wasm), RSA verify-only justifies the Marvin advisory ignore in `deny.toml`.
- `export/full` = 15 fixed bounded queries (`LIMIT 10000`), no N+1 joins through parents; CSV built with `String::with_capacity`, quotes escaped, no per-row allocation churn.
- Queue remains single-join (no per-card D1 subrequests) — D1 free-tier 50-queries cap respected.

Drift found:

1. **WBS "dev deploy" rows replaced by local-first** (ADR-0003) — cloud contact deferred to this iteration as decided; Terraform applied directly to prod account (4 resources).
2. **Zero Trust required a card on file** even on free tier — owner attached; team `absent-twin.cloudflareaccess.com`.
3. **Access policy schema drift** (provider v5 split app/policy) vs 11-infrastructure sketch — resolved per current provider docs, spec sketch outdated, code is source of truth.
4. **Production deploy dispatch is an owner action by design** (environment approval); pipeline proven to green on CI, first prod deploy pending owner `gh workflow run deploy-production` + approval.
5. Export scope grew beyond WBS wording (full JSON + cards.csv + tutor-context self-sufficiency) — traceable to 08 §7 and the LLM-tutor effectiveness audit; recorded, not creep.

Scope creep: none.

Gate run (2026-07-13): fmt ✓, clippy -D warnings ✓, **113 tests 0 failed** ✓ (incl. new replay suite), wasm32 builds ✓, 17-step smoke green ✓, ci+e2e green on `2e18b8e` ✓, local e2e stress 7/7 test-passes (1 wrangler spawn timeout, infra not test) ✓.

**M5 code complete.** Remaining to close M5: owner dispatches `deploy-production` + approves; prod smoke; iPhone PWA install (10.7 owner item). Next: iteration 11 = WBS 11 first real lesson + fix-forward list.

## Iteration 11 — WBS 11 first real lesson (owner-in-loop)

- 11.1 prod deploy: done in iteration 10 close-out (run 29317910454 green: gates, D1 backup, terraform drift gate, migrations, deploy, Access-aware smoke).
- 11.2 first real lesson: owner checklist at `docs/checklists/first-real-lesson.md`; execution pending owner.
- 11.3 fix-forward list: capture below as items surface.

### Fix-forward (11.3)

_(empty — owner adds items while running the checklist; severity: blocker / annoying / cosmetic)_

## Iteration 12.1 — MCP over Streamable HTTP + embedded OAuth 2.1 (M6)

Commits: 5c39920 (storage+ADR-0006), 7b5d927 (path classifier + gate), 6df3179
(discovery+DCR), 07d23ad (authorize+token), 1b8028e (transport+scopes),
47de575 (read tools), b1e52a7 (write tools), + infra/smoke (this commit).

- Transport: hand-rolled JSON-RPC over `POST /mcp` (stateless Streamable
  HTTP; GET/DELETE 405). No new crates anywhere in the iteration.
- OAuth 2.1 embedded in the worker: discovery metadata, RFC 7591 DCR
  (redirect hosts allowlisted to chatgpt.com/openai.com; localhost only when
  AUTH_ENFORCED=false), PKCE S256 only, single-use hashed codes (10 min),
  opaque tokens hashed at rest (access 1 h; refresh 30 d rotated on use),
  scope subset validation, opportunistic expiry purge in the token handler.
- Identity: `/oauth/authorize` stays behind Access; `auth::verify_owner`
  extracted and re-verified in-worker. Every token acts as the owner bounded
  by scopes (single-user, ADR-0006 §4).
- Tools: all 14 roadmap §3 tools + `get_tutor_context`; reads bridge to the
  existing handlers via synthetic internal requests (no duplicate queries);
  `complete_lesson` funnels into the same `complete_from_result` path as the
  UI import — idempotency via `client_operation_id` held.
- Refactors for reuse (mechanical): `parse_and_validate`,
  `complete_from_result`, `build_mutation_context` (takes `&[CardMutation]`),
  `push_card_mutation_statements` (takes mutations+pack version,
  `Option<lesson_id>`), `push_rechecks` (takes `Timestamp`).
- Edge topology: Terraform adds one `bypass` policy + five exact-path Access
  applications (`/mcp`, `/oauth/token`, `/oauth/register`, two well-known).
  `/oauth/authorize` deliberately not bypassed. `run_worker_first` grew to
  cover `/mcp`, `/oauth/*`, `/.well-known/*` (assets would swallow them).
- Gate: `MCP_ENABLED` (default off; prod on after Terraform). Off → 404 on
  every MCP/OAuth path.
- Token-leak audit (owner request): hashes-only at rest; zero logging calls
  in the worker; error texts never echo secrets; access tokens never in
  URLs; consent page same-origin only; smoke prints no token material.
- Verification: `scripts/smoke-mcp.sh` 15/15 locally (metadata, DCR, PKCE
  good+bad, code single-use, 401+WWW-Authenticate, GET 405, initialize,
  tools/list 15, read tool, complete_lesson + duplicate noop, scope
  enforcement, refresh rotation, disabled-gate 404). Deploy smoke extended
  with the prod auth-matrix asserts.

Drift: 13-roadmap §3 lists 14 tools — `get_tutor_context` added as a 15th
(traceable to 08 §7 tutor-context export; primary lesson entry point).
Recorded, not creep. `suspend_cards`/`upsert_cards` audit rows use
`source_lesson_id = NULL` outside lessons — schema already allowed it.

Owner actions to close M6: run `docs/checklists/mcp-connector.md`
(desktop + mobile), after `terraform apply` + deploy.

## Iteration 12.2 — zh-CN second pack (M7)

Commits: 7c140e1 (pack), 49fc716 (composition proof), cf6f45c (quote-escape fix).

- `language-packs/zh-cn`: Mandarin (Simplified), HSK framework, 4 card
  templates, 2 scenarios, seed phrase. CJK-aware normalization: canonical
  drops whitespace adjacent to hanzi while keeping single spaces in Latin
  runs (pinyin/names); comparison additionally strips ASCII+CJK punctuation
  and lowercases pinyin. Deterministic error taxonomy: zh.orth.form /
  zh.gram.structure / zh.voc.choice.
- Shared conformance suite passes unchanged (incl. proptest normalization
  idempotence) — the suite needed zero edits for a no-whitespace script.
- Composition test: es-ES + zh-CN in one registry, no collisions.
- **Zero core changes proof (M7 exit gate)**: pack commit 7c140e1 touches
  only `Cargo.toml`/`Cargo.lock` (member registration) + new files under
  `language-packs/zh-cn/` — no `crates/*`, no `apps/*` diffs. Dependency
  direction CI unchanged and green.
- Production composition still es-ES-only (worker lib.rs test enforces);
  zh-CN registers via test composition until the owner opts in.

Drift: none. M7 validated.

## Iteration 12.3 — personalization (M8)

Commits: be14789 (retention endpoint + stats page), 65539ee (fsrs-optimize).

- `GET /api/v1/review/retention`: observed retention (grade > Again) in
  weekly buckets from effective events; undo lineage excluded both ways in
  SQL (`SELECT_EVENTS_FOR_RETENTION`, rusqlite-tested). `/stats` page renders
  retention weeks + 7-day due forecast as text bars — no chart JS
  (09-frontend.md).
- `crates/fsrs-optimize`: native-only bin (never a worker dep — training is
  not wasm work). Reads `export/full` JSON, rebuilds effective per-card
  review prefixes (undo-aware, delta_t in days, first review 0), trains via
  `fsrs::compute_parameters`, prints the parameter set + apply pointer.
  **Data gate: refuses under 400 effective events** (05 §11 — defaults beat
  an overfit). Verified end-to-end on a 480-event synthetic fixture → 21
  parameters.
- M8 skips (declined-until-needed): signed pack distribution — single-owner
  build-time packs make signing pointless today; Anki export adapter — the
  in-app FSRS review subsystem IS the built-in replacement (ADR-0001, owner
  confirmed); JSON/CSV exports remain the substrate if ever needed.

Drift: none. M8 delivered to its "optional, not required for core" scope.
