//! Review sessions, events, undo, stats, and pack evaluation.
//! Scheduling rules live in review-engine; transport mapping lives elsewhere.

use crate::api::{new_id, now, operation_error};
use crate::repositories::{cards, enum_str, review, to_rfc3339};
use crate::{Env, Result};
use application::{QueueCandidate, build_queue, next_card_status};
use learning_core::{CardId, CardStatus, Timestamp};
use review_engine::{
    FallbackScheduler, FsrsScheduler, Grade, ReviewEventInput, ReviewScheduler, ScheduleError,
    ScheduledState, SchedulerSettings,
};
use rusqlite::{OptionalExtension as _, params};
use std::collections::HashMap;

enum Scheduler {
    Fallback(FallbackScheduler),
    Fsrs(FsrsScheduler),
}

impl Scheduler {
    fn for_algorithm(algorithm: &str) -> Result<Self> {
        if algorithm == "fallback" {
            Ok(Self::Fallback(FallbackScheduler))
        } else {
            FsrsScheduler::new()
                .map(Self::Fsrs)
                .map_err(|error| crate::Error::RustError(error.to_string()))
        }
    }

    fn apply_review(
        &self,
        state: &ScheduledState,
        event: &ReviewEventInput,
        settings: &SchedulerSettings,
    ) -> std::result::Result<ScheduledState, ScheduleError> {
        match self {
            Self::Fallback(scheduler) => scheduler.apply_review(state, event, settings),
            Self::Fsrs(scheduler) => scheduler.apply_review(state, event, settings),
        }
    }

    fn predicted_retrievability(
        &self,
        state: &ScheduledState,
        at: review_engine::Timestamp,
    ) -> std::result::Result<f64, ScheduleError> {
        match self {
            Self::Fallback(scheduler) => scheduler.predicted_retrievability(state, at),
            Self::Fsrs(scheduler) => scheduler.predicted_retrievability(state, at),
        }
    }
}

pub(crate) fn create_session_for(
    env: &Env,
    language_profile_id: &str,
    source: &str,
) -> Result<serde_json::Value> {
    if source != "online" && source != "offline" {
        return operation_error(400, "invalid_field", "source must be online|offline");
    }
    let id = new_id("rs");
    let database = env.connection()?;
    database.execute(
        review::INSERT_SESSION,
        params![id, language_profile_id, to_rfc3339(now()), source],
    )?;
    crate::json(&serde_json::json!({ "id": id }))
}

struct CandidateRow {
    card_id: String,
    status: String,
    intent: String,
    prompt: String,
    prompt_language: String,
    answer: String,
    accepted_answers_json: String,
    answer_evaluation_mode: String,
    explanation: Option<String>,
    pronunciation_note: Option<String>,
    algorithm: Option<String>,
    algorithm_version: Option<String>,
    parameter_set_id: Option<String>,
    difficulty: Option<f64>,
    stability: Option<f64>,
    last_review_at: Option<String>,
    due_at: Option<String>,
    scheduled_interval_seconds: Option<i64>,
    lapse_count: Option<u32>,
    review_count: Option<u32>,
    state_version: Option<u32>,
}

impl CandidateRow {
    fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            card_id: row.get("card_id")?,
            status: row.get("status")?,
            intent: row.get("intent")?,
            prompt: row.get("prompt")?,
            prompt_language: row.get("prompt_language")?,
            answer: row.get("answer")?,
            accepted_answers_json: row.get("accepted_answers_json")?,
            answer_evaluation_mode: row.get("answer_evaluation_mode")?,
            explanation: row.get("explanation")?,
            pronunciation_note: row.get("pronunciation_note")?,
            algorithm: row.get("algorithm")?,
            algorithm_version: row.get("algorithm_version")?,
            parameter_set_id: row.get("parameter_set_id")?,
            difficulty: row.get("difficulty")?,
            stability: row.get("stability")?,
            last_review_at: row.get("last_review_at")?,
            due_at: row.get("due_at")?,
            scheduled_interval_seconds: row.get("scheduled_interval_seconds")?,
            lapse_count: row.get("lapse_count")?,
            review_count: row.get("review_count")?,
            state_version: row.get("state_version")?,
        })
    }

    fn state(&self) -> Option<review::ScheduleStateRow> {
        Some(review::ScheduleStateRow {
            card_id: self.card_id.clone(),
            algorithm: self.algorithm.clone()?,
            algorithm_version: self.algorithm_version.clone()?,
            parameter_set_id: self.parameter_set_id.clone(),
            difficulty: self.difficulty,
            stability: self.stability,
            last_review_at: self.last_review_at.clone(),
            due_at: self.due_at.clone(),
            scheduled_interval_seconds: self.scheduled_interval_seconds,
            lapse_count: self.lapse_count.unwrap_or(0),
            review_count: self.review_count.unwrap_or(0),
            state_version: self.state_version.unwrap_or(1),
            state_hash: String::new(),
        })
    }
}

