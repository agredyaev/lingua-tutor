//! Lessons + lesson-result import (08-http-api.md §4, §7; WBS 4.5/4.7).
//! `complete` and `import/lesson-result/apply` share one code path.

use crate::api::{db, error_response, error_response_with, new_id, now, query_param};
use crate::handlers::profiles::fetch_profile;
use crate::repositories::{enum_str, lessons, records, to_rfc3339};
use application::{
    ErrorAction, ItemRef, LessonCompletionContext, LessonCompletionPlan, LessonOutcome,
    RECHECK_OFFSETS_DAYS, plan_lesson_completion,
};
use language_pack_api::NormalizationMode;
use learning_core::{LearnerLanguageProfile, LearningItemId, Timestamp};
use tutor_contract::{LessonResult, parse_lesson_result, validate};
use worker::{D1Database, Env, Request, Response, Result};

/// `POST /api/v1/lessons/start` — body `{ language_profile_id, scenario, planned_goal }`.
pub async fn start_lesson(
    registry: &language_pack_registry::LanguagePackRegistry,
    mut req: Request,
    env: &Env,
) -> Result<Response> {
    #[derive(serde::Deserialize)]
    struct Body {
        language_profile_id: String,
        scenario: String,
        #[serde(default)]
        planned_goal: String,
    }
    let Ok(body) = req.json::<Body>().await else {
        return error_response(
            400,
            "invalid_body",
            "expected language_profile_id + scenario",
        );
    };
    let database = db(env)?;
    let Some(profile_row) = fetch_profile(&database, &body.language_profile_id).await? else {
        return error_response(404, "not_found", "unknown profile");
    };
    let Ok(profile) = profile_row.into_domain() else {
        return error_response(500, "corrupt_row", "stored profile is invalid");
    };
    let pack_version = registry
        .get(&profile.language_pack_id.0)
        .map(|p| p.manifest().version.clone())
        .unwrap_or_else(|| "0.0.0".to_owned());
    let lesson_id = new_id("les");
    let at = to_rfc3339(now());
    database
        .prepare(lessons::INSERT_LESSON)
        .bind(&[
            lesson_id.clone().into(),
            profile.id.0.clone().into(),
            profile.language_pack_id.0.clone().into(),
            pack_version.into(),
            (profile.current_lesson_number + 1).into(),
            "active".into(),
            body.scenario.into(),
            body.planned_goal.into(),
            "pwa".into(),
            at.clone().into(),
            worker::wasm_bindgen::JsValue::NULL,
            worker::wasm_bindgen::JsValue::NULL,
            worker::wasm_bindgen::JsValue::NULL,
            worker::wasm_bindgen::JsValue::NULL,
            worker::wasm_bindgen::JsValue::NULL,
            at.into(),
        ])?
        .run()
        .await?;
    Response::from_json(&serde_json::json!({
        "id": lesson_id,
        "lesson_number": profile.current_lesson_number + 1
    }))
    .map(|r| r.with_status(201))
}

/// `GET /api/v1/lessons?language_profile_id={id}` — bounded list.
pub async fn list_lessons(req: &Request, env: &Env) -> Result<Response> {
    let url = req.url()?;
    let Some(profile_id) = query_param(&url, "language_profile_id") else {
        return error_response(400, "missing_param", "language_profile_id required");
    };
    let rows: Vec<lessons::LessonRow> = db(env)?
        .prepare(lessons::SELECT_LESSONS)
        .bind(&[profile_id.into(), 50.into()])?
        .all()
        .await?
        .results()?;
    let domain: Vec<_> = rows
        .into_iter()
        .filter_map(|r| r.into_domain().ok())
        .collect();
    Response::from_json(&domain)
}

/// `GET /api/v1/lessons/{lesson_id}`.
pub async fn get_lesson(env: &Env, lesson_id: &str) -> Result<Response> {
    let row = db(env)?
        .prepare(lessons::SELECT_LESSON)
        .bind(&[lesson_id.into()])?
        .first::<lessons::LessonRow>(None)
        .await?;
    match row.and_then(|r| r.into_domain().ok()) {
        Some(lesson) => Response::from_json(&lesson),
        None => error_response(404, "not_found", "unknown lesson"),
    }
}

