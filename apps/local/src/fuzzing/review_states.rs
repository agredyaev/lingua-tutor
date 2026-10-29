//! Queue ordering/capacity, historical budgets and deliberately malformed persisted states.
use super::*;
use rusqlite::params;

fn save_state(
    db: &rusqlite::Connection,
    card: &str,
    state: &review_engine::ScheduledState,
    at: &str,
) {
    let last = state
        .last_review_at
        .map(|t| crate::repositories::to_rfc3339(learning_core::Timestamp(t.0)));
    let due = state
        .due_at
        .map(|t| crate::repositories::to_rfc3339(learning_core::Timestamp(t.0)));
    crate::operations::card_write::upsert_schedule_state(
        db,
        card,
        &state.algorithm,
        &state.algorithm_version,
        state.parameter_set_id.as_deref(),
        state.difficulty,
        state.stability,
        last.as_deref(),
        due.as_deref(),
        state.scheduled_interval_seconds,
        state.lapse_count,
        state.review_count,
        state.state_version,
        &review_engine::state_hash(state),
        at,
    )
    .expect("persist schedule fixture");
}

/// Fuzz mixed due queues, long review histories and invalid persisted fields.
pub fn exercise_review_states(data: &[u8]) {
    let seed = data.first().copied().unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("lingua-review-states-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).expect("directory");
    let path = dir.join("db.sqlite3");
    crate::storage::open(&path).expect("database");
    let env = Env::new(path, "fuzz-key".into());
    let registry = crate::built_in_language_packs().expect("registry");
    let created = crate::operations::profile_write::create_profile(&registry, &env, "es-ES", "ru")
        .expect("profile");
    let profile = created["id"].as_str().expect("id");
    let version = language_pack_registry::manifest(&registry, "es-ES")
        .expect("manifest")
        .version;
    seed_fuzz_learning_item(&env, profile);
    let result = rich_lesson_result(profile, version, "review-states-bootstrap".into(), None, 0);
    crate::operations::complete_lesson::complete_lesson(&registry, &env, &result)
        .expect("bootstrap");
    let card = first_card_id(&env, profile).expect("card");
    let db = env.connection().expect("db");
    let now = crate::api::now();
    let at = crate::repositories::to_rfc3339(now);
    let old = crate::repositories::to_rfc3339(learning_core::Timestamp(now.0 - 10 * 86400));
    // Replay historical completed sessions on distinct local days, including duplicate days.
    for n in 1..=4 {
        let s = crate::operations::review_ops::create_session(&env, profile, "offline")
            .expect("historical session");
        let time = crate::repositories::to_rfc3339(learning_core::Timestamp(
            now.0 - i64::from(n % 3 + 1) * 86400,
        ));
        db.execute(
            "UPDATE review_sessions SET completed_at=?2,review_seconds=?3 WHERE id=?1",
            params![s["id"].as_str(), time, 600 + n * 60],
        )
        .expect("historical session date");
    }
    db.execute(
        "UPDATE learner_profile SET timezone=?1",
        [if seed & 1 == 0 {
            "UTC"
        } else {
            "Europe/Madrid"
        }],
    )
    .expect("timezone");
    if seed & 2 != 0 {
        db.execute("UPDATE learner_profile SET max_new_cards=0", [])
            .expect("zero budget");
    }
    let columns: Vec<String> = db
        .prepare("PRAGMA table_info(cards)")
        .expect("columns")
        .query_map([], |r| r.get(1))
        .expect("column query")
        .collect::<rusqlite::Result<_>>()
        .expect("column names");
    let select = columns
        .iter()
        .map(|c| match c.as_str() {
            "id" | "content_hash" => "?2".to_owned(),
            "status" => "?3".to_owned(),
            "created_at" => "?4".to_owned(),
            _ => format!("\"{c}\""),
        })
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        "INSERT INTO cards ({}) SELECT {select} FROM cards WHERE id=?1",
        columns.join(",")
    );
    let scheduler = if seed & 4 == 0 {
        review_engine::FSRS_ID
    } else {
        review_engine::FALLBACK_ID
    };
    let scratch = review_engine::new_scheduler_scratch(scheduler).expect("scratch");
    let base = review_engine::initial_state(scheduler, review_engine::Timestamp(now.0 - 86400 * 7))
        .expect("state");
    let state = review_engine::apply_review(
        scheduler,
        &scratch,
        &base,
        &review_engine::ReviewEventInput {
            grade: review_engine::Grade::Good,
            answered_at: review_engine::Timestamp(now.0 - 86400 * 7),
        },
        &review_engine::SchedulerSettings::default(),
    )
    .expect("scheduled state");
    for n in 0..(1 + usize::from(seed % 70)) {
        let id = format!("queue-{n:03}");
        let status = ["new", "review", "learning", "relearning", "candidate"][n % 5];
        db.execute(&sql, params![card, id, status, old])
            .expect("clone card");
        if status != "candidate" {
            save_state(&db, &id, &state, &at);
        }
    }
    let session =
        crate::operations::review_ops::create_session(&env, profile, "online").expect("session");
    let sid = session["id"].as_str().expect("session id");
    let queue = crate::operations::review_queue::queue(&env, sid).expect("valid mixed queue");
    let rows = queue["queue"].as_array().expect("queue array");
    assert!(rows.len() <= 50);
    let ids = rows
        .iter()
        .map(|r| r["card_id"].as_str().expect("queue card id"))
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(ids.len(), rows.len());
    // All answer evaluation paths use the real stored card and language pack.
    let answer = String::from_utf8_lossy(data);
    let _ = crate::handlers::review::evaluate(&registry, &env, &card, &answer)
        .expect("answer evaluation");
    let _ = no_storage_error(crate::handlers::review::evaluate(
        &registry, &env, "missing", &answer,
    ));
    // A long effective event stream followed by undo drives replay and recommendation rules.
    for n in 0..12 {
        let prior = schedule_hash(&env, &card).expect("current hash");
        let operation = format!("review-state-{n}");
        let answered =
            crate::repositories::to_rfc3339(learning_core::Timestamp(now.0 + i64::from(n)));
        crate::operations::review_ops::submit_event(
            &env,
            sid,
            crate::operations::review_ops::ReviewEventInputData {
                client_operation_id: &operation,
                card_id: &card,
                grade: 1 + seed % 4,
                presented_at: &at,
                answered_at: &answered,
                response_millis: 10000,
                answer_revealed: seed & 1 != 0,
                learner_note: None,
                prior_state_hash: &prior,
                source: "online",
            },
        )
        .expect("valid graded stream");
    }
    crate::operations::review_ops::undo_last(&env, sid).expect("undo stream tail");
    assert!(matches!(
        crate::operations::review_ops::undo_last(&env, sid),
        Err(Error::Operation {
            code: "already_undone",
            ..
        })
    ));
    crate::operations::review_ops::complete_session(&env, sid, 12, 600)
        .expect("complete review stream");
    assert!(matches!(
        crate::operations::review_ops::complete_session(&env, sid, 12, 600),
        Err(Error::Operation {
            code: "already_completed",
            ..
        })
    ));
    assert!(crate::operations::review_ops::complete_session(&env, "missing", 0, 0).is_err());
    assert!(crate::operations::review_ops::undo_last(&env, "missing").is_err());
    assert!(
        crate::operations::review_ops::create_session(&env, profile, "invalid-source").is_err()
    );
    db.execute_batch("PRAGMA ignore_check_constraints=ON")
        .expect("adversarial fixture mode");
    for (column, bad, good) in [
        ("desired_retention", "0.1", "0.9"),
        ("max_review_minutes", "0", "12"),
    ] {
        db.execute(&format!("UPDATE learner_profile SET {column}={bad}"), [])
            .expect("corrupt policy fixture");
        assert!(
            crate::operations::review_ops::scheduler_settings_for_profile(&db, profile).is_err()
        );
        db.execute(&format!("UPDATE learner_profile SET {column}={good}"), [])
            .expect("restore policy");
    }

    let field = match seed % 8 {
        0 => "language_pack_version",
        1 => "intent",
        2 => "status",
        3 => "created_by",
        4 => "accepted_answers_json",
        5 => "answer_evaluation_mode",
        6 => "language_pack_id",
        _ => "created_at",
    };
    let original: String = db
        .query_row(
            &format!("SELECT {field} FROM cards WHERE id=?1"),
            [&card],
            |r| r.get(0),
        )
        .expect("original card field");
    if field != "language_pack_id" {
        db.execute(
            &format!("UPDATE cards SET {field}='invalid' WHERE id=?1"),
            [&card],
        )
        .expect("corrupt field");
        let _ = crate::handlers::review::evaluate(&registry, &env, &card, &answer);
        let _ = crate::operations::review_queue::queue(&env, sid);
        db.execute(
            &format!("UPDATE cards SET {field}=?2 WHERE id=?1"),
            params![card, original],
        )
        .expect("restore field");
    }
    let hash = schedule_hash(&env, &card).expect("hash");
    for (grade, answered, prior, target, session) in [
        (0, at.as_str(), hash.as_str(), card.as_str(), sid),
        (3, "bad-time", hash.as_str(), card.as_str(), sid),
        (3, at.as_str(), "stale", card.as_str(), sid),
        (3, at.as_str(), hash.as_str(), "missing", sid),
        (3, at.as_str(), hash.as_str(), card.as_str(), "missing"),
    ] {
        assert!(
            crate::operations::review_ops::submit_event(
                &env,
                session,
                crate::operations::review_ops::ReviewEventInputData {
                    client_operation_id: "invalid-event",
                    card_id: target,
                    grade,
                    presented_at: &at,
                    answered_at: answered,
                    response_millis: 10,
                    answer_revealed: false,
                    learner_note: None,
                    prior_state_hash: prior,
                    source: "online"
                }
            )
            .is_err()
        );
    }
    let status: String = db
        .query_row("SELECT status FROM cards WHERE id=?1", [&card], |r| {
            r.get(0)
        })
        .expect("status");
    let input = || crate::operations::review_ops::ReviewEventInputData {
        client_operation_id: "adversarial-event",
        card_id: &card,
        grade: 3,
        presented_at: &at,
        answered_at: &at,
        response_millis: 500,
        answer_revealed: false,
        learner_note: None,
        prior_state_hash: &hash,
        source: "online",
    };
    for corrupt_status in ["invalid", "candidate", "suspended", "retired"] {
        db.execute(
            "UPDATE cards SET status=?2 WHERE id=?1",
            params![card, corrupt_status],
        )
        .expect("status fixture");
        assert!(crate::operations::review_ops::submit_event(&env, sid, input()).is_err());
    }
    db.execute(
        "UPDATE cards SET status=?2 WHERE id=?1",
        params![card, status],
    )
    .expect("restore status");
    db.execute(
        "UPDATE card_schedule_state SET state_hash='corrupt' WHERE card_id=?1",
        [&card],
    )
    .expect("corrupt hash");
    assert!(crate::operations::review_ops::submit_event(&env, sid, input()).is_err());
    db.execute(
        "UPDATE card_schedule_state SET state_hash=?2 WHERE card_id=?1",
        params![card, hash],
    )
    .expect("restore hash");
    for (algorithm, version) in [
        ("invalid", "x"),
        ("fsrs", "unavailable"),
        ("fallback", "unavailable"),
    ] {
        assert!(crate::operations::review_ops::select_scheduler(algorithm, version).is_err());
    }
    for ids in [
        serde_json::json!(null),
        serde_json::json!([42]),
        serde_json::json!([card, "missing"]),
    ] {
        assert!(crate::operations::suspend_cards::suspend_cards_json(&env, &ids, None).is_err());
    }
    db.execute("UPDATE cards SET status='retired' WHERE id=?1", [&card])
        .expect("retire");
    assert!(
        crate::operations::suspend_cards::suspend_cards_json(
            &env,
            &serde_json::json!([card]),
            None
        )
        .is_err()
    );
    let _ = crate::repositories::cards::select_card_sql();
    let _ = crate::repositories::cards::select_card_by_hash_sql();
    for t in [0, 1774742400, 1792886400, 1800000000] {
        for tz in ["UTC", "Europe/Madrid", "invalid"] {
            let _ = crate::operations::card_budget::local_day_start_utc(
                learning_core::Timestamp(t),
                tz,
            );
        }
    }
    assert_database_integrity(&env);
    drop(db);
    drop(env);
    std::fs::remove_dir_all(dir).expect("cleanup");
}
