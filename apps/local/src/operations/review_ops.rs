//! Review write operations with transaction-local authority and streaming replay.

use crate::repositories::{from_rfc3339, review, to_rfc3339};
use crate::{Env, Result};
use review_engine::{Grade, ReviewEventInput, ScheduledState, SchedulerSettings, Timestamp};
use rusqlite::{OptionalExtension as _, TransactionBehavior, params};

use super::card_write::{update_card_status, upsert_schedule_state};
use super::recommendation_write::insert_recommendation;

pub(crate) fn select_scheduler(algorithm: &str, version: &str) -> Result<u32> {
    let id = review_engine::algorithm_id(algorithm).ok_or_else(|| {
        crate::Error::operation(
            500,
            "corrupt_row",
            format!("unknown scheduler algorithm '{algorithm}'"),
            serde_json::Value::Null,
        )
    })?;
    let current = review_engine::initial_state(id, Timestamp(0))
        .map_err(|error| crate::Error::RustError(error.to_string()))?;
    if current.algorithm_version != version {
        return Err(crate::Error::operation(
            409,
            "scheduler_version_unavailable",
            format!(
                "persisted scheduler version '{version}' is not available; current is '{}'",
                current.algorithm_version
            ),
            serde_json::Value::Null,
        ));
    }
    Ok(id)
}

pub(crate) fn scheduler_settings_for_profile(
    database: &rusqlite::Connection,
    profile_id: &str,
) -> Result<SchedulerSettings> {
    let (desired_retention, max_new_cards, max_review_minutes, timezone): (f64, u8, u8, String) =
        database.query_row(
            "SELECT l.desired_retention,l.max_new_cards,l.max_review_minutes,l.timezone
         FROM learner_language_profiles p
         JOIN learner_profile l ON l.id=p.learner_id
         WHERE p.id=?1",
            params![profile_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )?;
    if !desired_retention.is_finite() || !(0.70..=0.97).contains(&desired_retention) {
        return Err(crate::Error::operation(
            500,
            "corrupt_row",
            "desired_retention must be finite and in [0.70,0.97]",
            serde_json::Value::Null,
        ));
    }
    if max_review_minutes == 0 {
        return Err(crate::Error::operation(
            500,
            "corrupt_row",
            "max_review_minutes must be positive",
            serde_json::Value::Null,
        ));
    }
    Ok(SchedulerSettings {
        desired_retention,
        max_new_cards_per_day: max_new_cards,
        target_review_minutes: (8_u8.min(max_review_minutes), max_review_minutes),
        max_review_batch: 50,
        max_interval_days: 36_500,
        timezone,
    })
}

pub(crate) struct ReviewEventInputData<'a> {
    pub client_operation_id: &'a str,
    pub card_id: &'a str,
    pub grade: u8,
    pub presented_at: &'a str,
    pub answered_at: &'a str,
    pub response_millis: u32,
    pub answer_revealed: bool,
    pub learner_note: Option<&'a str>,
    pub prior_state_hash: &'a str,
    pub source: &'a str,
}

pub(crate) fn create_session(
    env: &Env,
    profile_id: &str,
    source: &str,
) -> Result<serde_json::Value> {
    if !matches!(source, "online" | "offline") {
        return crate::api::operation_error(400, "invalid_field", "source must be online|offline");
    }
    let id = crate::api::new_id("rs");
    let at = to_rfc3339(crate::api::now());
    let database = env.connection()?;
    database.execute(review::INSERT_SESSION, params![id, profile_id, at, source])?;
    crate::json(&serde_json::json!({ "id": id }))
}

