//! Bounded review queue with transactional candidate promotion.

use crate::repositories::{cards, from_rfc3339, review, to_rfc3339};
use crate::{Env, Result};
use review_engine::ScheduledState;
use rusqlite::{OptionalExtension as _, TransactionBehavior, params};

use super::card_write::{update_card_status, upsert_schedule_state};
use super::review_ops::select_scheduler;

const MAX_QUEUE: usize = 50;
const MAX_PROMOTIONS: usize = u8::MAX as usize;

#[derive(Clone, Copy, Default)]
struct TextRef {
    start: usize,
    len: usize,
}

fn push_text(text: &mut String, value: &str) -> TextRef {
    let start = text.len();
    text.push_str(value);
    TextRef {
        start,
        len: value.len(),
    }
}

fn text_at(text: &str, reference: TextRef) -> &str {
    &text[reference.start..reference.start + reference.len]
}

struct QueueColumns {
    len: usize,
    card_id: [TextRef; MAX_QUEUE],
    category: [u8; MAX_QUEUE],
    due_at: [i64; MAX_QUEUE],
    retrievability: [f64; MAX_QUEUE],
    created_at: [i64; MAX_QUEUE],
}

fn empty_queue_columns() -> QueueColumns {
    QueueColumns {
        len: 0,
        card_id: [TextRef::default(); MAX_QUEUE],
        category: [0; MAX_QUEUE],
        due_at: [0; MAX_QUEUE],
        retrievability: [0.0; MAX_QUEUE],
        created_at: [0; MAX_QUEUE],
    }
}

fn shift_queue_column(columns: &mut QueueColumns, from: usize, to: usize) {
    columns.card_id[to] = columns.card_id[from];
    columns.category[to] = columns.category[from];
    columns.due_at[to] = columns.due_at[from];
    columns.retrievability[to] = columns.retrievability[from];
    columns.created_at[to] = columns.created_at[from];
}

