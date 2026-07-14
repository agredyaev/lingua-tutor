//! Repository SQL exercised against scratch SQLite (12-testing.md: real
//! database, not mocked SQL). Each constant runs with realistic params;
//! row structs round-trip through domain types.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use integration_tests::{migrated_connection, seed_profile_and_pack};
use rusqlite::params;
use worker_api::repositories::{lessons, oauth, packs, profiles, records};

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