pub(crate) fn submit_event(
    env: &Env,
    session_id: &str,
    input: ReviewEventInputData<'_>,
) -> Result<serde_json::Value> {
    let grade = Grade::try_from(input.grade).map_err(|_| {
        crate::Error::operation(
            400,
            "invalid_field",
            "grade must be 1..=4",
            serde_json::Value::Null,
        )
    })?;
    let answered_at = from_rfc3339(input.answered_at)
        .map(|timestamp| review_engine::Timestamp(timestamp.0))
        .map_err(|_| {
            crate::Error::operation(
                400,
                "invalid_timestamp",
                "answered_at must be RFC 3339",
                serde_json::Value::Null,
            )
        })?;

    let mut database = env.connection()?;
    let transaction = database.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let profile_id: String = transaction
        .query_row(review::SELECT_SESSION, params![session_id], |row| {
            row.get("learner_language_profile_id")
        })
        .optional()?
        .ok_or_else(|| {
            crate::Error::operation(404, "not_found", "unknown session", serde_json::Value::Null)
        })?;
    let settings = scheduler_settings_for_profile(&transaction, &profile_id)?;

    if let Some((event_id, card_id, result_state_hash)) = transaction
        .query_row(
            review::SELECT_EVENT_BY_OP,
            params![input.client_operation_id],
            |row| {
                Ok((
                    row.get::<_, String>("id")?,
                    row.get::<_, String>("card_id")?,
                    row.get::<_, String>("result_state_hash")?,
                ))
            },
        )
        .optional()?
    {
        transaction.commit()?;
        return crate::json(&serde_json::json!({
            "event_id": event_id,
            "card_id": card_id,
            "result_state_hash": result_state_hash,
            "duplicate": true,
            "next": null,
        }));
    }

    let status_text: String = transaction
        .query_row(
            "SELECT status FROM cards WHERE id=?1 AND learner_language_profile_id=?2",
            params![input.card_id, profile_id],
            |row| row.get(0),
        )
        .optional()?
        .ok_or_else(|| {
            crate::Error::operation(404, "not_found", "unknown card", serde_json::Value::Null)
        })?;
    let status = crate::repositories::card_status_from_db(&status_text).ok_or_else(|| {
        crate::Error::operation(
            500,
            "corrupt_row",
            "stored card status is invalid",
            serde_json::Value::Null,
        )
    })?;
    let new_status = next_status(status, grade).ok_or_else(|| {
        crate::Error::operation(
            409,
            "not_reviewable",
            "card is not in a reviewable status",
            serde_json::Value::Null,
        )
    })?;

    let (state, stored_hash) = load_state(&transaction, input.card_id)?.ok_or_else(|| {
        crate::Error::operation(
            409,
            "not_reviewable",
            "card has no schedule state",
            serde_json::Value::Null,
        )
    })?;
    let computed_hash = review_engine::state_hash(&state);
    if computed_hash != stored_hash {
        return Err(crate::Error::operation(
            500,
            "corrupt_row",
            "stored schedule hash does not match schedule columns",
            serde_json::Value::Null,
        ));
    }
    if computed_hash != input.prior_state_hash {
        return Err(crate::Error::operation(
            409,
            "conflict_stale_state",
            "state changed; refetch queue",
            serde_json::Value::Null,
        ));
    }

    let scheduler = select_scheduler(&state.algorithm, &state.algorithm_version)?;
    let result = review_engine::apply_review(
        scheduler,
        &review_engine::new_scheduler_scratch(scheduler)
            .map_err(|error| crate::Error::RustError(error.to_string()))?,
        &state,
        &ReviewEventInput { grade, answered_at },
        &settings,
    )
    .map_err(|error| {
        crate::Error::operation(
            500,
            "scheduler_error",
            error.to_string(),
            serde_json::Value::Null,
        )
    })?;

    let result_hash = review_engine::state_hash(&result);
    let at = to_rfc3339(crate::api::now());
    let event_id = crate::api::new_id("ev");

    transaction.execute(
        review::INSERT_EVENT,
        params![
            event_id,
            input.card_id,
            session_id,
            input.client_operation_id,
            input.presented_at,
            input.answered_at,
            input.grade,
            input.response_millis,
            u8::from(input.answer_revealed),
            input.learner_note,
            computed_hash,
            result_hash,
            result.algorithm_version,
            input.source,
            Option::<String>::None,
            at,
        ],
    )?;

    write_state(&transaction, input.card_id, &result, &result_hash, &at)?;

    update_card_status(
        &transaction,
        input.card_id,
        crate::repositories::card_status_db(&new_status),
        None,
        &at,
    )?;

    transaction.commit()?;

    crate::json(&serde_json::json!({
        "event_id": event_id,
        "card_id": input.card_id,
        "card_status": new_status,
        "new_state": {
            "due_at": result.due_at.map(|value| to_rfc3339(learning_core::Timestamp(value.0))),
            "scheduled_interval_seconds": result.scheduled_interval_seconds,
            "state_hash": result_hash,
            "review_count": result.review_count,
            "lapse_count": result.lapse_count,
        },
        "next": null,
    }))
}

