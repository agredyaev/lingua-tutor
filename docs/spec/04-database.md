# 04 — Database

Scope: SQLite ownership, current schema invariants and migration policy.
Depends on: [03-domain-model.md](03-domain-model.md).

## 1. Source of truth

The executable schema source of truth is `apps/local/migrations/`. `0001_init.sql` represents the current schema for a fresh database. Later numbered files are forward transformations required only for existing databases.

Do not duplicate a full SQL schema in documentation: migrations plus integration tests are the canonical executable specification.

Current schema version: **7**.

## 2. Current tables

The current database contains:

`learner_profile`, `learner_language_profiles`, `lessons`, `lesson_metrics`, `learning_items`, `learning_item_evidence`, `error_targets`, `error_occurrences`, `error_rechecks`, `listening_attempts`, `cards`, `card_tags`, `card_schedule_state`, `review_sessions`, `review_events`, `card_mutations`, `scheduler_parameter_sets`, `recommendations`, `planner_decisions`, and `review_daily_budget`.

There is no table representing an executable language implementation.

## 3. Language identity

`learner_language_profiles` stores `source_language` and `target_locale`. Child tables reference `learner_language_profile_id`; they do not duplicate target-language identity or implementation version.

The active-profile uniqueness index is `ux_llp_active_locale(learner_id, target_locale)`.

## 4. Semantic metadata

- `error_targets.error_code`: optional opaque tutor-provided detail code.
- `learning_item_evidence.metadata_json`: bounded evidence/measurement metadata.
- Cards do not store an evaluator mode; answer evaluation uses the stored expected answer plus accepted-answer set.
- Learning items do not store a target-language implementation metadata blob.

## 5. Persistence invariants

- IDs are opaque TEXT at the SQLite boundary.
- Timestamps are UTC RFC 3339 TEXT.
- `PRAGMA foreign_keys = ON` for every runtime connection.
- Review events are immutable; corrections/undo use compensating rows.
- Schedule update and review-event insertion occur in one transaction.
- Lesson completion is atomic.
- JSON fields are validated at operation boundaries before write.
- No network call occurs inside a SQLite transaction.

## 6. Migrations

- `0001_init.sql`: current fresh schema.
- `0002_planner_history.sql`–`0005_methodology_semantics.sql`: historical additive methodology changes.
- `0006_language_neutral_storage.sql`: removes obsolete language-implementation identity from existing v5 databases.
- `0007_semantic_metadata.sql`: renames retained semantic metadata and removes dead evaluator/item metadata columns.

Legacy schema text exists only as a test fixture to verify upgrade safety; it is not a production schema source.

Migration tests must prove foreign keys, named indexes, current schema shape, and preservation of user data across destructive transformations.
