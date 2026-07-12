//! Learning state, next lesson, errors, listening, tutor-context export
//! (08-http-api.md §3, §7, §8).

use crate::api::{db, error_response, new_id, now, query_param};
use crate::handlers::profiles::fetch_profile;
use crate::repositories::{records, to_rfc3339};
use worker::{Env, Request, Response, Result};

/// `GET /api/v1/learning-state?language_profile_id={id}` — item counts by
/// status + active error targets + due rechecks.
pub async fn learning_state(req: &Request, env: &Env) -> Result<Response> {
    let url = req.url()?;
    let Some(profile_id) = query_param(&url, "language_profile_id") else {
        return error_response(400, "missing_param", "language_profile_id required");
    };
    let database = db(env)?;
    #[derive(serde::Deserialize)]
    struct CountRow {
        status: String,
        n: u32,
    }
    let counts: Vec<CountRow> = database
        .prepare(
            "SELECT status, COUNT(*) AS n FROM learning_items
             WHERE learner_language_profile_id = ?1 GROUP BY status",
        )
        .bind(&[profile_id.clone().into()])?
        .all()
        .await?
        .results()?;
    let items: serde_json::Map<String, serde_json::Value> = counts
        .into_iter()
        .map(|c| (c.status, serde_json::json!(c.n)))
        .collect();

    #[derive(serde::Deserialize)]
    struct OneCount {
        n: u32,
    }
    let active_errors = database
        .prepare(
            "SELECT COUNT(*) AS n FROM error_targets
             WHERE learner_language_profile_id = ?1 AND status IN ('active','recheck')",
        )
        .bind(&[profile_id.clone().into()])?
        .first::<OneCount>(None)
        .await?
        .map_or(0, |c| c.n);
    let due_rechecks = database
        .prepare(
            "SELECT COUNT(*) AS n FROM error_rechecks r
             JOIN error_targets t ON t.id = r.error_target_id
             WHERE t.learner_language_profile_id = ?1
               AND r.completed_at IS NULL AND r.due_at <= ?2",
        )
        .bind(&[profile_id.into(), to_rfc3339(now()).into()])?
        .first::<OneCount>(None)
        .await?
        .map_or(0, |c| c.n);

    Response::from_json(&serde_json::json!({
        "items_by_status": items,
        "active_error_targets": active_errors,
        "due_error_rechecks": due_rechecks,
    }))
}

/// `GET /api/v1/next-lesson?language_profile_id={id}`.
pub async fn next_lesson(req: &Request, env: &Env) -> Result<Response> {
    let url = req.url()?;
    let Some(profile_id) = query_param(&url, "language_profile_id") else {
        return error_response(400, "missing_param", "language_profile_id required");
    };
    let database = db(env)?;
    let Some(profile_row) = fetch_profile(&database, &profile_id).await? else {
        return error_response(404, "not_found", "unknown profile");
    };
    let Ok(profile) = profile_row.into_domain() else {
        return error_response(500, "corrupt_row", "stored profile is invalid");
    };
    #[derive(serde::Deserialize)]
    struct GoalRow {
        next_goal: Option<String>,
        scenario: String,
    }
    let last = database
        .prepare(
            "SELECT next_goal, scenario FROM lessons
             WHERE learner_language_profile_id = ?1 AND status = 'completed'
             ORDER BY lesson_number DESC LIMIT 1",
        )
        .bind(&[profile.id.0.clone().into()])?
        .first::<GoalRow>(None)
        .await?;
    Response::from_json(&serde_json::json!({
        "lesson_number": profile.current_lesson_number + 1,
        "goal": last.as_ref().and_then(|l| l.next_goal.clone()),
        "previous_scenario": last.map(|l| l.scenario),
        "estimated_level": profile.estimated_level,
    }))
}

/// `GET /api/v1/error-targets?language_profile_id={id}&status=…`.
pub async fn list_error_targets(req: &Request, env: &Env) -> Result<Response> {
    let url = req.url()?;
    let Some(profile_id) = query_param(&url, "language_profile_id") else {
        return error_response(400, "missing_param", "language_profile_id required");
    };
    let status = query_param(&url, "status");
    let rows: Vec<records::ErrorTargetRow> = db(env)?
        .prepare(records::SELECT_ERROR_TARGETS)
        .bind(&[
            profile_id.into(),
            status.map_or(worker::wasm_bindgen::JsValue::NULL, Into::into),
            100.into(),
        ])?
        .all()
        .await?
        .results()?;
    let domain: Vec<_> = rows
        .into_iter()
        .filter_map(|r| r.into_domain().ok())
        .collect();
    Response::from_json(&domain)
}

