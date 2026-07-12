//! Review sessions, events, undo, stats, pack evaluation
//! (08-http-api.md §6; 05-review-engine.md §7; WBS 5.4, 5.6–5.8).

use crate::api::{db, error_response, new_id, now, query_param};
use crate::repositories::{cards, review, to_rfc3339};
use application::{QueueCandidate, build_queue, next_card_status};
use learning_core::{CardId, CardStatus, Timestamp};
use review_engine::{
    FallbackScheduler, FsrsScheduler, Grade, InitialReviewInput, ReviewEventInput, ReviewScheduler,
    SchedulerSettings,
};
use worker::{D1Database, Env, Request, Response, Result};

/// Scheduler for a persisted state (`algorithm` column decides).
fn scheduler_for(algorithm: &str) -> Result<Box<dyn ReviewScheduler>> {
    match algorithm {
        "fallback" => Ok(Box::new(FallbackScheduler)),
        _ => FsrsScheduler::new()
            .map(|s| Box::new(s) as Box<dyn ReviewScheduler>)
            .map_err(|e| worker::Error::RustError(e.to_string())),
    }
}

/// `POST /api/v1/review-sessions` — body `{ language_profile_id, source? }`.
pub async fn create_session(mut req: Request, env: &Env) -> Result<Response> {
    #[derive(serde::Deserialize)]
    struct Body {
        language_profile_id: String,
        #[serde(default)]
        source: Option<String>,
    }
    let Ok(body) = req.json::<Body>().await else {
        return error_response(400, "invalid_body", "expected language_profile_id");
    };
    let source = body.source.unwrap_or_else(|| "online".to_owned());
    if source != "online" && source != "offline" {
        return error_response(400, "invalid_field", "source must be online|offline");
    }
    let id = new_id("rs");
    db(env)?
        .prepare(review::INSERT_SESSION)
        .bind(&[
            id.clone().into(),
            body.language_profile_id.into(),
            to_rfc3339(now()).into(),
            source.into(),
        ])?
        .run()
        .await?;
    Response::from_json(&serde_json::json!({ "id": id })).map(|r| r.with_status(201))
}

#[derive(serde::Deserialize)]
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