/// `POST /api/v1/import/lesson-result/validate` — parse + validate + preview
/// + stateless validation token (08-http-api.md §7).
pub async fn validate_import(mut req: Request) -> Result<Response> {
    let text = req.text().await?;
    let (result, issues) = match parse_and_validate(&text) {
        Ok(pair) => pair,
        Err(response) => return response,
    };
    if !issues.is_empty() {
        return error_response_with(
            400,
            "validation_failed",
            "lesson result rejected",
            serde_json::json!({ "issues": issues }),
        );
    }
    let expires = now().0 + 15 * 60;
    Response::from_json(&serde_json::json!({
        "validation_token": validation_token(&text, expires),
        "expires_at": to_rfc3339(Timestamp(expires)),
        "preview": {
            "lesson_scenario": result.lesson.scenario,
            "new_learning_items": result.new_learning_items.len(),
            "evidence": result.evidence.len(),
            "error_occurrences": result.error_occurrences.len(),
            "listening_attempts": result.listening_attempts.len(),
            "card_mutations": result.card_mutations.len(),
        },
        "warnings": [],
    }))
}

/// `POST /api/v1/import/lesson-result/apply` — requires unexpired token from
/// the matching validate call; then the shared completion path.
pub async fn apply_import(mut req: Request, env: &Env) -> Result<Response> {
    let url = req.url()?;
    let Some(token) = query_param(&url, "validation_token") else {
        return error_response(
            409,
            "validation_required",
            "validate first, then apply with token",
        );
    };
    let text = req.text().await?;
    let Some(expires) = token_expiry_if_valid(&token, &text) else {
        return error_response(409, "validation_required", "token mismatch — re-validate");
    };
    if expires < now().0 {
        return error_response(409, "validation_expired", "token expired — re-validate");
    }
    let (result, issues) = match parse_and_validate(&text) {
        Ok(pair) => pair,
        Err(response) => return response,
    };
    if !issues.is_empty() {
        return error_response_with(
            400,
            "validation_failed",
            "lesson result rejected",
            serde_json::json!({ "issues": issues }),
        );
    }
    complete_from_result(env, &result).await
}

/// `POST /api/v1/lessons/{lesson_id}/complete` — body is the lesson-result
/// block; `lesson.lesson_id` must match the path.
pub async fn complete_lesson(mut req: Request, env: &Env, lesson_id: &str) -> Result<Response> {
    let text = req.text().await?;
    let (mut result, issues) = match parse_and_validate(&text) {
        Ok(pair) => pair,
        Err(response) => return response,
    };
    if !issues.is_empty() {
        return error_response_with(
            400,
            "validation_failed",
            "lesson result rejected",
            serde_json::json!({ "issues": issues }),
        );
    }
    result.lesson.lesson_id = Some(lesson_id.to_owned());
    complete_from_result(env, &result).await
}

fn parse_and_validate(
    text: &str,
) -> std::result::Result<(LessonResult, Vec<tutor_contract::ValidationIssue>), Result<Response>> {
    if text.len() > 256 * 1024 {
        return Err(error_response(413, "too_large", "body over 256 KB"));
    }
    match parse_lesson_result(text) {
        Ok(result) => {
            let issues = validate(&result);
            Ok((result, issues))
        }
        Err(e) => Err(error_response_with(
            400,
            "parse_error",
            "not a valid lesson-result document",
            serde_json::json!({ "detail": e.to_string() }),
        )),
    }
}

/// Stateless token: FNV over body + expiry. Owner-only API behind Cloudflare
/// Access (WBS 10.2); binds apply to the exact validated payload.
fn validation_token(body: &str, expires: i64) -> String {
    format!("vt_{}_{expires}", fnv(body.as_bytes(), expires))
}

fn token_expiry_if_valid(token: &str, body: &str) -> Option<i64> {
    let expires: i64 = token.rsplit('_').next()?.parse().ok()?;
    (validation_token(body, expires) == token).then_some(expires)
}

fn fnv(bytes: &[u8], seed: i64) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325 ^ (seed as u64);
    for b in bytes {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    format!("{hash:016x}")
}