pub(crate) fn queue(env: &Env, session_id: &str) -> Result<serde_json::Value> {
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

    let now = crate::api::now();
    let at = to_rfc3339(now);

    let mint_budget = super::card_budget::remaining_budget(
        &transaction,
        &profile_id,
        now,
        cards::COUNT_ACTIVATED_TODAY,
    )?;
    promote_candidates(&transaction, &profile_id, now, &at, mint_budget)?;

    let session_budget = super::card_budget::remaining_budget(
        &transaction,
        &profile_id,
        now,
        review::COUNT_NEW_CARDS_INTRODUCED_TODAY,
    )?;

    let max_batch = MAX_QUEUE;
    let mut queue_text = String::new();
    let mut due = empty_queue_columns();
    let mut new = empty_queue_columns();
    let new_limit = usize::from(session_budget).min(MAX_QUEUE);

    let mut statement = transaction.prepare(
        "SELECT c.id,c.status,c.created_at,
                s.algorithm,s.algorithm_version,s.parameter_set_id,s.difficulty,
                s.stability,s.last_review_at,s.due_at,s.scheduled_interval_seconds,
                s.lapse_count,s.review_count,s.state_version,s.state_hash
         FROM cards c
         LEFT JOIN card_schedule_state s ON s.card_id=c.id
         WHERE c.learner_language_profile_id=?1
           AND c.status IN ('new','learning','review','relearning')",
    )?;
    let mut rows = statement.query(params![profile_id])?;
    while let Some(row) = rows.next()? {
        let field = |index| -> Result<&str> {
            row.get_ref(index)?
                .as_str()
                .map_err(|error| crate::Error::RustError(error.to_string()))
        };
        let card_id = field(0)?;
        let status = field(1)?;
        let created_at = from_rfc3339(field(2)?)
            .map(|value| value.0)
            .map_err(|error| {
                crate::Error::operation(
                    500,
                    "corrupt_row",
                    error.to_string(),
                    serde_json::Value::Null,
                )
            })?;

        if status == "new" {
            if new_limit == 0 {
                continue;
            }
            if let Some(position) = rank_position(
                &new,
                new_limit,
                card_id,
                3,
                i64::MAX,
                1.0,
                created_at,
                &queue_text,
            ) {
                let id = push_text(&mut queue_text, card_id);
                insert_ranked_at(
                    &mut new,
                    new_limit,
                    position,
                    id,
                    3,
                    i64::MAX,
                    1.0,
                    created_at,
                );
            }
            continue;
        }

        let Some((state, state_hash)) = state_from_row(row)? else {
            continue;
        };
        if review_engine::state_hash(&state) != state_hash {
            return Err(crate::Error::operation(
                500,
                "corrupt_row",
                "stored schedule hash does not match schedule columns",
                serde_json::Value::Null,
            ));
        }
        let due_at = state.due_at.map_or(i64::MAX, |value| value.0);
        if due_at > now.0 {
            continue;
        }

        let scheduler = select_scheduler(&state.algorithm, &state.algorithm_version)?;
        let retrievability = review_engine::predicted_retrievability(
            scheduler,
            &state,
            review_engine::Timestamp(now.0),
        )
        .map_err(|error| crate::Error::RustError(error.to_string()))?;

        let category = match status {
            "relearning" => 0,
            "review" => 1,
            "learning" => 2,
            _ => continue,
        };
        if let Some(position) = rank_position(
            &due,
            max_batch,
            card_id,
            category,
            due_at,
            retrievability,
            created_at,
            &queue_text,
        ) {
            let id = push_text(&mut queue_text, card_id);
            insert_ranked_at(
                &mut due,
                max_batch,
                position,
                id,
                category,
                due_at,
                retrievability,
                created_at,
            );
        }
    }
    drop(rows);
    drop(statement);

    for i in 0..new.len {
        if let Some(position) = rank_position(
            &due,
            max_batch,
            text_at(&queue_text, new.card_id[i]),
            new.category[i],
            new.due_at[i],
            new.retrievability[i],
            new.created_at[i],
            &queue_text,
        ) {
            insert_ranked_at(
                &mut due,
                max_batch,
                position,
                new.card_id[i],
                new.category[i],
                new.due_at[i],
                new.retrievability[i],
                new.created_at[i],
            );
        }
    }

    let mut output: [serde_json::Value; MAX_QUEUE] =
        std::array::from_fn(|_| serde_json::Value::Null);
    for (i, value) in output[..due.len].iter_mut().enumerate() {
        *value = load_payload(&transaction, text_at(&queue_text, due.card_id[i]))?;
    }
    let output_len = due.len;

    transaction.commit()?;
    crate::json(&serde_json::json!({
        "session_id": session_id,
        "new_card_budget": session_budget,
        "queue": &output[..output_len],
    }))
}

fn promote_candidates(
    database: &rusqlite::Connection,
    profile_id: &str,
    now: learning_core::Timestamp,
    at: &str,
    budget: u8,
) -> Result<()> {
    if budget == 0 {
        return Ok(());
    }

    let mut promotion_text = String::new();
    let mut ids = [TextRef::default(); MAX_PROMOTIONS];
    let mut len = 0usize;
    {
        let mut statement = database.prepare(cards::SELECT_OLDEST_CANDIDATES)?;
        let mut rows = statement.query(params![profile_id, u32::from(budget)])?;
        while let Some(row) = rows.next()? {
            if len == usize::from(budget) || len == MAX_PROMOTIONS {
                break;
            }
            let id = row
                .get_ref(0)?
                .as_str()
                .map_err(|error| crate::Error::RustError(error.to_string()))?;
            ids[len] = push_text(&mut promotion_text, id);
            len += 1;
        }
    }
    if len == 0 {
        return Ok(());
    }

    for id_ref in &ids[..len] {
        let id = text_at(&promotion_text, *id_ref);
        update_card_status(database, id, "new", None, at)?;
        let state =
            review_engine::initial_state(review_engine::FSRS_ID, review_engine::Timestamp(now.0))
                .map_err(|error| crate::Error::RustError(error.to_string()))?;
        let last_review_at = state
            .last_review_at
            .map(|value| to_rfc3339(learning_core::Timestamp(value.0)));
        let due_at = state
            .due_at
            .map(|value| to_rfc3339(learning_core::Timestamp(value.0)));
        upsert_schedule_state(
            database,
            id,
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
            &review_engine::state_hash(&state),
            at,
        )?;
    }

    Ok(())
}

