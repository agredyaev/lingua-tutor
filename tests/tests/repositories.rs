//! Repository SQL exercised against scratch SQLite (12-testing.md: real
//! database, not mocked SQL). Each constant runs with realistic params;
//! row structs round-trip through domain types.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use integration_tests::{migrated_connection, seed_profile_and_pack};
use rusqlite::params;
use worker_api::repositories::{cards, lessons, oauth, packs, profiles, records, review};

const NOW: &str = "2026-07-12T10:00:00Z";

#[test]
fn learner_profile_upsert_and_select() {
    let conn = migrated_connection();
    conn.execute(
        profiles::UPSERT_LEARNER,
        params!["learner-1", "Europe/Madrid", 2, 20, 8, 0.90, 12, NOW],
    )
    .unwrap();
    // Second upsert updates, no duplicate.
    conn.execute(
        profiles::UPSERT_LEARNER,
        params!["learner-1", "Europe/Madrid", 3, 25, 6, 0.85, 10, NOW],
    )
    .unwrap();
    let row = conn
        .query_row(profiles::SELECT_LEARNER, [], |r| {
            Ok(profiles::LearnerProfileRow {
                id: r.get(0)?,
                timezone: r.get(1)?,
                daily_session_target: r.get(2)?,
                session_minutes_target: r.get(3)?,
                max_new_cards: r.get(4)?,
                desired_retention: r.get(5)?,
                max_review_minutes: r.get(6)?,
            })
        })
        .unwrap();
    let domain = row.into_domain();
    assert_eq!(domain.daily_session_target, 3);
    assert_eq!(domain.max_new_cards, 6);
}

