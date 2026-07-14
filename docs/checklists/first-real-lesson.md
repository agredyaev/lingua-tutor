# First real lesson — owner checklist (WBS 11.2)

Owner executes this end-to-end with real study intent. Record every friction
point in the fix-forward section of `docs/wbs-log.md` (Iteration 11) — one
bullet per item, with severity. This validates the copy/paste loop "in anger"
before MCP automates it.

## Pre-flight

- [ ] Prod reachable: https://lingua-tutor-api.aleksey-gredyaev-dev.workers.dev opens the Access login, One-Time PIN works.
- [ ] Language profile exists (es-ES) on the Languages page; create if missing.
- [ ] Dashboard renders: next target, due errors, review summary.
- [ ] iPhone: PWA installed (Share → Add to Home Screen), opens logged-in (10.7).

## Lesson loop (13-roadmap.md §2)

- [ ] Export page → "Fetch tutor context" → copy.
- [ ] New ChatGPT conversation (tutor project) → paste context → "Start the next language lesson".
- [ ] Run the lesson honestly (~20 min speaking-first; the tutor tracks metrics).
- [ ] Tutor returns exactly one lesson-result JSON block (schema_version 1).
- [ ] App /import page → paste → Validate → preview shows: lesson, new items, cards, error targets, mutations.
- [ ] Apply → success. Re-paste the same block → duplicate noop (no double rows).
- [ ] Dashboard advanced: lesson number, new cards visible, error targets show 1/3/7-day rechecks.

## Next day

- [ ] Review session end-to-end: grades incl. one Again, one undo → re-grade, session summary.
- [ ] Tutor context re-export now contains yesterday's evidence (errors, review summary, next goal).

## Friction capture

While doing the above, note in wbs-log Iteration 11 fix-forward:
severity (blocker / annoying / cosmetic), what happened, where
(page / endpoint / prompt). Spec-traceable items become WBS-12 sub-bullets;
everything else is rejected per WBS scope rules.
