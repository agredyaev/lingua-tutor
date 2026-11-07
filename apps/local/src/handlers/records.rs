//! Learning state, next lesson, errors, listening, and tutor-context operations.

use crate::api::{now, operation_error};
use crate::sqlite::to_rfc3339;
use crate::{Env, Result};
use rusqlite::{OptionalExtension as _, params};

const CALIBRATION_HISTORY_DECISIONS: i64 = adaptation::MAX_CALIBRATION_SNAPSHOTS as i64;

const SELECT_ERROR_TARGETS: &str =
    "SELECT t.id,t.learner_language_profile_id,t.core_category,t.error_code,
    t.original_production,t.corrected_production,t.explanation,t.occurrence_count,
    t.method_change_required,t.status,
    (
        SELECT repair_action
        FROM (
            SELECT r.repair_action AS repair_action,r.completed_at AS repaired_at
            FROM error_rechecks r
            WHERE r.error_target_id=t.id
              AND r.completed_at IS NOT NULL
              AND r.repair_action IS NOT NULL
            UNION ALL
            SELECT json_extract(rec.detail_json,'$.repair_action') AS repair_action,
                   rec.resolved_at AS repaired_at
            FROM recommendations rec
            WHERE rec.learner_language_profile_id=t.learner_language_profile_id
              AND rec.subject_type='error_target'
              AND rec.subject_id=t.id
              AND rec.status='accepted'
              AND json_valid(rec.detail_json)
              AND json_type(rec.detail_json,'$.repair_action')='text'
        )
        WHERE repair_action IS NOT NULL AND TRIM(repair_action)!=''
        ORDER BY repaired_at DESC
        LIMIT 1
    ) AS last_repair_action
 FROM error_targets t
 WHERE t.learner_language_profile_id=?1 AND (?2 IS NULL OR t.status=?2)
 ORDER BY
   CASE
     WHEN t.status IN ('active','escalated') THEN 0
     WHEN t.status='recheck' THEN 1
     ELSE 2
   END,
   t.method_change_required DESC,
   t.occurrence_count DESC,
   t.updated_at ASC,
   t.id ASC
 LIMIT ?3";

const SELECT_REDESIGN_CANDIDATES: &str = "SELECT r.id AS recommendation_id,
    r.detail_json, c.id AS card_id, c.prompt, c.answer, c.status AS card_status
 FROM recommendations r
 JOIN cards c ON c.id = r.subject_id
 WHERE r.learner_language_profile_id = ?1 AND r.status = 'open'
   AND r.action = 'redesign_card'
 ORDER BY r.created_at DESC LIMIT 50";

fn effective_next_lesson_number(
    database: &rusqlite::Connection,
    profile_id: &str,
    stored_current: u32,
) -> Result<u32> {
    if let Some(open_number) = database
        .query_row(
            "SELECT lesson_number
             FROM lessons
             WHERE learner_language_profile_id = ?1
               AND status IN ('planned','active')
             ORDER BY started_at DESC, lesson_number DESC
             LIMIT 1",
            params![profile_id],
            |row| row.get::<_, u32>(0),
        )
        .optional()?
    {
        return Ok(open_number);
    }

    let persisted_max: u32 = database.query_row(
        "SELECT COALESCE(MAX(lesson_number), 0)
         FROM lessons
         WHERE learner_language_profile_id = ?1",
        params![profile_id],
        |row| row.get(0),
    )?;
    Ok(stored_current.max(persisted_max).saturating_add(1))
}

/// Learning-state query — item counts by
/// status + active error targets + due rechecks.
pub(crate) fn learning_state_for(env: &Env, profile_id: &str) -> Result<serde_json::Value> {
    let database = env.connection()?;
    let items = item_status_counts(&database, profile_id)?;
    let active_errors: u32 = database.query_row(
        "SELECT COUNT(*) FROM error_targets
         WHERE learner_language_profile_id = ?1 AND status IN ('active','recheck')",
        params![profile_id],
        |row| row.get(0),
    )?;
    let due_rechecks: u32 = database.query_row(
        "SELECT COUNT(*) FROM error_rechecks r
         JOIN error_targets t ON t.id = r.error_target_id
         WHERE t.learner_language_profile_id = ?1
           AND r.completed_at IS NULL AND r.due_at <= ?2
           AND r.offset_days = (
               SELECT MIN(r2.offset_days)
               FROM error_rechecks r2
               WHERE r2.error_target_id=r.error_target_id
                 AND r2.cycle=r.cycle
                 AND r2.completed_at IS NULL
           )",
        params![profile_id, to_rfc3339(now())],
        |row| row.get(0),
    )?;
    crate::json(&serde_json::json!({
        "items_by_status": items,
        "active_error_targets": active_errors,
        "due_error_rechecks": due_rechecks,
    }))
}

// One bounded SQLite admission pass. The tuple is request-local scalar data,
// not a retained profile entity or a Row -> Domain mapping.
type ProfileColumns = (
    String,
    String,
    String,
    Option<String>,
    u32,
    serde_json::Value,
);

fn item_status_counts(
    database: &rusqlite::Connection,
    profile_id: &str,
) -> Result<shared_schema::ItemStatusCountsDto> {
    Ok(database.query_row(
        "SELECT
            COUNT(CASE WHEN status='candidate' THEN 1 END),
            COUNT(CASE WHEN status='introduced' THEN 1 END),
            COUNT(CASE WHEN status='learning' THEN 1 END),
            COUNT(CASE WHEN status='transfer_pending' THEN 1 END),
            COUNT(CASE WHEN status='provisionally_mastered' THEN 1 END),
            COUNT(CASE WHEN status='mastered' THEN 1 END),
            COUNT(CASE WHEN status='suspended' THEN 1 END)
         FROM learning_items
         WHERE learner_language_profile_id=?1",
        params![profile_id],
        |row| {
            Ok(shared_schema::ItemStatusCountsDto {
                candidate: row.get(0)?,
                introduced: row.get(1)?,
                learning: row.get(2)?,
                transfer_pending: row.get(3)?,
                provisionally_mastered: row.get(4)?,
                mastered: row.get(5)?,
                suspended: row.get(6)?,
            })
        },
    )?)
}