#[test]
fn language_profile_insert_select_update_roundtrip() {
    let conn = migrated_connection();
    seed_profile_and_pack(&conn); // creates llp-1
    let row = conn
        .query_row(profiles::SELECT_LANGUAGE_PROFILE, ["llp-1"], |r| {
            Ok(profiles::LanguageProfileRow {
                id: r.get(0)?,
                learner_id: r.get(1)?,
                language_pack_id: r.get(2)?,
                source_language: r.get(3)?,
                target_locale: r.get(4)?,
                estimated_level: r.get(5)?,
                current_lesson_number: r.get(6)?,
                status: r.get(7)?,
                is_default: r.get(8)?,
                preferences_json: r.get(9)?,
            })
        })
        .unwrap();
    let mut domain = row.into_domain().unwrap();
    assert_eq!(domain.source_language.as_str(), "ru");

    domain.estimated_level = Some("A2".to_owned());
    domain.current_lesson_number = 4;
    let back = profiles::LanguageProfileRow::from_domain(&domain);
    conn.execute(
        profiles::UPDATE_LANGUAGE_PROFILE,
        params![
            back.id,
            back.estimated_level,
            back.status,
            back.is_default,
            back.preferences_json,
            back.current_lesson_number,
            NOW
        ],
    )
    .unwrap();
    let level: Option<String> = conn
        .query_row(
            "SELECT estimated_level FROM learner_language_profiles WHERE id='llp-1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(level.as_deref(), Some("A2"));
}

#[test]
fn pack_upsert_from_manifest() {
    let conn = migrated_connection();
    let pack = language_pack_es_es::register();
    let row = packs::PackRow::from_manifest(pack.manifest()).unwrap();
    conn.execute(
        packs::UPSERT_PACK,
        params![
            row.id,
            row.package_name,
            row.version,
            row.contract_version,
            row.target_language,
            row.target_locale,
            row.display_name,
            row.capabilities_json,
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
            params!["les-1", NOW, 1260, "summary", "next goal"],
        )
        .unwrap();
    assert_eq!(changed, 1);
    // Completing again: no matching row (status already completed).
    let changed_again = conn
        .execute(
            lessons::COMPLETE_LESSON,
            params!["les-1", NOW, 1260, "summary", "next goal"],
        )
        .unwrap();
    assert_eq!(changed_again, 0, "complete is not repeatable");

    let row = conn
        .query_row(lessons::SELECT_LESSON_BY_KEY, ["op-import-1"], lesson_row)
        .unwrap();
    let domain = row.into_domain().unwrap();
    assert_eq!(domain.lesson_number, 1);
    assert_eq!(domain.tutor_summary.as_deref(), Some("summary"));

    conn.execute(
        lessons::INSERT_METRICS,
        params!["les-1", 540, 6, 3, Some(0.7), Some(2100), 2, Some(0.6), NOW],
    )
    .unwrap();
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
    let row = conn
        .query_row(
            records::SELECT_ITEM_BY_NORMALIZED,
            params!["llp-1", "¿qué tal?"],
            item_row,
        )
        .unwrap();
    let domain = row.into_domain().unwrap();
    assert_eq!(domain.target_text, "¿Qué tal?");

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
            NOW
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
    let row = conn
        .query_row(records::SELECT_ERROR_TARGET, ["et-1"], error_row)
        .unwrap();
    let domain = row.into_domain().unwrap();
    assert_eq!(domain.occurrence_count, 2);
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
            Some("¿tiene algo…?"),
            0,
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

fn lesson_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<lessons::LessonRow> {
    Ok(lessons::LessonRow {
        id: r.get(0)?,
        learner_language_profile_id: r.get(1)?,
        language_pack_id: r.get(2)?,
        language_pack_version: r.get(3)?,
        lesson_number: r.get(4)?,
        status: r.get(5)?,
        scenario: r.get(6)?,
        planned_goal: r.get(7)?,
        source: r.get(8)?,
        started_at: r.get(9)?,
        completed_at: r.get(10)?,
        duration_seconds: r.get(11)?,
        tutor_summary: r.get(12)?,
        next_goal: r.get(13)?,
        idempotency_key: r.get(14)?,
    })
}

fn item_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<records::ItemRow> {
    Ok(records::ItemRow {
        id: r.get(0)?,
        learner_language_profile_id: r.get(1)?,
        language_pack_id: r.get(2)?,
        item_type: r.get(3)?,
        target_text: r.get(4)?,
        normalized_target_text: r.get(5)?,
        prompt_text: r.get(6)?,
        prompt_language: r.get(7)?,
        status: r.get(8)?,
        language_metadata_json: r.get(9)?,
        introduced_lesson_id: r.get(10)?,
    })
}

fn error_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<records::ErrorTargetRow> {
    Ok(records::ErrorTargetRow {
        id: r.get(0)?,
        learner_language_profile_id: r.get(1)?,
        language_pack_id: r.get(2)?,
        language_pack_version: r.get(3)?,
        core_category: r.get(4)?,
        pack_error_code: r.get(5)?,
        original_production: r.get(6)?,
        corrected_production: r.get(7)?,
        explanation: r.get(8)?,
        occurrence_count: r.get(9)?,
        method_change_required: r.get(10)?,
        status: r.get(11)?,
    })
}

// --- oauth repository (WBS 12.1a; 13-roadmap.md §3, 10-security.md §2) ---

fn seed_oauth_client(conn: &rusqlite::Connection) {
    conn.execute(
        oauth::INSERT_CLIENT,
        params![
            "cli-1",
            "ChatGPT",
            r#"["https://chatgpt.com/connector_platform_oauth_redirect"]"#,
            NOW
        ],
    )
    .unwrap();
}

#[test]
fn oauth_client_roundtrip_parses_redirect_uris() {
    let conn = migrated_connection();
    seed_oauth_client(&conn);
    let row = conn
        .query_row(oauth::SELECT_CLIENT, ["cli-1"], |r| {
            Ok(oauth::ClientRow {
                client_id: r.get(0)?,
                client_name: r.get(1)?,
                redirect_uris_json: r.get(2)?,
            })
        })
        .unwrap();
    assert_eq!(
        row.redirect_uris().unwrap(),
        vec!["https://chatgpt.com/connector_platform_oauth_redirect".to_owned()]
    );
}

#[test]
fn oauth_code_consumes_exactly_once_and_respects_expiry() {
    let conn = migrated_connection();
    seed_oauth_client(&conn);
    conn.execute(
        oauth::INSERT_CODE,
        params![
            "code-hash-1",
            "cli-1",
            "https://chatgpt.com/cb",
            "learning.read lessons.write",
            "challenge-s256",
            "2026-07-12T10:10:00Z"
        ],
    )
    .unwrap();
    let first = conn
        .query_row(oauth::CONSUME_CODE, params!["code-hash-1", NOW], |r| {
            Ok((
                r.get::<_, String>(0)?, // client_id
                r.get::<_, String>(1)?, // redirect_uri
                r.get::<_, String>(2)?, // scope
                r.get::<_, String>(3)?, // code_challenge
            ))
        })
        .unwrap();
    assert_eq!(first.2, "learning.read lessons.write");
    // Second consume: single-use — no row.
    let second = conn.query_row(oauth::CONSUME_CODE, params!["code-hash-1", NOW], |r| {
        r.get::<_, String>(0)
    });
    assert!(second.is_err(), "code must be single-use");
    // Expired code never consumes.
    conn.execute(
        oauth::INSERT_CODE,
        params![
            "code-hash-2",
            "cli-1",
            "https://chatgpt.com/cb",
            "learning.read",
            "chal",
            "2026-07-12T09:00:00Z"
        ],
    )
    .unwrap();
    let expired = conn.query_row(oauth::CONSUME_CODE, params!["code-hash-2", NOW], |r| {
        r.get::<_, String>(0)
    });
    assert!(expired.is_err(), "expired code must not consume");
}

#[test]
fn oauth_token_lookup_filters_expiry_and_revocation() {
    let conn = migrated_connection();
    seed_oauth_client(&conn);
    conn.execute(
        oauth::INSERT_TOKEN,
        params![
            "at-hash",
            "access",
            "cli-1",
            "learning.read",
            "2026-07-12T11:00:00Z",
            NOW
        ],
    )
    .unwrap();
    let scope: String = conn
        .query_row(
            oauth::SELECT_TOKEN,
            params!["at-hash", "access", NOW],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(scope, "learning.read");
    // Wrong kind — miss.
    assert!(
        conn.query_row(
            oauth::SELECT_TOKEN,
            params!["at-hash", "refresh", NOW],
            |r| r.get::<_, String>(0)
        )
        .is_err()
    );
    // Expired — miss.
    assert!(
        conn.query_row(
            oauth::SELECT_TOKEN,
            params!["at-hash", "access", "2026-07-12T12:00:00Z"],
            |r| r.get::<_, String>(0)
        )
        .is_err()
    );
    // Revoked — miss.
    conn.execute(oauth::REVOKE_TOKEN, params!["at-hash"])
        .unwrap();
    assert!(
        conn.query_row(
            oauth::SELECT_TOKEN,
            params!["at-hash", "access", NOW],
            |r| r.get::<_, String>(0)
        )
        .is_err()
    );
}

#[test]
fn oauth_purge_removes_expired_rows() {
    let conn = migrated_connection();
    seed_oauth_client(&conn);
    conn.execute(
        oauth::INSERT_TOKEN,
        params![
            "old",
            "access",
            "cli-1",
            "learning.read",
            "2026-07-12T09:00:00Z",
            NOW
        ],
    )
    .unwrap();
    conn.execute(
        oauth::INSERT_CODE,
        params![
            "old-code",
            "cli-1",
            "https://chatgpt.com/cb",
            "learning.read",
            "chal",
            "2026-07-12T09:00:00Z"
        ],
    )
    .unwrap();
    conn.execute(oauth::PURGE_EXPIRED_TOKENS, params![NOW])
        .unwrap();
    conn.execute(oauth::PURGE_EXPIRED_CODES, params![NOW])
        .unwrap();
    let tokens: i64 = conn
        .query_row("SELECT COUNT(*) FROM oauth_tokens", [], |r| r.get(0))
        .unwrap();
    let codes: i64 = conn
        .query_row("SELECT COUNT(*) FROM oauth_codes", [], |r| r.get(0))
        .unwrap();
    assert_eq!((tokens, codes), (0, 0));
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

#[test]
fn oauth_client_count_reflects_registrations() {
    let conn = migrated_connection();
    let count = || -> i64 {
        conn.query_row(oauth::COUNT_CLIENTS, [], |r| r.get(0))
            .unwrap()
    };
    assert_eq!(count(), 0);
    seed_oauth_client(&conn);
    assert_eq!(count(), 1);
    conn.execute(
        oauth::INSERT_CLIENT,
        params!["cli-2", "second", r#"["https://chatgpt.com/cb2"]"#, NOW],
    )
    .unwrap();
    assert_eq!(count(), 2);
}