/// `GET /api/v1/error-targets/{id}`.
pub async fn get_error_target(env: &Env, target_id: &str) -> Result<Response> {
    let row = db(env)?
        .prepare(records::SELECT_ERROR_TARGET)
        .bind(&[target_id.into()])?
        .first::<records::ErrorTargetRow>(None)
        .await?;
    match row.and_then(|r| r.into_domain().ok()) {
        Some(target) => Response::from_json(&target),
        None => error_response(404, "not_found", "unknown error target"),
    }
}

/// `POST /api/v1/error-targets/{id}/rechecks/{recheck_id}/complete`
/// — body `{ "passed": true }`. Failed recheck → target back to active.
pub async fn complete_recheck(
    mut req: Request,
    env: &Env,
    target_id: &str,
    recheck_id: &str,
) -> Result<Response> {
    #[derive(serde::Deserialize)]
    struct Body {
        passed: bool,
    }
    let Ok(body) = req.json::<Body>().await else {
        return error_response(400, "invalid_body", "expected { passed: bool }");
    };
    let database = db(env)?;
    let at = to_rfc3339(now());
    database
        .prepare(records::COMPLETE_ERROR_RECHECK)
        .bind(&[
            recheck_id.into(),
            at.clone().into(),
            u8::from(body.passed).into(),
        ])?
        .run()
        .await?;
    // Failed 1/3/7 recheck returns the target to active practice (03 §6).
    let new_status = if body.passed { "recheck" } else { "active" };
    database
        .prepare("UPDATE error_targets SET status = ?2, updated_at = ?3 WHERE id = ?1")
        .bind(&[target_id.into(), new_status.into(), at.into()])?
        .run()
        .await?;
    if !body.passed {
        #[derive(serde::Deserialize)]
        struct TargetRow {
            learner_language_profile_id: String,
        }
        #[derive(serde::Deserialize)]
        struct RecheckRow {
            offset_days: u32,
        }
        let profile = database
            .prepare("SELECT learner_language_profile_id FROM error_targets WHERE id = ?1")
            .bind(&[target_id.into()])?
            .first::<TargetRow>(None)
            .await?;
        let offset = database
            .prepare("SELECT offset_days FROM error_rechecks WHERE id = ?1")
            .bind(&[recheck_id.into()])?
            .first::<RecheckRow>(None)
            .await?
            .map_or(1, |r| r.offset_days);
        if let Some(profile) = profile {
            let rec = adaptation::recheck_failed(target_id, offset);
            crate::handlers::recommendations::insert_all(
                &database,
                &profile.learner_language_profile_id,
                &[rec],
            )
            .await?;
        }
    }
    Response::from_json(&serde_json::json!({ "recheck_id": recheck_id, "passed": body.passed }))
}

/// `POST /api/v1/listening-attempts`.
pub async fn create_listening_attempt(mut req: Request, env: &Env) -> Result<Response> {
    #[derive(serde::Deserialize)]
    struct Body {
        language_profile_id: String,
        #[serde(default)]
        learning_item_id: Option<String>,
        #[serde(default)]
        lesson_id: Option<String>,
        #[serde(default)]
        source_reference: Option<String>,
        #[serde(default)]
        transcript: Option<String>,
        success: bool,
    }
    let Ok(body) = req.json::<Body>().await else {
        return error_response(400, "invalid_body", "malformed listening attempt");
    };
    let id = new_id("la");
    db(env)?
        .prepare(records::INSERT_LISTENING_ATTEMPT)
        .bind(&[
            id.clone().into(),
            body.language_profile_id.into(),
            body.learning_item_id
                .map_or(worker::wasm_bindgen::JsValue::NULL, Into::into),
            body.lesson_id
                .map_or(worker::wasm_bindgen::JsValue::NULL, Into::into),
            body.source_reference
                .map_or(worker::wasm_bindgen::JsValue::NULL, Into::into),
            body.transcript
                .map_or(worker::wasm_bindgen::JsValue::NULL, Into::into),
            u8::from(body.success).into(),
            to_rfc3339(now()).into(),
        ])?
        .run()
        .await?;
    Response::from_json(&serde_json::json!({ "id": id })).map(|r| r.with_status(201))
}