/// Shared completion path: fetch context, plan, persist atomically.
async fn complete_from_result(env: &Env, result: &LessonResult) -> Result<Response> {
    let database = db(env)?;
    let Some(profile_row) = fetch_profile(&database, &result.language_profile_id).await? else {
        return error_response(404, "not_found", "unknown profile");
    };
    let Ok(profile) = profile_row.into_domain() else {
        return error_response(500, "corrupt_row", "stored profile is invalid");
    };
    let registry = match crate::built_in_language_packs() {
        Ok(registry) => registry,
        Err(e) => return error_response(500, "pack_registry", &e.to_string()),
    };
    let Some(pack) = registry.get(&profile.language_pack_id.0) else {
        return error_response(409, "pack_incompatible", "profile pack not registered");
    };
    let context = build_context(&database, result, profile, pack).await?;
    let plan = match plan_lesson_completion(result, &context) {
        Ok(plan) => plan,
        Err(e) => return error_response(422, "apply_rejected", &e.to_string()),
    };
    if plan.lesson == LessonOutcome::DuplicateNoop {
        return Response::from_json(&serde_json::json!({
            "status": "duplicate", "client_operation_id": result.client_operation_id
        }));
    }
    let mutation_ctx = build_mutation_context(&database, result, &context).await?;
    let mutations = application::plan_card_mutations(&result.card_mutations, &mutation_ctx);
    persist_plan(
        &database,
        result,
        &context,
        &plan,
        &mutations,
        &mutation_ctx,
    )
    .await
}

async fn build_context(
    database: &D1Database,
    result: &LessonResult,
    profile: LearnerLanguageProfile,
    pack: &dyn language_pack_api::LanguagePack,
) -> Result<LessonCompletionContext> {
    let duplicate = database
        .prepare(lessons::SELECT_LESSON_BY_KEY)
        .bind(&[result.client_operation_id.clone().into()])?
        .first::<lessons::LessonRow>(None)
        .await?;

    let existing_lesson = match &result.lesson.lesson_id {
        Some(id) => database
            .prepare(lessons::SELECT_LESSON)
            .bind(&[id.clone().into()])?
            .first::<lessons::LessonRow>(None)
            .await?
            .and_then(|r| r.into_domain().ok()),
        None => None,
    };

    let normalized: Vec<String> = result
        .new_learning_items
        .iter()
        .map(|i| {
            pack.normalize_target_text(&i.target_text, NormalizationMode::Canonical)
                .0
        })
        .collect();

    let mut items_by_normalized = Vec::new();
    for text in &normalized {
        let row = database
            .prepare(records::SELECT_ITEM_BY_NORMALIZED)
            .bind(&[profile.id.0.clone().into(), text.clone().into()])?
            .first::<records::ItemRow>(None)
            .await?;
        if let Some(item) = row.and_then(|r| r.into_domain().ok()) {
            items_by_normalized.push(item);
        }
    }

    let mut known_item_ids = Vec::new();
    let referenced: std::collections::BTreeSet<&str> = result
        .evidence
        .iter()
        .map(|e| e.learning_item_id.as_str())
        .chain(
            result
                .mastery_changes
                .iter()
                .map(|m| m.learning_item_id.as_str()),
        )
        .chain(
            result
                .listening_attempts
                .iter()
                .filter_map(|a| a.learning_item_id.as_deref()),
        )
        .collect();
    for id in referenced {
        let row = database
            .prepare(records::SELECT_ITEM)
            .bind(&[id.into()])?
            .first::<records::ItemRow>(None)
            .await?;
        if row.is_some() {
            known_item_ids.push(LearningItemId(id.to_owned()));
        }
    }

    let mut evidence_for_mastery = Vec::new();
    for change in &result.mastery_changes {
        let rows: Vec<records::EvidenceRow> = database
            .prepare(records::SELECT_EVIDENCE_FOR_ITEM)
            .bind(&[change.learning_item_id.clone().into()])?
            .all()
            .await?
            .results()?;
        let trail: Vec<_> = rows
            .into_iter()
            .filter_map(|r| r.into_domain().ok())
            .collect();
        evidence_for_mastery.push((LearningItemId(change.learning_item_id.clone()), trail));
    }

    let mut error_targets = Vec::new();
    for occurrence in &result.error_occurrences {
        if let Some(id) = &occurrence.error_target_id {
            let row = database
                .prepare(records::SELECT_ERROR_TARGET)
                .bind(&[id.clone().into()])?
                .first::<records::ErrorTargetRow>(None)
                .await?;
            if let Some(target) = row.and_then(|r| r.into_domain().ok()) {
                error_targets.push(target);
            }
        }
    }

    Ok(LessonCompletionContext {
        profile,
        now: now(),
        existing_lesson,
        duplicate_import: duplicate.is_some(),
        items_by_normalized,
        normalized_new_items: normalized,
        known_item_ids,
        error_targets,
        evidence_for_mastery,
    })
}

