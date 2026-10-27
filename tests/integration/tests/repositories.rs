//! Repository SQL exercised against scratch SQLite (12-testing.md: real
//! database, not mocked SQL). Each constant runs with realistic params;
//! row structs round-trip through domain types.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use integration_tests::{migrated_connection, seed_profile_and_pack};
use lingua_tutor_local::repositories::{cards, lessons, packs, profiles, records, review};
use rusqlite::params;

const NOW: &str = "2026-07-12T10:00:00Z";

#[test]
fn learner_profile_upsert_and_select() {
    let conn = migrated_connection();
    conn.execute(
        profiles::UPSERT_LEARNER,
        params!["learner-1", "Europe/Madrid", 2, 20, 8, 0.90, 12, NOW],
    )
    .unwrap();
    // The second upsert updates the same row; inspect the authoritative columns.
    conn.execute(
        profiles::UPSERT_LEARNER,
        params!["learner-1", "Europe/Madrid", 3, 25, 6, 0.85, 10, NOW],
    )
    .unwrap();
    let (daily_target, max_cards): (u8, u8) = conn
        .query_row(profiles::SELECT_LEARNER, [], |row| {
            Ok((row.get("daily_session_target")?, row.get("max_new_cards")?))
        })
        .unwrap();
    assert_eq!(daily_target, 3);
    assert_eq!(max_cards, 6);
    let count: u32 = conn
        .query_row("SELECT COUNT(*) FROM learner_profile", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 1);
}

#[test]
fn language_profile_insert_select_update_roundtrip() {
    let conn = migrated_connection();
    seed_profile_and_pack(&conn);
    let (source, status, preferences): (String, String, String) = conn
        .query_row(profiles::SELECT_LANGUAGE_PROFILE, ["llp-1"], |row| {
            Ok((
                row.get("source_language")?,
                row.get("status")?,
                row.get("preferences_json")?,
            ))
        })
        .unwrap();
    assert_eq!(source, "ru");
    assert_eq!(status, "active");
    assert_eq!(preferences, "{}");

    conn.execute(
        profiles::UPDATE_LANGUAGE_PROFILE,
        params!["llp-1", Some("A2"), status, 1_u8, preferences, 4_u32, NOW],
    )
    .unwrap();
    let (level, number): (Option<String>, u32) = conn.query_row(
        "SELECT estimated_level, current_lesson_number FROM learner_language_profiles WHERE id='llp-1'",
        [], |row| Ok((row.get(0)?, row.get(1)?)),
    ).unwrap();
    assert_eq!(level.as_deref(), Some("A2"));
    assert_eq!(number, 4);
}

#[test]
fn pack_upsert_from_manifest() {
    let conn = migrated_connection();
    let (manifest, _) = language_pack_es_es::register();
    let capabilities_json = serde_json::to_string(&manifest.capabilities).unwrap();
    conn.execute(
        packs::UPSERT_PACK,
        params![
            manifest.id,
            manifest.package,
            manifest.version,
            manifest.contract_version,
            manifest.target_language,
            manifest.target_locale,
            manifest.display_name,
            capabilities_json,
            NOW
        ],
    )
    .unwrap();
    let (id, enabled): (String, u8) = conn
        .query_row(packs::SELECT_PACK, ["es-ES"], |r| {
            Ok((r.get(0)?, r.get(8)?))
        })
        .unwrap();
    assert_eq!(id, "es-ES");
    assert_eq!(enabled, 1);
}

