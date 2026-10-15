//! Lessons + lesson-result import.
//! Transport parsing is thin; lesson orchestration operates directly on one
//! request-local SQLite connection/transaction.

use crate::api::{new_id, now, operation_error, operation_error_with};
use crate::handlers::profiles::fetch_profile;
use crate::repositories::{enum_str, lessons, records, to_rfc3339};
use crate::{Env, Result};
use application::{
    ErrorAction, ItemRef, LessonCompletionContext, LessonCompletionPlan, LessonOutcome,
    RECHECK_OFFSETS_DAYS, plan_lesson_completion,
};
use language_pack_api::NormalizationMode;
use learning_core::{LearnerLanguageProfile, LearningItemId, Timestamp};
use rusqlite::{OptionalExtension as _, TransactionBehavior, params};
use tutor_contract::{LessonResult, parse_lesson_result, validate};

pub(crate) fn start_lesson_operation(
    registry: &language_pack_registry::LanguagePackRegistry,
    env: &Env,
    language_profile_id: &str,
    scenario: &str,
    planned_goal: &str,
    source: &str,
) -> Result<serde_json::Value> {
    let mut database = env.connection()?;
    let transaction = database.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let Some(profile_row) = fetch_profile(&transaction, language_profile_id)? else {
        return operation_error(404, "not_found", "unknown profile");
    };
    let Ok(profile) = profile_row.into_domain() else {
        return operation_error(500, "corrupt_row", "stored profile is invalid");
    };
    let pack_version = registry
        .get(&profile.language_pack_id.0)
        .map(|pack| pack.manifest().version.clone())
        .unwrap_or_else(|| "0.0.0".to_owned());
    let lesson_id = new_id("les");
    let at = to_rfc3339(now());
    transaction.execute(
        lessons::INSERT_LESSON,
        params![
            lesson_id,
            profile.id.0,
            profile.language_pack_id.0,
            pack_version,
            profile.current_lesson_number + 1,
            "active",
            scenario,
            planned_goal,
            source,
            at,
            Option::<String>::None,
            Option::<u32>::None,
            Option::<String>::None,
            Option::<String>::None,
            Option::<String>::None,
            at,
        ],
    )?;

    let (planner_input, adaptive_plan) =
        crate::handlers::records::adaptive_lesson_planning(&transaction, &profile)?;
    let capability_state =
        crate::handlers::records::item_capability_states(&transaction, &profile.id.0)?;
    let task_candidates = adaptation::score_task_candidates(&capability_state, &adaptive_plan, 20);
    let focus_threads = adaptation::build_tutor_focus_threads(&capability_state, now().0, 30);
    transaction.execute(
        "INSERT INTO planner_decisions
         (id, learner_language_profile_id, lesson_id, planner_version,
          input_json, plan_json, task_candidates_json, capability_state_json,
          focus_threads_json, outcome_json, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, NULL, ?10, ?10)",
        params![
            new_id("pd"),
            profile.id.0,
            lesson_id,
            "planner-v3",
            serde_json::to_string(&planner_input)?,
            serde_json::to_string(&adaptive_plan)?,
            serde_json::to_string(&task_candidates)?,
            serde_json::to_string(&capability_state)?,
            serde_json::to_string(&focus_threads)?,
            at,
        ],
    )?;
    transaction.commit()?;
    crate::json(&serde_json::json!({
        "id": lesson_id,
        "lesson_number": profile.current_lesson_number + 1
    }))
}