pub(crate) fn undo_last(env: &Env, session_id: &str) -> Result<serde_json::Value> {
    let mut database = env.connection()?;
    let transaction = database.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let profile_id: String = transaction
        .query_row(review::SELECT_SESSION, params![session_id], |row| {
            row.get("learner_language_profile_id")
        })
        .optional()?
        .ok_or_else(|| {
            crate::Error::operation(404, "not_found", "unknown session", serde_json::Value::Null)
        })?;
    let settings = scheduler_settings_for_profile(&transaction, &profile_id)?;

    let last = transaction
        .query_row(
            review::SELECT_LAST_SESSION_EVENT,
            params![session_id],
            |row| {
                Ok((
                    row.get::<_, String>("id")?,
                    row.get::<_, String>("card_id")?,
                    row.get::<_, String>("client_operation_id")?,
                    row.get::<_, u8>("grade")?,
                    row.get::<_, Option<String>>("compensates_event_id")?,
                ))
            },
        )
        .optional()?;
    let Some((last_id, card_id, client_operation_id, grade, compensates)) = last else {
        return crate::api::operation_error(404, "not_found", "no events in session");
    };
    if compensates.is_some() {
        return crate::api::operation_error(
            409,
            "already_undone",
            "only the immediately previous grade",
        );
    }

    let (current_state, _) = load_state(&transaction, &card_id)?
        .ok_or_else(|| crate::Error::RustError("card schedule missing for undo".to_owned()))?;
    let scheduler = select_scheduler(&current_state.algorithm, &current_state.algorithm_version)?;

    let created_at: String = transaction
        .query_row(
            "SELECT created_at FROM cards WHERE id=?1",
            params![card_id],
            |row| row.get(0),
        )
        .optional()?
        .ok_or_else(|| crate::Error::RustError("card missing for undo".to_owned()))?;
    let activated_at = from_rfc3339(&created_at)
        .map(|timestamp| review_engine::Timestamp(timestamp.0))
        .map_err(|error| crate::Error::RustError(error.to_string()))?;

    let scratch = review_engine::new_scheduler_scratch(scheduler)
        .map_err(|error| crate::Error::RustError(error.to_string()))?;
    let mut replayed = review_engine::initial_state(scheduler, activated_at)
        .map_err(|error| crate::Error::RustError(error.to_string()))?;
    let mut replayed_status = learning_core::CardStatus::New;
    {
        let mut statement = transaction.prepare(review::SELECT_EFFECTIVE_EVENTS_FOR_CARD)?;
        let mut rows = statement.query(params![card_id, last_id])?;
        while let Some(row) = rows.next()? {
            let grade = Grade::try_from(row.get::<_, u8>("grade")?).map_err(|_| {
                crate::Error::operation(
                    500,
                    "corrupt_row",
                    "stored review grade is invalid",
                    serde_json::Value::Null,
                )
            })?;
            let answered_text: String = row.get("answered_at")?;
            let answered_at = from_rfc3339(&answered_text)
                .map(|timestamp| review_engine::Timestamp(timestamp.0))
                .map_err(|error| {
                    crate::Error::operation(
                        500,
                        "corrupt_row",
                        error.to_string(),
                        serde_json::Value::Null,
                    )
                })?;
            replayed_status = next_status(replayed_status, grade).ok_or_else(|| {
                crate::Error::RustError("invalid replayed card transition".to_owned())
            })?;
            replayed = review_engine::apply_review(
                scheduler,
                &scratch,
                &replayed,
                &ReviewEventInput { grade, answered_at },
                &settings,
            )
            .map_err(|error| crate::Error::RustError(error.to_string()))?;
        }
    }

    let state_hash = review_engine::state_hash(&replayed);
    let event_id = crate::api::new_id("ev");
    let at = to_rfc3339(crate::api::now());
    transaction.execute(
        review::INSERT_EVENT,
        params![
            event_id,
            card_id,
            session_id,
            format!("undo-{client_operation_id}"),
            at,
            at,
            grade,
            0_u32,
            1_u8,
            Option::<String>::None,
            state_hash,
            state_hash,
            replayed.algorithm_version,
            "online",
            last_id,
            at,
        ],
    )?;
    write_state(&transaction, &card_id, &replayed, &state_hash, &at)?;
    let current_status: String =
        transaction.query_row("SELECT status FROM cards WHERE id=?1", [&card_id], |r| {
            r.get(0)
        })?;
    if matches!(
        current_status.as_str(),
        "new" | "learning" | "review" | "relearning"
    ) {
        update_card_status(
            &transaction,
            &card_id,
            crate::repositories::card_status_db(&replayed_status),
            None,
            &at,
        )?;
    }
    transaction.commit()?;

    crate::json(&serde_json::json!({
        "compensating_event_id": event_id,
        "compensates_event_id": last_id,
        "state_hash": state_hash,
    }))
}