fn state_from_row<'a>(row: &'a rusqlite::Row<'_>) -> Result<Option<(ScheduledState, &'a str)>> {
    let algorithm: Option<String> = row.get(3)?;
    let Some(algorithm) = algorithm else {
        return Ok(None);
    };
    let algorithm_version: String = row.get::<_, Option<String>>(4)?.ok_or_else(|| {
        crate::Error::operation(
            500,
            "corrupt_row",
            "schedule algorithm_version is missing",
            serde_json::Value::Null,
        )
    })?;
    let timestamp = |index| -> Result<Option<review_engine::Timestamp>> {
        let value = match row.get_ref(index)? {
            rusqlite::types::ValueRef::Null => return Ok(None),
            rusqlite::types::ValueRef::Text(raw) => std::str::from_utf8(raw)
                .map_err(|error| crate::Error::RustError(error.to_string()))?,
            _ => {
                return Err(crate::Error::operation(
                    500,
                    "corrupt_row",
                    "invalid schedule timestamp type",
                    serde_json::Value::Null,
                ));
            }
        };
        from_rfc3339(value)
            .map(|parsed| Some(review_engine::Timestamp(parsed.0)))
            .map_err(|error| {
                crate::Error::operation(
                    500,
                    "corrupt_row",
                    error.to_string(),
                    serde_json::Value::Null,
                )
            })
    };
    let last_review_at = timestamp(8)?;
    let due_at = timestamp(9)?;
    let state_hash = match row.get_ref(14)? {
        rusqlite::types::ValueRef::Null => "",
        rusqlite::types::ValueRef::Text(raw) => {
            std::str::from_utf8(raw).map_err(|error| crate::Error::RustError(error.to_string()))?
        }
        _ => {
            return Err(crate::Error::operation(
                500,
                "corrupt_row",
                "invalid schedule state hash type",
                serde_json::Value::Null,
            ));
        }
    };

    Ok(Some((
        ScheduledState {
            algorithm,
            algorithm_version,
            parameter_set_id: row.get(5)?,
            difficulty: row.get(6)?,
            stability: row.get(7)?,
            last_review_at,
            due_at,
            scheduled_interval_seconds: row.get(10)?,
            lapse_count: row.get::<_, Option<u32>>(11)?.unwrap_or(0),
            review_count: row.get::<_, Option<u32>>(12)?.unwrap_or(0),
            state_version: row.get::<_, Option<u32>>(13)?.unwrap_or(1),
        },
        state_hash,
    )))
}

/// Identify the insertion slot while the candidate ID is still borrowed
/// from SQLite. Rejected records never enter the retained string arena.
#[allow(clippy::too_many_arguments)]
fn rank_position(
    columns: &QueueColumns,
    limit: usize,
    card_id: &str,
    category: u8,
    due_at: i64,
    retrievability: f64,
    created_at: i64,
    text: &str,
) -> Option<usize> {
    if limit == 0 {
        return None;
    }
    let capacity = limit.min(MAX_QUEUE);
    let mut position = columns.len.min(capacity);
    for index in 0..columns.len.min(capacity) {
        if precedes(
            category,
            due_at,
            retrievability,
            created_at,
            card_id,
            columns.category[index],
            columns.due_at[index],
            columns.retrievability[index],
            columns.created_at[index],
            text_at(text, columns.card_id[index]),
        ) {
            position = index;
            break;
        }
    }
    (columns.len < capacity || position < capacity).then_some(position)
}

