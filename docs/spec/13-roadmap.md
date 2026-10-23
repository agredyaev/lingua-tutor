# 13 — Roadmap and Risks

## 1. Current architecture baseline

The production target uses one Leptos PWA, one native Rust runtime, and one SQLite database.
The browser boundary binds to loopback and uses `POST /rpc`.
ChatGPT integration uses one in-process MCP JSON-RPC dispatcher through OpenAI Secure MCP Tunnel.

## 2. Product milestones

- Core records: learner/language profiles, lessons, evidence, errors, listening attempts.
- Cards/review: due queue, immutable review events, scheduler state, undo, backlog protection.
- Language packs: versioned pack contract, es-ES implementation, conformance suite.
- Planner loop: review evidence, item × capability projections, task scoring, spaced tutor-focus threads, redesign candidates, recurring error targets, lesson planning, and decision→outcome calibration.
- Local hardening: install/update flow, backup/restore, health checks, PWA/e2e validation.
- MCP: bounded read/write tools call the same application handlers as the PWA operation boundary.
- Synthetic-pack validation continuously proves that core/review logic is language-neutral.

## 3. MCP contract

MCP owns no business logic and performs no direct SQL.
Tools expose bounded application operations for language-pack discovery, learner state, lesson lifecycle, review summaries, card mutation, and error recording.

No tool accepts raw SQL, arbitrary URLs, arbitrary executable code, or unbounded result requests.

## 4. Risk register

| Risk | Mitigation |
|---|---|
| Tutor creates poor/excess cards | validation, duplicate detection, activation budget, redesign evidence |
| Review success mistaken for speaking ability | separate evidence kinds |
| Offline conflicts | immutable events, operation IDs, prior-state hashes, deterministic replay |
| Review overload | workload cap, backlog protection, gradual throttling |
| Tunnel unavailable | local UI and copy/import workflow remain usable |
| SQLite loss/corruption | WAL, integrity/FK checks, automatic local backups, restore drill |
| Core couples to one language | synthetic-pack tests and dependency-direction checks |
| Pack upgrade changes history | persist pack identity/version; no silent history rewrite |