pub(crate) fn complete_session(
    env: &Env,
    session_id: &str,
    card_count: u32,
    review_seconds: u32,
) -> Result<serde_json::Value> {
    let mut database = env.connection()?;
    let transaction = database.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let profile_id: String = transaction
        .query_row(review::SELECT_SESSION, params![session_id], |row| {
            row.get("learner_language_profile_id")
        })
        .optional()?
        .ok_or_else(|| {
            crate::Error::operation(404, "not_found", "unknown session", serde_json::Value::Null)
        })?;
    let at = to_rfc3339(crate::api::now());
    let changed = transaction.execute(
        review::COMPLETE_SESSION,
        params![session_id, at, card_count, review_seconds],
    )?;
    if changed == 0 {
        return Err(crate::Error::operation(
            409,
            "already_completed",
            "review session is already completed",
            serde_json::Value::Null,
        ));
    }

    let mut created = 0_u32;
    let mut evidence = transaction.prepare(review::SELECT_SESSION_CARD_EVIDENCE)?;
    let mut rows = evidence.query(params![session_id])?;
    while let Some(row) = rows.next()? {
        let card_id: String = row.get("card_id")?;
        let learning_item_id: Option<String> = row.get("source_learning_item_id")?;
        let spont_fail: u32 = row.get("spont_fail")?;
        let spont_ok: u32 = row.get("spont_ok")?;
        let prompted_ok: u32 = row.get("prompted_ok")?;
        let listen_fail: u32 = row.get("listen_fail")?;
        let review_count: u32 = row.get("review_count")?;

        let mut recent_grades = [0_u8; adaptation::GRADE_WINDOW];
        let mut recent_grade_len = 0usize;
        let mut latency_sum = 0_u64;
        let mut latency_count = 0_u32;
        {
            let mut grades = transaction.prepare(
                "SELECT grade,response_millis
                 FROM review_events
                 WHERE card_id=?1
                   AND compensates_event_id IS NULL
                   AND NOT EXISTS (
                       SELECT 1 FROM review_events x
                       WHERE x.compensates_event_id=review_events.id)
                 ORDER BY created_at DESC,id DESC
                 LIMIT 10",
            )?;
            let mut grade_rows = grades.query(params![card_id])?;
            while let Some(grade_row) = grade_rows.next()? {
                if recent_grade_len == adaptation::GRADE_WINDOW {
                    break;
                }
                recent_grades[recent_grade_len] = grade_row.get::<_, u8>(0)?;
                recent_grade_len += 1;
                latency_sum += u64::from(grade_row.get::<_, u32>(1)?);
                latency_count += 1;
            }
        }

        let signals = adaptation::CardSignalView {
            card_id: &card_id,
            learning_item_id: learning_item_id.as_deref(),
            recent_grades: &recent_grades[..recent_grade_len],
            avg_response_millis: (latency_count > 0)
                .then(|| u32::try_from(latency_sum / u64::from(latency_count)).unwrap_or(u32::MAX)),
            spontaneous_failures: spont_fail,
            spontaneous_successes: spont_ok,
            prompted_successes: prompted_ok,
            listening_failures: listen_fail,
            review_count,
        };

        let mut emit_error: Option<crate::Error> = None;
        adaptation::visit_card_rules(signals, |recommendation| {
            if emit_error.is_some() {
                return;
            }
            let detail = serde_json::json!({ "detail": recommendation.detail }).to_string();
            match insert_recommendation(
                &transaction,
                &profile_id,
                recommendation.subject_type,
                Some(recommendation.subject_id),
                recommendation.action,
                &detail,
                &at,
            ) {
                Ok(()) => created += 1,
                Err(error) => emit_error = Some(error),
            }
        });
        if let Some(error) = emit_error {
            return Err(error);
        }
    }
    drop(rows);
    drop(evidence);

    transaction.commit()?;

    crate::json(&serde_json::json!({
        "completed": true,
        "recommendations_created": created,
    }))
}

