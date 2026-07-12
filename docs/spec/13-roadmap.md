# 13 — Roadmap, Deferred Specs, Risks

Scope: milestones M0–M8, thin specs for deferred items (MCP, optimization, second pack, Anki export), risk register.
Depends on: all component files.

## 1. Milestones

### M0 — Compatibility + architecture spike

Deliver: workspace + documented crate responsibilities; minimal Leptos PWA; minimal Worker; D1 binding + first migration; language-neutral domain primitives; `LanguagePack` trait + manifest schema; `LanguagePackRegistry` with synthetic + es-ES packs; shared conformance suite; `ReviewScheduler` trait; FSRS crate compile spike + binary-size measurement; deterministic fallback scheduler; basic schedule golden test; Terraform skeleton; CI gates + dependency-direction checks; ADRs for integrated review subsystem + plugin model; MCP feasibility note.

Exit: web + Worker builds pass; one pack registered + selected by profile; core contract tests pass on synthetic pack; one review event calculates + persists next due date; fallback path documented; no Anki dependency; no Spanish-specific field in core schemas.

### M1 — Core records

Deliver: learner profile; pack metadata persistence; learner language profiles; lessons + metrics; learning items; errors; listening attempts; dashboard shell; versioned tutor-context export; versioned lesson-result validate/import; card-mutation contract types (no full review UI); D1 repositories + integration tests; migrations; README with exact local commands.

### M2 — Review + cards MVP

Deliver: generic card model; tutor card-mutation workflow; due queue; review UI; review events; scheduling state; basic stats; backlog protection; offline event queue; pack-driven answer evaluation.

Exit: tutor creates cards via lesson completion; daily reviews work; restart loses nothing; duplicate events rejected; no manual card maintenance needed; review-engine tests pass without a real pack.

### M3 — Spanish pack completeness

`language-pack-es-es`: manifest; Russian prompts; normalization + accepted-answer policies; error taxonomy; tutor instruction bundle; scenario catalog; card templates; phrase seeds for initial curriculum; conformance tests.

### M4 — Adaptive tutor loop

Review evidence in tutor context; redesign candidates; listening/production gap recommendations; split/merge/suspend workflows; lesson planning from review evidence; 1/3/7-day error integration; pack-aware error classification output.

### M5 — Production hardening

Cloudflare Access; Terraform prod env; backups + restore; observability; PWA install; accessibility; performance validation; security review; pack compatibility diagnostics.

### M6 — MCP (optional, gated)

Only after validation of: ChatGPT account availability, OAuth 2.1, transport, write confirmation, mobile behavior, latency. Deliver: OAuth; read tools; lesson completion write; card mutations; confirmation behavior; mobile validation. Spec: §3.

### M7 — Second-pack validation

Small second pack or enhanced synthetic pack proving: zero core modifications; unchanged review subsystem; generic API/DB naming; isolation of pack normalization + error classification.

### M8 — Optional personalization (not required for core product)

FSRS parameter optimization ([05-review-engine.md](05-review-engine.md) §11 constraints); workload forecasting; advanced retention charts; signed declarative pack distribution; optional generic Anki-oriented export adapter (consumes exports; never system of record).

## 2. Fallback lesson flow (v1 first-class, not deferred)

No-MCP workflow: open app → select profile → app shows next target, due errors, review summary → **Copy tutor context** → new ChatGPT conversation in tutor project → `Start the next language lesson` → lesson → tutor returns versioned lesson-result block → paste into app → validate + preview → apply → atomic commit.

## 3. MCP spec (thin, M6)

Optional interface over the same `application` service layer. No business logic, no duplicate queries.

Scopes: `language_packs.read`, `learning.read`, `lessons.write`, `cards.write`, `errors.write`, `exports.read`.

Tools: `list_language_packs`, `get_active_language_profile`, `get_language_pack_context`, `get_learning_state`, `get_next_lesson`, `get_due_error_targets`, `get_review_summary`, `get_card_redesign_candidates`, `start_lesson`, `complete_lesson`, `upsert_cards`, `suspend_cards`, `log_error_occurrence`, `correct_lesson_record`.

`complete_lesson` = preferred write path; accepts validated card mutations atomically with lesson evidence. Every lesson/mutation request includes `language_profile_id`, `language_pack_id`, expected pack contract/version. No tool accepts raw SQL, arbitrary URLs, arbitrary code, or unbounded result requests.

## 4. Risk register

| # | Risk | Mitigation |
|---|---|---|
| 1 | Scheduler crate incompatible with Worker | M0 spike; app-owned trait; pinned version; fallback scheduler |
| 2 | Tutor creates too many / poor cards | quality validation; activation budget; duplicate detection; evidence requirement; redesign metrics; destructive-change preview |
| 3 | Review success mistaken for speaking ability | separate evidence kinds: review, prompted, spontaneous, listening, transfer |
| 4 | `Hard` misused after forgetting | UI definitions + tutor reminders; `Again` required after failed recall |
| 5 | Offline conflicts | immutable events; client operation IDs; prior-state hashes; bounded batches; deterministic replay |
| 6 | Review overload kills adherence | retention default 0.90; review-time cap; backlog protection; gradual throttling |
| 7 | ChatGPT custom app unavailable | copy/import workflow stays first-class |
| 8 | Accidental D1 replacement | lifecycle protection; plan review; backup; separate envs |
| 9 | Core couples to Spanish pack | synthetic test pack; dependency-direction CI; generic names; M7 validation |
| 10 | Plugin contract too broad | only proven extension points; no workflow execution; no storage access |
| 11 | Pack upgrade changes history interpretation | pack ID+version persisted on records; no silent re-normalization |
| 12 | Pedagogical over-automation | raw evidence visible; user can reject/edit/suspend/restore; recommendations never rewrite history |