fn read_profile_columns(
    database: &rusqlite::Connection,
    profile_id: &str,
) -> Result<Option<ProfileColumns>> {
    let raw = database
        .query_row(
            "SELECT learner_id, source_language, target_locale,
                estimated_level, current_lesson_number, preferences_json, status
         FROM learner_language_profiles WHERE id = ?1",
            params![profile_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, u32>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                ))
            },
        )
        .optional()?;
    let Some((learner_id, source, target, level, number, raw_preferences, status)) = raw else {
        return Ok(None);
    };
    if !matches!(status.as_str(), "active" | "paused" | "archived") {
        return operation_error(500, "corrupt_row", "stored profile is invalid");
    }
    let preferences = match serde_json::from_str::<serde_json::Value>(&raw_preferences) {
        Ok(value) => value,
        Err(_) => return operation_error(500, "corrupt_row", "stored profile is invalid"),
    };
    Ok(Some((
        learner_id,
        source,
        target,
        level,
        number,
        preferences,
    )))
}

fn fixed_plan_wire(plan: &adaptation::AdaptiveLessonPlanFixed) -> serde_json::Value {
    let diagnostic_probes: Vec<&str> = (0..adaptation::plan_diagnostic_probe_count(plan))
        .filter_map(|index| adaptation::plan_diagnostic_probe(plan, index))
        .collect();
    serde_json::json!({
        "primary_objective": plan.primary_objective,
        "difficulty": plan.difficulty,
        "max_new_items": plan.max_new_items,
        "priorities": adaptation::plan_priorities(plan),
        "diagnostic_probes": diagnostic_probes,
        "success_criteria": adaptation::plan_success_criteria(plan),
        "recent_signals": plan.recent_signals,
    })
}

fn adaptive_lesson_planning_at(
    database: &rusqlite::Connection,
    profile_id: &str,
    preferences: &serde_json::Value,
    at: learning_core::Timestamp,
) -> Result<(serde_json::Value, serde_json::Value, &'static str)> {
    let max_new_items_per_lesson = preferences
        .get("max_new_items_per_lesson")
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| u8::try_from(value).ok());
    let preferred_difficulty = preferences
        .get("preferred_difficulty")
        .and_then(serde_json::Value::as_str);

    let view = crate::operations::planner_read::read_planner_input(
        database,
        profile_id,
        at,
        adaptation::PlannerPreferencesView {
            max_new_items_per_lesson,
            preferred_difficulty,
        },
    )?;
    let plan = adaptation::plan_next_lesson_fixed(&view);
    let primary_objective = plan.primary_objective;
    Ok((
        serde_json::to_value(view)?,
        fixed_plan_wire(&plan),
        primary_objective,
    ))
}

struct AdaptiveLessonSnapshot {
    planner_input: serde_json::Value,
    plan: serde_json::Value,
    primary_objective: &'static str,
    item_count: usize,
    tutor_tasks: Vec<adaptation::TutorTask>,
}

fn adaptive_lesson_snapshot(
    database: &rusqlite::Connection,
    profile_id: &str,
    preferences: &serde_json::Value,
) -> Result<AdaptiveLessonSnapshot> {
    let at = now();
    let (planner_input, mut plan, primary_objective) =
        adaptive_lesson_planning_at(database, profile_id, preferences, at)?;
    let mut selection = crate::operations::adaptive_selection::new_selection(
        crate::operations::adaptive_selection::DEFAULT_CANDIDATE_LIMIT,
        crate::operations::adaptive_selection::DEFAULT_FOCUS_LIMIT,
    );
    let item_count = crate::operations::capability_read::visit_item_capabilities(
        database,
        profile_id,
        at.0,
        |state| {
            crate::operations::adaptive_selection::observe_selection(
                &mut selection,
                adaptation::item_capability_view(state),
                primary_objective,
                at.0,
            );
            Ok(())
        },
    )?;
    let tutor_tasks = crate::operations::adaptive_selection::selection_tutor_tasks(&selection, 16);
    let base_max_new_items = plan
        .get("max_new_items")
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| u8::try_from(value).ok())
        .unwrap_or(0);
    plan["max_new_items"] = serde_json::json!(
        crate::operations::adaptive_selection::effective_new_item_cap(
            base_max_new_items,
            &tutor_tasks,
        )
    );
    Ok(AdaptiveLessonSnapshot {
        planner_input,
        plan,
        primary_objective,
        item_count,
        tutor_tasks,
    })
}