/// `GET /api/v1/export/tutor-context?language_profile_id={id}` — compact
/// context for pasting into ChatGPT (08-http-api.md §7). Versioned.
pub async fn tutor_context(
    registry: &language_pack_registry::LanguagePackRegistry,
    req: &Request,
    env: &Env,
) -> Result<Response> {
    let url = req.url()?;
    let Some(profile_id) = query_param(&url, "language_profile_id") else {
        return error_response(400, "missing_param", "language_profile_id required");
    };
    let database = db(env)?;
    let Some(profile_row) = fetch_profile(&database, &profile_id).await? else {
        return error_response(404, "not_found", "unknown profile");
    };
    let Ok(profile) = profile_row.into_domain() else {
        return error_response(500, "corrupt_row", "stored profile is invalid");
    };
    let Some(pack) = registry.get(&profile.language_pack_id.0) else {
        return error_response(409, "pack_incompatible", "profile pack not registered");
    };

    let error_rows: Vec<records::ErrorTargetRow> = database
        .prepare(records::SELECT_ERROR_TARGETS)
        .bind(&[
            profile.id.0.clone().into(),
            worker::wasm_bindgen::JsValue::NULL,
            20.into(),
        ])?
        .all()
        .await?
        .results()?;
    let errors: Vec<serde_json::Value> = error_rows
        .into_iter()
        .filter_map(|r| r.into_domain().ok())
        .filter(|t| t.status != learning_core::ErrorTargetStatus::Resolved)
        .map(|t| {
            serde_json::json!({
                "error_target_id": t.id.0,
                "core_category": t.core_category,
                "pack_error_code": t.pack_error_code,
                "original_production": t.original_production,
                "corrected_production": t.corrected_production,
                "occurrence_count": t.occurrence_count,
                "method_change_required": t.method_change_required,
            })
        })
        .collect();

    let learner_view = language_pack_api::LearnerLanguageProfile {
        source_language: profile.source_language.0.clone(),
        target_locale: profile.target_locale.0.clone(),
        estimated_level: profile.estimated_level.clone(),
    };
    let instructions = pack.tutor_instructions(&learner_view);

    // 8.4: review + redesign evidence enter the tutor context.
    let open_recommendations: Vec<serde_json::Value> = database
        .prepare(crate::repositories::recommendations::SELECT_OPEN)
        .bind(&[profile.id.0.clone().into(), 20.into()])?
        .all()
        .await?
        .results()?;
    let redesign: Vec<serde_json::Value> = database
        .prepare(crate::repositories::recommendations::SELECT_REDESIGN_CANDIDATES)
        .bind(&[profile.id.0.clone().into()])?
        .all()
        .await?
        .results()?;
    #[derive(serde::Deserialize)]
    struct EventRow {
        grade: u8,
    }
    let week_ago = to_rfc3339(learning_core::Timestamp(now().0 - 7 * 86_400));
    let week_events: Vec<EventRow> = database
        .prepare(crate::repositories::review::SELECT_EVENTS_SINCE)
        .bind(&[profile.id.0.clone().into(), week_ago.into()])?
        .all()
        .await?
        .results()?;
    let again_count = week_events.iter().filter(|e| e.grade == 1).count();

    Response::from_json(&serde_json::json!({
        "schema_version": "1",
        "language_profile_id": profile.id.0,
        "language_pack": pack.manifest(),
        "next_lesson_number": profile.current_lesson_number + 1,
        "estimated_level": profile.estimated_level,
        "tutor_instructions": instructions,
        "active_error_targets": errors,
        "review_summary_7d": {
            "reviews": week_events.len(),
            "again_count": again_count,
        },
        "redesign_candidates": redesign,
        "open_recommendations": open_recommendations,
        "curriculum_scenarios": pack.curriculum_catalog().scenarios,
        "lesson_result_contract": {
            "schema_version": tutor_contract::SCHEMA_VERSION,
            "endpoint": "/api/v1/import/lesson-result/validate",
        },
    }))
}

