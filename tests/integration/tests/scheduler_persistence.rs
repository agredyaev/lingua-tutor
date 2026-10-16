//! M0 exit gate (15-wbs.md): one review event persists the next due date —
//! scheduler output written to `review_events` + `card_schedule_state` in one
//! transaction shape, read back intact.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use integration_tests::{migrated_connection, seed_profile_and_pack};
use review_engine::{
    FsrsScheduler, Grade, InitialReviewInput, ReviewEventInput, ReviewScheduler, SchedulerSettings,
    Timestamp,
};

/// Repository-boundary conversion (04-database.md §1): unix seconds → RFC 3339.
fn rfc3339(t: Timestamp) -> String {
    chrono::DateTime::from_timestamp(t.0, 0)
        .unwrap()
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

#[test]
fn review_event_persists_next_due_date() {
    let conn = migrated_connection();
    seed_profile_and_pack(&conn);
    conn.execute_batch(
        "INSERT INTO cards (id, learner_language_profile_id, language_pack_id,
             language_pack_version, intent, status, prompt, prompt_language, answer,
             answer_language, answer_evaluation_mode, creation_reason, content_hash,
             created_by, created_at, updated_at)
         VALUES ('card-1', 'llp-1', 'test-pack', '0.1.0', 'production', 'new', 'p',
                 'ru', 'a', 'zz', 'exact', 'test', 'hash-1', 'system',
                 '2026-07-12T00:00:00Z', '2026-07-12T00:00:00Z');
         INSERT INTO review_sessions (id, learner_language_profile_id, started_at,
             source, created_at)
         VALUES ('rs-1', 'llp-1', '2026-07-12T00:00:00Z', 'online',
                 '2026-07-12T00:00:00Z');",
    )
    .unwrap();

    let scheduler = FsrsScheduler::new().unwrap();
    let settings = SchedulerSettings::default();
    let t0 = Timestamp(1_780_000_000);
    let prior = scheduler
        .initial_state(InitialReviewInput { now: t0 })
        .unwrap();
    let result = scheduler
        .apply_review(
            &prior,
            &ReviewEventInput {
                grade: Grade::Good,
                answered_at: t0,
            },
            &settings,
        )
        .unwrap();

    // Event + schedule state in one transaction (04-database.md §1).
    let tx = conn.unchecked_transaction().unwrap();
    tx.execute(
        "INSERT INTO review_events (id, card_id, review_session_id,
             client_operation_id, presented_at, answered_at, grade, response_millis,
             answer_revealed, prior_state_hash, result_state_hash, algorithm_version,
             source, created_at)
         VALUES ('ev-1', 'card-1', 'rs-1', 'op-1', '2026-07-12T00:00:00Z',
                 '2026-07-12T00:00:05Z', 3, 5000, 1, ?1, ?2, ?3, 'online',
                 '2026-07-12T00:00:05Z')",
        rusqlite::params![
            prior.state_hash(),
            result.state_hash(),
            result.algorithm_version
        ],
    )
    .unwrap();
    tx.execute(
        "INSERT INTO card_schedule_state (card_id, algorithm, algorithm_version,
             difficulty, stability, last_review_at, due_at,
             scheduled_interval_seconds, lapse_count, review_count, state_version,
             state_hash, updated_at)
         VALUES ('card-1', ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11,
                 '2026-07-12T00:00:05Z')",
        rusqlite::params![
            result.algorithm,
            result.algorithm_version,
            result.difficulty,
            result.stability,
            result.last_review_at.map(rfc3339),
            result.due_at.map(rfc3339),
            result.scheduled_interval_seconds,
            result.lapse_count,
            result.review_count,
            result.state_version,
            result.state_hash(),
        ],
    )
    .unwrap();
    tx.commit().unwrap();

    let (due_at, hash): (String, String) = conn
        .query_row(
            "SELECT css.due_at, css.state_hash
             FROM card_schedule_state css
             JOIN review_events re ON re.card_id = css.card_id
             WHERE re.id = 'ev-1'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(
        due_at,
        rfc3339(result.due_at.unwrap()),
        "next due date persisted"
    );
    assert_eq!(hash, result.state_hash(), "state hash round-trips");
}