fn load_state(
    database: &rusqlite::Connection,
    card_id: &str,
) -> Result<Option<(ScheduledState, String)>> {
    database
        .query_row(review::SELECT_SCHEDULE_STATE, params![card_id], |row| {
            let last_review_at: Option<String> = row.get("last_review_at")?;
            let due_at: Option<String> = row.get("due_at")?;
            let last_review_at = last_review_at
                .as_deref()
                .map(from_rfc3339)
                .transpose()
                .map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        0,
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })?
                .map(|value| review_engine::Timestamp(value.0));
            let due_at = due_at
                .as_deref()
                .map(from_rfc3339)
                .transpose()
                .map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        0,
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })?
                .map(|value| review_engine::Timestamp(value.0));
            Ok((
                ScheduledState {
                    algorithm: row.get("algorithm")?,
                    algorithm_version: row.get("algorithm_version")?,
                    parameter_set_id: row.get("parameter_set_id")?,
                    difficulty: row.get("difficulty")?,
                    stability: row.get("stability")?,
                    last_review_at,
                    due_at,
                    scheduled_interval_seconds: row.get("scheduled_interval_seconds")?,
                    lapse_count: row.get("lapse_count")?,
                    review_count: row.get("review_count")?,
                    state_version: row.get("state_version")?,
                },
                row.get("state_hash")?,
            ))
        })
        .optional()
        .map_err(Into::into)
}

fn write_state(
    database: &rusqlite::Connection,
    card_id: &str,
    state: &ScheduledState,
    state_hash: &str,
    at: &str,
) -> Result<()> {
    let last_review_at = state
        .last_review_at
        .map(|value| to_rfc3339(learning_core::Timestamp(value.0)));
    let due_at = state
        .due_at
        .map(|value| to_rfc3339(learning_core::Timestamp(value.0)));

    upsert_schedule_state(
        database,
        card_id,
        &state.algorithm,
        &state.algorithm_version,
        state.parameter_set_id.as_deref(),
        state.difficulty,
        state.stability,
        last_review_at.as_deref(),
        due_at.as_deref(),
        state.scheduled_interval_seconds,
        state.lapse_count,
        state.review_count,
        state.state_version,
        state_hash,
        at,
    )?;
    Ok(())
}

fn next_status(
    current: learning_core::CardStatus,
    grade: Grade,
) -> Option<learning_core::CardStatus> {
    use learning_core::CardStatus;

    let passed = grade != Grade::Again;
    match current {
        CardStatus::New => Some(CardStatus::Learning),
        CardStatus::Learning => Some(if passed {
            CardStatus::Review
        } else {
            CardStatus::Learning
        }),
        CardStatus::Review => Some(if passed {
            CardStatus::Review
        } else {
            CardStatus::Relearning
        }),
        CardStatus::Relearning => Some(if passed {
            CardStatus::Review
        } else {
            CardStatus::Relearning
        }),
        CardStatus::Candidate | CardStatus::Suspended | CardStatus::Retired => None,
    }
}