/// Card-mutation planning context: existing cards for duplicate detection +
/// target lookups, remaining activation budget (07 §4).
async fn build_mutation_context(
    database: &D1Database,
    result: &LessonResult,
    ctx: &LessonCompletionContext,
) -> Result<application::MutationContext> {
    use crate::repositories::cards as card_repo;
    let mut existing = Vec::new();
    for mutation in &result.card_mutations {
        if let Some(payload) = &mutation.card {
            let hash = learning_core::Card::content_hash_of(
                payload.intent,
                &payload.prompt,
                &payload.answer,
                &payload.accepted_answers,
            );
            let row = database
                .prepare(card_repo::select_card_by_hash_sql())
                .bind(&[ctx.profile.id.0.clone().into(), hash.into()])?
                .first::<card_repo::CardRow>(None)
                .await?;
            if let Some(card) = row.and_then(|r| r.into_domain().ok()) {
                existing.push(card);
            }
        }
        if let Some(card_id) = &mutation.card_id {
            let row = database
                .prepare(card_repo::select_card_sql())
                .bind(&[card_id.clone().into()])?
                .first::<card_repo::CardRow>(None)
                .await?;
            if let Some(card) = row.and_then(|r| r.into_domain().ok()) {
                existing.push(card);
            }
        }
    }
    #[derive(serde::Deserialize)]
    struct OneCount {
        n: u32,
    }
    let day_start = to_rfc3339(learning_core::Timestamp(ctx.now.0 - ctx.now.0 % 86_400));
    let activated_today = database
        .prepare(card_repo::COUNT_ACTIVATED_TODAY)
        .bind(&[ctx.profile.id.0.clone().into(), day_start.into()])?
        .first::<OneCount>(None)
        .await?
        .map_or(0, |c| c.n);
    #[allow(clippy::cast_possible_truncation)] // min() bounds to u8 range
    let remaining = 8_u8.saturating_sub(activated_today.min(255) as u8);
    Ok(application::MutationContext {
        existing_cards: existing,
        remaining_activation_budget: remaining,
        now: ctx.now,
    })
}

