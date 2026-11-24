# 12 — Testing

Scope: subsystem tests, architecture gates and acceptance commands.

## 1. Scheduler and review

Review-engine uses deterministic golden vectors, replay/property tests, lapse/overdue cases, state hashing and fail-closed persisted algorithm/version validation. Review persistence tests cover idempotency, compensation/undo, atomic event + schedule updates and immutable history.

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

Scratch SQLite tests validate current table/index shape, foreign keys and constraints. Dedicated upgrade tests verify historical schemas through v9, including language-neutral storage, semantic metadata cleanup, and removal of unused review-experiment metadata without loss of learning data.

## 6. End-to-end

Browser and RPC/MCP flows cover lesson completion, review, dashboard updates, card changes, backup/restore and shared persistence. The Android CI job is configured to install a test-signed x86-64 APK, launch the setup page, check the SQLite file and relaunch after a force-stop. This does not establish ARM64 physical-device compatibility or live OpenAI tunnel connectivity.

The release acceptance workflow downloads the published APK and SHA256SUMS, verifies the digest and signature, installs without modification, creates two language profiles, checks dashboard, review, library, progress and history, verifies persistence after force-stop, checks the local runtime without network connectivity and captures crash and UI evidence. Later releases exercise native JSON/CSV export, lesson import and validation, review/undo, card suspend/restore and persisted lesson/card data through the UI. v0.1.10 lacks accessible import/export routes: the workflow reports that scope as blocked, not passed. The test uses Android 35 x86-64 and synthetic data only. Physical ARM64 and a live OpenAI tunnel remain unverified.

## 7. Performance

Representative single-learner performance tests remain explicit release/ignored gates. Optimize only after profiling; preserve bounded queues/batches and indexed due-card access.

## 8. Acceptance commands

```bash
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked --workspace
cargo deny --locked check
(cd apps/web && trunk build --release --cargo-profile wasm-release)
cargo build --locked -p lingua-tutor-local --bin lingua-tutor --release
```