#[cfg(test)]
// Test fixtures intentionally fail fast and use compact tuple snapshots.
#[allow(clippy::unwrap_used, clippy::type_complexity)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn create_env(label: &str) -> (PathBuf, Env) {
        let path = std::env::temp_dir().join(format!(
            "lingua-review-{label}-{}.sqlite3",
            uuid::Uuid::new_v4()
        ));
        let db = crate::storage::open(&path).unwrap();
        db.execute_batch(
            r#"INSERT INTO language_packs
                (id,package_name,version,contract_version,target_language,target_locale,
                 display_name,installed_at,updated_at)
               VALUES('es-ES','language-pack-es-es','0.2.0',1,'es','es-ES','Spanish','2026-10-16T00:00:00Z','2026-10-16T00:00:00Z');
               INSERT INTO learner_profile(id,created_at,updated_at)
               VALUES('learner-1','2026-10-16T00:00:00Z','2026-10-16T00:00:00Z');
               INSERT INTO learner_language_profiles
                (id,learner_id,language_pack_id,source_language,target_locale,created_at,updated_at)
               VALUES('llp-1','learner-1','es-ES','ru','es-ES','2026-10-16T00:00:00Z','2026-10-16T00:00:00Z');
               INSERT INTO review_sessions
                (id,learner_language_profile_id,started_at,source,card_count,created_at)
               VALUES('rs-1','llp-1','2026-10-16T00:00:00Z','online',0,'2026-10-16T00:00:00Z');
               INSERT INTO cards
                (id,learner_language_profile_id,language_pack_id,language_pack_version,intent,status,
                 prompt,prompt_language,answer,answer_language,accepted_answers_json,
                 answer_evaluation_mode,creation_reason,content_hash,content_version,
                 created_by,created_at,updated_at)
               VALUES('card-1','llp-1','es-ES','0.2.0','production','new',
                 'p','ru','a','es-ES','[]','exact','seed','h',1,'tutor',
                 '2026-10-16T00:00:00Z','2026-10-16T00:00:00Z');"#
        ).unwrap();

        let state = review_engine::initial_state(
            review_engine::FSRS_ID,
            review_engine::Timestamp(1_760_572_800),
        )
        .unwrap();
        let hash = review_engine::state_hash(&state);
        db.execute(
            review::UPSERT_SCHEDULE_STATE,
            params![
                "card-1",
                state.algorithm,
                state.algorithm_version,
                Option::<String>::None,
                state.difficulty,
                state.stability,
                Option::<String>::None,
                state
                    .due_at
                    .map(|value| to_rfc3339(learning_core::Timestamp(value.0))),
                state.scheduled_interval_seconds,
                state.lapse_count,
                state.review_count,
                state.state_version,
                hash,
                "2026-10-16T00:00:00Z"
            ],
        )
        .unwrap();
        drop(db);
        (path.clone(), Env::new(path, "key".to_owned()))
    }

    fn snapshot(env: &Env) -> (String, u32, u32, u32) {
        let db = env.connection().unwrap();
        let status: String = db
            .query_row("SELECT status FROM cards WHERE id='card-1'", [], |r| {
                r.get(0)
            })
            .unwrap();
        let (review_count, lapse_count): (u32, u32) = db
            .query_row(
                "SELECT review_count,lapse_count FROM card_schedule_state WHERE card_id='card-1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        let events: u32 = db
            .query_row("SELECT COUNT(*) FROM review_events", [], |r| r.get(0))
            .unwrap();
        (status, review_count, lapse_count, events)
    }

    #[test]
    fn submit_applies_grade_and_advances_schedule_once() {
        let (path, env) = create_env("submit");
        let prior: String = env
            .connection()
            .unwrap()
            .query_row(
                "SELECT state_hash FROM card_schedule_state WHERE card_id='card-1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        submit_event(
            &env,
            "rs-1",
            ReviewEventInputData {
                client_operation_id: "op-1",
                card_id: "card-1",
                grade: 3,
                presented_at: "2026-10-16T00:01:00Z",
                answered_at: "2026-10-16T00:01:05Z",
                response_millis: 5000,
                answer_revealed: false,
                learner_note: None,
                prior_state_hash: &prior,
                source: "online",
            },
        )
        .unwrap();
        assert_eq!(snapshot(&env), ("learning".to_owned(), 1, 0, 1));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn duplicate_submit_does_not_double_apply() {
        let (path, env) = create_env("duplicate");
        let prior: String = env
            .connection()
            .unwrap()
            .query_row(
                "SELECT state_hash FROM card_schedule_state WHERE card_id='card-1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let input = || ReviewEventInputData {
            client_operation_id: "op-1",
            card_id: "card-1",
            grade: 3,
            presented_at: "2026-10-16T00:01:00Z",
            answered_at: "2026-10-16T00:01:05Z",
            response_millis: 5000,
            answer_revealed: false,
            learner_note: None,
            prior_state_hash: &prior,
            source: "online",
        };
        submit_event(&env, "rs-1", input()).unwrap();
        let first = snapshot(&env);
        submit_event(&env, "rs-1", input()).unwrap();
        assert_eq!(snapshot(&env), first);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn undo_replays_without_last_grade_and_writes_compensation() {
        let (path, env) = create_env("undo");
        let prior: String = env
            .connection()
            .unwrap()
            .query_row(
                "SELECT state_hash FROM card_schedule_state WHERE card_id='card-1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        submit_event(
            &env,
            "rs-1",
            ReviewEventInputData {
                client_operation_id: "op-u",
                card_id: "card-1",
                grade: 3,
                presented_at: "2026-10-16T00:01:00Z",
                answered_at: "2026-10-16T00:01:05Z",
                response_millis: 5000,
                answer_revealed: false,
                learner_note: None,
                prior_state_hash: &prior,
                source: "online",
            },
        )
        .unwrap();
        // UUID lexical order is unrelated to insertion order within one second.
        env.connection()
            .unwrap()
            .execute(
                "UPDATE review_events SET id='zzzz-original',created_at=?1",
                [to_rfc3339(crate::api::now())],
            )
            .unwrap();
        undo_last(&env, "rs-1").unwrap();
        env.connection()
            .unwrap()
            .execute(
                "UPDATE review_events SET created_at='2026-10-28T00:00:00Z'",
                [],
            )
            .unwrap();
        let before_repeat = snapshot(&env);
        assert!(matches!(
            undo_last(&env, "rs-1"),
            Err(crate::Error::Operation {
                code: "already_undone",
                ..
            })
        ));
        assert_eq!(snapshot(&env), before_repeat);
        let state = snapshot(&env);
        assert_eq!(state.0, "new");
        assert_eq!(state.1, 0);
        assert_eq!(state.2, 0);
        crate::operations::review_queue::queue(&env, "rs-1").unwrap();
        assert_eq!(state.3, 2);
        let compensations: u32 = env
            .connection()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM review_events WHERE compensates_event_id IS NOT NULL",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(compensations, 1);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn undo_preserves_fallback_scheduler() {
        let (path, env) = create_env("fallback");
        let db = env.connection().unwrap();
        let state = review_engine::initial_state(
            review_engine::FALLBACK_ID,
            review_engine::Timestamp(1_760_572_800),
        )
        .unwrap();
        let hash = review_engine::state_hash(&state);
        db.execute(
            review::UPSERT_SCHEDULE_STATE,
            params![
                "card-1",
                state.algorithm,
                state.algorithm_version,
                Option::<String>::None,
                state.difficulty,
                state.stability,
                Option::<String>::None,
                state
                    .due_at
                    .map(|value| to_rfc3339(learning_core::Timestamp(value.0))),
                state.scheduled_interval_seconds,
                state.lapse_count,
                state.review_count,
                state.state_version,
                hash,
                "2026-10-16T00:00:00Z"
            ],
        )
        .unwrap();
        drop(db);
        submit_event(
            &env,
            "rs-1",
            ReviewEventInputData {
                client_operation_id: "op-f",
                card_id: "card-1",
                grade: 3,
                presented_at: "2026-10-16T00:01:00Z",
                answered_at: "2026-10-16T00:01:05Z",
                response_millis: 5000,
                answer_revealed: false,
                learner_note: None,
                prior_state_hash: &hash,
                source: "online",
            },
        )
        .unwrap();
        undo_last(&env, "rs-1").unwrap();
        let algorithm: String = env
            .connection()
            .unwrap()
            .query_row(
                "SELECT algorithm FROM card_schedule_state WHERE card_id='card-1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(algorithm, "fallback");
        let _ = std::fs::remove_file(path);
    }

    fn seed_recommendation_case(env: &Env) {
        let db = env.connection().unwrap();
        db.execute_batch(
            r#"INSERT INTO learning_items
                (id,learner_language_profile_id,language_pack_id,item_type,target_text,
                 normalized_target_text,status,language_metadata_json,created_at,updated_at)
               VALUES('li-1','llp-1','es-ES','phrase','hola','hola','learning','{}',
                      '2026-10-16T00:00:00Z','2026-10-16T00:00:00Z');
               UPDATE cards SET source_learning_item_id='li-1' WHERE id='card-1';"#,
        )
        .unwrap();
    }

    fn recommendation_snapshot(env: &Env) -> Vec<(String, String, String, String)> {
        let db = env.connection().unwrap();
        db.prepare(
            "SELECT subject_type,subject_id,action,status
             FROM recommendations ORDER BY subject_type,subject_id,action",
        )
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap()
    }

    #[test]
    fn evaluate_review_answer_uses_stored_card_contract() {
        let (path, env) = create_env("evaluation");
        let registry = crate::built_in_language_packs().unwrap();
        let actual = crate::handlers::review::evaluate(&registry, &env, "card-1", "a").unwrap();
        assert_eq!(actual["verdict"], "correct");
        assert_eq!(actual["matched_answer"], "a");
        let db = env.connection().unwrap();
        db.execute(
            "UPDATE cards SET language_pack_version='invalid-version' WHERE id='card-1'",
            [],
        )
        .unwrap();
        assert!(crate::handlers::review::evaluate(&registry, &env, "card-1", "a").is_err());
        db.execute("UPDATE cards SET language_pack_version='0.2.0', accepted_answers_json='invalid-json' WHERE id='card-1'",[]).unwrap();
        assert!(crate::handlers::review::evaluate(&registry, &env, "card-1", "a").is_err());
        drop(db);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn review_read_views_stream_without_changing_wire_contract() {
        let (path, env) = create_env("stream-stats");
        let db = env.connection().unwrap();
        let now = crate::api::now().0;
        let at = to_rfc3339(learning_core::Timestamp(now));
        db.execute("UPDATE cards SET status='review' WHERE id='card-1'", [])
            .unwrap();
        db.execute(
            "UPDATE card_schedule_state SET due_at=?1 WHERE card_id='card-1'",
            [to_rfc3339(learning_core::Timestamp(now + 90_000))],
        )
        .unwrap();
        for (index, grade, millis) in [(1, 1, 2000), (2, 2, 3000), (3, 4, 5000)] {
            db.execute(
                "INSERT INTO review_events
                 (id,card_id,review_session_id,client_operation_id,presented_at,
                  answered_at,grade,response_millis,answer_revealed,prior_state_hash,
                  result_state_hash,algorithm_version,source,created_at)
                 VALUES (?1,'card-1','rs-1',?1,?2,?2,?3,?4,0,'h0','h1','v1','online',?2)",
                params![format!("event-{index}"), at, grade, millis],
            )
            .unwrap();
        }
        db.execute(
            "INSERT INTO review_events
             (id,card_id,review_session_id,client_operation_id,presented_at,
              answered_at,grade,response_millis,answer_revealed,prior_state_hash,
              result_state_hash,algorithm_version,source,compensates_event_id,created_at)
             VALUES ('undo-2','card-1','rs-1','undo-2',?1,?1,3,100,0,
                     'h0','h1','v1','online','event-2',?1)",
            [at],
        )
        .unwrap();
        drop(db);

        let summary = crate::handlers::review::summary_for(&env, "llp-1").unwrap();
        assert_eq!(summary["reviews_today"], 3);
        assert_eq!(
            summary["grade_distribution"],
            serde_json::json!({
                "again": 1, "hard": 1, "good": 0, "easy": 1
            })
        );
        assert_eq!(summary["review_seconds_today"], 10);

        let forecast = crate::handlers::review::forecast_for(&env, "llp-1", 3).unwrap();
        assert_eq!(forecast["due_per_day"], serde_json::json!([0, 1, 0]));

        let retention = crate::handlers::review::retention_for(&env, "llp-1", 7).unwrap();
        assert_eq!(retention["reviews"], 2);
        assert_eq!(retention["retention"], 0.5);
        assert_eq!(retention["weeks"][0]["reviews"], 2);
        assert_eq!(
            retention["weeks"][0]["grades"],
            serde_json::json!({
                "again": 1, "hard": 0, "good": 0, "easy": 1
            })
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn complete_session_streams_rules_and_creates_recommendations() {
        let (path, env) = create_env("complete");
        seed_recommendation_case(&env);
        let prior: String = env
            .connection()
            .unwrap()
            .query_row(
                "SELECT state_hash FROM card_schedule_state WHERE card_id='card-1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        submit_event(
            &env,
            "rs-1",
            ReviewEventInputData {
                client_operation_id: "op-c",
                card_id: "card-1",
                grade: 1,
                presented_at: "2026-10-16T00:01:00Z",
                answered_at: "2026-10-16T00:01:05Z",
                response_millis: 5000,
                answer_revealed: true,
                learner_note: None,
                prior_state_hash: &prior,
                source: "online",
            },
        )
        .unwrap();
        let result = complete_session(&env, "rs-1", 1, 5).unwrap();
        assert_eq!(result["completed"], true);
        let recs = recommendation_snapshot(&env);
        assert!(!recs.is_empty());
        assert!(recs.iter().all(|row| row.3 == "open"));
        let _ = std::fs::remove_file(path);
    }
    #[test]
    fn scheduler_settings_reject_pathological_retention() {
        let (path, env) = create_env("profile-retention-bound");
        {
            let db = env.connection().unwrap();
            db.execute("UPDATE learner_profile SET desired_retention=0.995", [])
                .unwrap();
            assert!(scheduler_settings_for_profile(&db, "llp-1").is_err());
        }
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn scheduler_settings_follow_profile_retention() {
        let (path, env) = create_env("profile-retention");
        {
            let db = env.connection().unwrap();
            db.execute(
                "UPDATE learner_profile
                 SET desired_retention=0.97, max_new_cards=5, max_review_minutes=9, timezone='UTC'",
                [],
            )
            .unwrap();
            let settings = scheduler_settings_for_profile(&db, "llp-1").unwrap();
            assert!((settings.desired_retention - 0.97).abs() < f64::EPSILON);
            assert_eq!(settings.max_new_cards_per_day, 5);
            assert_eq!(settings.target_review_minutes.1, 9);
            assert_eq!(settings.timezone, "UTC");
        }
        let _ = std::fs::remove_file(path);
    }
}