/// Builds the D1 batch for a plan and runs it (04-database.md §1: atomic).
async fn persist_plan(
    database: &D1Database,
    result: &LessonResult,
    ctx: &LessonCompletionContext,
    plan: &LessonCompletionPlan,
    mutations: &[application::PlannedMutation],
    mutation_ctx: &application::MutationContext,
) -> Result<Response> {
    let at = to_rfc3339(ctx.now);
    let mut statements = Vec::new();

    let lesson_id = match &plan.lesson {
        LessonOutcome::DuplicateNoop => unreachable!("handled by caller"),
        LessonOutcome::CompleteExisting(id) => {
            statements.push(
                database.prepare(lessons::COMPLETE_LESSON).bind(&[
                    id.0.clone().into(),
                    at.clone().into(),
                    result
                        .lesson
                        .duration_seconds
                        .map_or(worker::wasm_bindgen::JsValue::NULL, Into::into),
                    opt(&result.lesson.tutor_summary),
                    opt(&result.lesson.next_goal),
                ])?,
            );
            id.0.clone()
        }
        LessonOutcome::CreateCompleted { lesson_number } => {
            let id = new_id("les");
            statements.push(
                database.prepare(lessons::INSERT_LESSON).bind(&[
                    id.clone().into(),
                    ctx.profile.id.0.clone().into(),
                    ctx.profile.language_pack_id.0.clone().into(),
                    result.language_pack_version.clone().into(),
                    (*lesson_number).into(),
                    "completed".into(),
                    result.lesson.scenario.clone().into(),
                    String::new().into(),
                    "import".into(),
                    at.clone().into(),
                    at.clone().into(),
                    result
                        .lesson
                        .duration_seconds
                        .map_or(worker::wasm_bindgen::JsValue::NULL, Into::into),
                    opt(&result.lesson.tutor_summary),
                    opt(&result.lesson.next_goal),
                    result.client_operation_id.clone().into(),
                    at.clone().into(),
                ])?,
            );
            id
        }
    };

    if let Some(metrics) = &result.metrics {
        statements.push(
            database.prepare(lessons::INSERT_METRICS).bind(&[
                lesson_id.clone().into(),
                metrics.speaking_seconds.into(),
                metrics.unaided_recall_count.into(),
                metrics.aided_recall_count.into(),
                metrics
                    .listening_score
                    .map_or(worker::wasm_bindgen::JsValue::NULL, Into::into),
                metrics
                    .avg_answer_millis
                    .map_or(worker::wasm_bindgen::JsValue::NULL, Into::into),
                metrics.long_pause_count.into(),
                metrics
                    .confidence
                    .map_or(worker::wasm_bindgen::JsValue::NULL, Into::into),
                at.clone().into(),
            ])?,
        );
    }

    // New items: IDs assigned here; ItemRef::New(index) resolves via this map.
    let mut new_item_ids = Vec::with_capacity(plan.new_items.len());
    for planned in &plan.new_items {
        let source = &result.new_learning_items[planned.source_index];
        let id = new_id("li");
        statements.push(database.prepare(records::INSERT_ITEM).bind(&[
            id.clone().into(),
            ctx.profile.id.0.clone().into(),
            ctx.profile.language_pack_id.0.clone().into(),
            enum_str(&source.item_type).into(),
            source.target_text.clone().into(),
            planned.normalized_target_text.clone().into(),
            opt(&source.prompt_text),
            opt(&source.prompt_language),
            "introduced".into(),
            "{}".into(),
            lesson_id.clone().into(),
            at.clone().into(),
        ])?);
        new_item_ids.push(id);
    }
    let resolve = |item: &ItemRef| -> String {
        match item {
            ItemRef::Existing(id) => id.0.clone(),
            ItemRef::New(index) => new_item_ids.get(*index).cloned().unwrap_or_default(),
        }
    };

    for planned in &plan.evidence {
        let source = &result.evidence[planned.source_index];
        statements.push(database.prepare(records::INSERT_EVIDENCE).bind(&[
            new_id("ev").into(),
            resolve(&planned.item).into(),
            lesson_id.clone().into(),
            enum_str(&source.kind).into(),
            u8::from(source.success).into(),
            opt(&source.detail),
            worker::wasm_bindgen::JsValue::NULL,
            at.clone().into(),
        ])?);
    }

    for planned in &plan.listening {
        let source = &result.listening_attempts[planned.source_index];
        statements.push(
            database.prepare(records::INSERT_LISTENING_ATTEMPT).bind(&[
                new_id("la").into(),
                ctx.profile.id.0.clone().into(),
                planned
                    .item
                    .as_ref()
                    .map_or(worker::wasm_bindgen::JsValue::NULL, |i| resolve(i).into()),
                lesson_id.clone().into(),
                opt(&source.source_reference),
                opt(&source.transcript),
                u8::from(source.success).into(),
                at.clone().into(),
            ])?,
        );
    }

    for action in &plan.error_actions {
        push_error_statements(
            database,
            &mut statements,
            result,
            ctx,
            action,
            &lesson_id,
            &at,
        )?;
    }

    for (item_id, status) in &plan.item_status_changes {
        statements.push(database.prepare(records::UPDATE_ITEM_STATUS).bind(&[
            item_id.0.clone().into(),
            enum_str(status).into(),
            at.clone().into(),
        ])?);
    }

    // Lesson counter forward (07 §2: lesson numbering owned by profile).
    let advanced = matches!(plan.lesson, LessonOutcome::CreateCompleted { .. });
    if advanced {
        statements.push(
            database
                .prepare("UPDATE learner_language_profiles SET current_lesson_number = current_lesson_number + 1, updated_at = ?2 WHERE id = ?1")
                .bind(&[ctx.profile.id.0.clone().into(), at.clone().into()])?,
        );
    }

    let card_summary = push_card_mutation_statements(
        database,
        &mut statements,
        result,
        ctx,
        mutations,
        mutation_ctx,
        &lesson_id,
        &at,
    )?;

    database.batch(statements).await?;
    Response::from_json(&serde_json::json!({
        "lesson_id": lesson_id,
        "new_learning_items": new_item_ids,
        "reused_learning_items": plan.reused_items.iter().map(|(_, id)| id.0.clone()).collect::<Vec<_>>(),
        "card_mutations": card_summary,
        "warnings": plan.warnings,
    }))
}