/// Next-lesson query.
pub(crate) fn next_lesson_for(env: &Env, profile_id: &str) -> Result<serde_json::Value> {
    let database = env.connection()?;
    let Some((_, _, _, estimated_level, current_lesson_number, preferences)) =
        read_profile_columns(&database, profile_id)?
    else {
        return operation_error(404, "not_found", "unknown profile");
    };
    let next_lesson_number =
        effective_next_lesson_number(&database, profile_id, current_lesson_number)?;
    let last = database
        .query_row(
            "SELECT next_goal, scenario FROM lessons
             WHERE learner_language_profile_id = ?1 AND status = 'completed'
             ORDER BY lesson_number DESC LIMIT 1",
            params![profile_id],
            |row| Ok((row.get::<_, Option<String>>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?;
    let snapshot = adaptive_lesson_snapshot(&database, profile_id, &preferences)?;
    crate::json(&serde_json::json!({
        "lesson_number": next_lesson_number,
        "estimated_level": estimated_level,
        "learner_preferences": preferences,
        "previous_goal": last.as_ref().and_then(|row| row.0.clone()),
        "previous_scenario": last.map(|row| row.1),
        "planner_input": snapshot.planner_input,
        "adaptive_plan": snapshot.plan,
        "tutor_tasks": snapshot.tutor_tasks,
        "tracked_item_count": snapshot.item_count,
    }))
}

// Direct bounded SQLite -> wire projection; no domain record or mapper chain.
fn project_error_wire(row: &rusqlite::Row<'_>) -> rusqlite::Result<Option<serde_json::Value>> {
    let category: String = row.get("core_category")?;
    let status: String = row.get("status")?;
    let (Ok(_), Ok(_)) = (
        serde_json::from_value::<learning_core::CoreErrorCategory>(serde_json::json!(category)),
        serde_json::from_value::<learning_core::ErrorTargetStatus>(serde_json::json!(status)),
    ) else {
        return Ok(None);
    };
    Ok(Some(serde_json::json!({
        "id": row.get::<_, String>("id")?,
        "language_profile_id": row.get::<_, String>("learner_language_profile_id")?,
        "core_category": category,
        "error_code": row.get::<_, Option<String>>("error_code")?,
        "original_production": row.get::<_, String>("original_production")?,
        "corrected_production": row.get::<_, String>("corrected_production")?,
        "explanation": row.get::<_, Option<String>>("explanation")?,
        "occurrence_count": row.get::<_, u32>("occurrence_count")?,
        "method_change_required": row.get::<_, u8>("method_change_required")? != 0,
        "status": status,
        "last_repair_action": row.get::<_, Option<String>>("last_repair_action")?,
    })))
}

/// Error-target query with optional status filter.
pub(crate) fn list_error_targets_for(
    env: &Env,
    profile_id: &str,
    status: Option<&str>,
) -> Result<serde_json::Value> {
    let database = env.connection()?;
    let mut statement = database.prepare(SELECT_ERROR_TARGETS)?;
    let mut rows = statement.query(params![profile_id, status, 100])?;
    let mut output = Vec::new();
    while let Some(row) = rows.next()? {
        if let Some(wire) = project_error_wire(row)? {
            output.push(wire);
        }
    }
    crate::json(&output)
}

fn project_active_error_wire(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<Option<serde_json::Value>> {
    let category: String = row.get("core_category")?;
    let status: String = row.get("status")?;
    if serde_json::from_value::<learning_core::CoreErrorCategory>(serde_json::json!(category))
        .is_err()
        || serde_json::from_value::<learning_core::ErrorTargetStatus>(serde_json::json!(status))
            .is_err()
        || status == "resolved"
    {
        return Ok(None);
    }
    Ok(Some(serde_json::json!({
        "error_target_id": row.get::<_, String>("id")?,
        "core_category": category,
        "error_code": row.get::<_, Option<String>>("error_code")?,
        "original_production": row.get::<_, String>("original_production")?,
        "corrected_production": row.get::<_, String>("corrected_production")?,
        "occurrence_count": row.get::<_, u32>("occurrence_count")?,
        "method_change_required": row.get::<_, u8>("method_change_required")? != 0,
        "status": status,
        "last_repair_action": row.get::<_, Option<String>>("last_repair_action")?,
    })))
}

fn active_error_context(
    database: &rusqlite::Connection,
    profile_id: &str,
) -> Result<Vec<serde_json::Value>> {
    let mut statement = database.prepare(SELECT_ERROR_TARGETS)?;
    let mut rows = statement.query(params![profile_id, Option::<String>::None, 20])?;
    let mut output = Vec::new();
    while let Some(row) = rows.next()? {
        if let Some(wire) = project_active_error_wire(row)? {
            output.push(wire);
        }
    }
    Ok(output)
}

// Single bounded SQL pass: five wire summaries and ten calibration inputs.
// Calibration owns only the required contiguous columns; no per-decision objects.
fn planner_history_context(
    database: &rusqlite::Connection,
    profile_id: &str,
) -> Result<(Vec<serde_json::Value>, Vec<adaptation::InterventionEffect>)> {
    let mut statement = database.prepare(
        "SELECT planner_version, input_json, plan_json,
                capability_state_json, outcome_json, created_at
         FROM planner_decisions
         WHERE learner_language_profile_id = ?1
         ORDER BY created_at DESC LIMIT ?2",
    )?;
    let mut rows = statement.query(params![profile_id, CALIBRATION_HISTORY_DECISIONS])?;
    let mut summaries = Vec::with_capacity(5);
    let mut timestamps = Vec::with_capacity(adaptation::MAX_CALIBRATION_SNAPSHOTS);
    let mut capability_json = Vec::with_capacity(adaptation::MAX_CALIBRATION_SNAPSHOTS);
    let mut outcome_json = Vec::with_capacity(adaptation::MAX_CALIBRATION_SNAPSHOTS);

    while let Some(row) = rows.next()? {
        let created_at: String = row.get("created_at")?;
        let persisted_outcome: Option<String> = row.get("outcome_json")?;
        if summaries.len() < 5 {
            let input: String = row.get("input_json")?;
            let plan: String = row.get("plan_json")?;
            let input = serde_json::from_str::<serde_json::Value>(&input).unwrap_or_default();
            let plan = serde_json::from_str::<serde_json::Value>(&plan).unwrap_or_default();
            let outcome = persisted_outcome
                .as_deref()
                .and_then(|value| serde_json::from_str::<serde_json::Value>(value).ok());
            let mut error_recheck_repairs = String::new();
            let mut repair_count = 0_u8;
            if let Some(entries) = outcome
                .as_ref()
                .and_then(|value| value.get("error_rechecks"))
                .and_then(serde_json::Value::as_array)
            {
                for entry in entries {
                    if repair_count == adaptation::DUE_RECHECK_PRIMARY_LIMIT {
                        break;
                    }
                    if entry
                        .get("repair_applied")
                        .and_then(serde_json::Value::as_bool)
                        != Some(true)
                    {
                        continue;
                    }
                    let Some(target_id) = entry
                        .get("error_target_id")
                        .and_then(serde_json::Value::as_str)
                        .filter(|value| value.len() <= tutor_contract::bounds::MAX_ID)
                    else {
                        continue;
                    };
                    let Some(action) = entry
                        .get("repair_action")
                        .and_then(serde_json::Value::as_str)
                        .filter(|value| value.len() <= tutor_contract::bounds::MAX_ID)
                    else {
                        continue;
                    };
                    if !error_recheck_repairs.is_empty() {
                        error_recheck_repairs.push(';');
                    }
                    error_recheck_repairs.push_str(target_id);
                    error_recheck_repairs.push('=');
                    error_recheck_repairs.push_str(action);
                    repair_count = repair_count.saturating_add(1);
                }
            }
            summaries.push(serde_json::json!({
                "planner_version": row.get::<_, String>("planner_version")?,
                "input_summary": {
                    "due_reviews": input.get("due_reviews"),
                    "overdue_reviews": input.get("overdue_reviews"),
                    "due_error_rechecks": input.get("due_error_rechecks"),
                    "method_change_errors": input.get("method_change_errors"),
                    "recent": input.get("recent"),
                },
                "plan_summary": {
                    "primary_objective": plan.get("primary_objective"),
                    "difficulty": plan.get("difficulty"),
                    "max_new_items": plan.get("max_new_items"),
                },
                "outcome_summary": outcome.as_ref().map(|value| serde_json::json!({
                    "scenario": value.get("scenario"),
                    "metrics": value.get("metrics"),
                    "evidence_count": value
                        .get("evidence")
                        .and_then(serde_json::Value::as_array)
                        .map_or(0, Vec::len),
                    "pronunciation_repeat_count": value
                        .get("evidence")
                        .and_then(serde_json::Value::as_array)
                        .map_or(0, |entries| entries.iter().filter(|entry| {
                            entry.get("pronunciation_repeat_required")
                                .and_then(serde_json::Value::as_bool) == Some(true)
                        }).count()),
                    "pronunciation_unintelligible_count": value
                        .get("evidence")
                        .and_then(serde_json::Value::as_array)
                        .map_or(0, |entries| entries.iter().filter(|entry| {
                            entry.get("pronunciation_intelligible")
                                .and_then(serde_json::Value::as_bool) == Some(false)
                        }).count()),
                    "diagnostic_probe_count": value.get("diagnostic_probe_count"),
                    "calibration_valid": value.get("calibration_valid"),
                    "intervention_count": value
                        .get("interventions")
                        .and_then(serde_json::Value::as_array)
                        .map_or(0, Vec::len),
                    "error_count": value
                        .get("error_occurrences")
                        .and_then(serde_json::Value::as_array)
                        .map_or(0, Vec::len),
                    "error_recheck_repairs": error_recheck_repairs,
                    "error_recommendation_repairs": value.get("error_recommendation_repairs"),
                    "next_goal": value.get("next_goal"),
                })),
                "created_at": created_at,
            }));
        }

        if let Ok(at) = crate::sqlite::from_rfc3339(&created_at) {
            timestamps.push(at.0);
            capability_json.push(row.get::<_, String>("capability_state_json")?);
            outcome_json.push(persisted_outcome.unwrap_or_else(|| "{}".to_owned()));
        }
    }

    let input = timestamps
        .iter()
        .copied()
        .zip(&capability_json)
        .zip(&outcome_json)
        .map(|((at, items), outcome)| (at, items.as_str(), outcome.as_str()));
    let effects = adaptation::summarize_intervention_history(
        input,
        adaptation::INTERVENTION_CALIBRATION_MIN_DELAY_SECONDS,
    );
    Ok((summaries, effects))
}

pub(crate) fn tutor_context_for(env: &Env, profile_id: &str) -> Result<serde_json::Value> {
    let database = env.connection()?;
    let Some((
        _,
        source_language,
        target_locale,
        estimated_level,
        current_lesson_number,
        preferences,
    )) = read_profile_columns(&database, profile_id)?
    else {
        return operation_error(404, "not_found", "unknown profile");
    };
    let next_lesson_number =
        effective_next_lesson_number(&database, profile_id, current_lesson_number)?;
    let errors = active_error_context(&database, profile_id)?;

    let level = estimated_level.as_deref().unwrap_or("не определён");
    let instructions = serde_json::json!({
        "instructions": format!(
            "{}\n\nТекущий профиль: target_locale={target_locale}; source_language={source_language}; estimated_level={level}.",
            include_str!("../methodology_instructions_ru.md")
        ),
        "correction_conventions": include_str!("../methodology_corrections_ru.md"),
    });
    let mut snapshot = adaptive_lesson_snapshot(&database, profile_id, &preferences)?;

    let (planner_history, intervention_effectiveness) =
        planner_history_context(&database, profile_id)?;

    let open_recommendations = {
        let mut statement = database.prepare(crate::sqlite::SELECT_OPEN)?;
        statement
            .query_map(params![profile_id, 20], |row| {
                Ok(serde_json::json!({
                    "id": row.get::<_, String>("id")?,
                    "subject_type": row.get::<_, String>("subject_type")?,
                    "subject_id": row.get::<_, Option<String>>("subject_id")?,
                    "action": row.get::<_, String>("action")?,
                    "detail_json": row.get::<_, String>("detail_json")?,
                    "created_at": row.get::<_, String>("created_at")?,
                }))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };

    let redesign = {
        let mut statement = database.prepare(SELECT_REDESIGN_CANDIDATES)?;
        statement
            .query_map(params![profile_id], |row| {
                Ok(serde_json::json!({
                    "recommendation_id": row.get::<_, String>("recommendation_id")?,
                    "detail_json": row.get::<_, String>("detail_json")?,
                    "card_id": row.get::<_, String>("card_id")?,
                    "prompt": row.get::<_, String>("prompt")?,
                    "answer": row.get::<_, String>("answer")?,
                    "card_status": row.get::<_, String>("card_status")?,
                }))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };

    let week_ago = to_rfc3339(learning_core::Timestamp(now().0 - 7 * 86_400));
    let (week_review_count, again_count) = {
        let mut statement = database.prepare(crate::sqlite::SELECT_EVENTS_SINCE)?;
        let mut rows = statement.query(params![profile_id, week_ago])?;
        let mut count = 0_usize;
        let mut again = 0_usize;
        while let Some(row) = rows.next()? {
            count += 1;
            again += usize::from(row.get::<_, u8>("grade")? == 1);
        }
        (count, again)
    };

    // The wire list and the compact lookup column are the only retained
    // outputs of this bounded scan (at most 60 rows). No per-item object graph.
    let mut items_json = Vec::new();
    {
        let mut statement = database.prepare(
            "SELECT id, target_text, status
             FROM learning_items
             WHERE learner_language_profile_id = ?1
               AND status NOT IN ('candidate','suspended')
             ORDER BY updated_at DESC LIMIT 60",
        )?;
        let mut rows = statement.query(params![profile_id])?;
        while let Some(row) = rows.next()? {
            items_json.push(serde_json::json!({
                "learning_item_id": row.get::<_, String>("id")?,
                "target_text": row.get::<_, String>("target_text")?,
                "status": row.get::<_, String>("status")?,
            }));
        }
    }

    let persisted_lesson_plan: Option<(String, Option<u32>)> = database
        .query_row(
            "SELECT COALESCE(json_extract(pd.plan_json,'$.primary_objective'),''),
                    json_extract(pd.plan_json,'$.max_new_items')
             FROM lessons l
             JOIN planner_decisions pd ON pd.lesson_id=l.id
             WHERE l.learner_language_profile_id=?1
               AND l.status IN ('planned','active')
             ORDER BY COALESCE(l.started_at,l.created_at) DESC,l.lesson_number DESC
             LIMIT 1",
            params![profile_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    if let Some((_, Some(persisted_cap))) = persisted_lesson_plan.as_ref() {
        let live_cap = snapshot
            .plan
            .get("max_new_items")
            .and_then(serde_json::Value::as_u64)
            .and_then(|value| u32::try_from(value).ok())
            .unwrap_or_default();
        snapshot.plan["max_new_items"] = serde_json::json!(live_cap.min(*persisted_cap));
    }
    let recheck_objective = persisted_lesson_plan
        .as_ref()
        .map(|(objective, _)| objective.as_str())
        .filter(|objective| !objective.is_empty())
        .unwrap_or(snapshot.primary_objective);
    let due_recheck_limit = if recheck_objective == "run_error_rechecks" {
        adaptation::DUE_RECHECK_PRIMARY_LIMIT
    } else {
        adaptation::DUE_RECHECK_MAINTENANCE_LIMIT
    };
    let due_rechecks = {
        let mut statement = database.prepare(
            "SELECT r.id AS recheck_id, r.offset_days, t.id AS error_target_id,
                    t.original_production, t.corrected_production,
                    (
                        SELECT repair_action
                        FROM (
                            SELECT prior.repair_action AS repair_action,
                                   prior.completed_at AS repaired_at
                            FROM error_rechecks prior
                            WHERE prior.error_target_id=t.id
                              AND prior.completed_at IS NOT NULL
                              AND prior.repair_action IS NOT NULL
                            UNION ALL
                            SELECT json_extract(rec.detail_json,'$.repair_action') AS repair_action,
                                   rec.resolved_at AS repaired_at
                            FROM recommendations rec
                            WHERE rec.learner_language_profile_id=t.learner_language_profile_id
                              AND rec.subject_type='error_target'
                              AND rec.subject_id=t.id
                              AND rec.status='accepted'
                              AND json_valid(rec.detail_json)
                              AND json_type(rec.detail_json,'$.repair_action')='text'
                        )
                        WHERE repair_action IS NOT NULL AND TRIM(repair_action)!=''
                        ORDER BY repaired_at DESC
                        LIMIT 1
                    ) AS last_repair_action
             FROM error_rechecks r
             JOIN error_targets t ON t.id = r.error_target_id
             WHERE t.learner_language_profile_id = ?1
               AND r.completed_at IS NULL AND r.due_at <= ?2
               AND r.offset_days = (
                   SELECT MIN(r2.offset_days)
                   FROM error_rechecks r2
                   WHERE r2.error_target_id=r.error_target_id
                     AND r2.cycle=r.cycle
                     AND r2.completed_at IS NULL
               )
             ORDER BY r.due_at, r.id LIMIT ?3",
        )?;
        statement
            .query_map(
                params![profile_id, to_rfc3339(now()), due_recheck_limit],
                |row| {
                    Ok(serde_json::json!({
                        "recheck_id": row.get::<_, String>("recheck_id")?,
                        "offset_days": row.get::<_, u32>("offset_days")?,
                        "error_target_id": row.get::<_, String>("error_target_id")?,
                        "original_production": row.get::<_, String>("original_production")?,
                        "corrected_production": row.get::<_, String>("corrected_production")?,
                        "last_repair_action": row.get::<_, Option<String>>("last_repair_action")?,
                    }))
                },
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };

    let goal: Option<String> = database
        .query_row(
            "SELECT next_goal FROM lessons
             WHERE learner_language_profile_id = ?1 AND status = 'completed'
             ORDER BY lesson_number DESC LIMIT 1",
            params![profile_id],
            |row| row.get(0),
        )
        .optional()?
        .flatten();

    let existing_card_answers = {
        let mut statement = database.prepare(
            "SELECT answer, intent FROM cards
             WHERE learner_language_profile_id = ?1 AND status != 'retired'
             ORDER BY created_at DESC LIMIT 50",
        )?;
        statement
            .query_map(params![profile_id], |row| {
                Ok(serde_json::json!({
                    "answer": row.get::<_, String>("answer")?,
                    "intent": row.get::<_, String>("intent")?,
                }))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };

    crate::json(&serde_json::json!({
        "schema_version": "1",
        "language_profile_id": profile_id,
        "source_language": source_language,
        "target_locale": target_locale,
        "next_lesson_number": next_lesson_number,
        "estimated_level": estimated_level,
        "learner_preferences": preferences,
        "tutor_instructions": instructions,
        "planner_input": snapshot.planner_input,
        "adaptive_lesson_plan": snapshot.plan,
        "tutor_tasks": snapshot.tutor_tasks,
        "tracked_item_count": snapshot.item_count,
        "planner_history": planner_history,
        "intervention_effectiveness": intervention_effectiveness,
        "active_error_targets": errors,
        "previous_next_goal_hypothesis": goal,
        "learning_items": items_json,
        "due_error_rechecks": due_rechecks,
        "existing_card_answers": existing_card_answers,
        "review_summary_7d": {
            "reviews": week_review_count,
            "again_count": again_count,
        },
        "redesign_candidates": redesign,
        "open_recommendations": open_recommendations,
        "lesson_result_contract": lesson_result_contract(profile_id, &source_language),
    }))
}

pub(crate) fn dashboard_for(env: &Env, profile_id: &str) -> Result<serde_json::Value> {
    let database = env.connection()?;
    let Some((learner_id, _, _, _, current_lesson_number, _)) =
        read_profile_columns(&database, profile_id)?
    else {
        return operation_error(404, "not_found", "unknown profile");
    };
    let next_lesson_number =
        effective_next_lesson_number(&database, profile_id, current_lesson_number)?;
    let at = crate::api::now();
    let now_text = to_rfc3339(at);
    let timezone: String = database.query_row(
        "SELECT timezone FROM learner_profile WHERE id=?1",
        params![learner_id],
        |row| row.get(0),
    )?;
    let day_start = to_rfc3339(crate::operations::card_budget::local_day_start_utc(
        at, &timezone,
    )?);

    fn count<P: rusqlite::Params>(
        database: &rusqlite::Connection,
        sql: &str,
        params: P,
    ) -> Result<u32> {
        Ok(database.query_row(sql, params, |row| row.get(0))?)
    }

    let due_cards = count(
        &database,
        "SELECT COUNT(*) FROM card_schedule_state s
         JOIN cards c ON c.id = s.card_id
         WHERE c.learner_language_profile_id = ?1
           AND c.status IN ('new','learning','review','relearning')
           AND s.due_at <= ?2",
        params![profile_id, now_text],
    )?;
    let active_errors = count(
        &database,
        "SELECT COUNT(*) FROM error_targets
         WHERE learner_language_profile_id = ?1 AND status IN ('active','recheck')",
        params![profile_id],
    )?;
    let method_change = count(
        &database,
        "SELECT COUNT(*) FROM error_targets
         WHERE learner_language_profile_id = ?1
           AND method_change_required = 1 AND status IN ('active','escalated')",
        params![profile_id],
    )?;
    let due_rechecks = count(
        &database,
        "SELECT COUNT(*) FROM error_rechecks r
         JOIN error_targets t ON t.id = r.error_target_id
         WHERE t.learner_language_profile_id = ?1
           AND r.completed_at IS NULL AND r.due_at <= ?2
           AND r.offset_days = (
               SELECT MIN(r2.offset_days)
               FROM error_rechecks r2
               WHERE r2.error_target_id=r.error_target_id
                 AND r2.cycle=r.cycle
                 AND r2.completed_at IS NULL
           )",
        params![profile_id, now_text],
    )?;
    let overdue = count(
        &database,
        crate::sqlite::COUNT_OVERDUE,
        params![profile_id, now_text],
    )?;

    let (reviews_today, review_seconds_today) = {
        let mut statement = database.prepare(crate::sqlite::SELECT_EVENTS_SINCE)?;
        let mut rows = statement.query(params![profile_id, day_start])?;
        let mut count = 0_u64;
        let mut total_millis = 0_u64;
        while let Some(row) = rows.next()? {
            count += 1;
            total_millis += u64::from(row.get::<_, u32>("response_millis")?);
        }
        (
            u32::try_from(count).unwrap_or(u32::MAX),
            total_millis / 1000,
        )
    };

    let items = item_status_counts(&database, profile_id)?;

    let goal: Option<String> = database
        .query_row(
            "SELECT next_goal FROM lessons
             WHERE learner_language_profile_id = ?1 AND status = 'completed'
             ORDER BY lesson_number DESC LIMIT 1",
            params![profile_id],
            |row| row.get(0),
        )
        .optional()?
        .flatten();

    let budget =
        crate::operations::card_budget::remaining_activation_budget(&database, profile_id, at)?;

    let recommendation_details = {
        let mut statement = database.prepare(crate::sqlite::SELECT_OPEN)?;
        statement
            .query_map(params![profile_id, 5], |row| {
                row.get::<_, String>("detail_json")
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };
    let open_recommendations: Vec<String> = recommendation_details
        .iter()
        .filter_map(|detail_json| {
            serde_json::from_str::<serde_json::Value>(detail_json)
                .ok()
                .and_then(|value| value["detail"].as_str().map(str::to_owned))
        })
        .collect();

    let mut exceptions = Vec::new();
    if overdue > 0 {
        exceptions.push(format!(
            "{overdue} significantly overdue review item(s) — review pressure increased"
        ));
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

    crate::json(&shared_schema::DashboardDto {
        exceptions,
        due_cards,
        new_card_budget: budget,
        reviews_today,
        review_seconds_today: u32::try_from(review_seconds_today).unwrap_or(u32::MAX),
        active_error_targets: active_errors,
        due_error_rechecks: due_rechecks,
        next_lesson_number,
        next_lesson_goal: goal,
        items_by_status: items,
        open_recommendations,
    })
}

pub(crate) fn health(env: &Env) -> Result<serde_json::Value> {
    let database = env.connection()?;
    let latest_migration = database
        .query_row(
            "SELECT name FROM schema_migrations ORDER BY id DESC LIMIT 1",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional();
    let (sqlite_ok, latest_migration) = match latest_migration {
        Ok(row) => (true, row),
        Err(_) => (false, None),
    };
    crate::json(&serde_json::json!({
        "status": if sqlite_ok { "ok" } else { "degraded" },
        "version": env!("CARGO_PKG_VERSION"),
        "sqlite_connected": sqlite_ok,
        "latest_migration": latest_migration,
        "schema_version": latest_migration,
        "scheduler": {
            "algorithm": "fsrs",
            "algorithm_version": "fsrs-rs-6.6.1"
        },
    }))
}

fn lesson_result_contract(profile_id: &str, source_language: &str) -> serde_json::Value {
    let enums = serde_json::json!({
        "item_type": ["phrase","collocation","pattern","word","response_frame"],
        "evidence_kind": ["recognition","listening","prompted_production",
                           "spontaneous_production","transfer","correction"],
        "core_category": ["grammar","vocabulary","word_order","register",
                           "pronunciation","orthography","other"],
        "mastery_status": ["candidate","introduced","learning","transfer_pending",
                            "provisionally_mastered","mastered","suspended"],
        "card_operation": ["create","update","suspend","restore","retire",
                            "add_accepted_answer","remove_accepted_answer"],
        "card_intent": ["production","listening","contrast","transfer"],
        "probe_phase": ["diagnostic","practice","delayed_retrieval","transfer"],
        "recommendation_resolution_status": ["accepted","dismissed"],
    });
    let shape_examples = serde_json::json!({
        "new_learning_item": {
            "client_item_key": "new-1", "item_type": "phrase",
            "target_text": "…", "prompt_text": "…", "prompt_language": source_language
        },
        "evidence": {
            "client_item_key": "new-1",
            "kind": "spontaneous_production", "success": false,
            "context": "<конкретная реальная ситуация>",
            "assistance_level": 0,
            "probe_phase": "diagnostic",
            "pronunciation_intelligible": true,
            "pronunciation_repeat_required": false
        },
        "transfer_evidence": {
            "learning_item_id": "<existing-learning-item-id>",
            "kind": "transfer", "success": true,
            "context": "<новая коммуникативная ситуация>",
            "context_family": "service.ask_help",
            "assistance_level": 0,
            "probe_phase": "transfer"
        },
        "intervention": {
            "action": "spontaneous_retrieval", "client_item_key": "new-1",
            "target_capability": "spontaneous_production", "evidence_index": 0
        },
        "listening_attempt": {
            "client_item_key": "new-1", "success": true,
            "assistance_level": 0, "probe_phase": "delayed_retrieval",
            "context_tag": "<конкретная ситуация слушания>",
            "source_reference": "lesson-audio-1"
        },
        "error_occurrence": {
            "error_target_id": null, "core_category": "grammar",
            "error_code": "<stable-error-code>", "original_production": "…",
            "corrected_production": "…"
        },
        "error_recheck": {
            "error_target_id": "<из due_error_rechecks>",
            "recheck_id": "<из due_error_rechecks>", "passed": true,
            "assistance_level": 0,
            "context": "<конкретная ситуация перепроверки>",
            "repair_applied": false, "repair_action": null
        },
        "recommendation_resolution": {
            "recommendation_id": "<из open_recommendations>",
            "status": "accepted", "repair_action": null
        },
        "mastery_change": {
            "learning_item_id": "<existing-learning-item-id>",
            "new_status": "provisionally_mastered"
        },
        "card_mutation": {
            "operation": "create", "client_operation_id": "op-…-c1",
            "card": {
                "intent": "production", "prompt": "…", "answer": "…",
                "prompt_language": source_language, "creation_reason": "…"
            }
        }
    });
    let template = serde_json::json!({
        "schema_version": tutor_contract::SCHEMA_VERSION,
        "client_operation_id": "op-<уникальный-id-урока>",
        "language_profile_id": profile_id,
        "lesson": { "scenario": "<scenario_id>", "duration_seconds": 0,
                     "tutor_summary": "…", "next_goal": "…" },
        "metrics": { "task_attempt_count": 0, "task_success_count": 0,
                      "task_success_rubric": null },
        "new_learning_items": [],
        "evidence": [],
        "interventions": [],
        "listening_attempts": [],
        "error_occurrences": [],
        "error_rechecks": [],
        "recommendation_resolutions": [],
        "mastery_changes": [],
        "card_mutations": [],
    });
    serde_json::json!({
        "schema_version": tutor_contract::SCHEMA_VERSION,
        "submit": {
            "mcp_tool": "complete_lesson",
            "mcp_lesson_id": "use start_lesson.id",
            "json_rpc_method": "complete_lesson"
        },
        "instructions": "В конце урока верни ровно один JSON-блок schema_version=2 по template. template — безопасный submit skeleton: optional arrays в нём пустые по умолчанию. shape_examples — только справочник формы объектов; не копируй строки из него автоматически и добавляй в template только реально наблюдавшиеся/выполненные действия. При вызове MCP complete_lesson обязательно передай top-level lesson_id, равный id, полученному от start_lesson. Для calibration-eligible outcome нужен минимум 1 diagnostic baseline и минимум 2 measurement-eligible probes overall; diagnostic/delayed_retrieval/transfer считаются для lesson coverage, а practice — нет. Lesson с меньшей coverage всё равно сохраняй: runtime пометит его calibration_valid=false; не добавляй probe только ради порога. Для каждого выбранного evidence/listening probe обязательны assistance_level, probe_phase и конкретный context/context_tag. Для выполненного tutor_task сначала соблюдай его task-local measurement_requirements: они конкретизируют этот общий контракт и должны быть прочитаны до предъявления probe, а не восстановлены постфактум. Каждый listening_attempt должен ссылаться ровно на один learning item и иметь непустой source_reference на реально предъявленный audio/live source; text-only interaction не создаёт listening evidence. response_millis добавляй только если инструмент измерил время от готовности probe до начала ответа/first committed response; не используй полную длительность набора/реплики и не выдумывай миллисекунды. Для production/transfer pronunciation_intelligible и pronunciation_repeat_required указывай вместе только если речь реально была слышна; в text-only interaction оба поля опускай. Список due_error_rechecks уже bounded runtime: проведи каждую показанную перепроверку как maintenance obligation текущего урока и верни её outcome в error_rechecks с исходными error_target_id и recheck_id, assistance_level=0 и конкретным context; не пытайся вручную догонять непоказанный backlog. Если recheck failed, до завершения урока примени corrective repair, поставь repair_applied=true и укажи стабильный короткий repair_action, чтобы следующий Tutor не повторял тот же неработающий способ; только после этого начинается свежий spaced recheck cycle. Не подменяй recheck обычным error_occurrence. Если реально выполнил open recommendation, верни её id в recommendation_resolutions со status=accepted; для accepted error-target repair (activate_mini_target/require_method_change) обязательно укажи стабильный короткий repair_action реально применённого способа. Если явно проверил рекомендацию и признал её неприменимой — status=dismissed. Не закрывай recommendation только потому, что увидел её в context, и не выдумывай recommendation_id. В metrics обязательно отрази реально проведённые communicative tasks через task_attempt_count/task_success_count; form-only drill (`скажи/повтори X`) не является communicative task и не увеличивает task_attempt_count. При task_attempt_count>0 используй task_success_rubric=goal_completed_independently_v2. Для v2 success исходная цель должна быть достигнута через самостоятельно построенное решение на target language; source-language или gesture-only workaround не является языковым task success, если он заменяет необходимый L2 материал. Не выдумывай speaking_seconds/confidence/aggregate latency, если они не наблюдались. В interventions записывай только реально применённые методы, а не запланированные tutor_tasks. Для существующего материала используй learning_item_id; для материала этого урока — client_item_key. Для transfer используй стабильный semantic context_family. assistance_level=0 только при реально самостоятельном ответе; после подсказки используй prompted_production.",
        "enums": enums,
        "template": template,
        "shape_examples": shape_examples,
    })
}

#[cfg(test)]
// Test fixtures intentionally fail fast and use compact tuple snapshots.
#[allow(clippy::unwrap_used, clippy::type_complexity)]
mod error_projection_tests {
    use super::*;

    const ERROR_ROW: &str = "SELECT 'err-1' AS id, 'llp-1' AS learner_language_profile_id,
        'grammar' AS core_category, 'CODE' AS error_code,
        'estoy médico' AS original_production, 'soy médico' AS corrected_production,
        'explain' AS explanation, 5 AS occurrence_count,
        1 AS method_change_required, 'recheck' AS status,
        NULL AS last_repair_action";

    #[test]
    fn error_target_sql_projection_matches_wire_fixture() {
        let db = rusqlite::Connection::open_in_memory().unwrap();
        let wire = db
            .query_row(ERROR_ROW, [], project_error_wire)
            .unwrap()
            .unwrap();
        assert_eq!(
            wire,
            serde_json::json!({
                "id":"err-1","language_profile_id":"llp-1",
                "core_category":"grammar","error_code":"CODE",
                "original_production":"estoy médico","corrected_production":"soy médico",
                "explanation":"explain","occurrence_count":5,"method_change_required":true,
                "status":"recheck","last_repair_action":null
            })
        );
    }

    #[test]
    fn active_error_projection_matches_context_fixture() {
        let db = rusqlite::Connection::open_in_memory().unwrap();
        let expected = serde_json::json!({
            "error_target_id":"err-1","core_category":"grammar","error_code":"CODE",
            "original_production":"estoy médico","corrected_production":"soy médico",
            "occurrence_count":5,"method_change_required":true,"status":"recheck",
            "last_repair_action":null
        });
        assert_eq!(
            db.query_row(ERROR_ROW, [], project_active_error_wire)
                .unwrap()
                .unwrap(),
            expected
        );
        let resolved = ERROR_ROW.replace("'recheck' AS status", "'resolved' AS status");
        assert!(
            db.query_row(&resolved, [], project_active_error_wire)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn error_target_sql_projection_skips_invalid_enums_and_versions() {
        let db = rusqlite::Connection::open_in_memory().unwrap();
        for invalid in [
            ERROR_ROW.replace("'grammar' AS core_category", "'invalid' AS core_category"),
            ERROR_ROW.replace("'recheck' AS status", "'invalid' AS status"),
        ] {
            assert!(
                db.query_row(&invalid, [], project_error_wire)
                    .unwrap()
                    .is_none()
            );
        }
    }
}
