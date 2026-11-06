# 12 — Testing

Scope: subsystem tests, architecture gates and acceptance commands.

## 1. Scheduler and review

Review-engine uses deterministic golden vectors, replay/property tests, lapse/overdue cases, state hashing and fallback behavior. Review persistence tests cover idempotency, compensation/undo, atomic event + schedule updates and immutable history.

## 2. Tutor/card flow

Tests cover lesson-result validation, profile ownership, duplicate detection, card create/update/status transitions, accepted-answer changes, content-hash stability, activation budgets and atomic rollback.

## 3. Adaptation and mastery

Table-driven tests cover recall gaps, review backlog, listening evidence, prompted/spontaneous mismatch, repeated errors, transfer context families, method-change thresholds and calibration from measured outcomes.

## 4. Language neutrality

Architecture gates must prove:

- no executable target-language crate or registry is compiled;
- no virtual language behavior dispatch exists;
- tutor/wire contracts expose profile identity and scalar language metadata only;
- current storage contains no duplicated language-implementation identity;
- core contains no target-language normalization/classification rules;
- methodology assets contain no target-language rules or examples.

Ordinary profile and tutor-result fixtures are sufficient; no executable synthetic language implementation is required.

## 5. Migrations

Scratch SQLite tests validate current table/index shape, foreign keys and constraints. Dedicated upgrade tests verify v5→v6 language-neutral storage and v6→v7 semantic metadata migration without loss of learning data.

## 6. End-to-end

Browser and RPC/MCP flows cover lesson completion, review, dashboard updates, card changes, backup/restore and shared persistence.

## 7. Performance

Representative single-learner performance tests remain explicit release/ignored gates. Optimize only after profiling; preserve bounded queues/batches and indexed due-card access.

## 8. Acceptance commands

```bash
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
cd apps/web && trunk build --release --cargo-profile wasm-release
cd ../..
cargo build -p lingua-tutor-local --bin lingua-tutor --release
```
