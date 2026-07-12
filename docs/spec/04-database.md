# 04 — Database

Scope: full D1 (SQLite) schema, indexes, constraints, migration policy. Canonical source for table and column names.
Depends on: [03-domain-model.md](03-domain-model.md).

## 1. Rules

- IDs: TEXT, opaque, UUIDv7/ULID. Timestamps: TEXT, UTC RFC 3339. Booleans: INTEGER 0/1.
- `PRAGMA foreign_keys = ON` per connection (D1 supports it; enable in repository layer).
- Migrations: forward-only in production, applied via `wrangler d1 migrations apply`. Numbered `migrations/NNNN_name.sql`.
- No destructive cascades over learning history. `ON DELETE RESTRICT` default.
- All `*_json` columns validated at service boundary before write.
- Review events immutable: no UPDATE/DELETE; corrections = compensating rows.
- `card_schedule_state` update + `review_events` insert: one transaction (D1 batch).
- Every language-specific row carries `language_pack_id`; historical rows keep the pack version that created or last semantically changed them.

## 2. DDL

```sql
CREATE TABLE language_packs (
    id                TEXT PRIMARY KEY,          -- stable pack ID, e.g. 'es-ES'
    package_name      TEXT NOT NULL,             -- e.g. 'language-pack-es-es'
    version           TEXT NOT NULL,             -- semver
    contract_version  INTEGER NOT NULL,
    target_language   TEXT NOT NULL,             -- BCP 47 language
    target_locale     TEXT NOT NULL,             -- BCP 47 locale
    display_name      TEXT NOT NULL,
    capabilities_json TEXT NOT NULL DEFAULT '[]',
    enabled           INTEGER NOT NULL DEFAULT 1,
    installed_at      TEXT NOT NULL,
    updated_at        TEXT NOT NULL
);

CREATE TABLE learner_profile (
    id                     TEXT PRIMARY KEY,
    timezone               TEXT NOT NULL DEFAULT 'Europe/Madrid',
    daily_session_target   INTEGER NOT NULL DEFAULT 2,
    session_minutes_target INTEGER NOT NULL DEFAULT 20,
    max_new_cards          INTEGER NOT NULL DEFAULT 8,
    desired_retention      REAL NOT NULL DEFAULT 0.90,
    max_review_minutes     INTEGER NOT NULL DEFAULT 12,
    created_at             TEXT NOT NULL,
    updated_at             TEXT NOT NULL
);

CREATE TABLE learner_language_profiles (
    id                    TEXT PRIMARY KEY,
    learner_id            TEXT NOT NULL REFERENCES learner_profile(id),
    language_pack_id      TEXT NOT NULL REFERENCES language_packs(id),
    source_language       TEXT NOT NULL,
    target_locale         TEXT NOT NULL,
    estimated_level       TEXT,
    current_lesson_number INTEGER NOT NULL DEFAULT 0,
    status                TEXT NOT NULL DEFAULT 'active'
                          CHECK (status IN ('active','paused','archived')),
    is_default            INTEGER NOT NULL DEFAULT 0,
    preferences_json      TEXT NOT NULL DEFAULT '{}',
    created_at            TEXT NOT NULL,
    updated_at            TEXT NOT NULL
);
CREATE UNIQUE INDEX ux_llp_active_pack
    ON learner_language_profiles(learner_id, language_pack_id)
    WHERE status = 'active';

CREATE TABLE lessons (
    id                          TEXT PRIMARY KEY,
    learner_language_profile_id TEXT NOT NULL REFERENCES learner_language_profiles(id),
    language_pack_id            TEXT NOT NULL REFERENCES language_packs(id),
    language_pack_version       TEXT NOT NULL,
    lesson_number               INTEGER NOT NULL,
    status                      TEXT NOT NULL
                                CHECK (status IN ('planned','active','completed','abandoned')),
    scenario                    TEXT NOT NULL,
    planned_goal                TEXT NOT NULL,
    source                      TEXT NOT NULL CHECK (source IN ('pwa','import','mcp')),
    started_at                  TEXT,
    completed_at                TEXT,
    duration_seconds            INTEGER,
    tutor_summary               TEXT,
    next_goal                   TEXT,
    idempotency_key             TEXT UNIQUE,
    created_at                  TEXT NOT NULL,
    updated_at                  TEXT NOT NULL,
    UNIQUE (learner_language_profile_id, lesson_number)
);

CREATE TABLE lesson_metrics (
    lesson_id               TEXT PRIMARY KEY REFERENCES lessons(id),
    speaking_seconds        INTEGER NOT NULL DEFAULT 0,
    unaided_recall_count    INTEGER NOT NULL DEFAULT 0,
    aided_recall_count      INTEGER NOT NULL DEFAULT 0,
    listening_score         REAL,
    avg_answer_millis       INTEGER,
    long_pause_count        INTEGER NOT NULL DEFAULT 0,
    confidence              REAL,
    created_at              TEXT NOT NULL
);

CREATE TABLE learning_items (
    id                          TEXT PRIMARY KEY,
    learner_language_profile_id TEXT NOT NULL REFERENCES learner_language_profiles(id),
    language_pack_id            TEXT NOT NULL REFERENCES language_packs(id),
    item_type                   TEXT NOT NULL
        CHECK (item_type IN ('phrase','collocation','pattern','word','response_frame')),
    target_text                 TEXT NOT NULL,
    normalized_target_text      TEXT NOT NULL,
    prompt_text                 TEXT,
    prompt_language             TEXT,
    status                      TEXT NOT NULL DEFAULT 'candidate'
        CHECK (status IN ('candidate','introduced','learning','transfer_pending',
                          'provisionally_mastered','mastered','suspended')),
    language_metadata_json      TEXT NOT NULL DEFAULT '{}',
    introduced_lesson_id        TEXT REFERENCES lessons(id),
    created_at                  TEXT NOT NULL,
    updated_at                  TEXT NOT NULL
);
CREATE INDEX ix_items_profile_status
    ON learning_items(learner_language_profile_id, status);
CREATE INDEX ix_items_normalized
    ON learning_items(learner_language_profile_id, normalized_target_text);

CREATE TABLE learning_item_evidence (
    id                 TEXT PRIMARY KEY,
    learning_item_id   TEXT NOT NULL REFERENCES learning_items(id),
    lesson_id          TEXT REFERENCES lessons(id),
    kind               TEXT NOT NULL
        CHECK (kind IN ('recognition','listening','prompted_production',
                        'spontaneous_production','transfer','correction')),
    success            INTEGER NOT NULL,
    detail             TEXT,
    pack_metadata_json TEXT,
    recorded_at        TEXT NOT NULL
);
CREATE INDEX ix_evidence_item ON learning_item_evidence(learning_item_id, kind);

CREATE TABLE error_targets (
    id                          TEXT PRIMARY KEY,
    learner_language_profile_id TEXT NOT NULL REFERENCES learner_language_profiles(id),
    language_pack_id            TEXT NOT NULL REFERENCES language_packs(id),
    language_pack_version       TEXT NOT NULL,
    core_category               TEXT NOT NULL,
    pack_error_code             TEXT,
    original_production         TEXT NOT NULL,
    corrected_production        TEXT NOT NULL,
    explanation                 TEXT,
    occurrence_count            INTEGER NOT NULL DEFAULT 1,
    method_change_required      INTEGER NOT NULL DEFAULT 0,
    status                      TEXT NOT NULL DEFAULT 'active'
        CHECK (status IN ('active','recheck','resolved','escalated')),
    created_at                  TEXT NOT NULL,
    updated_at                  TEXT NOT NULL
);
CREATE INDEX ix_errors_profile_status ON error_targets(learner_language_profile_id, status);

CREATE TABLE error_occurrences (
    id              TEXT PRIMARY KEY,
    error_target_id TEXT NOT NULL REFERENCES error_targets(id),
    lesson_id       TEXT REFERENCES lessons(id),
    context         TEXT,
    recorded_at     TEXT NOT NULL
);

CREATE TABLE error_rechecks (
    id              TEXT PRIMARY KEY,
    error_target_id TEXT NOT NULL REFERENCES error_targets(id),
    offset_days     INTEGER NOT NULL CHECK (offset_days IN (1,3,7)),
    due_at          TEXT NOT NULL,
    completed_at    TEXT,
    passed          INTEGER,
    UNIQUE (error_target_id, offset_days)
);
CREATE INDEX ix_rechecks_due ON error_rechecks(due_at) WHERE completed_at IS NULL;

CREATE TABLE listening_attempts (
    id                          TEXT PRIMARY KEY,
    learner_language_profile_id TEXT NOT NULL REFERENCES learner_language_profiles(id),
    learning_item_id            TEXT REFERENCES learning_items(id),
    lesson_id                   TEXT REFERENCES lessons(id),
    source_reference            TEXT,          -- no raw audio stored
    transcript                  TEXT,
    success                     INTEGER NOT NULL,
    recorded_at                 TEXT NOT NULL
);

CREATE TABLE cards (
    id                          TEXT PRIMARY KEY,
    learner_language_profile_id TEXT NOT NULL REFERENCES learner_language_profiles(id),
    language_pack_id            TEXT NOT NULL REFERENCES language_packs(id),
    language_pack_version       TEXT NOT NULL,
    intent                      TEXT NOT NULL
        CHECK (intent IN ('production','listening','contrast','transfer')),
    status                      TEXT NOT NULL DEFAULT 'candidate'
        CHECK (status IN ('candidate','new','learning','review','relearning',
                          'suspended','retired')),
    prompt                      TEXT NOT NULL,
    prompt_language             TEXT NOT NULL,
    answer                      TEXT NOT NULL,
    answer_language             TEXT NOT NULL,
    accepted_answers_json       TEXT NOT NULL DEFAULT '[]',
    answer_evaluation_mode      TEXT NOT NULL,     -- pack-defined mode identifier
    explanation                 TEXT,
    pronunciation_note          TEXT,
    source_learning_item_id     TEXT REFERENCES learning_items(id),
    source_error_target_id      TEXT REFERENCES error_targets(id),
    source_lesson_id            TEXT REFERENCES lessons(id),
    creation_reason             TEXT NOT NULL,
    priority_score              REAL,
    content_hash                TEXT NOT NULL,
    content_version             INTEGER NOT NULL DEFAULT 1,
    parent_card_id              TEXT REFERENCES cards(id),
    suspended_reason            TEXT,
    created_by                  TEXT NOT NULL CHECK (created_by IN ('tutor','user','system')),
    created_at                  TEXT NOT NULL,
    updated_at                  TEXT NOT NULL
);
CREATE INDEX ix_cards_profile_status ON cards(learner_language_profile_id, status);
CREATE INDEX ix_cards_source_item ON cards(source_learning_item_id);
CREATE INDEX ix_cards_source_error ON cards(source_error_target_id);
CREATE INDEX ix_cards_content_hash ON cards(learner_language_profile_id, content_hash);

CREATE TABLE card_tags (
    card_id        TEXT NOT NULL REFERENCES cards(id),
    tag_type       TEXT NOT NULL,        -- 'scenario' | 'skill' | 'pack'
    normalized_tag TEXT NOT NULL,        -- pack tags namespaced: 'pack.es-ES.grammar.preposition'
    PRIMARY KEY (card_id, tag_type, normalized_tag)
);

CREATE TABLE card_schedule_state (
    card_id                    TEXT PRIMARY KEY REFERENCES cards(id),
    algorithm                  TEXT NOT NULL,
    algorithm_version          TEXT NOT NULL,
    parameter_set_id           TEXT REFERENCES scheduler_parameter_sets(id),
    difficulty                 REAL,
    stability                  REAL,
    last_review_at             TEXT,
    due_at                     TEXT,
    scheduled_interval_seconds INTEGER,
    lapse_count                INTEGER NOT NULL DEFAULT 0,
    review_count               INTEGER NOT NULL DEFAULT 0,
    state_version              INTEGER NOT NULL DEFAULT 1,
    state_hash                 TEXT NOT NULL,
    updated_at                 TEXT NOT NULL
);
CREATE INDEX ix_schedule_due ON card_schedule_state(due_at);

CREATE TABLE review_sessions (
    id                          TEXT PRIMARY KEY,
    learner_language_profile_id TEXT NOT NULL REFERENCES learner_language_profiles(id),
    started_at                  TEXT NOT NULL,
    completed_at                TEXT,
    source                      TEXT NOT NULL CHECK (source IN ('online','offline')),
    card_count                  INTEGER NOT NULL DEFAULT 0,
    review_seconds              INTEGER,
    created_at                  TEXT NOT NULL
);

CREATE TABLE review_events (
    id                  TEXT PRIMARY KEY,
    card_id             TEXT NOT NULL REFERENCES cards(id),
    review_session_id   TEXT NOT NULL REFERENCES review_sessions(id),
    client_operation_id TEXT NOT NULL UNIQUE,
    presented_at        TEXT NOT NULL,
    answered_at         TEXT NOT NULL,
    grade               INTEGER NOT NULL CHECK (grade BETWEEN 1 AND 4),
    response_millis     INTEGER NOT NULL CHECK (response_millis >= 0),
    answer_revealed     INTEGER NOT NULL,
    learner_note        TEXT,
    prior_state_hash    TEXT NOT NULL,
    result_state_hash   TEXT NOT NULL,
    algorithm_version   TEXT NOT NULL,
    source              TEXT NOT NULL CHECK (source IN ('online','offline')),
    compensates_event_id TEXT REFERENCES review_events(id),  -- undo/correction lineage
    created_at          TEXT NOT NULL
);
CREATE INDEX ix_events_card ON review_events(card_id, created_at);
CREATE INDEX ix_events_session ON review_events(review_session_id);

CREATE TABLE card_mutations (
    id                    TEXT PRIMARY KEY,
    card_id               TEXT REFERENCES cards(id),   -- NULL for rejected creates
    operation             TEXT NOT NULL
        CHECK (operation IN ('create','update','split','merge','suspend',
                             'restore','retire','add_accepted_answer',
                             'remove_accepted_answer','add_listening_variant','link')),
    client_operation_id   TEXT NOT NULL UNIQUE,
    before_json           TEXT,
    after_json            TEXT,
    source_lesson_id      TEXT REFERENCES lessons(id),
    actor                 TEXT NOT NULL CHECK (actor IN ('tutor','user','system')),
    reason                TEXT NOT NULL,
    language_pack_id      TEXT NOT NULL,
    language_pack_version TEXT NOT NULL,
    validation_result     TEXT NOT NULL CHECK (validation_result IN ('applied','rejected','warned')),
    created_at            TEXT NOT NULL
);

CREATE TABLE scheduler_parameter_sets (
    id                          TEXT PRIMARY KEY,
    learner_language_profile_id TEXT REFERENCES learner_language_profiles(id), -- NULL = global default
    algorithm                   TEXT NOT NULL,
    algorithm_version           TEXT NOT NULL,
    parameters_json             TEXT NOT NULL,
    origin                      TEXT NOT NULL CHECK (origin IN ('default','optimized')),
    activated_at                TEXT,
    created_at                  TEXT NOT NULL
);

CREATE TABLE recommendations (
    id                          TEXT PRIMARY KEY,
    learner_language_profile_id TEXT NOT NULL REFERENCES learner_language_profiles(id),
    subject_type                TEXT NOT NULL
        CHECK (subject_type IN ('learning_item','error_target','card','listening_item',
                                'review_load','language_profile','pack_upgrade_warning')),
    subject_id                  TEXT,
    action                      TEXT NOT NULL,
    detail_json                 TEXT NOT NULL DEFAULT '{}',
    status                      TEXT NOT NULL DEFAULT 'open'
        CHECK (status IN ('open','accepted','dismissed','expired')),
    created_at                  TEXT NOT NULL,
    resolved_at                 TEXT
);
CREATE INDEX ix_recommendations_open
    ON recommendations(learner_language_profile_id, status);
```

## 3. Ownership

- `card_schedule_state`, `review_events`: written only by review-engine via `application`. Language packs never write them.
- `card_mutations`: append-only audit; one row per attempted mutation, applied or not.
- `review_events.compensates_event_id`: undo and corrections reference the compensated event; original rows never change.

## 4. Migration policy

- `migrations/0001_init.sql` creates the full schema above.
- Production: forward-only; no down migrations. Destructive change requires prior backup ([11-infrastructure.md](11-infrastructure.md) §5).
- Migration tests run in CI against a scratch D1/SQLite ([12-testing.md](12-testing.md)).