#[allow(clippy::too_many_arguments)]
fn push_error_statements(
    database: &D1Database,
    statements: &mut Vec<worker::D1PreparedStatement>,
    result: &LessonResult,
    ctx: &LessonCompletionContext,
    action: &ErrorAction,
    lesson_id: &str,
    at: &str,
) -> Result<()> {
    match action {
        ErrorAction::NewTarget { source_index } => {
            let source = &result.error_occurrences[*source_index];
            let target_id = new_id("et");
            statements.push(database.prepare(records::INSERT_ERROR_TARGET).bind(&[
                target_id.clone().into(),
                ctx.profile.id.0.clone().into(),
                ctx.profile.language_pack_id.0.clone().into(),
                result.language_pack_version.clone().into(),
                enum_str(&source.core_category).into(),
                opt(&source.pack_error_code),
                source.original_production.clone().into(),
                source.corrected_production.clone().into(),
                opt(&source.explanation),
                1.into(),
                0.into(),
                "active".into(),
                at.into(),
            ])?);
            statements.push(database.prepare(records::INSERT_ERROR_OCCURRENCE).bind(&[
                new_id("eo").into(),
                target_id.clone().into(),
                lesson_id.into(),
                worker::wasm_bindgen::JsValue::NULL,
                at.into(),
            ])?);
            push_rechecks(database, statements, &target_id, ctx)?;
        }
        ErrorAction::Register {
            source_index,
            target_id,
            new_count,
            method_change,
            ..
        } => {
            let _ = source_index;
            statements.push(database.prepare(records::UPDATE_ERROR_TARGET).bind(&[
                target_id.0.clone().into(),
                (*new_count).into(),
                u8::from(*method_change).into(),
                "recheck".into(),
                at.into(),
            ])?);
            statements.push(database.prepare(records::INSERT_ERROR_OCCURRENCE).bind(&[
                new_id("eo").into(),
                target_id.0.clone().into(),
                lesson_id.into(),
                worker::wasm_bindgen::JsValue::NULL,
                at.into(),
            ])?);
            push_rechecks(database, statements, &target_id.0, ctx)?;
        }
    }
    Ok(())
}

fn push_rechecks(
    database: &D1Database,
    statements: &mut Vec<worker::D1PreparedStatement>,
    target_id: &str,
    ctx: &LessonCompletionContext,
) -> Result<()> {
    for offset in RECHECK_OFFSETS_DAYS {
        let due = Timestamp(ctx.now.0 + i64::from(offset) * 86_400);
        statements.push(database.prepare(records::INSERT_ERROR_RECHECK).bind(&[
            new_id("rc").into(),
            target_id.into(),
            offset.into(),
            to_rfc3339(due).into(),
        ])?);
    }
    Ok(())
}

fn opt(value: &Option<String>) -> worker::wasm_bindgen::JsValue {
    value
        .as_ref()
        .map_or(worker::wasm_bindgen::JsValue::NULL, |v| v.clone().into())
}

