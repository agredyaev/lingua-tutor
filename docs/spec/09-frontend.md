# 09 — Frontend

Scope: `apps/web` Leptos CSR PWA. Routes, UX requirements, offline, binding Leptos practices (source: https://book.leptos.dev/).
Depends on: [08-local-rpc.md](08-local-rpc.md), [05-review-engine.md](05-review-engine.md).

## 1. App structure

- Leptos CSR, built with Trunk, target `wasm32-unknown-unknown`.
- `api_client.rs`: one typed client over `shared-schema` DTOs. No handwritten JSON in components.
- JS limited to: WASM loader, PWA service worker, isolated chart adapter only if native SVG insufficient.
- Mobile-first. Must work installed on iPhone (PWA).

## 2. Binding Leptos practices

| Rule | Detail |
|---|---|
| Signals | `signal()` for state; `signal_local()` for `!Send` browser types. |
| Read without clone | `.read()` / `.with()` for inspection (`names.read().is_empty()`), `.get()` only when a clone is needed. |
| Derived state | Derived signal closures for cheap computations; `Memo::new()` for expensive/cached ones. |
| Effects | Writing signals from effects is forbidden (official Leptos guidance: inefficient, loop-prone). Top-down data flow via derived signals/memos. Effects only for true side effects (DOM APIs, storage). |
| API reads | `LocalResource` (CSR; browser fetch futures are `!Send`). Wrap in `<Suspense>`/`<Transition>` with fallback. `<Transition>` on review queue to avoid flicker between cards. |
| API writes | `Action` for every mutation: grade submit, lesson import apply, card mutations. Pending state from `action.pending()` disables the trigger control — this is the double-submit guard. |
| Errors | Typed API error enum in `shared-schema`; `<ErrorBoundary>` around each page; render error + retry, never blank screen. |
| Global state | `provide_context`/`use_context` for: active language profile, learner settings, offline queue handle. No global mutable statics. |
| Routing | `leptos_router`. Routes = §3. |
| Components | Props typed; children composition per book conventions; components small, one responsibility. |

## 3. Routes

```text
/                     → redirect /dashboard
/dashboard
/languages
/languages/:profile_id
/learn
/review
/review/summary
/cards
/cards/:id
/cards/redesign
/lessons
/lessons/:id
/learning-items
/learning-items/:id
/errors
/errors/:id
/listening
/import
/export
/settings
/system
```

## 4. Review UX

One card at a time; speaking aloud prioritized; answer hidden until requested; grade buttons show definitions (`Hard` = passing, `Again` after forgetting); double-submit prevented (Action pending); keyboard (1–4, space=reveal) + touch; immediate undo control; progress shown without pressure; no charts during review; answer + accepted alternatives readable; short explanation after grading.

## 5. Card management UX

List shows: due status, intent, source, creation reason, recent grade history, redesign warnings, suspend/restore, tutor mutation history, duplicate candidates, search + filters.

## 6. Dashboard

Content = `get_dashboard` RPC result ([08-local-rpc.md](08-local-rpc.md) §2). Actionable exceptions first (backlog, method-change targets, rechecks due).

## 7. Offline (bounded)

- Service worker caches static assets only. Dynamic application data is not intercepted; bounded review state is handled explicitly in `offline.rs`.
- Prefetch bounded due-review batch (≤ `max_review_batch`).
- Offline review events → bounded `localStorage` queue with `client_operation_id` + `prior_state_hash` (ADR-0005).
- On reconnect: replay in order, idempotent; `409 conflict_stale_state` → drop local schedule copy, refetch, keep event for audit resolution.
- Offline semantic card edits rejected client-side.
- Stale scheduling state detected via state hash; reconcile safely, never guess.

## 8. Import/export pages

`/import`: paste lesson-result → validate → preview diff (incl. destructive mutations highlighted) → apply with validation token. `/export`: full JSON, tutor context (copy button — primary no-MCP flow), cards.csv.

## 9. Size budget

Compressed payload > 3 MB requires explicit review. Measures:

```toml
[profile.wasm-release]
inherits = "release"
opt-level = 'z'
lto = true
codegen-units = 1
```

Trunk release builds use this profile. Avoid size-heavy deps (`regex`, full ICU) in `apps/web`. WASM is served by the local runtime; release size is measured in CI. Binary size measured in CI ([12-testing.md](12-testing.md)).
