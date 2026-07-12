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