pub(crate) fn list_lessons_for(env: &Env, profile_id: &str) -> Result<serde_json::Value> {
    let database = env.connection()?;
    let rows: Vec<lessons::LessonRow> = {
        let mut statement = database.prepare(lessons::SELECT_LESSONS)?;
        statement
            .query_map(params![profile_id, 50_u32], lessons::LessonRow::from_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };
    let domain: Vec<_> = rows
        .into_iter()
        .filter_map(|row| row.into_domain().ok())
        .collect();
    crate::json(&domain)
}

pub(crate) fn get_lesson(env: &Env, lesson_id: &str) -> Result<serde_json::Value> {
    let database = env.connection()?;
    let row = database
        .query_row(
            lessons::SELECT_LESSON,
            params![lesson_id],
            lessons::LessonRow::from_row,
        )
        .optional()?;
    match row.and_then(|row| row.into_domain().ok()) {
        Some(lesson) => crate::json(&lesson),
        None => operation_error(404, "not_found", "unknown lesson"),
    }
}

pub(crate) fn validate_import_text(text: &str, env: &Env) -> Result<serde_json::Value> {
    let result = parse_lesson_result_text(text)?;
    let issues = validate(&result);
    if !issues.is_empty() {
        return operation_error_with(
            400,
            "validation_failed",
            "lesson result rejected",
            serde_json::json!({ "issues": issues }),
        );
    }
    let expires = now().0 + 15 * 60;
    let key = env.signing_key();
    crate::json(&serde_json::json!({
        "validation_token": validation_token(key, text, expires),
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

pub(crate) fn apply_import_text(
    registry: &language_pack_registry::LanguagePackRegistry,
    text: &str,
    token: &str,
    env: &Env,
) -> Result<serde_json::Value> {
    let Some(expires) = token_expiry_if_valid(env.signing_key(), token, text) else {
        return operation_error(409, "validation_required", "token mismatch — re-validate");
    };
    if expires < now().0 {
        return operation_error(409, "validation_expired", "token expired — re-validate");
    }
    let result = validate_lesson_result(parse_lesson_result_text(text)?)?;
    complete_from_result(registry, env, &result)
}

pub(crate) fn complete_lesson_result(
    registry: &language_pack_registry::LanguagePackRegistry,
    env: &Env,
    lesson_id: &str,
    mut result: LessonResult,
) -> Result<serde_json::Value> {
    result = validate_lesson_result(result)?;
    result.lesson.lesson_id = Some(lesson_id.to_owned());
    complete_from_result(registry, env, &result)
}

pub(crate) fn parse_lesson_result_text(text: &str) -> Result<LessonResult> {
    if text.len() > 256 * 1024 {
        return operation_error(413, "too_large", "body over 256 KB");
    }
    parse_lesson_result(text).map_err(|error| {
        crate::Error::operation(
            400,
            "parse_error",
            "not a valid lesson-result document",
            serde_json::json!({ "detail": error.to_string() }),
        )
    })
}

pub(crate) fn validate_lesson_result(result: LessonResult) -> Result<LessonResult> {
    let issues = validate(&result);
    if !issues.is_empty() {
        return operation_error_with(
            400,
            "validation_failed",
            "lesson result rejected",
            serde_json::json!({ "issues": issues }),
        );
    }
    Ok(result)
}

fn validation_token(key: &str, body: &str, expires: i64) -> String {
    use hmac::Mac as _;
    #[allow(clippy::expect_used)]
    let mut mac = hmac::Hmac::<sha2::Sha256>::new_from_slice(key.as_bytes())
        .expect("HMAC accepts any key length");
    mac.update(body.as_bytes());
    mac.update(&expires.to_be_bytes());
    format!("vt_{}_{expires}", hex::encode(mac.finalize().into_bytes()))
}

fn token_expiry_if_valid(key: &str, token: &str, body: &str) -> Option<i64> {
    let expires: i64 = token.rsplit('_').next()?.parse().ok()?;
    let expected = validation_token(key, body, expires);
    let mismatch = expected
        .as_bytes()
        .iter()
        .zip(token.as_bytes())
        .fold(expected.len() != token.len(), |acc, (a, b)| acc | (a != b));
    (!mismatch).then_some(expires)
}

pub(crate) fn complete_from_result(
    registry: &language_pack_registry::LanguagePackRegistry,
    env: &Env,
    result: &LessonResult,
) -> Result<serde_json::Value> {
    let mut database = env.connection()?;
    let transaction = database.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let Some(profile_row) = fetch_profile(&transaction, &result.language_profile_id)? else {
        return operation_error(404, "not_found", "unknown profile");
    };
    let Ok(profile) = profile_row.into_domain() else {
        return operation_error(500, "corrupt_row", "stored profile is invalid");
    };
    let Some(pack) = registry.get(&profile.language_pack_id.0) else {
        return operation_error(409, "pack_incompatible", "profile pack not registered");
    };
    let context = build_context(&transaction, result, profile, pack)?;
    let plan = match plan_lesson_completion(result, &context) {
        Ok(plan) => plan,
        Err(error) => return operation_error(422, "apply_rejected", &error.to_string()),
    };
    if plan.lesson == LessonOutcome::DuplicateNoop {
        return crate::json(&serde_json::json!({
            "status": "duplicate",
            "client_operation_id": result.client_operation_id
        }));
    }
    let mutation_ctx = build_mutation_context(&transaction, &result.card_mutations, &context)?;
    let mutations = application::plan_card_mutations(&result.card_mutations, &mutation_ctx);
    let response = persist_plan(
        &transaction,
        result,
        &context,
        &plan,
        &mutations,
        &mutation_ctx,
    )?;
    transaction.commit()?;
    Ok(response)
}

fn build_context(
    database: &rusqlite::Connection,
    result: &LessonResult,
    profile: LearnerLanguageProfile,
    pack: &dyn language_pack_api::LanguagePack,
) -> Result<LessonCompletionContext> {
    let duplicate = database
        .query_row(
            lessons::SELECT_LESSON_BY_KEY,
            params![result.client_operation_id],
            lessons::LessonRow::from_row,
        )
        .optional()?;

    let existing_lesson = match &result.lesson.lesson_id {
        Some(id) => database
            .query_row(
                lessons::SELECT_LESSON,
                params![id],
                lessons::LessonRow::from_row,
            )
            .optional()?
            .and_then(|row| row.into_domain().ok()),
        None => database
            .query_row(
                lessons::SELECT_LATEST_OPEN_LESSON,
                params![profile.id.0],
                lessons::LessonRow::from_row,
            )
            .optional()?
            .and_then(|row| row.into_domain().ok()),
    };

    let normalized: Vec<String> = result
        .new_learning_items
        .iter()
        .map(|item| {
            pack.normalize_target_text(&item.target_text, NormalizationMode::Canonical)
                .0
        })
        .collect();

    let mut items_by_normalized = Vec::new();
    for text in &normalized {
        let row = database
            .query_row(
                records::SELECT_ITEM_BY_NORMALIZED,
                params![profile.id.0, text],
                records::ItemRow::from_row,
            )
            .optional()?;
        if let Some(item) = row.and_then(|row| row.into_domain().ok()) {
            items_by_normalized.push(item);
        }
    }

    let mut known_item_ids = Vec::new();
    let referenced: std::collections::BTreeSet<&str> = result
        .evidence
        .iter()
        .filter_map(|evidence| evidence.learning_item_id.as_deref())
        .chain(
            result
                .mastery_changes
                .iter()
                .map(|change| change.learning_item_id.as_str()),
        )
        .chain(
            result
                .listening_attempts
                .iter()
                .filter_map(|attempt| attempt.learning_item_id.as_deref()),
        )
        .collect();
    for id in referenced {
        let exists = database
            .query_row(records::SELECT_ITEM, params![id], |_| Ok(()))
            .optional()?
            .is_some();
        if exists {
            known_item_ids.push(LearningItemId(id.to_owned()));
        }
    }

    let mut evidence_for_mastery = Vec::new();
    for change in &result.mastery_changes {
        let rows: Vec<records::EvidenceRow> = {
            let mut statement = database.prepare(records::SELECT_EVIDENCE_FOR_ITEM)?;
            statement
                .query_map(
                    params![change.learning_item_id],
                    records::EvidenceRow::from_row,
                )?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        let trail = rows
            .into_iter()
            .filter_map(|row| row.into_domain().ok())
            .collect();
        evidence_for_mastery.push((LearningItemId(change.learning_item_id.clone()), trail));
    }

    let mut error_targets = Vec::new();
    for occurrence in &result.error_occurrences {
        if let Some(id) = &occurrence.error_target_id {
            let row = database
                .query_row(
                    records::SELECT_ERROR_TARGET,
                    params![id],
                    records::ErrorTargetRow::from_row,
                )
                .optional()?;
            if let Some(target) = row.and_then(|row| row.into_domain().ok()) {
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

pub(crate) fn build_mutation_context(
    database: &rusqlite::Connection,
    mutations: &[tutor_contract::CardMutation],
    ctx: &LessonCompletionContext,
) -> Result<application::MutationContext> {
    use crate::repositories::cards as card_repo;
    let mut existing = Vec::new();
    for mutation in mutations {
        if let Some(payload) = &mutation.card {
            let hash = learning_core::Card::content_hash_of(
                payload.intent,
                &payload.prompt,
                &payload.answer,
                &payload.accepted_answers,
            );
            let sql = card_repo::select_card_by_hash_sql();
            let row = database
                .query_row(
                    &sql,
                    params![ctx.profile.id.0, hash],
                    card_repo::CardRow::from_row,
                )
                .optional()?;
            if let Some(card) = row.and_then(|row| row.into_domain().ok()) {
                existing.push(card);
            }
        }
        if let Some(card_id) = &mutation.card_id {
            let sql = card_repo::select_card_sql();
            let row = database
                .query_row(&sql, params![card_id], card_repo::CardRow::from_row)
                .optional()?;
            if let Some(card) = row.and_then(|row| row.into_domain().ok()) {
                existing.push(card);
            }
        }
    }
    let day_start = to_rfc3339(learning_core::Timestamp(ctx.now.0 - ctx.now.0 % 86_400));
    let activated_today: u32 = database.query_row(
        card_repo::COUNT_ACTIVATED_TODAY,
        params![ctx.profile.id.0, day_start],
        |row| row.get(0),
    )?;
    #[allow(clippy::cast_possible_truncation)]
    let remaining = 8_u8.saturating_sub(activated_today.min(255) as u8);
    Ok(application::MutationContext {
        existing_cards: existing,
        remaining_activation_budget: remaining,
        now: ctx.now,
    })
}

fn persist_plan(
    database: &rusqlite::Connection,
    result: &LessonResult,
    ctx: &LessonCompletionContext,
    plan: &LessonCompletionPlan,
    mutations: &[application::PlannedMutation],
    mutation_ctx: &application::MutationContext,
) -> Result<serde_json::Value> {
    let at = to_rfc3339(ctx.now);
    let lesson_id = match &plan.lesson {
        LessonOutcome::DuplicateNoop => unreachable!("handled by caller"),
        LessonOutcome::CompleteExisting(id) => {
            database.execute(
                lessons::COMPLETE_LESSON,
                params![
                    id.0,
                    at,
                    result.lesson.duration_seconds,
                    result.lesson.tutor_summary,
                    result.lesson.next_goal,
                ],
            )?;
            id.0.clone()
        }
        LessonOutcome::CreateCompleted { lesson_number } => {
            let id = new_id("les");
            database.execute(
                lessons::INSERT_LESSON,
                params![
                    id,
                    ctx.profile.id.0,
                    ctx.profile.language_pack_id.0,
                    result.language_pack_version,
                    lesson_number,
                    "completed",
                    result.lesson.scenario,
                    "",
                    "import",
                    at,
                    at,
                    result.lesson.duration_seconds,
                    result.lesson.tutor_summary,
                    result.lesson.next_goal,
                    result.client_operation_id,
                    at,
                ],
            )?;
            id
        }
    };

    if let Some(metrics) = &result.metrics {
        database.execute(
            lessons::INSERT_METRICS,
            params![
                lesson_id,
                metrics.speaking_seconds,
                metrics.unaided_recall_count,
                metrics.aided_recall_count,
                metrics.listening_score,
                metrics.avg_answer_millis,
                metrics.long_pause_count,
                metrics.confidence,
                at,
            ],
        )?;
    }

    let mut new_item_ids = Vec::with_capacity(plan.new_items.len());
    for planned in &plan.new_items {
        let source = &result.new_learning_items[planned.source_index];
        let id = new_id("li");
        database.execute(
            records::INSERT_ITEM,
            params![
                id,
                ctx.profile.id.0,
                ctx.profile.language_pack_id.0,
                enum_str(&source.item_type),
                source.target_text,
                planned.normalized_target_text,
                source.prompt_text,
                source.prompt_language,
                "introduced",
                "{}",
                lesson_id,
                at,
            ],
        )?;
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
        database.execute(
            records::INSERT_EVIDENCE,
            params![
                new_id("ev"),
                resolve(&planned.item),
                lesson_id,
                enum_str(&source.kind),
                u8::from(source.success),
                source.detail,
                serde_json::json!({
                    "context_key": source
                        .context
                        .as_deref()
                        .unwrap_or(result.lesson.scenario.as_str()),
                    "context_family": source.context_family,
                    "assistance_level": source.assistance_level,
                    "response_millis": source.response_millis,
                    "probe_phase": source.probe_phase
                })
                .to_string(),
                at,
            ],
        )?;
    }

    for planned in &plan.listening {
        let source = &result.listening_attempts[planned.source_index];
        let learning_item_id = planned.item.as_ref().map(&resolve);
        database.execute(
            records::INSERT_LISTENING_ATTEMPT,
            params![
                new_id("la"),
                ctx.profile.id.0,
                learning_item_id,
                lesson_id,
                source.source_reference,
                source.transcript,
                u8::from(source.success),
                at,
            ],
        )?;
    }

    for action in &plan.error_actions {
        apply_error_action(database, result, ctx, action, &lesson_id, &at)?;
    }

    for (item_id, status) in &plan.item_status_changes {
        database.execute(
            records::UPDATE_ITEM_STATUS,
            params![item_id.0, enum_str(status), at],
        )?;
    }

    if matches!(plan.lesson, LessonOutcome::CreateCompleted { .. }) {
        database.execute(
            "UPDATE learner_language_profiles
             SET current_lesson_number = current_lesson_number + 1, updated_at = ?2
             WHERE id = ?1",
            params![ctx.profile.id.0, at],
        )?;
    }

    let card_summary = apply_card_mutations(
        database,
        &result.card_mutations,
        &result.language_pack_version,
        ctx,
        mutations,
        mutation_ctx,
        Some(&lesson_id),
        &at,
    )?;

    let outcome = serde_json::json!({
        "scenario": result.lesson.scenario,
        "metrics": result.metrics,
        "evidence": result.evidence,
        "error_occurrences": result.error_occurrences,
        "mastery_changes": result.mastery_changes,
        "next_goal": result.lesson.next_goal,
    });
    database.execute(
        "UPDATE planner_decisions
         SET outcome_json = ?2, updated_at = ?3
         WHERE lesson_id = ?1",
        params![lesson_id, outcome.to_string(), at],
    )?;

    let escalations: Vec<adaptation::Recommendation> = plan
        .error_actions
        .iter()
        .flat_map(|action| match action {
            ErrorAction::Register {
                target_id,
                new_count,
                ..
            } => adaptation::error_escalation(&target_id.0, *new_count),
            ErrorAction::NewTarget { .. } => Vec::new(),
        })
        .collect();
    crate::handlers::recommendations::insert_all(database, &ctx.profile.id.0, &escalations)?;

    crate::json(&serde_json::json!({
        "lesson_id": lesson_id,
        "new_learning_items": new_item_ids,
        "reused_learning_items": plan
            .reused_items
            .iter()
            .map(|(_, id)| id.0.clone())
            .collect::<Vec<_>>(),
        "card_mutations": card_summary,
        "warnings": plan.warnings,
    }))
}

fn apply_error_action(
    database: &rusqlite::Connection,
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
            database.execute(
                records::INSERT_ERROR_TARGET,
                params![
                    target_id,
                    ctx.profile.id.0,
                    ctx.profile.language_pack_id.0,
                    result.language_pack_version,
                    enum_str(&source.core_category),
                    source.pack_error_code,
                    source.original_production,
                    source.corrected_production,
                    source.explanation,
                    1_u32,
                    0_u8,
                    "active",
                    at,
                ],
            )?;
            database.execute(
                records::INSERT_ERROR_OCCURRENCE,
                params![
                    new_id("eo"),
                    target_id,
                    lesson_id,
                    Option::<String>::None,
                    at
                ],
            )?;
            insert_rechecks(database, &target_id, ctx.now)?;
        }
        ErrorAction::Register {
            target_id,
            new_count,
            method_change,
            ..
        } => {
            database.execute(
                records::UPDATE_ERROR_TARGET,
                params![
                    target_id.0,
                    new_count,
                    u8::from(*method_change),
                    "recheck",
                    at,
                ],
            )?;
            database.execute(
                records::INSERT_ERROR_OCCURRENCE,
                params![
                    new_id("eo"),
                    target_id.0,
                    lesson_id,
                    Option::<String>::None,
                    at,
                ],
            )?;
            insert_rechecks(database, &target_id.0, ctx.now)?;
        }
    }
    Ok(())
}

pub(crate) fn insert_rechecks(
    database: &rusqlite::Connection,
    target_id: &str,
    now_ts: Timestamp,
) -> Result<()> {
    for offset in RECHECK_OFFSETS_DAYS {
        let due = Timestamp(now_ts.0 + i64::from(offset) * 86_400);
        database.execute(
            records::INSERT_ERROR_RECHECK,
            params![new_id("rc"), target_id, offset, to_rfc3339(due)],
        )?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub(crate) fn apply_card_mutations(
    database: &rusqlite::Connection,
    mutations_src: &[tutor_contract::CardMutation],
    pack_version: &str,
    ctx: &LessonCompletionContext,
    mutations: &[application::PlannedMutation],
    mutation_ctx: &application::MutationContext,
    lesson_id: Option<&str>,
    at: &str,
) -> Result<Vec<serde_json::Value>> {
    use crate::repositories::cards as card_repo;
    use application::MutationEffect;

    let mut summary = Vec::with_capacity(mutations.len());
    for planned in mutations {
        let source = &mutations_src[planned.source_index];
        let mut created_card_id: Option<String> = None;

        match &planned.effect {
            Some(MutationEffect::CreateCard {
                source_index,
                initial_status,
            }) => {
                let payload = mutations_src[*source_index]
                    .card
                    .as_ref()
                    .ok_or_else(|| crate::Error::RustError("create without payload".into()))?;
                let card_id = new_id("crd");
                let hash = learning_core::Card::content_hash_of(
                    payload.intent,
                    &payload.prompt,
                    &payload.answer,
                    &payload.accepted_answers,
                );
                database.execute(
                    card_repo::INSERT_CARD,
                    params![
                        card_id,
                        ctx.profile.id.0,
                        ctx.profile.language_pack_id.0,
                        pack_version,
                        enum_str(&payload.intent),
                        enum_str(initial_status),
                        payload.prompt,
                        payload.prompt_language.clone().unwrap_or_else(|| ctx
                            .profile
                            .source_language
                            .0
                            .clone()),
                        payload.answer,
                        ctx.profile.target_locale.0,
                        serde_json::to_string(&payload.accepted_answers)
                            .unwrap_or_else(|_| "[]".to_owned()),
                        "exact",
                        payload.explanation,
                        Option::<String>::None,
                        payload.source_learning_item_id,
                        payload.source_error_target_id,
                        lesson_id,
                        payload.creation_reason,
                        payload.priority_score,
                        hash,
                        1_u32,
                        Option::<String>::None,
                        Option::<String>::None,
                        "tutor",
                        at,
                    ],
                )?;
                for (tag_type, tags) in [
                    ("scenario", &payload.scenario_tags),
                    ("skill", &payload.skill_tags),
                ] {
                    for tag in tags {
                        database.execute(
                            card_repo::INSERT_CARD_TAG,
                            params![card_id, tag_type, tag.trim().to_lowercase()],
                        )?;
                    }
                }
                if *initial_status == learning_core::CardStatus::New {
                    use review_engine::ReviewScheduler as _;
                    let scheduler = review_engine::FsrsScheduler::new()
                        .map_err(|error| crate::Error::RustError(error.to_string()))?;
                    let state = scheduler
                        .initial_state(review_engine::InitialReviewInput {
                            now: review_engine::Timestamp(ctx.now.0),
                        })
                        .map_err(|error| crate::Error::RustError(error.to_string()))?;
                    let row =
                        crate::repositories::review::ScheduleStateRow::from_state(&card_id, &state);
                    database.execute(
                        crate::repositories::review::UPSERT_SCHEDULE_STATE,
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
                }
                created_card_id = Some(card_id);
            }
            Some(MutationEffect::UpdateContent {
                card_id,
                source_index,
            }) => {
                let payload = mutations_src[*source_index]
                    .card
                    .as_ref()
                    .ok_or_else(|| crate::Error::RustError("update without payload".into()))?;
                let hash = learning_core::Card::content_hash_of(
                    payload.intent,
                    &payload.prompt,
                    &payload.answer,
                    &payload.accepted_answers,
                );
                database.execute(
                    card_repo::UPDATE_CARD_CONTENT,
                    params![
                        card_id.0,
                        payload.prompt,
                        payload.explanation,
                        Option::<String>::None,
                        serde_json::to_string(&payload.accepted_answers)
                            .unwrap_or_else(|_| "[]".to_owned()),
                        hash,
                        at,
                    ],
                )?;
            }
            Some(MutationEffect::SetStatus {
                card_id,
                status,
                reason,
            }) => {
                database.execute(
                    card_repo::UPDATE_CARD_STATUS,
                    params![card_id.0, enum_str(status), reason, at],
                )?;
            }
            Some(
                MutationEffect::AddAcceptedAnswer { card_id, answer }
                | MutationEffect::RemoveAcceptedAnswer { card_id, answer },
            ) => {
                if let Some(target) = mutation_ctx
                    .existing_cards
                    .iter()
                    .find(|card| &card.id == card_id)
                {
                    let mut accepted = target.accepted_answers.clone();
                    let adding = matches!(
                        planned.effect,
                        Some(MutationEffect::AddAcceptedAnswer { .. })
                    );
                    if adding {
                        accepted.push(answer.clone());
                    } else {
                        accepted.retain(|candidate| {
                            candidate.trim().to_lowercase() != answer.trim().to_lowercase()
                        });
                    }
                    let hash = learning_core::Card::content_hash_of(
                        target.intent,
                        &target.prompt,
                        &target.answer,
                        &accepted,
                    );
                    database.execute(
                        card_repo::UPDATE_ACCEPTED_ANSWERS,
                        params![
                            card_id.0,
                            serde_json::to_string(&accepted).unwrap_or_else(|_| "[]".to_owned()),
                            hash,
                            at,
                        ],
                    )?;
                }
            }
            None => {}
        }

        let audit_card_id = created_card_id.clone().or_else(|| source.card_id.clone());
        let after_json = source
            .card
            .as_ref()
            .and_then(|card| serde_json::to_string(card).ok());
        database.execute(
            card_repo::INSERT_CARD_MUTATION,
            params![
                new_id("cm"),
                audit_card_id,
                enum_str(&source.operation),
                source.client_operation_id,
                Option::<String>::None,
                after_json,
                lesson_id,
                "tutor",
                planned
                    .note
                    .clone()
                    .or_else(|| source.reason.clone())
                    .unwrap_or_else(|| "lesson result".to_owned()),
                ctx.profile.language_pack_id.0,
                pack_version,
                match planned.verdict {
                    application::Verdict::Applied => "applied",
                    application::Verdict::Rejected => "rejected",
                    application::Verdict::Warned => "warned",
                },
                at,
            ],
        )?;

        summary.push(serde_json::json!({
            "client_operation_id": source.client_operation_id,
            "verdict": planned.verdict,
            "note": planned.note,
            "card_id": audit_card_id,
        }));
    }
    Ok(summary)
}