/// `GET /api/v1/dashboard?language_profile_id={id}` — one aggregated payload
/// (08-http-api.md §3): exceptions first; one round trip for the mobile
/// dashboard (12-testing.md §8 <500ms target).
#[allow(clippy::too_many_lines)]
pub async fn dashboard(req: &Request, env: &Env) -> Result<Response> {
    let url = req.url()?;
    let Some(profile_id) = query_param(&url, "language_profile_id") else {
        return error_response(400, "missing_param", "language_profile_id required");
    };
    let database = db(env)?;
    let Some(profile_row) = fetch_profile(&database, &profile_id).await? else {
        return error_response(404, "not_found", "unknown profile");
    };
    let Ok(profile) = profile_row.into_domain() else {
        return error_response(500, "corrupt_row", "stored profile is invalid");
    };
    let at = crate::api::now();
    let now_text = to_rfc3339(at);
    let day_start = to_rfc3339(learning_core::Timestamp(at.0 - at.0 % 86_400));

    async fn count(
        database: &worker::D1Database,
        sql: &str,
        params: &[worker::wasm_bindgen::JsValue],
    ) -> Result<u32> {
        #[derive(serde::Deserialize)]
        struct OneCount {
            n: u32,
        }
        Ok(database
            .prepare(sql)
            .bind(params)?
            .first::<OneCount>(None)
            .await?
            .map_or(0, |c| c.n))
    }

    let due_cards = count(
        &database,
        "SELECT COUNT(*) AS n FROM card_schedule_state s
         JOIN cards c ON c.id = s.card_id
         WHERE c.learner_language_profile_id = ?1
           AND c.status IN ('new','learning','review','relearning')
           AND s.due_at <= ?2",
        &[profile_id.clone().into(), now_text.clone().into()],
    )
    .await?;
    let active_errors = count(
        &database,
        "SELECT COUNT(*) AS n FROM error_targets
         WHERE learner_language_profile_id = ?1 AND status IN ('active','recheck')",
        &[profile_id.clone().into()],
    )
    .await?;
    let method_change = count(
        &database,
        "SELECT COUNT(*) AS n FROM error_targets
         WHERE learner_language_profile_id = ?1 AND method_change_required = 1
           AND status != 'resolved'",
        &[profile_id.clone().into()],
    )
    .await?;
    let due_rechecks = count(
        &database,
        "SELECT COUNT(*) AS n FROM error_rechecks r
         JOIN error_targets t ON t.id = r.error_target_id
         WHERE t.learner_language_profile_id = ?1
           AND r.completed_at IS NULL AND r.due_at <= ?2",
        &[profile_id.clone().into(), now_text.clone().into()],
    )
    .await?;
    let overdue = count(
        &database,
        crate::repositories::review::COUNT_OVERDUE,
        &[profile_id.clone().into(), now_text.clone().into()],
    )
    .await?;

    #[derive(serde::Deserialize)]
    struct EventRow {
        response_millis: u32,
    }
    let events: Vec<EventRow> = database
        .prepare(crate::repositories::review::SELECT_EVENTS_SINCE)
        .bind(&[profile_id.clone().into(), day_start.into()])?
        .all()
        .await?
        .results()?;
    let review_seconds_today: u64 = events
        .iter()
        .map(|e| u64::from(e.response_millis))
        .sum::<u64>()
        / 1000;

    #[derive(serde::Deserialize)]
    struct CountRow {
        status: String,
        n: u32,
    }
    let counts: Vec<CountRow> = database
        .prepare(
            "SELECT status, COUNT(*) AS n FROM learning_items
             WHERE learner_language_profile_id = ?1 GROUP BY status",
        )
        .bind(&[profile_id.clone().into()])?
        .all()
        .await?
        .results()?;
    let items: std::collections::BTreeMap<String, u32> =
        counts.into_iter().map(|c| (c.status, c.n)).collect();

    #[derive(serde::Deserialize)]
    struct GoalRow {
        next_goal: Option<String>,
    }
    let goal = database
        .prepare(
            "SELECT next_goal FROM lessons
             WHERE learner_language_profile_id = ?1 AND status = 'completed'
             ORDER BY lesson_number DESC LIMIT 1",
        )
        .bind(&[profile_id.clone().into()])?
        .first::<GoalRow>(None)
        .await?
        .and_then(|g| g.next_goal);

    let budget = crate::handlers::review::effective_budget(&database, &profile_id, at).await?;

    #[derive(serde::Deserialize)]
    struct RecRow {
        detail_json: String,
    }
    let rec_rows: Vec<RecRow> = database
        .prepare(crate::repositories::recommendations::SELECT_OPEN)
        .bind(&[profile_id.clone().into(), 5.into()])?
        .all()
        .await?
        .results()?;
    let open_recommendations: Vec<String> = rec_rows
        .iter()
        .filter_map(|r| {
            serde_json::from_str::<serde_json::Value>(&r.detail_json)
                .ok()
                .and_then(|v| v["detail"].as_str().map(str::to_owned))
        })
        .collect();

    // Exceptions first (09-frontend.md §6).
    let mut exceptions = Vec::new();
    if overdue > 0 {
        exceptions.push(format!("{overdue} overdue reviews — clear backlog first"));
    }
    if method_change > 0 {
        exceptions.push(format!(
            "{method_change} error targets need a method change"
        ));
    }
    if due_rechecks > 0 {
        exceptions.push(format!("{due_rechecks} error rechecks due"));
    }
    if budget == 0 {
        exceptions.push("new cards paused by backlog protection".to_owned());
    }

    Response::from_json(&shared_schema::DashboardDto {
        exceptions,
        due_cards,
        new_card_budget: budget,
        reviews_today: u32::try_from(events.len()).unwrap_or(u32::MAX),
        review_seconds_today: u32::try_from(review_seconds_today).unwrap_or(u32::MAX),
        active_error_targets: active_errors,
        due_error_rechecks: due_rechecks,
        next_lesson_number: profile.current_lesson_number + 1,
        next_lesson_goal: goal,
        items_by_status: items,
        open_recommendations,
    })
}
