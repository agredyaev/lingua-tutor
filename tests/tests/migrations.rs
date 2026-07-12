//! Migration suite: applies `migrations/*.sql` to scratch SQLite and asserts
//! the schema matches 04-database.md §2 (12-testing.md migration tests).

// tests exempt from panic-family lints (16-rust-guidelines.md §2)
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use rusqlite::Connection;
use std::fs;
use std::path::PathBuf;

fn migrations_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../migrations")
}

/// Fresh in-memory DB with every migration applied in numeric order.
fn migrated_connection() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.pragma_update(None, "foreign_keys", "ON").unwrap();

    let mut files: Vec<PathBuf> = fs::read_dir(migrations_dir())
        .expect("migrations/ directory exists")
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "sql"))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "no migration files found");

    for file in files {
        let sql = fs::read_to_string(&file).unwrap();
        conn.execute_batch(&sql)
            .unwrap_or_else(|e| panic!("{} failed: {e}", file.display()));
    }
    conn
}

fn names(conn: &Connection, kind: &str) -> Vec<String> {
    let mut stmt = conn
        .prepare("SELECT name FROM sqlite_master WHERE type = ?1 AND name NOT LIKE 'sqlite_%' ORDER BY name")
        .unwrap();
    stmt.query_map([kind], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

#[test]
fn all_tables_from_04_database_exist() {
    let conn = migrated_connection();
    let expected = [
        "card_mutations",
        "card_schedule_state",
        "card_tags",
        "cards",
        "error_occurrences",
        "error_rechecks",
        "error_targets",
        "language_packs",
        "learner_language_profiles",
        "learner_profile",
        "learning_item_evidence",
        "learning_items",
        "lesson_metrics",
        "lessons",
        "listening_attempts",
        "recommendations",
        "review_events",
        "review_sessions",
        "scheduler_parameter_sets",
    ];
    assert_eq!(names(&conn, "table"), expected);
}

#[test]
fn all_named_indexes_exist() {
    let conn = migrated_connection();
    let indexes = names(&conn, "index");
    for expected in [
        "ux_llp_active_pack",
        "ix_items_profile_status",
        "ix_items_normalized",
        "ix_evidence_item",
        "ix_errors_profile_status",
        "ix_rechecks_due",
        "ix_cards_profile_status",
        "ix_cards_source_item",
        "ix_cards_source_error",
        "ix_cards_content_hash",
        "ix_schedule_due",
        "ix_events_card",
        "ix_events_session",
        "ix_recommendations_open",
    ] {
        assert!(indexes.contains(&expected.to_owned()), "missing {expected}");
    }
}

#[test]
fn card_status_check_constraint_rejects_invalid_value() {
    let conn = migrated_connection();
    seed_profile_and_pack(&conn);
    let err = insert_card(&conn, "card-1", "bogus_status");
    assert!(err.is_err(), "invalid card status must be rejected");
}

#[test]
fn review_event_client_operation_id_is_unique() {
    let conn = migrated_connection();
    seed_profile_and_pack(&conn);
    insert_card(&conn, "card-1", "new").unwrap();
    conn.execute_batch(
        "INSERT INTO review_sessions (id, learner_language_profile_id, started_at, source, created_at)
         VALUES ('rs-1', 'llp-1', '2026-07-12T00:00:00Z', 'online', '2026-07-12T00:00:00Z');",
    )
    .unwrap();

    let insert_event = |id: &str| {
        conn.execute(
            "INSERT INTO review_events
             (id, card_id, review_session_id, client_operation_id, presented_at, answered_at,
              grade, response_millis, answer_revealed, prior_state_hash, result_state_hash,
              algorithm_version, source, created_at)
             VALUES (?1, 'card-1', 'rs-1', 'client-op-1', '2026-07-12T00:00:00Z',
                     '2026-07-12T00:00:01Z', 3, 900, 1, 'h0', 'h1', 'fallback-1', 'online',
                     '2026-07-12T00:00:01Z')",
            [id],
        )
    };
    insert_event("ev-1").unwrap();
    assert!(
        insert_event("ev-2").is_err(),
        "duplicate client_operation_id must be rejected"
    );
}

#[test]
fn foreign_keys_enforced() {
    let conn = migrated_connection();
    let result = conn.execute_batch(
        "INSERT INTO lessons (id, learner_language_profile_id, language_pack_id,
             language_pack_version, lesson_number, status, scenario, planned_goal,
             source, created_at, updated_at)
         VALUES ('l-1', 'missing-profile', 'missing-pack', '1.0.0', 1, 'planned',
                 's', 'g', 'pwa', '2026-07-12T00:00:00Z', '2026-07-12T00:00:00Z');",
    );
    assert!(result.is_err(), "FK to missing profile must be rejected");
}

fn seed_profile_and_pack(conn: &Connection) {
    conn.execute_batch(
        "INSERT INTO language_packs (id, package_name, version, contract_version,
             target_language, target_locale, display_name, installed_at, updated_at)
         VALUES ('test-pack', 'synthetic-test-pack', '0.1.0', 1, 'zz', 'zz-ZZ',
                 'Synthetic', '2026-07-12T00:00:00Z', '2026-07-12T00:00:00Z');
         INSERT INTO learner_profile (id, created_at, updated_at)
         VALUES ('learner-1', '2026-07-12T00:00:00Z', '2026-07-12T00:00:00Z');
         INSERT INTO learner_language_profiles (id, learner_id, language_pack_id,
             source_language, target_locale, created_at, updated_at)
         VALUES ('llp-1', 'learner-1', 'test-pack', 'ru', 'zz-ZZ',
                 '2026-07-12T00:00:00Z', '2026-07-12T00:00:00Z');",
    )
    .unwrap();
}

fn insert_card(conn: &Connection, id: &str, status: &str) -> rusqlite::Result<usize> {
    conn.execute(
        "INSERT INTO cards (id, learner_language_profile_id, language_pack_id,
             language_pack_version, intent, status, prompt, prompt_language, answer,
             answer_language, answer_evaluation_mode, creation_reason, content_hash,
             created_by, created_at, updated_at)
         VALUES (?1, 'llp-1', 'test-pack', '0.1.0', 'production', ?2, 'p', 'ru', 'a',
                 'zz', 'exact', 'test', 'hash-1', 'system',
                 '2026-07-12T00:00:00Z', '2026-07-12T00:00:00Z')",
        [id, status],
    )
}