pub(crate) fn queue(env: &Env, session_id: &str) -> Result<serde_json::Value> {
    let mut database = env.connection()?;
    let session = database
        .query_row(review::SELECT_SESSION, params![session_id], |row| {
            row.get::<_, String>("learner_language_profile_id")
        })
        .optional()?;
    let Some(profile_id) = session else {
        return operation_error(404, "not_found", "unknown session");
    };
    let at = now();

    let mint_budget = effective_budget(&database, &profile_id, at)?;
    promote_candidates(&mut database, &profile_id, at, mint_budget)?;

    let rows: Vec<CandidateRow> = {
        let mut statement = database.prepare(review::SELECT_QUEUE_CANDIDATES)?;
        statement
            .query_map(params![profile_id], CandidateRow::from_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };

    let mut candidates = Vec::with_capacity(rows.len());
    for row in &rows {
        let status: CardStatus = match crate::repositories::parse_enum("status", &row.status) {
            Ok(status) => status,
            Err(_) => continue,
        };
        let (due_at, retrievability) = match row.state().map(review::ScheduleStateRow::into_state) {
            Some(Ok(state)) => {
                let scheduler = Scheduler::for_algorithm(&state.algorithm)?;
                let retrievability = scheduler
                    .predicted_retrievability(&state, review_engine::Timestamp(at.0))
                    .ok();
                (state.due_at.map(|t| Timestamp(t.0)), retrievability)
            }
            _ => (Some(at), None),
        };
        candidates.push(QueueCandidate {
            card_id: CardId(row.card_id.clone()),
            status,
            due_at,
            retrievability,
        });
    }

    let session_budget = session_new_card_budget(&database, &profile_id, at)?;
    let settings = SchedulerSettings::default();
    let ordered = build_queue(&candidates, at, session_budget, settings.max_review_batch);
    let rows_by_id: HashMap<&str, &CandidateRow> =
        rows.iter().map(|row| (row.card_id.as_str(), row)).collect();
    let mut items = Vec::with_capacity(ordered.len());
    for card_id in &ordered {
        let Some(row) = rows_by_id.get(card_id.0.as_str()).copied() else {
            continue;
        };
        let accepted: Vec<String> =
            serde_json::from_str(&row.accepted_answers_json).unwrap_or_default();
        let state_hash = row
            .state()
            .and_then(|state| state.into_state().ok())
            .map(|state| state.state_hash());
        items.push(serde_json::json!({
            "card_id": row.card_id,
            "intent": row.intent,
            "status": row.status,
            "prompt": row.prompt,
            "prompt_language": row.prompt_language,
            "answer": row.answer,
            "accepted_answers": accepted,
            "answer_evaluation_mode": row.answer_evaluation_mode,
            "explanation": row.explanation,
            "pronunciation_note": row.pronunciation_note,
            "prior_state_hash": state_hash,
        }));
    }
    crate::json(&serde_json::json!({
        "session_id": session_id,
        "new_card_budget": session_budget,
        "queue": items,
    }))
}

pub(crate) fn effective_budget(
    database: &rusqlite::Connection,
    profile_id: &str,
    at: Timestamp,
) -> Result<u8> {
    budget_from(database, profile_id, at, cards::COUNT_ACTIVATED_TODAY)
}

fn session_new_card_budget(
    database: &rusqlite::Connection,
    profile_id: &str,
    at: Timestamp,
) -> Result<u8> {
    budget_from(
        database,
        profile_id,
        at,
        review::COUNT_NEW_CARDS_INTRODUCED_TODAY,
    )
}

fn budget_from(
    database: &rusqlite::Connection,
    profile_id: &str,
    at: Timestamp,
    consumed_today_query: &str,
) -> Result<u8> {
    let overdue: u32 = database.query_row(
        review::COUNT_OVERDUE,
        params![profile_id, to_rfc3339(at)],
        |row| row.get(0),
    )?;
    let consumed_today: u32 = database.query_row(
        consumed_today_query,
        params![profile_id, to_rfc3339(Timestamp(at.0 - at.0 % 86_400))],
        |row| row.get(0),
    )?;
    let base = 8_u8;
    let budget = adaptation::new_card_budget(base, base, overdue, None);
    #[allow(clippy::cast_possible_truncation)]
    Ok(budget.saturating_sub(consumed_today.min(255) as u8))
}

fn promote_candidates(
    database: &mut rusqlite::Connection,
    profile_id: &str,
    at: Timestamp,
    budget: u8,
) -> Result<()> {
    if budget == 0 {
        return Ok(());
    }
    let rows: Vec<String> = {
        let mut statement = database.prepare(cards::SELECT_OLDEST_CANDIDATES)?;
        statement
            .query_map(params![profile_id, u32::from(budget)], |row| row.get(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };
    if rows.is_empty() {
        return Ok(());
    }
    let scheduler = FsrsScheduler::new().map_err(|e| crate::Error::RustError(e.to_string()))?;
    let at_str = to_rfc3339(at);
    let tx = database.transaction()?;
    for id in rows {
        tx.execute(
            cards::UPDATE_CARD_STATUS,
            params![
                id,
                enum_str(&CardStatus::New),
                Option::<String>::None,
                at_str
            ],
        )?;
        let state = scheduler
            .initial_state(review_engine::InitialReviewInput {
                now: review_engine::Timestamp(at.0),
            })
            .map_err(|e| crate::Error::RustError(e.to_string()))?;
        let state_row = review::ScheduleStateRow::from_state(&id, &state);
        write_state(&tx, &state_row, &at_str)?;
    }
    tx.commit()?;
    Ok(())
}

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

pub(crate) fn submit_event_params(
    env: &Env,
    session_id: &str,
    body: ReviewEventParams,
) -> Result<serde_json::Value> {
    let Ok(grade) = Grade::try_from(body.grade) else {
        return operation_error(400, "invalid_field", "grade must be 1..=4");
    };
    let mut database = env.connection()?;

    if let Some((event_id, card_id, result_state_hash)) = database
        .query_row(
            review::SELECT_EVENT_BY_OP,
            params![body.client_operation_id],
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
        return crate::json(&serde_json::json!({
            "event_id": event_id,
            "card_id": card_id,
            "result_state_hash": result_state_hash,
            "duplicate": true,
            "next": null,
        }));
    }

    let card_row = database
        .query_row(
            &cards::select_card_sql(),
            params![body.card_id],
            cards::CardRow::from_row,
        )
        .optional()?;
    let Some(card_row) = card_row else {
        return operation_error(404, "not_found", "unknown card");
    };
    let Ok(card) = card_row.into_domain() else {
        return operation_error(500, "corrupt_row", "stored card is invalid");
    };

    let state_row = database
        .query_row(
            review::SELECT_SCHEDULE_STATE,
            params![body.card_id],
            review::ScheduleStateRow::from_row,
        )
        .optional()?;
    let state = match state_row {
        Some(row) => match row.into_state() {
            Ok(state) => state,
            Err(error) => return operation_error(500, "corrupt_row", &error.to_string()),
        },
        None => return operation_error(409, "not_reviewable", "card has no schedule state"),
    };

    if state.state_hash() != body.prior_state_hash {
        return operation_error(409, "conflict_stale_state", "state changed; refetch queue");
    }
    let Some(new_card_status) = next_card_status(card.status, grade) else {
        return operation_error(409, "not_reviewable", "card is not in a reviewable status");
    };
    let scheduler = Scheduler::for_algorithm(&state.algorithm)?;
    let Ok(answered_at) =
        crate::repositories::from_rfc3339(&body.answered_at).map(|t| review_engine::Timestamp(t.0))
    else {
        return operation_error(400, "invalid_timestamp", "answered_at must be RFC 3339");
    };
    let result = match scheduler.apply_review(
        &state,
        &ReviewEventInput { grade, answered_at },
        &SchedulerSettings::default(),
    ) {
        Ok(result) => result,
        Err(error) => return operation_error(500, "scheduler_error", &error.to_string()),
    };

    let event_id = new_id("ev");
    let state_row = review::ScheduleStateRow::from_state(&body.card_id, &result);
    let at = to_rfc3339(now());
    let tx = database.transaction()?;
    tx.execute(
        review::INSERT_EVENT,
        params![
            event_id,
            body.card_id,
            session_id,
            body.client_operation_id,
            body.presented_at,
            body.answered_at,
            body.grade,
            body.response_millis,
            u8::from(body.answer_revealed),
            body.learner_note,
            state.state_hash(),
            result.state_hash(),
            result.algorithm_version,
            body.source,
            Option::<String>::None,
            at,
        ],
    )?;
    write_state(&tx, &state_row, &at)?;
    tx.execute(
        cards::UPDATE_CARD_STATUS,
        params![
            body.card_id,
            enum_str(&new_card_status),
            Option::<String>::None,
            at
        ],
    )?;
    tx.commit()?;

    crate::json(&serde_json::json!({
        "event_id": event_id,
        "card_id": body.card_id,
        "card_status": new_card_status,
        "new_state": {
            "due_at": state_row.due_at,
            "scheduled_interval_seconds": state_row.scheduled_interval_seconds,
            "state_hash": state_row.state_hash,
            "review_count": state_row.review_count,
            "lapse_count": state_row.lapse_count,
        },
        "next": null,
    }))
}

fn write_state(
    database: &rusqlite::Connection,
    row: &review::ScheduleStateRow,
    at: &str,
) -> Result<()> {
    database.execute(
        review::UPSERT_SCHEDULE_STATE,
        params![
            row.card_id,
            row.algorithm,
            row.algorithm_version,
            row.parameter_set_id,
            row.difficulty,
            row.stability,
            row.last_review_at,
            row.due_at,
            row.scheduled_interval_seconds,
            row.lapse_count,
            row.review_count,
            row.state_version,
            row.state_hash,
            at,
        ],
    )?;
    Ok(())
}

struct ReplayEventRow {
    id: String,
    card_id: String,
    client_operation_id: String,
    grade: u8,
    compensates_event_id: Option<String>,
}

impl ReplayEventRow {
    fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            card_id: row.get("card_id")?,
            client_operation_id: row.get("client_operation_id")?,
            grade: row.get("grade")?,
            compensates_event_id: row.get("compensates_event_id")?,
        })
    }
}