#[test]
fn lesson_insert_complete_and_idempotency_lookup() {
    let conn = migrated_connection();
    seed_profile_and_pack(&conn);
    conn.execute(
        lessons::INSERT_LESSON,
        params![
            "les-1",
            "llp-1",
            "test-pack",
            "0.1.0",
            1,
            "active",
            "pharmacy",
            "ask for meds",
            "import",
            Some(NOW),
            Option::<String>::None,
            Option::<u32>::None,
            Option::<String>::None,
            Option::<String>::None,
            Some("op-import-1"),
            NOW
        ],
    )
    .unwrap();
    let changed = conn
        .execute(
            lessons::COMPLETE_LESSON,
            params!["les-1", NOW, 1260, "summary", "next goal", "op-repo-1"],
        )
        .unwrap();
    assert_eq!(changed, 1);
    // Completing again: no matching row (status already completed).
    let changed_again = conn
        .execute(
            lessons::COMPLETE_LESSON,
            params!["les-1", NOW, 1260, "summary", "next goal", "op-repo-1"],
        )
        .unwrap();
    assert_eq!(changed_again, 0, "complete is not repeatable");

    let (lesson_number, tutor_summary): (u32, Option<String>) = conn
        .query_row(lessons::SELECT_LESSON_BY_KEY, ["op-repo-1"], |row| {
            Ok((row.get("lesson_number")?, row.get("tutor_summary")?))
        })
        .unwrap();
    assert_eq!(lesson_number, 1);
    assert_eq!(tutor_summary.as_deref(), Some("summary"));

    conn.execute(
        lessons::INSERT_METRICS,
        params![
            "les-1",
            540,
            6,
            3,
            Some(0.7),
            Some(2100),
            2,
            0,
            0,
            Option::<&str>::None,
            Some(0.6),
            NOW
        ],
    )
    .unwrap();
}

#[test]
fn latest_open_lesson_excludes_completed_and_picks_most_recent() {
    // Regression for the production incident where a completion call
    // without `lesson_id` always tried to *create* a new lesson row,
    // colliding with the `active` one `start_lesson` already reserved a
    // `lesson_number` for. `SELECT_LATEST_OPEN_LESSON` is what lets the
    // handler attach to that row instead.
    let conn = migrated_connection();
    seed_profile_and_pack(&conn);
    let insert = |id: &str, number: i64, status: &str, started_at: &str| {
        conn.execute(
            lessons::INSERT_LESSON,
            params![
                id,
                "llp-1",
                "test-pack",
                "0.1.0",
                number,
                status,
                "pharmacy",
                "ask for meds",
                "mcp",
                started_at,
                Option::<String>::None,
                Option::<u32>::None,
                Option::<String>::None,
                Option::<String>::None,
                Option::<String>::None,
                NOW,
            ],
        )
        .unwrap();
    };
    insert("les-completed", 1, "completed", "2026-07-10T00:00:00Z");
    insert("les-older-active", 2, "active", "2026-07-11T00:00:00Z");
    insert("les-newest-active", 3, "active", "2026-07-12T00:00:00Z");

    let id: String = conn
        .query_row(lessons::SELECT_LATEST_OPEN_LESSON, ["llp-1"], |row| {
            row.get("id")
        })
        .unwrap();
    assert_eq!(id, "les-newest-active");
}