/// Card mutations → statements: card rows + tags + one audit row per attempt
/// (07 §3: applied or not). Returns per-mutation outcome for the response.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn push_card_mutation_statements(
    database: &D1Database,
    statements: &mut Vec<worker::D1PreparedStatement>,
    result: &LessonResult,
    ctx: &LessonCompletionContext,
    mutations: &[application::PlannedMutation],
    mutation_ctx: &application::MutationContext,
    lesson_id: &str,
    at: &str,
) -> Result<Vec<serde_json::Value>> {
    use crate::repositories::cards as card_repo;
    use application::MutationEffect;

    let mut summary = Vec::with_capacity(mutations.len());
    for planned in mutations {
        let source = &result.card_mutations[planned.source_index];
        let mut created_card_id: Option<String> = None;

        match &planned.effect {
            Some(MutationEffect::CreateCard {
                source_index,
                initial_status,
            }) => {
                let payload = result.card_mutations[*source_index]
                    .card
                    .as_ref()
                    .ok_or_else(|| worker::Error::RustError("create without payload".into()))?;
                let card_id = new_id("crd");
                let hash = learning_core::Card::content_hash_of(
                    payload.intent,
                    &payload.prompt,
                    &payload.answer,
                    &payload.accepted_answers,
                );
                statements.push(
                    database.prepare(card_repo::INSERT_CARD).bind(&[
                        card_id.clone().into(),
                        ctx.profile.id.0.clone().into(),
                        ctx.profile.language_pack_id.0.clone().into(),
                        result.language_pack_version.clone().into(),
                        enum_str(&payload.intent).into(),
                        enum_str(initial_status).into(),
                        payload.prompt.clone().into(),
                        payload
                            .prompt_language
                            .clone()
                            .unwrap_or_else(|| ctx.profile.source_language.0.clone())
                            .into(),
                        payload.answer.clone().into(),
                        ctx.profile.target_locale.0.clone().into(),
                        serde_json::to_string(&payload.accepted_answers)
                            .unwrap_or_else(|_| "[]".to_owned())
                            .into(),
                        "exact".into(),
                        opt(&payload.explanation),
                        worker::wasm_bindgen::JsValue::NULL,
                        opt(&payload.source_learning_item_id),
                        opt(&payload.source_error_target_id),
                        lesson_id.into(),
                        payload.creation_reason.clone().into(),
                        payload
                            .priority_score
                            .map_or(worker::wasm_bindgen::JsValue::NULL, Into::into),
                        hash.into(),
                        1.into(),
                        worker::wasm_bindgen::JsValue::NULL,
                        worker::wasm_bindgen::JsValue::NULL,
                        "tutor".into(),
                        at.into(),
                    ])?,
                );
                for (tag_type, tags) in [
                    ("scenario", &payload.scenario_tags),
                    ("skill", &payload.skill_tags),
                ] {
                    for tag in tags {
                        statements.push(database.prepare(card_repo::INSERT_CARD_TAG).bind(&[
                            card_id.clone().into(),
                            tag_type.into(),
                            tag.trim().to_lowercase().into(),
                        ])?);
                    }
                }
                if *initial_status == learning_core::CardStatus::New {
                    // Activation persists the initial schedule state so the
                    // first queue fetch and grade share one stable hash.
                    use review_engine::ReviewScheduler as _;
                    let scheduler = review_engine::FsrsScheduler::new()
                        .map_err(|e| worker::Error::RustError(e.to_string()))?;
                    let state = scheduler
                        .initial_state(review_engine::InitialReviewInput {
                            now: review_engine::Timestamp(ctx.now.0),
                        })
                        .map_err(|e| worker::Error::RustError(e.to_string()))?;
                    let row =
                        crate::repositories::review::ScheduleStateRow::from_state(&card_id, &state);
                    statements.push(
                        database
                            .prepare(crate::repositories::review::UPSERT_SCHEDULE_STATE)
                            .bind(&[
                                row.card_id.clone().into(),
                                row.algorithm.clone().into(),
                                row.algorithm_version.clone().into(),
                                worker::wasm_bindgen::JsValue::NULL,
                                worker::wasm_bindgen::JsValue::NULL,
                                worker::wasm_bindgen::JsValue::NULL,
                                worker::wasm_bindgen::JsValue::NULL,
                                row.due_at
                                    .clone()
                                    .map_or(worker::wasm_bindgen::JsValue::NULL, Into::into),
                                worker::wasm_bindgen::JsValue::NULL,
                                0.into(),
                                0.into(),
                                row.state_version.into(),
                                row.state_hash.clone().into(),
                                at.into(),
                            ])?,
                    );
                }
                created_card_id = Some(card_id);
            }
            Some(MutationEffect::UpdateContent {
                card_id,
                source_index,
            }) => {
                let payload = result.card_mutations[*source_index]
                    .card
                    .as_ref()
                    .ok_or_else(|| worker::Error::RustError("update without payload".into()))?;
                let hash = learning_core::Card::content_hash_of(
                    payload.intent,
                    &payload.prompt,
                    &payload.answer,
                    &payload.accepted_answers,
                );
                statements.push(
                    database.prepare(card_repo::UPDATE_CARD_CONTENT).bind(&[
                        card_id.0.clone().into(),
                        payload.prompt.clone().into(),
                        opt(&payload.explanation),
                        worker::wasm_bindgen::JsValue::NULL,
                        serde_json::to_string(&payload.accepted_answers)
                            .unwrap_or_else(|_| "[]".to_owned())
                            .into(),
                        hash.into(),
                        at.into(),
                    ])?,
                );
            }
            Some(MutationEffect::SetStatus {
                card_id,
                status,
                reason,
            }) => {
                statements.push(database.prepare(card_repo::UPDATE_CARD_STATUS).bind(&[
                    card_id.0.clone().into(),
                    enum_str(status).into(),
                    opt(reason),
                    at.into(),
                ])?);
            }
            Some(
                MutationEffect::AddAcceptedAnswer { card_id, answer }
                | MutationEffect::RemoveAcceptedAnswer { card_id, answer },
            ) => {
                let target = mutation_ctx
                    .existing_cards
                    .iter()
                    .find(|c| &c.id == card_id);
                if let Some(target) = target {
                    let mut accepted = target.accepted_answers.clone();
                    let adding = matches!(
                        planned.effect,
                        Some(MutationEffect::AddAcceptedAnswer { .. })
                    );
                    if adding {
                        accepted.push(answer.clone());
                    } else {
                        accepted
                            .retain(|a| a.trim().to_lowercase() != answer.trim().to_lowercase());
                    }
                    let hash = learning_core::Card::content_hash_of(
                        target.intent,
                        &target.prompt,
                        &target.answer,
                        &accepted,
                    );
                    statements.push(
                        database
                            .prepare(card_repo::UPDATE_ACCEPTED_ANSWERS)
                            .bind(&[
                                card_id.0.clone().into(),
                                serde_json::to_string(&accepted)
                                    .unwrap_or_else(|_| "[]".to_owned())
                                    .into(),
                                hash.into(),
                                at.into(),
                            ])?,
                    );
                }
            }
            None => {}
        }

        // Audit row for every attempt (idempotent on client_operation_id).
        let audit_card_id = created_card_id.clone().or_else(|| source.card_id.clone());
        statements.push(
            database.prepare(card_repo::INSERT_CARD_MUTATION).bind(&[
                new_id("cm").into(),
                audit_card_id
                    .clone()
                    .map_or(worker::wasm_bindgen::JsValue::NULL, Into::into),
                enum_str(&source.operation).into(),
                source.client_operation_id.clone().into(),
                worker::wasm_bindgen::JsValue::NULL,
                source
                    .card
                    .as_ref()
                    .and_then(|c| serde_json::to_string(c).ok())
                    .map_or(worker::wasm_bindgen::JsValue::NULL, Into::into),
                lesson_id.into(),
                "tutor".into(),
                planned
                    .note
                    .clone()
                    .or_else(|| source.reason.clone())
                    .unwrap_or_else(|| "lesson result".to_owned())
                    .into(),
                ctx.profile.language_pack_id.0.clone().into(),
                result.language_pack_version.clone().into(),
                match planned.verdict {
                    application::Verdict::Applied => "applied",
                    application::Verdict::Rejected => "rejected",
                    application::Verdict::Warned => "warned",
                }
                .into(),
                at.into(),
            ])?,
        );

        summary.push(serde_json::json!({
            "client_operation_id": source.client_operation_id,
            "verdict": planned.verdict,
            "note": planned.note,
            "card_id": audit_card_id,
        }));
    }
    Ok(summary)
}