pub(crate) fn undo_last(env: &Env, session_id: &str) -> Result<serde_json::Value> {
    let mut database = env.connection()?;
    let last = database
        .query_row(
            review::SELECT_LAST_SESSION_EVENT,
            params![session_id],
            ReplayEventRow::from_row,
        )
        .optional()?;
    let Some(last) = last else {
        return operation_error(404, "not_found", "no events in session");
    };
    if last.compensates_event_id.is_some() {
        return operation_error(409, "already_undone", "only the immediately previous grade");
    }

    let effective: Vec<ReviewEventInput> = {
        let mut statement = database.prepare(review::SELECT_EFFECTIVE_EVENTS_FOR_CARD)?;
        statement
            .query_map(params![last.card_id, last.id], |row| {
                Ok((
                    row.get::<_, u8>("grade")?,
                    row.get::<_, String>("answered_at")?,
                ))
            })?
            .filter_map(|row| {
                let (grade, answered_at) = row.ok()?;
                let grade = Grade::try_from(grade).ok()?;
                let answered_at = crate::repositories::from_rfc3339(&answered_at)
                    .map(|timestamp| review_engine::Timestamp(timestamp.0))
                    .ok()?;
                Some(Ok(ReviewEventInput { grade, answered_at }))
            })
            .collect::<rusqlite::Result<Vec<_>>>()?
    };

    let created_at = database
        .query_row(
            "SELECT created_at FROM cards WHERE id = ?1",
            params![last.card_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .ok_or_else(|| crate::Error::RustError("card missing for undo".to_owned()))?;
    let activated_at = crate::repositories::from_rfc3339(&created_at)
        .map(|t| review_engine::Timestamp(t.0))
        .map_err(|e| crate::Error::RustError(e.to_string()))?;

    let scheduler = FsrsScheduler::new().map_err(|e| crate::Error::RustError(e.to_string()))?;
    let state = review_engine::replay(
        &scheduler,
        activated_at,
        &effective,
        &SchedulerSettings::default(),
    )
    .map_err(|e| crate::Error::RustError(e.to_string()))?;

    let event_id = new_id("ev");
    let at = to_rfc3339(now());
    let state_row = review::ScheduleStateRow::from_state(&last.card_id, &state);
    let tx = database.transaction()?;
    tx.execute(
        review::INSERT_EVENT,
        params![
            event_id,
            last.card_id,
            session_id,
            format!("undo-{}", last.client_operation_id),
            at,
            at,
            last.grade,
            0_u32,
            1_u8,
            Option::<String>::None,
            state_row.state_hash,
            state_row.state_hash,
            state.algorithm_version,
            "online",
            last.id,
            at,
        ],
    )?;
    write_state(&tx, &state_row, &at)?;
    tx.commit()?;
    crate::json(&serde_json::json!({
        "compensating_event_id": event_id,
        "compensates_event_id": last.id,
        "state_hash": state_row.state_hash,
    }))
}

pub(crate) fn complete_session(
    env: &Env,
    session_id: &str,
    card_count: u32,
    review_seconds: u32,
) -> Result<serde_json::Value> {
    let database = env.connection()?;
    let profile_id = database
        .query_row(review::SELECT_SESSION, params![session_id], |row| {
            row.get::<_, String>("learner_language_profile_id")
        })
        .optional()?;
    let Some(profile_id) = profile_id else {
        return operation_error(404, "not_found", "unknown session");
    };
    database.execute(
        review::COMPLETE_SESSION,
        params![session_id, to_rfc3339(now()), card_count, review_seconds],
    )?;

    let signals = collect_session_signals(&database, session_id)?;
    let recommendations: Vec<adaptation::Recommendation> = signals
        .iter()
        .flat_map(adaptation::evaluate_card_rules)
        .collect();
    let created =
        crate::handlers::recommendations::insert_all(&database, &profile_id, &recommendations)?;
    crate::json(&serde_json::json!({
        "completed": true,
        "recommendations_created": created,
    }))
}

fn collect_session_signals(
    database: &rusqlite::Connection,
    session_id: &str,
) -> Result<Vec<adaptation::CardSignals>> {
    struct GradeRow {
        card_id: String,
        grade: u8,
        response_millis: u32,
    }
    struct EvidenceRow {
        card_id: String,
        source_learning_item_id: Option<String>,
        spont_fail: u32,
        spont_ok: u32,
        prompted_ok: u32,
        listen_fail: u32,
        review_count: u32,
    }
    let grades: Vec<GradeRow> = {
        let mut statement = database.prepare(review::SELECT_SESSION_CARD_SIGNALS)?;
        statement
            .query_map(params![session_id], |row| {
                Ok(GradeRow {
                    card_id: row.get("card_id")?,
                    grade: row.get("grade")?,
                    response_millis: row.get("response_millis")?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };
    let evidence: Vec<EvidenceRow> = {
        let mut statement = database.prepare(review::SELECT_SESSION_CARD_EVIDENCE)?;
        statement
            .query_map(params![session_id], |row| {
                Ok(EvidenceRow {
                    card_id: row.get("card_id")?,
                    source_learning_item_id: row.get("source_learning_item_id")?,
                    spont_fail: row.get("spont_fail")?,
                    spont_ok: row.get("spont_ok")?,
                    prompted_ok: row.get("prompted_ok")?,
                    listen_fail: row.get("listen_fail")?,
                    review_count: row.get("review_count")?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };

    let mut signals = Vec::with_capacity(evidence.len());
    for row in evidence {
        let card_grades: Vec<u8> = grades
            .iter()
            .filter(|grade| grade.card_id == row.card_id)
            .map(|grade| grade.grade)
            .collect();
        let latencies: Vec<u32> = grades
            .iter()
            .filter(|grade| grade.card_id == row.card_id)
            .map(|grade| grade.response_millis)
            .collect();
        let avg = (!latencies.is_empty())
            .then(|| latencies.iter().map(|m| u64::from(*m)).sum::<u64>() / latencies.len() as u64);
        signals.push(adaptation::CardSignals {
            card_id: row.card_id,
            learning_item_id: row.source_learning_item_id,
            recent_grades: card_grades,
            avg_response_millis: avg.and_then(|value| u32::try_from(value).ok()),
            spontaneous_failures: row.spont_fail,
            spontaneous_successes: row.spont_ok,
            prompted_successes: row.prompted_ok,
            listening_failures: row.listen_fail,
            review_count: row.review_count,
        });
    }
    Ok(signals)
}

pub(crate) fn summary_for(env: &Env, profile_id: &str) -> Result<serde_json::Value> {
    let at = now();
    let day_start = to_rfc3339(Timestamp(at.0 - at.0 % 86_400));
    let database = env.connection()?;
    let events: Vec<(u8, u32)> = {
        let mut statement = database.prepare(review::SELECT_EVENTS_SINCE)?;
        statement
            .query_map(params![profile_id, day_start], |row| {
                Ok((row.get("grade")?, row.get("response_millis")?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };
    let mut grades = [0_u32; 4];
    let mut millis = 0_u64;
    for (grade, response_millis) in &events {
        if (1..=4).contains(grade) {
            grades[usize::from(*grade - 1)] += 1;
        }
        millis += u64::from(*response_millis);
    }
    crate::json(&serde_json::json!({
        "reviews_today": events.len(),
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
    let rows: Vec<String> = {
        let mut statement = database.prepare(review::SELECT_DUE_BETWEEN)?;
        statement
            .query_map(params![profile_id, horizon], |row| row.get("due_at"))?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };
    let mut per_day = vec![0_u32; usize::try_from(days).unwrap_or(7)];
    for due_at in rows {
        if let Ok(due) = crate::repositories::from_rfc3339(&due_at) {
            let offset = ((due.0 - at.0).max(0)) / 86_400;
            if let Ok(index) = usize::try_from(offset)
                && index < per_day.len()
            {
                per_day[index] += 1;
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
    let events: Vec<(u8, String)> = {
        let mut statement = database.prepare(review::SELECT_EVENTS_FOR_RETENTION)?;
        statement
            .query_map(params![profile_id, to_rfc3339(since)], |row| {
                Ok((row.get("grade")?, row.get("created_at")?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };
    let bucket_count = usize::try_from((days + 6) / 7).unwrap_or(4);
    let mut buckets = vec![(0_u32, 0_u32, [0_u32; 4]); bucket_count];
    let mut total = (0_u32, 0_u32);
    for (grade, created_at) in &events {
        let Ok(created) = crate::repositories::from_rfc3339(created_at) else {
            continue;
        };
        let offset = ((created.0 - since.0).max(0)) / (7 * 86_400);
        let Ok(index) = usize::try_from(offset) else {
            continue;
        };
        let Some(bucket) = buckets.get_mut(index.min(bucket_count.saturating_sub(1))) else {
            continue;
        };
        let success = u32::from(*grade > 1);
        bucket.0 += 1;
        bucket.1 += success;
        if (1..=4).contains(grade) {
            bucket.2[usize::from(*grade - 1)] += 1;
        }
        total.0 += 1;
        total.1 += success;
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
    registry: &language_pack_registry::LanguagePackRegistry,
    env: &Env,
    card_id: &str,
    learner_answer: &str,
) -> Result<serde_json::Value> {
    let database = env.connection()?;
    let card_row = database
        .query_row(
            &cards::select_card_sql(),
            params![card_id],
            cards::CardRow::from_row,
        )
        .optional()?;
    let Some(card_row) = card_row else {
        return operation_error(404, "not_found", "unknown card");
    };
    let Ok(card) = card_row.into_domain() else {
        return operation_error(500, "corrupt_row", "stored card is invalid");
    };
    let Some(pack) = registry.get(&card.language_pack_id.0) else {
        return operation_error(409, "pack_incompatible", "card's pack not registered");
    };
    let evaluation = pack.evaluate_answer(&language_pack_api::AnswerEvaluationRequest {
        expected_answer: card.answer,
        accepted_answers: card.accepted_answers,
        learner_answer: learner_answer.to_owned(),
        evaluation_mode: card.answer_evaluation_mode,
    });
    crate::json(&evaluation)
}