#[test]
fn item_evidence_and_duplicate_lookup() {
    let conn = migrated_connection();
    seed_profile_and_pack(&conn);
    conn.execute(
        records::INSERT_ITEM,
        params![
            "li-1",
            "llp-1",
            "test-pack",
            "phrase",
            "¿Qué tal?",
            "¿qué tal?",
            Some("Как дела?"),
            Some("ru"),
            "introduced",
            "{}",
            Option::<String>::None,
            NOW
        ],
    )
    .unwrap();
    let text: String = conn
        .query_row(
            records::SELECT_ITEM_BY_NORMALIZED,
            params!["llp-1", "¿qué tal?"],
            |row| row.get("target_text"),
        )
        .unwrap();
    assert_eq!(text, "¿Qué tal?");

    conn.execute(
        records::INSERT_EVIDENCE,
        params![
            "ev-1",
            "li-1",
            Option::<String>::None,
            "spontaneous_production",
            1,
            Option::<String>::None,
            Option::<String>::None,
            NOW,
            0_u32
        ],
    )
    .unwrap();
    conn.execute(
        records::UPDATE_ITEM_STATUS,
        params!["li-1", "learning", NOW],
    )
    .unwrap();
    let status: String = conn
        .query_row(
            "SELECT status FROM learning_items WHERE id='li-1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(status, "learning");
}

#[test]
fn error_target_lifecycle_and_due_rechecks() {
    let conn = migrated_connection();
    seed_profile_and_pack(&conn);
    conn.execute(
        records::INSERT_ERROR_TARGET,
        params![
            "et-1",
            "llp-1",
            "test-pack",
            "0.1.0",
            "grammar",
            Some("es.verb.ser_estar"),
            "estoy médico",
            "soy médico",
            Option::<String>::None,
            1,
            0,
            "active",
            NOW
        ],
    )
    .unwrap();
    conn.execute(
        records::INSERT_ERROR_OCCURRENCE,
        params![
            "eo-1",
            "et-1",
            Option::<String>::None,
            Some("pharmacy dialogue"),
            "medium",
            NOW
        ],
    )
    .unwrap();
    conn.execute(
        records::UPDATE_ERROR_TARGET,
        params!["et-1", 2, 0, "recheck", NOW],
    )
    .unwrap();

    for (id, offset, due) in [
        ("rc-1", 1, "2026-07-13T10:00:00Z"),
        ("rc-3", 3, "2026-07-15T10:00:00Z"),
        ("rc-7", 7, "2026-07-19T10:00:00Z"),
    ] {
        conn.execute(
            records::INSERT_ERROR_RECHECK,
            params![id, "et-1", offset, due],
        )
        .unwrap();
    }
    // Duplicate offset silently ignored (ON CONFLICT DO NOTHING).
    conn.execute(
        records::INSERT_ERROR_RECHECK,
        params!["rc-dup", "et-1", 1, "2026-07-13T10:00:00Z"],
    )
    .unwrap();
    let due_count: u32 = conn
        .query_row(
            "SELECT COUNT(*) FROM error_rechecks WHERE completed_at IS NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(due_count, 3);

    let mut stmt = conn.prepare(records::SELECT_DUE_RECHECKS).unwrap();
    let due: Vec<String> = stmt
        .query_map(["2026-07-15T12:00:00Z"], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(due, ["rc-1", "rc-3"], "only elapsed rechecks are due");

    conn.execute(records::COMPLETE_ERROR_RECHECK, params!["rc-1", NOW, 1])
        .unwrap();
    let count: u32 = conn
        .query_row(records::SELECT_ERROR_TARGET, ["et-1"], |row| {
            row.get("occurrence_count")
        })
        .unwrap();
    assert_eq!(count, 2);
}

#[test]
fn listening_attempt_insert_and_list() {
    let conn = migrated_connection();
    seed_profile_and_pack(&conn);
    conn.execute(
        records::INSERT_LISTENING_ATTEMPT,
        params![
            "la-1",
            "llp-1",
            Option::<String>::None,
            Option::<String>::None,
            Some("lesson-audio-3"),
            None::<String>,
            None::<String>,
            None::<String>,
            Some("¿tiene algo…?"),
            0,
            None::<u8>,
            None::<u32>,
            None::<String>,
            NOW
        ],
    )
    .unwrap();
    let count: u32 = conn
        .query_row(
            "SELECT COUNT(*) FROM listening_attempts WHERE learner_language_profile_id='llp-1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 1);
}

#[test]
fn retention_query_excludes_undo_lineage_both_ways() {
    let conn = migrated_connection();
    seed_profile_and_pack(&conn);
    conn.execute_batch(
        "INSERT INTO cards (id, learner_language_profile_id, language_pack_id,
             language_pack_version, intent, status, prompt, prompt_language, answer,
             answer_language, accepted_answers_json, answer_evaluation_mode,
             creation_reason, created_by, created_at, updated_at, content_hash,
             content_version)
         VALUES ('crd-1', 'llp-1', 'test-pack', '0.1.0', 'production', 'review',
                 'p', 'ru', 'a', 'zz-ZZ', '[]', 'exact', 'seed', 'tutor',
                 '2026-07-12T00:00:00Z', '2026-07-12T00:00:00Z', 'h', 1);
         INSERT INTO review_sessions (id, learner_language_profile_id, started_at,
             source, created_at)
         VALUES ('rs-1', 'llp-1', '2026-07-12T00:00:00Z', 'online',
                 '2026-07-12T00:00:00Z');",
    )
    .unwrap();
    let insert_event = |id: &str, grade: i64, compensates: Option<&str>| {
        conn.execute(
            "INSERT INTO review_events (id, card_id, review_session_id,
                 client_operation_id, presented_at, answered_at, grade,
                 response_millis, answer_revealed, prior_state_hash,
                 result_state_hash, algorithm_version, source, compensates_event_id,
                 created_at)
             VALUES (?1, 'crd-1', 'rs-1', ?1, '2026-07-12T01:00:00Z',
                     '2026-07-12T01:00:00Z', ?2, 1000, 1, 'x', 'y', 'v', 'online',
                     ?3, '2026-07-12T01:00:00Z')",
            rusqlite::params![id, grade, compensates],
        )
        .unwrap();
    };
    insert_event("ev-kept", 3, None); // effective
    insert_event("ev-undone", 1, None); // later compensated → excluded
    insert_event("ev-undo", 3, Some("ev-undone")); // the compensation → excluded
    let grades: Vec<i64> = conn
        .prepare(review::SELECT_EVENTS_FOR_RETENTION)
        .unwrap()
        .query_map(params!["llp-1", "2026-07-11T00:00:00Z"], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(grades, vec![3], "only the effective event survives");
}

#[test]
fn count_new_cards_introduced_today_counts_first_grade_not_creation() {
    // `COUNT_NEW_CARDS_INTRODUCED_TODAY` is the session admission budget: it
    // must count by first-grade date, not by card creation date — a card
    // minted `new` today but never yet shown must NOT count as "already
    // introduced", or it can never enter its own introducing session.
    let conn = migrated_connection();
    seed_profile_and_pack(&conn);
    conn.execute_batch(
        "INSERT INTO cards (id, learner_language_profile_id, language_pack_id,
             language_pack_version, intent, status, prompt, prompt_language, answer,
             answer_language, accepted_answers_json, answer_evaluation_mode,
             creation_reason, created_by, created_at, updated_at, content_hash,
             content_version)
         VALUES
           ('crd-old-graded-today', 'llp-1', 'test-pack', '0.1.0', 'production',
            'learning', 'p', 'ru', 'a', 'zz-ZZ', '[]', 'exact', 'seed', 'tutor',
            '2020-01-01T00:00:00Z', '2020-01-01T00:00:00Z', 'h1', 1),
           ('crd-new-ungraded', 'llp-1', 'test-pack', '0.1.0', 'production',
            'new', 'p', 'ru', 'a', 'zz-ZZ', '[]', 'exact', 'seed', 'tutor',
            '2026-07-12T00:00:00Z', '2026-07-12T00:00:00Z', 'h2', 1),
           ('crd-reviewed-again', 'llp-1', 'test-pack', '0.1.0', 'production',
            'review', 'p', 'ru', 'a', 'zz-ZZ', '[]', 'exact', 'seed', 'tutor',
            '2020-01-01T00:00:00Z', '2020-01-01T00:00:00Z', 'h3', 1);
         INSERT INTO review_sessions (id, learner_language_profile_id, started_at,
             source, created_at)
         VALUES ('rs-1', 'llp-1', '2026-07-12T00:00:00Z', 'online',
                 '2026-07-12T00:00:00Z');",
    )
    .unwrap();

    // Graded for the very first time today, despite being minted years ago.
    conn.execute(
        review::UPSERT_SCHEDULE_STATE,
        params![
            "crd-old-graded-today",
            "fsrs",
            "v1",
            Option::<String>::None,
            Option::<f64>::None,
            Option::<f64>::None,
            Option::<String>::None,
            Option::<String>::None,
            Option::<i64>::None,
            0_i64,
            1_i64,
            1_i64,
            "hash-a",
            "2026-07-12T01:00:00Z",
        ],
    )
    .unwrap();
    // Its second-ever grade lands today too — must not double count.
    conn.execute(
        review::UPSERT_SCHEDULE_STATE,
        params![
            "crd-reviewed-again",
            "fsrs",
            "v1",
            Option::<String>::None,
            Option::<f64>::None,
            Option::<f64>::None,
            Option::<String>::None,
            Option::<String>::None,
            Option::<i64>::None,
            0_i64,
            2_i64,
            1_i64,
            "hash-b",
            "2026-07-12T01:00:00Z",
        ],
    )
    .unwrap();
    let insert_event = |id: &str, card_id: &str, result_hash: &str| {
        conn.execute(
            "INSERT INTO review_events (id, card_id, review_session_id,
                 client_operation_id, presented_at, answered_at, grade,
                 response_millis, answer_revealed, prior_state_hash,
                 result_state_hash, algorithm_version, source, compensates_event_id,
                 created_at)
             VALUES (?1, ?2, 'rs-1', ?1, '2026-07-12T01:00:00Z',
                     '2026-07-12T01:00:00Z', 3, 1000, 1, 'x', ?3, 'v1', 'online',
                     NULL, '2026-07-12T01:00:00Z')",
            rusqlite::params![id, card_id, result_hash],
        )
        .unwrap();
    };
    insert_event("ev-1", "crd-old-graded-today", "hash-a");
    insert_event("ev-2", "crd-reviewed-again", "hash-b");
    // `crd-new-ungraded` has no schedule-state row at all — never shown yet.

    let n: i64 = conn
        .query_row(
            review::COUNT_NEW_CARDS_INTRODUCED_TODAY,
            params!["llp-1", "2026-07-12T00:00:00Z"],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        n, 1,
        "only the card graded for the very first time today counts"
    );
}

#[test]
fn oldest_candidates_selected_first_and_bounded() {
    // A card that misses the daily mint budget lands as `candidate` with no
    // other path back to `new` — `SELECT_OLDEST_CANDIDATES` is what lets
    // `promote_candidates` give the longest-waiting ones first claim on the
    // next day's budget.
    let conn = migrated_connection();
    seed_profile_and_pack(&conn);
    conn.execute_batch(
        "INSERT INTO cards (id, learner_language_profile_id, language_pack_id,
             language_pack_version, intent, status, prompt, prompt_language, answer,
             answer_language, accepted_answers_json, answer_evaluation_mode,
             creation_reason, created_by, created_at, updated_at, content_hash,
             content_version)
         VALUES
           ('crd-a', 'llp-1', 'test-pack', '0.1.0', 'production', 'candidate',
            'p', 'ru', 'a', 'zz-ZZ', '[]', 'exact', 'seed', 'tutor',
            '2026-07-10T00:00:00Z', '2026-07-10T00:00:00Z', 'h1', 1),
           ('crd-b', 'llp-1', 'test-pack', '0.1.0', 'production', 'candidate',
            'p', 'ru', 'a', 'zz-ZZ', '[]', 'exact', 'seed', 'tutor',
            '2026-07-11T00:00:00Z', '2026-07-11T00:00:00Z', 'h2', 1),
           ('crd-c', 'llp-1', 'test-pack', '0.1.0', 'production', 'new',
            'p', 'ru', 'a', 'zz-ZZ', '[]', 'exact', 'seed', 'tutor',
            '2026-07-09T00:00:00Z', '2026-07-09T00:00:00Z', 'h3', 1);",
    )
    .unwrap();
    let ids: Vec<String> = conn
        .prepare(cards::SELECT_OLDEST_CANDIDATES)
        .unwrap()
        .query_map(params!["llp-1", 1_i64], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(
        ids,
        vec!["crd-a"],
        "oldest candidate first, non-candidate excluded, bounded by limit"
    );
}