/// A selected row moves only scalar values and one arena offset.
#[allow(clippy::too_many_arguments)]
fn insert_ranked_at(
    columns: &mut QueueColumns,
    limit: usize,
    position: usize,
    card_id: TextRef,
    category: u8,
    due_at: i64,
    retrievability: f64,
    created_at: i64,
) {
    let capacity = limit.min(MAX_QUEUE);
    debug_assert!(capacity > 0 && position < capacity);
    let end = if columns.len < capacity {
        columns.len
    } else {
        capacity - 1
    };
    for index in (position..end).rev() {
        shift_queue_column(columns, index, index + 1);
    }
    columns.card_id[position] = card_id;
    columns.category[position] = category;
    columns.due_at[position] = due_at;
    columns.retrievability[position] = retrievability;
    columns.created_at[position] = created_at;
    if columns.len < capacity {
        columns.len += 1;
    }
}

#[allow(clippy::too_many_arguments)]
fn precedes(
    left_category: u8,
    left_due_at: i64,
    left_retrievability: f64,
    left_created_at: i64,
    left_card_id: &str,
    right_category: u8,
    right_due_at: i64,
    right_retrievability: f64,
    right_created_at: i64,
    right_card_id: &str,
) -> bool {
    left_category
        .cmp(&right_category)
        .then_with(|| {
            if left_category == 1 {
                left_retrievability
                    .partial_cmp(&right_retrievability)
                    .unwrap_or(std::cmp::Ordering::Equal)
            } else if left_category == 3 {
                left_created_at.cmp(&right_created_at)
            } else {
                left_due_at.cmp(&right_due_at)
            }
        })
        .then_with(|| left_card_id.cmp(right_card_id))
        .is_lt()
}

fn load_payload(database: &rusqlite::Connection, card_id: &str) -> Result<serde_json::Value> {
    database
        .query_row(
            "SELECT c.id,c.intent,c.status,c.prompt,c.prompt_language,c.answer,
                    c.accepted_answers_json,c.answer_evaluation_mode,c.explanation,
                    c.pronunciation_note,
                    s.algorithm,s.algorithm_version,s.parameter_set_id,s.difficulty,
                    s.stability,s.last_review_at,s.due_at,s.scheduled_interval_seconds,
                    s.lapse_count,s.review_count,s.state_version,s.state_hash
             FROM cards c
             LEFT JOIN card_schedule_state s ON s.card_id=c.id
             WHERE c.id=?1",
            params![card_id],
            |row| {
                let accepted_text: String = row.get(6)?;
                let accepted: serde_json::Value =
                    serde_json::from_str(&accepted_text).unwrap_or_else(|_| serde_json::json!([]));

                let state = {
                    let algorithm: Option<String> = row.get(10)?;
                    if let Some(algorithm) = algorithm {
                        let algorithm_version: String = row
                            .get::<_, Option<String>>(11)?
                            .ok_or(rusqlite::Error::InvalidQuery)?;
                        let last_review_at: Option<String> = row.get(15)?;
                        let due_at: Option<String> = row.get(16)?;
                        Some(ScheduledState {
                            algorithm,
                            algorithm_version,
                            parameter_set_id: row.get(12)?,
                            difficulty: row.get(13)?,
                            stability: row.get(14)?,
                            last_review_at: last_review_at
                                .as_deref()
                                .and_then(|value| from_rfc3339(value).ok())
                                .map(|value| review_engine::Timestamp(value.0)),
                            due_at: due_at
                                .as_deref()
                                .and_then(|value| from_rfc3339(value).ok())
                                .map(|value| review_engine::Timestamp(value.0)),
                            scheduled_interval_seconds: row.get(17)?,
                            lapse_count: row.get::<_, Option<u32>>(18)?.unwrap_or(0),
                            review_count: row.get::<_, Option<u32>>(19)?.unwrap_or(0),
                            state_version: row.get::<_, Option<u32>>(20)?.unwrap_or(1),
                        })
                    } else {
                        None
                    }
                };
                let prior_state_hash = state.map(|state| review_engine::state_hash(&state));

                Ok(serde_json::json!({
                    "card_id": row.get::<_,String>(0)?,
                    "intent": row.get::<_,String>(1)?,
                    "status": row.get::<_,String>(2)?,
                    "prompt": row.get::<_,String>(3)?,
                    "prompt_language": row.get::<_,String>(4)?,
                    "answer": row.get::<_,String>(5)?,
                    "accepted_answers": accepted,
                    "answer_evaluation_mode": row.get::<_,String>(7)?,
                    "explanation": row.get::<_,Option<String>>(8)?,
                    "pronunciation_note": row.get::<_,Option<String>>(9)?,
                    "prior_state_hash": prior_state_hash,
                }))
            },
        )
        .map_err(Into::into)
}

