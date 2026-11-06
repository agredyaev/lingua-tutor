# 13 — Roadmap and Risks

## 1. Current architecture baseline

The product is local-first: Leptos PWA + one native Rust runtime + one SQLite database. Browser RPC and in-process MCP use the same processing and persistence paths.

Language selection is profile metadata. Linguistic interpretation belongs to the tutor boundary; core methodology is target-language neutral.

## 2. Product milestones

- Core records: learner/language profiles, lessons, evidence, errors and listening attempts.
- Cards/review: due queue, immutable review events, scheduler state, undo and backlog protection.
- Adaptive loop: capability projections, task scoring, focus spacing, recurring errors, lesson planning and decision→outcome calibration.
- Local hardening: install/update flow, backup/restore, health checks and PWA/e2e validation.
- MCP: bounded read/write tools call the same application processing paths as browser RPC.

## 3. MCP contract

MCP owns no business logic and performs no direct SQL. Tools expose bounded profile, learner-state, lesson, review, card and error operations.

No tool accepts raw SQL, arbitrary executable code or unbounded result requests.

## 4. Risk register

| Risk | Mitigation |
|---|---|
| Tutor creates poor/excess cards | validation, duplicate detection, activation budget, redesign evidence |
| Review success mistaken for speaking ability | separate evidence kinds and transfer requirements |
| Offline conflicts | immutable events, operation IDs, state hashes, deterministic replay |
| Review overload | workload cap, backlog protection, gradual throttling |
| Tunnel unavailable | local UI remains usable |
| SQLite loss/corruption | WAL, integrity/FK checks, automatic backups, restore drill |
| Core starts encoding one target language | architecture gates + language-neutral tutor boundary |
| Linguistic output is inconsistent | structured contract, bounded categories/codes, evidence-driven methodology |