/// `GET /api/v1/review-sessions/{session_id}/queue` — ordered, bounded batch
/// with card fronts + state hashes.
pub async fn queue(env: &Env, session_id: &str) -> Result<Response> {
    let database = db(env)?;
    #[derive(serde::Deserialize)]
    struct SessionRow {
        learner_language_profile_id: String,
    }
    let Some(session) = database
        .prepare(review::SELECT_SESSION)
        .bind(&[session_id.into()])?
        .first::<SessionRow>(None)
        .await?
    else {
        return error_response(404, "not_found", "unknown session");
    };
    let profile_id = session.learner_language_profile_id;
    let at = now();

    let rows: Vec<CandidateRow> = database
        .prepare(review::SELECT_QUEUE_CANDIDATES)
        .bind(&[profile_id.clone().into()])?
        .all()
        .await?
        .results()?;

    let mut candidates = Vec::with_capacity(rows.len());
    for row in &rows {
        let status: CardStatus = match crate::repositories::parse_enum("status", &row.status) {
            Ok(status) => status,
            Err(_) => continue,
        };
        let (due_at, retrievability) = match row.state().map(review::ScheduleStateRow::into_state) {
            Some(Ok(state)) => {
                let scheduler = scheduler_for(&state.algorithm)?;
                let r = scheduler
                    .predicted_retrievability(&state, review_engine::Timestamp(at.0))
                    .ok();
                (state.due_at.map(|t| Timestamp(t.0)), r)
            }
            _ => (Some(at), None), // never-scheduled activated card: due now
        };
        candidates.push(QueueCandidate {
            card_id: CardId(row.card_id.clone()),
            status,
            due_at,
            retrievability,
        });
    }

    let budget = effective_budget(&database, &profile_id, at).await?;
    let settings = SchedulerSettings::default();
    let ordered = build_queue(&candidates, at, budget, settings.max_review_batch);

    // Card fronts come from the same join — no per-card queries; a 50-card
    // batch must stay far under D1's 50-queries-per-invocation free-tier cap
    // (16-rust-guidelines.md §5, 11-infrastructure.md §7).
    let mut items = Vec::with_capacity(ordered.len());
    for card_id in &ordered {
        let Some(row) = rows.iter().find(|r| r.card_id == card_id.0) else {
            continue;
        };
        let accepted: Vec<String> =
            serde_json::from_str(&row.accepted_answers_json).unwrap_or_default();
        let state_hash = row
            .state()
            .and_then(|s| s.into_state().ok())
            .map(|s| s.state_hash());
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
    Response::from_json(&serde_json::json!({
        "session_id": session_id,
        "new_card_budget": budget,
        "queue": items,
    }))
}

/// Backlog-protected new-card budget (05 §9) from live signals.
pub(crate) async fn effective_budget(
    database: &D1Database,
    profile_id: &str,
    at: Timestamp,
) -> Result<u8> {
    #[derive(serde::Deserialize)]
    struct OneCount {
        n: u32,
    }
    let overdue = database
        .prepare(review::COUNT_OVERDUE)
        .bind(&[profile_id.into(), to_rfc3339(at).into()])?
        .first::<OneCount>(None)
        .await?
        .map_or(0, |c| c.n);
    let activated_today = database
        .prepare(cards::COUNT_ACTIVATED_TODAY)
        .bind(&[
            profile_id.into(),
            to_rfc3339(Timestamp(at.0 - at.0 % 86_400)).into(),
        ])?
        .first::<OneCount>(None)
        .await?
        .map_or(0, |c| c.n);
    let base = 8_u8; // learner default (04-database.md learner_profile)
    // Daily signal history lands with analytics (WBS 8); overdue + gradual
    // restore rules apply from day one.
    let budget = adaptation::new_card_budget(base, base, overdue, None);
    #[allow(clippy::cast_possible_truncation)] // clamped by budget ≤ 255
    Ok(budget.saturating_sub(activated_today.min(255) as u8))
}

/// `POST /api/v1/review-sessions/{session_id}/events` — grade submission:
/// idempotent, stale-hash-guarded, event + state in one batch.
pub async fn submit_event(mut req: Request, env: &Env, session_id: &str) -> Result<Response> {
    #[derive(serde::Deserialize)]
    struct Body {
        client_operation_id: String,
        card_id: String,
        grade: u8,
        presented_at: String,
        answered_at: String,
        response_millis: u32,
        answer_revealed: bool,
        #[serde(default)]
        learner_note: Option<String>,
        prior_state_hash: String,
        #[serde(default = "default_source")]
        source: String,
    }
    fn default_source() -> String {
        "online".to_owned()
    }
    let Ok(body) = req.json::<Body>().await else {
        return error_response(400, "invalid_body", "malformed review event");
    };
    let Ok(grade) = Grade::try_from(body.grade) else {
        return error_response(400, "invalid_field", "grade must be 1..=4");
    };
    let database = db(env)?;

    // Idempotency: duplicate returns the original result, writes nothing.
    #[derive(serde::Deserialize)]
    struct ExistingEvent {
        id: String,
        card_id: String,
        result_state_hash: String,
    }
    if let Some(existing) = database
        .prepare(review::SELECT_EVENT_BY_OP)
        .bind(&[body.client_operation_id.clone().into()])?
        .first::<ExistingEvent>(None)
        .await?
    {
        return Response::from_json(&serde_json::json!({
            "event_id": existing.id,
            "card_id": existing.card_id,
            "result_state_hash": existing.result_state_hash,
            "duplicate": true,
            "next": null,
        }));
    }

    let Some(card_row) = database
        .prepare(cards::select_card_sql())
        .bind(&[body.card_id.clone().into()])?
        .first::<cards::CardRow>(None)
        .await?
    else {
        return error_response(404, "not_found", "unknown card");
    };
    let Ok(card) = card_row.into_domain() else {
        return error_response(500, "corrupt_row", "stored card is invalid");
    };

    let state = match database
        .prepare(review::SELECT_SCHEDULE_STATE)
        .bind(&[body.card_id.clone().into()])?
        .first::<review::ScheduleStateRow>(None)
        .await?
    {
        Some(row) => match row.into_state() {
            Ok(state) => state,
            Err(e) => return error_response(500, "corrupt_row", &e.to_string()),
        },
        None => {
            // Activation writes the initial state (lessons.rs); absence
            // means the card was never activated.
            return error_response(409, "not_reviewable", "card has no schedule state");
        }
    };

    if state.state_hash() != body.prior_state_hash {
        return error_response(409, "conflict_stale_state", "state changed; refetch queue");
    }
    let Some(new_card_status) = next_card_status(card.status, grade) else {
        return error_response(409, "not_reviewable", "card is not in a reviewable status");
    };

    let scheduler = scheduler_for(&state.algorithm)?;
    let answered_at = crate::repositories::from_rfc3339(&body.answered_at)
        .map(|t| review_engine::Timestamp(t.0))
        .unwrap_or_else(|_| now_re());
    let result = match scheduler.apply_review(
        &state,
        &ReviewEventInput { grade, answered_at },
        &SchedulerSettings::default(),
    ) {
        Ok(result) => result,
        Err(e) => return error_response(500, "scheduler_error", &e.to_string()),
    };

    let event_id = new_id("ev");
    let state_row = review::ScheduleStateRow::from_state(&body.card_id, &result);
    let at = to_rfc3339(now());
    database
        .batch(vec![
            database.prepare(review::INSERT_EVENT).bind(&[
                event_id.clone().into(),
                body.card_id.clone().into(),
                session_id.into(),
                body.client_operation_id.into(),
                body.presented_at.into(),
                body.answered_at.into(),
                body.grade.into(),
                body.response_millis.into(),
                u8::from(body.answer_revealed).into(),
                body.learner_note
                    .map_or(worker::wasm_bindgen::JsValue::NULL, Into::into),
                state.state_hash().into(),
                result.state_hash().into(),
                result.algorithm_version.clone().into(),
                body.source.into(),
                worker::wasm_bindgen::JsValue::NULL,
                at.clone().into(),
            ])?,
            bind_state(&database, &state_row, &at)?,
            database.prepare(cards::UPDATE_CARD_STATUS).bind(&[
                body.card_id.clone().into(),
                crate::repositories::enum_str(&new_card_status).into(),
                worker::wasm_bindgen::JsValue::NULL,
                at.clone().into(),
            ])?,
        ])
        .await?;

    Response::from_json(&serde_json::json!({
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
        "next": null, // client refetches the queue (WBS 6 UI contract)
    }))
}

fn bind_state(
    database: &D1Database,
    row: &review::ScheduleStateRow,
    at: &str,
) -> Result<worker::D1PreparedStatement> {
    database.prepare(review::UPSERT_SCHEDULE_STATE).bind(&[
        row.card_id.clone().into(),
        row.algorithm.clone().into(),
        row.algorithm_version.clone().into(),
        row.parameter_set_id
            .clone()
            .map_or(worker::wasm_bindgen::JsValue::NULL, Into::into),
        row.difficulty
            .map_or(worker::wasm_bindgen::JsValue::NULL, Into::into),
        row.stability
            .map_or(worker::wasm_bindgen::JsValue::NULL, Into::into),
        row.last_review_at
            .clone()
            .map_or(worker::wasm_bindgen::JsValue::NULL, Into::into),
        row.due_at
            .clone()
            .map_or(worker::wasm_bindgen::JsValue::NULL, Into::into),
        #[allow(clippy::cast_precision_loss)] // intervals « 2^53, exact in f64
        row.scheduled_interval_seconds
            .map_or(worker::wasm_bindgen::JsValue::NULL, |v| (v as f64).into()),
        row.lapse_count.into(),
        row.review_count.into(),
        row.state_version.into(),
        row.state_hash.clone().into(),
        at.into(),
    ])
}

fn now_re() -> review_engine::Timestamp {
    review_engine::Timestamp(now().0)
}

/// `POST /api/v1/review-sessions/{session_id}/undo-last` — compensating
/// event + state recomputed by deterministic replay (05 §7: never deletes).
pub async fn undo_last(env: &Env, session_id: &str) -> Result<Response> {
    let database = db(env)?;
    #[derive(serde::Deserialize, Clone)]
    struct EventRow {
        id: String,
        card_id: String,
        client_operation_id: String,
        answered_at: String,
        grade: u8,
        compensates_event_id: Option<String>,
    }
    let Some(last) = database
        .prepare(review::SELECT_LAST_SESSION_EVENT)
        .bind(&[session_id.into()])?
        .first::<EventRow>(None)
        .await?
    else {
        return error_response(404, "not_found", "no events in session");
    };
    if last.compensates_event_id.is_some() {
        return error_response(409, "already_undone", "only the immediately previous grade");
    }

    // Replay all effective events except the undone one.
    let history: Vec<EventRow> = database
        .prepare(review::SELECT_EVENTS_FOR_CARD)
        .bind(&[last.card_id.clone().into()])?
        .all()
        .await?
        .results()?;
    let compensated: Vec<&str> = history
        .iter()
        .filter_map(|e| e.compensates_event_id.as_deref())
        .collect();
    let effective: Vec<&EventRow> = history
        .iter()
        .filter(|e| e.compensates_event_id.is_none())
        .filter(|e| !compensated.contains(&e.id.as_str()))
        .filter(|e| e.id != last.id)
        .collect();

    let scheduler = FsrsScheduler::new().map_err(|e| worker::Error::RustError(e.to_string()))?;
    let settings = SchedulerSettings::default();
    let mut state = scheduler
        .initial_state(InitialReviewInput { now: now_re() })
        .map_err(|e| worker::Error::RustError(e.to_string()))?;
    for event in &effective {
        let Ok(grade) = Grade::try_from(event.grade) else {
            continue;
        };
        let answered = crate::repositories::from_rfc3339(&event.answered_at)
            .map(|t| review_engine::Timestamp(t.0))
            .unwrap_or_else(|_| now_re());
        state = scheduler
            .apply_review(
                &state,
                &ReviewEventInput {
                    grade,
                    answered_at: answered,
                },
                &settings,
            )
            .map_err(|e| worker::Error::RustError(e.to_string()))?;
    }

    let event_id = new_id("ev");
    let at = to_rfc3339(now());
    let state_row = review::ScheduleStateRow::from_state(&last.card_id, &state);
    database
        .batch(vec![
            database.prepare(review::INSERT_EVENT).bind(&[
                event_id.clone().into(),
                last.card_id.clone().into(),
                session_id.into(),
                format!("undo-{}", last.client_operation_id).into(),
                at.clone().into(),
                at.clone().into(),
                last.grade.into(),
                0.into(),
                1.into(),
                worker::wasm_bindgen::JsValue::NULL,
                state_row.state_hash.clone().into(),
                state_row.state_hash.clone().into(),
                state.algorithm_version.clone().into(),
                "online".into(),
                last.id.clone().into(),
                at.clone().into(),
            ])?,
            bind_state(&database, &state_row, &at)?,
        ])
        .await?;
    Response::from_json(&serde_json::json!({
        "compensating_event_id": event_id,
        "compensates_event_id": last.id,
        "state_hash": state_row.state_hash,
    }))
}

/// `POST /api/v1/review-sessions/{session_id}/complete` — closes the session
/// and runs the adaptive rule table over its cards (WBS 8.1: recommendations,
/// never silent mutation).
pub async fn complete_session(mut req: Request, env: &Env, session_id: &str) -> Result<Response> {
    #[derive(serde::Deserialize)]
    struct Body {
        card_count: u32,
        review_seconds: u32,
    }
    let Ok(body) = req.json::<Body>().await else {
        return error_response(400, "invalid_body", "expected card_count + review_seconds");
    };
    let database = db(env)?;
    #[derive(serde::Deserialize)]
    struct SessionRow {
        learner_language_profile_id: String,
    }
    let Some(session) = database
        .prepare(review::SELECT_SESSION)
        .bind(&[session_id.into()])?
        .first::<SessionRow>(None)
        .await?
    else {
        return error_response(404, "not_found", "unknown session");
    };
    database
        .prepare(review::COMPLETE_SESSION)
        .bind(&[
            session_id.into(),
            to_rfc3339(now()).into(),
            body.card_count.into(),
            body.review_seconds.into(),
        ])?
        .run()
        .await?;

    let signals = collect_session_signals(&database, session_id).await?;
    let recommendations: Vec<adaptation::Recommendation> = signals
        .iter()
        .flat_map(adaptation::evaluate_card_rules)
        .collect();
    let created = crate::handlers::recommendations::insert_all(
        &database,
        &session.learner_language_profile_id,
        &recommendations,
    )
    .await?;
    Response::from_json(&serde_json::json!({
        "completed": true,
        "recommendations_created": created,
    }))
}

/// Two fixed queries → bounded per-card signal windows (adaptation input).
async fn collect_session_signals(
    database: &D1Database,
    session_id: &str,
) -> Result<Vec<adaptation::CardSignals>> {
    #[derive(serde::Deserialize)]
    struct GradeRow {
        card_id: String,
        grade: u8,
        response_millis: u32,
    }
    #[derive(serde::Deserialize)]
    struct EvidenceRow {
        card_id: String,
        source_learning_item_id: Option<String>,
        spont_fail: u32,
        spont_ok: u32,
        prompted_ok: u32,
        listen_fail: u32,
        review_count: u32,
    }
    let grades: Vec<GradeRow> = database
        .prepare(review::SELECT_SESSION_CARD_SIGNALS)
        .bind(&[session_id.into()])?
        .all()
        .await?
        .results()?;
    let evidence: Vec<EvidenceRow> = database
        .prepare(review::SELECT_SESSION_CARD_EVIDENCE)
        .bind(&[session_id.into()])?
        .all()
        .await?
        .results()?;

    let mut signals: Vec<adaptation::CardSignals> = Vec::with_capacity(evidence.len());
    for row in evidence {
        let card_grades: Vec<u8> = grades
            .iter()
            .filter(|g| g.card_id == row.card_id)
            .map(|g| g.grade)
            .collect();
        let latencies: Vec<u32> = grades
            .iter()
            .filter(|g| g.card_id == row.card_id)
            .map(|g| g.response_millis)
            .collect();
        let avg = (!latencies.is_empty())
            .then(|| latencies.iter().map(|m| u64::from(*m)).sum::<u64>() / latencies.len() as u64);
        signals.push(adaptation::CardSignals {
            card_id: row.card_id,
            learning_item_id: row.source_learning_item_id,
            recent_grades: card_grades,
            avg_response_millis: avg.and_then(|a| u32::try_from(a).ok()),
            spontaneous_failures: row.spont_fail,
            spontaneous_successes: row.spont_ok,
            prompted_successes: row.prompted_ok,
            listening_failures: row.listen_fail,
            review_count: row.review_count,
        });
    }
    Ok(signals)
}

/// `GET /api/v1/review/summary?language_profile_id={id}` — today's counts +
/// grade distribution (WBS 5.8).
pub async fn summary(req: &Request, env: &Env) -> Result<Response> {
    let url = req.url()?;
    let Some(profile_id) = query_param(&url, "language_profile_id") else {
        return error_response(400, "missing_param", "language_profile_id required");
    };
    let at = now();
    let day_start = to_rfc3339(Timestamp(at.0 - at.0 % 86_400));
    #[derive(serde::Deserialize)]
    struct EventRow {
        grade: u8,
        response_millis: u32,
    }
    let events: Vec<EventRow> = db(env)?
        .prepare(review::SELECT_EVENTS_SINCE)
        .bind(&[profile_id.into(), day_start.into()])?
        .all()
        .await?
        .results()?;
    let mut grades = [0_u32; 4];
    let mut millis: u64 = 0;
    for event in &events {
        if (1..=4).contains(&event.grade) {
            grades[usize::from(event.grade - 1)] += 1;
        }
        millis += u64::from(event.response_millis);
    }
    Response::from_json(&serde_json::json!({
        "reviews_today": events.len(),
        "grade_distribution": { "again": grades[0], "hard": grades[1],
                                 "good": grades[2], "easy": grades[3] },
        "review_seconds_today": millis / 1000,
    }))
}

/// `GET /api/v1/review/forecast?language_profile_id={id}&days=7`.
pub async fn forecast(req: &Request, env: &Env) -> Result<Response> {
    let url = req.url()?;
    let Some(profile_id) = query_param(&url, "language_profile_id") else {
        return error_response(400, "missing_param", "language_profile_id required");
    };
    let days: i64 = query_param(&url, "days")
        .and_then(|d| d.parse().ok())
        .unwrap_or(7)
        .clamp(1, 30);
    let at = now();
    let horizon = to_rfc3339(Timestamp(at.0 + days * 86_400));
    #[derive(serde::Deserialize)]
    struct DueRow {
        due_at: String,
    }
    let rows: Vec<DueRow> = db(env)?
        .prepare(review::SELECT_DUE_BETWEEN)
        .bind(&[profile_id.into(), horizon.into()])?
        .all()
        .await?
        .results()?;
    let mut per_day = vec![0_u32; usize::try_from(days).unwrap_or(7)];
    for row in &rows {
        if let Ok(due) = crate::repositories::from_rfc3339(&row.due_at) {
            let offset = ((due.0 - at.0).max(0)) / 86_400;
            if let Ok(index) = usize::try_from(offset)
                && index < per_day.len()
            {
                per_day[index] += 1;
            }
        }
    }
    Response::from_json(&serde_json::json!({ "days": days, "due_per_day": per_day }))
}

/// `POST /api/v1/review/evaluate` — pack-driven answer check before grading
/// (WBS 5.7). Body `{ card_id, learner_answer }`.
pub async fn evaluate(
    registry: &language_pack_registry::LanguagePackRegistry,
    mut req: Request,
    env: &Env,
) -> Result<Response> {
    #[derive(serde::Deserialize)]
    struct Body {
        card_id: String,
        learner_answer: String,
    }
    let Ok(body) = req.json::<Body>().await else {
        return error_response(400, "invalid_body", "expected card_id + learner_answer");
    };
    let Some(card_row) = db(env)?
        .prepare(cards::select_card_sql())
        .bind(&[body.card_id.into()])?
        .first::<cards::CardRow>(None)
        .await?
    else {
        return error_response(404, "not_found", "unknown card");
    };
    let Ok(card) = card_row.into_domain() else {
        return error_response(500, "corrupt_row", "stored card is invalid");
    };
    let Some(pack) = registry.get(&card.language_pack_id.0) else {
        return error_response(409, "pack_incompatible", "card's pack not registered");
    };
    let evaluation = pack.evaluate_answer(&language_pack_api::AnswerEvaluationRequest {
        expected_answer: card.answer,
        accepted_answers: card.accepted_answers,
        learner_answer: body.learner_answer,
        evaluation_mode: card.answer_evaluation_mode,
    });
    Response::from_json(&evaluation)
}