#[cfg(test)]
// Test fixtures intentionally fail fast and use compact tuple snapshots.
#[allow(clippy::unwrap_used, clippy::type_complexity)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn create_env(label: &str, max_new_cards: u8) -> (PathBuf, Env) {
        let path = std::env::temp_dir().join(format!(
            "lingua-review-queue-{label}-{}.sqlite3",
            uuid::Uuid::new_v4()
        ));
        let db = crate::storage::open(&path).unwrap();
        db.execute_batch(&format!(
            r#"INSERT INTO language_packs
                (id,package_name,version,contract_version,target_language,target_locale,
                 display_name,installed_at,updated_at)
               VALUES('es-ES','language-pack-es-es','0.2.0',1,'es','es-ES','Spanish','2026-10-10T00:00:00Z','2026-10-10T00:00:00Z');
               INSERT INTO learner_profile
                (id,timezone,max_new_cards,created_at,updated_at)
               VALUES('learner-1','Europe/Madrid',{max_new_cards},'2026-10-10T00:00:00Z','2026-10-10T00:00:00Z');
               INSERT INTO learner_language_profiles
                (id,learner_id,language_pack_id,source_language,target_locale,created_at,updated_at)
               VALUES('llp-1','learner-1','es-ES','ru','es-ES','2026-10-10T00:00:00Z','2026-10-10T00:00:00Z');
               INSERT INTO review_sessions
                (id,learner_language_profile_id,started_at,source,card_count,created_at)
               VALUES('rs-1','llp-1','2026-10-16T00:00:00Z','online',0,'2026-10-16T00:00:00Z');"#
        )).unwrap();
        drop(db);
        (path.clone(), Env::new(path, "key".to_owned()))
    }

    fn insert_card(db: &rusqlite::Connection, id: &str, status: &str, created_at: &str) {
        db.execute(
            "INSERT INTO cards
             (id,learner_language_profile_id,language_pack_id,language_pack_version,
              intent,status,prompt,prompt_language,answer,answer_language,
              accepted_answers_json,answer_evaluation_mode,creation_reason,content_hash,
              content_version,created_by,created_at,updated_at)
             VALUES(?1,'llp-1','es-ES','0.2.0','production',?2,?1,'ru','a','es-ES',
                    '[]','exact','seed',?1,1,'tutor',?3,?3)",
            params![id, status, created_at],
        )
        .unwrap();
    }

    #[test]
    fn rejected_topk_candidate_leaves_text_arena_unchanged() {
        let mut retained = empty_queue_columns();
        let mut text = String::new();
        let first = rank_position(&retained, 1, "first", 3, i64::MAX, 1.0, 100, &text).unwrap();
        let id = push_text(&mut text, "first");
        insert_ranked_at(&mut retained, 1, first, id, 3, i64::MAX, 1.0, 100);
        let before = text.len();
        assert_eq!(
            rank_position(&retained, 1, "later", 3, i64::MAX, 1.0, 200, &text),
            None,
        );
        assert_eq!(text.len(), before);
        assert_eq!(text_at(&text, retained.card_id[0]), "first");
        let replacement =
            rank_position(&retained, 1, "older", 3, i64::MAX, 1.0, 50, &text).unwrap();
        let replacement_id = push_text(&mut text, "older");
        insert_ranked_at(
            &mut retained,
            1,
            replacement,
            replacement_id,
            3,
            i64::MAX,
            1.0,
            50,
        );
        assert_eq!(text_at(&text, retained.card_id[0]), "older");
        assert_eq!(retained.len, 1);
    }

    #[test]
    fn profile_budget_limits_candidate_promotion() {
        let (path, env) = create_env("budget", 2);
        {
            let db = env.connection().unwrap();
            for id in ["c1", "c2", "c3", "c4"] {
                insert_card(&db, id, "candidate", "2020-01-01T00:00:00Z");
            }
        }

        let value = queue(&env, "rs-1").unwrap();
        let db = env.connection().unwrap();
        let promoted: u32 = db
            .query_row("SELECT COUNT(*) FROM cards WHERE status='new'", [], |row| {
                row.get(0)
            })
            .unwrap();
        let schedules: u32 = db
            .query_row("SELECT COUNT(*) FROM card_schedule_state", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(promoted, 2);
        assert_eq!(schedules, 2);
        assert_eq!(value["queue"].as_array().unwrap().len(), 2);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn new_card_queue_orders_timestamp_then_unicode_id() {
        let (path, env) = create_env("ordered-new", 8);
        {
            let db = env.connection().unwrap();
            for (id, at) in [
                ("ñ-z", "2026-10-15T00:00:00Z"),
                ("a-id", "2026-10-15T00:00:00Z"),
                ("old", "2026-10-14T00:00:00Z"),
                ("last", "2026-10-16T00:00:00Z"),
            ] {
                insert_card(&db, id, "new", at);
            }
        }
        let result = queue(&env, "rs-1").unwrap();
        let ids: Vec<&str> = result["queue"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["card_id"].as_str().unwrap())
            .collect();
        assert_eq!(ids, ["old", "a-id", "ñ-z", "last"]);
        assert!(
            result["queue"]
                .as_array()
                .unwrap()
                .iter()
                .all(|row| row["prior_state_hash"].is_null())
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn invalid_created_at_rejects_queue_without_partial_promotion() {
        let (path, env) = create_env("invalid-created", 2);
        {
            let db = env.connection().unwrap();
            insert_card(&db, "corrupt", "new", "not-a-timestamp");
        }
        assert!(queue(&env, "rs-1").is_err());
        let db = env.connection().unwrap();
        let count: u32 = db
            .query_row(
                "SELECT COUNT(*) FROM cards WHERE id='corrupt' AND status='new'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
        drop(db);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn scans_beyond_old_five_hundred_row_cutoff() {
        let (path, env) = create_env("scan", 0);
        let now = crate::api::now();
        {
            let db = env.connection().unwrap();
            for index in 0..501 {
                let id = format!("card-{index:03}");
                insert_card(&db, &id, "review", "2020-01-01T00:00:00Z");
                let state = ScheduledState {
                    algorithm: "fallback".to_owned(),
                    algorithm_version: "fallback-1".to_owned(),
                    parameter_set_id: None,
                    difficulty: None,
                    stability: None,
                    last_review_at: Some(review_engine::Timestamp(if index == 500 {
                        now.0 - 3 * 86_400
                    } else {
                        now.0 - 86_400
                    })),
                    due_at: Some(review_engine::Timestamp(if index == 500 {
                        now.0 - 3 * 86_400
                    } else {
                        now.0 - 60
                    })),
                    scheduled_interval_seconds: Some(86_400),
                    lapse_count: 0,
                    review_count: 1,
                    state_version: 2,
                };
                db.execute(
                    review::UPSERT_SCHEDULE_STATE,
                    params![
                        id,
                        state.algorithm,
                        state.algorithm_version,
                        Option::<String>::None,
                        state.difficulty,
                        state.stability,
                        state
                            .last_review_at
                            .map(|value| to_rfc3339(learning_core::Timestamp(value.0))),
                        state
                            .due_at
                            .map(|value| to_rfc3339(learning_core::Timestamp(value.0))),
                        state.scheduled_interval_seconds,
                        state.lapse_count,
                        state.review_count,
                        state.state_version,
                        review_engine::state_hash(&state),
                        to_rfc3339(now)
                    ],
                )
                .unwrap();
            }
        }

        let value = queue(&env, "rs-1").unwrap();
        let ids = value["queue"].as_array().unwrap();
        assert!(ids.iter().any(|item| item["card_id"] == "card-500"));
        assert_eq!(ids.len(), 50);
        let _ = std::fs::remove_file(path);
    }
}
