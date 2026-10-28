//! Review sessions, events, undo, stats, and pack evaluation.
//! Scheduling rules live in review-engine; transport mapping lives elsewhere.

use crate::api::{now, operation_error};
use crate::repositories::{review, to_rfc3339};
use crate::{Env, Result};
use learning_core::Timestamp;
use rusqlite::{OptionalExtension as _, params};

#[derive(serde::Deserialize)]
pub(crate) struct ReviewEventParams {
    pub client_operation_id: String,
    pub card_id: String,
    pub grade: u8,
    pub presented_at: String,
    pub answered_at: String,
    pub response_millis: u32,
    pub answer_revealed: bool,
    #[serde(default)]
    pub learner_note: Option<String>,
    pub prior_state_hash: String,
    #[serde(default = "default_source")]
    pub source: String,
}

fn default_source() -> String {
    "online".to_owned()
}

pub(crate) fn summary_for(env: &Env, profile_id: &str) -> Result<serde_json::Value> {
    let at = now();
    let database = env.connection()?;
    let timezone: String = database
        .query_row(
            "SELECT l.timezone
         FROM learner_language_profiles p
         JOIN learner_profile l ON l.id=p.learner_id
         WHERE p.id=?1",
            params![profile_id],
            |row| row.get(0),
        )
        .optional()?
        .ok_or_else(|| {
            crate::Error::operation(404, "not_found", "unknown profile", serde_json::Value::Null)
        })?;
    let day_start = to_rfc3339(crate::operations::card_budget::local_day_start_utc(
        at, &timezone,
    )?);
    let mut grades = [0_u32; 4];
    let mut millis = 0_u64;
    let mut reviews_today = 0_u64;
    {
        let mut statement = database.prepare(review::SELECT_EVENTS_SINCE)?;
        let mut rows = statement.query(params![profile_id, day_start])?;
        while let Some(row) = rows.next()? {
            let grade: u8 = row.get("grade")?;
            let response_millis: u32 = row.get("response_millis")?;
            if (1..=4).contains(&grade) {
                grades[usize::from(grade - 1)] += 1;
            }
            millis += u64::from(response_millis);
            reviews_today += 1;
        }
    }
    crate::json(&serde_json::json!({
        "reviews_today": reviews_today,
        "grade_distribution": {
            "again": grades[0], "hard": grades[1], "good": grades[2], "easy": grades[3]
        },
        "review_seconds_today": millis / 1000,
    }))
}

pub(crate) fn forecast_for(env: &Env, profile_id: &str, days: i64) -> Result<serde_json::Value> {
    let days = days.clamp(1, 30);
    let at = now();
    let horizon = to_rfc3339(Timestamp(at.0 + days * 86_400));
    let database = env.connection()?;
    let mut per_day = vec![0_u32; usize::try_from(days).unwrap_or(7)];
    {
        let mut statement = database.prepare(review::SELECT_DUE_BETWEEN)?;
        let mut rows = statement.query(params![profile_id, horizon])?;
        while let Some(row) = rows.next()? {
            let due_at: String = row.get("due_at")?;
            if let Ok(due) = crate::repositories::from_rfc3339(&due_at) {
                let offset = ((due.0 - at.0).max(0)) / 86_400;
                if let Ok(index) = usize::try_from(offset)
                    && index < per_day.len()
                {
                    per_day[index] += 1;
                }
            }
        }
    }
    crate::json(&serde_json::json!({ "days": days, "due_per_day": per_day }))
}

pub(crate) fn retention_for(env: &Env, profile_id: &str, days: i64) -> Result<serde_json::Value> {
    let days = days.clamp(7, 365);
    let at = now();
    let since = Timestamp(at.0 - days * 86_400);
    let database = env.connection()?;
    let bucket_count = usize::try_from((days + 6) / 7).unwrap_or(4);
    let mut buckets = vec![(0_u32, 0_u32, [0_u32; 4]); bucket_count];
    let mut total = (0_u32, 0_u32);
    {
        let mut statement = database.prepare(review::SELECT_EVENTS_FOR_RETENTION)?;
        let mut rows = statement.query(params![profile_id, to_rfc3339(since)])?;
        while let Some(row) = rows.next()? {
            let grade: u8 = row.get("grade")?;
            let created_at: String = row.get("created_at")?;
            let Ok(created) = crate::repositories::from_rfc3339(&created_at) else {
                continue;
            };
            let offset = ((created.0 - since.0).max(0)) / (7 * 86_400);
            let Ok(index) = usize::try_from(offset) else {
                continue;
            };
            let Some(bucket) = buckets.get_mut(index.min(bucket_count.saturating_sub(1))) else {
                continue;
            };
            let success = u32::from(grade > 1);
            bucket.0 += 1;
            bucket.1 += success;
            if (1..=4).contains(&grade) {
                bucket.2[usize::from(grade - 1)] += 1;
            }
            total.0 += 1;
            total.1 += success;
        }
    }
    let rate = |ok: u32, count: u32| (count > 0).then(|| f64::from(ok) / f64::from(count));
    let weeks: Vec<serde_json::Value> = buckets
        .iter()
        .enumerate()
        .map(|(index, (count, ok, grades))| {
            serde_json::json!({
                "week_start": to_rfc3339(Timestamp(
                    since.0 + i64::try_from(index).unwrap_or_default() * 7 * 86_400
                )),
                "reviews": count,
                "retention": rate(*ok, *count),
                "grades": {
                    "again": grades[0], "hard": grades[1], "good": grades[2], "easy": grades[3]
                },
            })
        })
        .collect();
    crate::json(&serde_json::json!({
        "days": days,
        "reviews": total.0,
        "retention": rate(total.1, total.0),
        "weeks": weeks,
    }))
}

pub(crate) fn evaluate(
    registry: &crate::BuiltInPackRegistry,
    env: &Env,
    card_id: &str,
    learner_answer: &str,
) -> Result<serde_json::Value> {
    let database = env.connection()?;
    let card = database
        .query_row(
            "SELECT language_pack_id, answer, accepted_answers_json,
                answer_evaluation_mode, language_pack_version, intent, status, created_by
         FROM cards WHERE id=?1",
            params![card_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                ))
            },
        )
        .optional()?;
    let Some((pack_id, answer, accepted_json, evaluation_mode, version, intent, status, actor)) =
        card
    else {
        return operation_error(404, "not_found", "unknown card");
    };

    if semver::Version::parse(&version).is_err()
        || crate::repositories::card_intent_from_db(&intent).is_none()
        || crate::repositories::card_status_from_db(&status).is_none()
        || crate::repositories::card_actor_from_db(&actor).is_none()
    {
        return operation_error(500, "corrupt_row", "stored card is invalid");
    }
    let Ok(accepted_answers) = serde_json::from_str::<Vec<String>>(&accepted_json) else {
        return operation_error(500, "corrupt_row", "stored card is invalid");
    };
    if language_pack_registry::get(registry, &pack_id).is_none() {
        return operation_error(409, "pack_incompatible", "card's pack not registered");
    }
    let evaluation =
        language_pack_es_es::evaluate_answer(&language_pack_api::AnswerEvaluationRequest {
            expected_answer: answer,
            accepted_answers,
            learner_answer: learner_answer.to_owned(),
            evaluation_mode,
        });
    crate::json(&evaluation)
}
