//! Learning state, next lesson, errors, listening, and tutor-context operations.

use crate::api::{now, operation_error};
use crate::repositories::{records, to_rfc3339};
use crate::{Env, Result};
use rusqlite::{OptionalExtension as _, params};

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
    let mut items = serde_json::Map::new();
    {
        let mut statement = database.prepare(
            "SELECT status, COUNT(*) AS n FROM learning_items
         WHERE learner_language_profile_id = ?1 GROUP BY status",
        )?;
        let mut rows = statement.query(params![profile_id])?;
        while let Some(row) = rows.next()? {
            items.insert(
                row.get::<_, String>("status")?,
                serde_json::json!(row.get::<_, u32>("n")?),
            );
        }
    }
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
           AND r.completed_at IS NULL AND r.due_at <= ?2",
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
    String,
    Option<String>,
    u32,
    serde_json::Value,
);

fn read_profile_columns(
    database: &rusqlite::Connection,
    profile_id: &str,
) -> Result<Option<ProfileColumns>> {
    let raw = database
        .query_row(
            "SELECT learner_id, language_pack_id, source_language, target_locale,
                estimated_level, current_lesson_number, preferences_json, status
         FROM learner_language_profiles WHERE id = ?1",
            params![profile_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, u32>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                ))
            },
        )
        .optional()?;
    let Some((learner_id, pack_id, source, target, level, number, raw_preferences, status)) = raw
    else {
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
        pack_id,
        source,
        target,
        level,
        number,
        preferences,
    )))
}

fn fixed_plan_wire(plan: &adaptation::AdaptiveLessonPlanFixed<'_>) -> serde_json::Value {
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
        "priority_scenarios": plan.priority_scenarios,
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

    let mut scenarios = [""; 20];
    let mut scenario_count = 0;
    if let Some(values) = preferences
        .get("priority_scenarios")
        .and_then(serde_json::Value::as_array)
    {
        for value in values.iter().filter_map(serde_json::Value::as_str).take(20) {
            scenarios[scenario_count] = value;
            scenario_count += 1;
        }
    }

    let view = crate::operations::planner_read::read_planner_input(
        database,
        profile_id,
        at,
        adaptation::PlannerPreferencesView {
            max_new_items_per_lesson,
            preferred_difficulty,
            priority_scenarios: &scenarios[..scenario_count],
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
    item_count: usize,
    tutor_tasks: Vec<adaptation::TutorTask>,
}

fn adaptive_lesson_snapshot(
    database: &rusqlite::Connection,
    profile_id: &str,
    preferences: &serde_json::Value,
) -> Result<AdaptiveLessonSnapshot> {
    let at = now();
    let (planner_input, plan, primary_objective) =
        adaptive_lesson_planning_at(database, profile_id, preferences, at)?;
    let mut selection = crate::operations::adaptive_selection::new_selection(24, 32);
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
    Ok(AdaptiveLessonSnapshot {
        planner_input,
        plan,
        item_count,
        tutor_tasks,
    })
}

/// Next-lesson query.
pub(crate) fn next_lesson_for(env: &Env, profile_id: &str) -> Result<serde_json::Value> {
    let database = env.connection()?;
    let Some((_, _, _, _, estimated_level, current_lesson_number, preferences)) =
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
    let version: String = row.get("language_pack_version")?;
    let category: String = row.get("core_category")?;
    let status: String = row.get("status")?;
    let (Ok(version), Ok(_), Ok(_)) = (
        semver::Version::parse(&version),
        serde_json::from_value::<learning_core::CoreErrorCategory>(serde_json::json!(category)),
        serde_json::from_value::<learning_core::ErrorTargetStatus>(serde_json::json!(status)),
    ) else {
        return Ok(None);
    };
    Ok(Some(serde_json::json!({
        "id": row.get::<_, String>("id")?,
        "language_profile_id": row.get::<_, String>("learner_language_profile_id")?,
        "language_pack_id": row.get::<_, String>("language_pack_id")?,
        "language_pack_version": version.to_string(),
        "core_category": category,
        "pack_error_code": row.get::<_, Option<String>>("pack_error_code")?,
        "original_production": row.get::<_, String>("original_production")?,
        "corrected_production": row.get::<_, String>("corrected_production")?,
        "explanation": row.get::<_, Option<String>>("explanation")?,
        "occurrence_count": row.get::<_, u32>("occurrence_count")?,
        "method_change_required": row.get::<_, u8>("method_change_required")? != 0,
        "status": status,
    })))
}

/// Error-target query with optional status filter.
pub(crate) fn list_error_targets_for(
    env: &Env,
    profile_id: &str,
    status: Option<&str>,
) -> Result<serde_json::Value> {
    let database = env.connection()?;
    let mut statement = database.prepare(records::SELECT_ERROR_TARGETS)?;
    let mut rows = statement.query(params![profile_id, status, 100])?;
    let mut output = Vec::new();
    while let Some(row) = rows.next()? {
        if let Some(wire) = project_error_wire(row)? {
            output.push(wire);
        }
    }
    crate::json(&output)
}

pub(crate) fn get_error_target(env: &Env, target_id: &str) -> Result<serde_json::Value> {
    let database = env.connection()?;
    let item = database
        .query_row(
            records::SELECT_ERROR_TARGET,
            params![target_id],
            project_error_wire,
        )
        .optional()?
        .flatten();
    match item {
        Some(wire) => crate::json(&wire),
        None => operation_error(404, "not_found", "unknown error target"),
    }
}

#[derive(serde::Deserialize)]
pub(crate) struct ListeningAttemptParams {
    pub language_profile_id: String,
    #[serde(default)]
    pub learning_item_id: Option<String>,
    #[serde(default)]
    pub lesson_id: Option<String>,
    #[serde(default)]
    pub source_reference: Option<String>,
    #[serde(default)]
    pub source_variant_id: Option<String>,
    #[serde(default)]
    pub talker_key: Option<String>,
    pub context_tag: String,
    #[serde(default)]
    pub transcript: Option<String>,
    pub success: bool,
    pub assistance_level: u8,
    pub response_millis: u32,
    pub probe_phase: tutor_contract::ProbePhase,
}

fn project_active_error_wire(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<Option<serde_json::Value>> {
    let version: String = row.get("language_pack_version")?;
    let category: String = row.get("core_category")?;
    let status: String = row.get("status")?;
    if semver::Version::parse(&version).is_err()
        || serde_json::from_value::<learning_core::CoreErrorCategory>(serde_json::json!(category))
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
        "pack_error_code": row.get::<_, Option<String>>("pack_error_code")?,
        "original_production": row.get::<_, String>("original_production")?,
        "corrected_production": row.get::<_, String>("corrected_production")?,
        "occurrence_count": row.get::<_, u32>("occurrence_count")?,
        "method_change_required": row.get::<_, u8>("method_change_required")? != 0,
    })))
}

fn active_error_context(
    database: &rusqlite::Connection,
    profile_id: &str,
) -> Result<Vec<serde_json::Value>> {
    let mut statement = database.prepare(records::SELECT_ERROR_TARGETS)?;
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
         ORDER BY created_at DESC LIMIT 10",
    )?;
    let mut rows = statement.query(params![profile_id])?;
    let mut summaries = Vec::with_capacity(5);
    let mut timestamps = Vec::with_capacity(10);
    let mut capability_json = Vec::with_capacity(10);
    let mut outcome_json = Vec::with_capacity(10);

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
                    "next_goal": value.get("next_goal"),
                })),
                "created_at": created_at,
            }));
        }

        if let Ok(at) = crate::repositories::from_rfc3339(&created_at) {
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
    let effects = adaptation::summarize_intervention_history(input, 6 * 60 * 60);
    Ok((summaries, effects))
}

pub(crate) fn tutor_context_for(
    registry: &crate::BuiltInPackRegistry,
    env: &Env,
    profile_id: &str,
) -> Result<serde_json::Value> {
    let database = env.connection()?;
    let Some((
        _,
        pack_id,
        source_language,
        _target_locale,
        estimated_level,
        current_lesson_number,
        preferences,
    )) = read_profile_columns(&database, profile_id)?
    else {
        return operation_error(404, "not_found", "unknown profile");
    };
    let next_lesson_number =
        effective_next_lesson_number(&database, profile_id, current_lesson_number)?;
    let Some(pack) = language_pack_registry::get(registry, &pack_id) else {
        return operation_error(409, "pack_incompatible", "profile pack not registered");
    };
    let Some(manifest) = language_pack_registry::manifest(registry, &pack_id) else {
        return operation_error(
            409,
            "pack_incompatible",
            "profile pack manifest not registered",
        );
    };
    let errors = active_error_context(&database, profile_id)?;

    let instructions =
        language_pack_es_es::tutor_instructions(&source_language, estimated_level.as_deref());
    let snapshot = adaptive_lesson_snapshot(&database, profile_id, &preferences)?;

    let (planner_history, intervention_effectiveness) =
        planner_history_context(&database, profile_id)?;

    let open_recommendations = {
        let mut statement = database.prepare(crate::repositories::recommendations::SELECT_OPEN)?;
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
        let mut statement =
            database.prepare(crate::repositories::recommendations::SELECT_REDESIGN_CANDIDATES)?;
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
        let mut statement = database.prepare(crate::repositories::review::SELECT_EVENTS_SINCE)?;
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
    let mut normalized_targets = Vec::new();
    let mut items_json = Vec::new();
    {
        let mut statement = database.prepare(
            "SELECT id, target_text, normalized_target_text, status
             FROM learning_items
             WHERE learner_language_profile_id = ?1
               AND status NOT IN ('candidate','suspended')
             ORDER BY updated_at DESC LIMIT 60",
        )?;
        let mut rows = statement.query(params![profile_id])?;
        while let Some(row) = rows.next()? {
            normalized_targets.push(row.get::<_, String>("normalized_target_text")?);
            items_json.push(serde_json::json!({
                "learning_item_id": row.get::<_, String>("id")?,
                "target_text": row.get::<_, String>("target_text")?,
                "status": row.get::<_, String>("status")?,
            }));
        }
    }

    let due_rechecks = {
        let mut statement = database.prepare(
            "SELECT r.id AS recheck_id, r.offset_days, t.id AS error_target_id,
                    t.original_production, t.corrected_production
             FROM error_rechecks r
             JOIN error_targets t ON t.id = r.error_target_id
             WHERE t.learner_language_profile_id = ?1
               AND r.completed_at IS NULL AND r.due_at <= ?2
             ORDER BY r.due_at LIMIT 20",
        )?;
        statement
            .query_map(params![profile_id, to_rfc3339(now())], |row| {
                Ok(serde_json::json!({
                    "recheck_id": row.get::<_, String>("recheck_id")?,
                    "offset_days": row.get::<_, u32>("offset_days")?,
                    "error_target_id": row.get::<_, String>("error_target_id")?,
                    "original_production": row.get::<_, String>("original_production")?,
                    "corrected_production": row.get::<_, String>("corrected_production")?,
                }))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };

    let canonical_context_families: Vec<&str> =
        (0..language_pack_es_es::context_family_count(pack))
            .filter_map(|index| u32::try_from(index).ok())
            .filter_map(|index| language_pack_es_es::context_family_at(pack, index))
            .collect();

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

    let known = &normalized_targets;
    let mut seed_candidates: Vec<u32> = (0..language_pack_es_es::seed_count(pack))
        .filter_map(|index| u32::try_from(index).ok())
        .filter(|&index| {
            let Some(seed) = language_pack_es_es::seed_at(pack, index) else {
                return false;
            };
            let canonical = language_pack_es_es::normalize_target_text(
                seed.target_text,
                language_pack_api::NormalizationMode::Canonical,
            )
            .0;
            !known.iter().any(|value| value == &canonical)
        })
        .collect();
    let preferred_scenarios = snapshot
        .plan
        .get("priority_scenarios")
        .and_then(serde_json::Value::as_array);
    seed_candidates.sort_by_key(|&index| {
        let scenario_id = language_pack_es_es::seed_at(pack, index)
            .map(|seed| seed.scenario_id)
            .unwrap_or("");
        preferred_scenarios
            .and_then(|values| {
                values
                    .iter()
                    .position(|scenario| scenario.as_str() == Some(scenario_id))
            })
            .unwrap_or(usize::MAX)
    });
    let suggested_seeds: Vec<serde_json::Value> = seed_candidates
        .into_iter()
        .take(10)
        .filter_map(|index| language_pack_es_es::seed_at(pack, index))
        .map(|seed| {
            serde_json::json!({
                "target_text": seed.target_text,
                "prompt": seed.prompt,
                "scenario_id": seed.scenario_id,
                "preferred_scenario": preferred_scenarios.is_some_and(|values| {
                    values
                        .iter()
                        .any(|scenario| scenario.as_str() == Some(seed.scenario_id))
                })
            })
        })
        .collect();

    let curriculum_scenarios: Vec<serde_json::Value> =
        (0..language_pack_es_es::scenario_count(pack))
            .filter_map(|index| u32::try_from(index).ok())
            .filter_map(|index| language_pack_es_es::scenario_at(pack, index))
            .map(|scenario| {
                serde_json::json!({
                    "id": scenario.id,
                    "title": scenario.title,
                    "level": scenario.level,
                })
            })
            .collect();

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
        "language_pack": manifest,
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
        "suggested_new_phrases": suggested_seeds,
        "existing_card_answers": existing_card_answers,
        "review_summary_7d": {
            "reviews": week_review_count,
            "again_count": again_count,
        },
        "redesign_candidates": redesign,
        "open_recommendations": open_recommendations,
        "curriculum_scenarios": curriculum_scenarios,
        "canonical_context_families": canonical_context_families,
        "lesson_result_contract": lesson_result_contract(profile_id, &pack_id, &manifest),
    }))
}

pub(crate) fn dashboard_for(env: &Env, profile_id: &str) -> Result<serde_json::Value> {
    let database = env.connection()?;
    let Some((learner_id, _, _, _, _, current_lesson_number, _)) =
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
           AND method_change_required = 1 AND status != 'resolved'",
        params![profile_id],
    )?;
    let due_rechecks = count(
        &database,
        "SELECT COUNT(*) FROM error_rechecks r
         JOIN error_targets t ON t.id = r.error_target_id
         WHERE t.learner_language_profile_id = ?1
           AND r.completed_at IS NULL AND r.due_at <= ?2",
        params![profile_id, now_text],
    )?;
    let overdue = count(
        &database,
        crate::repositories::review::COUNT_OVERDUE,
        params![profile_id, now_text],
    )?;

    let (reviews_today, review_seconds_today) = {
        let mut statement = database.prepare(crate::repositories::review::SELECT_EVENTS_SINCE)?;
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

    let item_counts = {
        let mut statement = database.prepare(
            "SELECT status, COUNT(*) AS n FROM learning_items
             WHERE learner_language_profile_id = ?1
             GROUP BY status",
        )?;
        statement
            .query_map(params![profile_id], |row| {
                Ok((row.get::<_, String>("status")?, row.get::<_, u32>("n")?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };
    let items: std::collections::BTreeMap<String, u32> = item_counts.into_iter().collect();

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

    let budget = crate::operations::card_budget::remaining_budget(
        &database,
        profile_id,
        at,
        crate::repositories::cards::COUNT_ACTIVATED_TODAY,
    )?;

    let recommendation_details = {
        let mut statement = database.prepare(crate::repositories::recommendations::SELECT_OPEN)?;
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
            "algorithm_version": "fsrs-rs-6.6.1",
            "parameter_set": "default"
        },
    }))
}

fn lesson_result_contract(
    profile_id: &str,
    pack_id: &str,
    manifest: &language_pack_registry::ManifestView<'_>,
) -> serde_json::Value {
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
    });
    let template = serde_json::json!({
        "schema_version": tutor_contract::SCHEMA_VERSION,
        "client_operation_id": "op-<уникальный-id-урока>",
        "language_profile_id": profile_id,
        "language_pack_id": pack_id,
        "language_pack_version": manifest.version,
        "lesson": { "scenario": "<scenario_id>", "duration_seconds": 0,
                     "tutor_summary": "…", "next_goal": "…" },
        "metrics": { "speaking_seconds": 0, "unaided_recall_count": 0,
                      "aided_recall_count": 0, "long_pause_count": 0 },
        "new_learning_items": [ { "client_item_key": "new-1", "item_type": "phrase",
                                   "target_text": "…", "prompt_text": "…",
                                   "prompt_language": "ru" } ],
        "evidence": [ { "client_item_key": "new-1",
                         "kind": "spontaneous_production", "success": true,
                         "context": "<конкретная реальная ситуация>",
                         "context_family": "<семантическая функция, напр. transport.ask_stop>",
                         "assistance_level": 0,
                         "response_millis": 1800,
                         "probe_phase": "diagnostic",
                         "pronunciation_intelligible": true,
                         "pronunciation_repeat_required": false } ],
        "interventions": [ { "action": "fade_prompts", "client_item_key": "new-1",
                              "target_capability": "spontaneous_production",
                              "evidence_index": 0 } ],
        "listening_attempts": [ { "client_item_key": "new-1", "success": true,
                                  "assistance_level": 0, "response_millis": 1400,
                                  "probe_phase": "diagnostic",
                                  "context_tag": "<конкретная ситуация слушания>",
                                  "source_reference": "lesson-audio-1" } ],
        "error_occurrences": [ { "error_target_id": null, "core_category": "grammar",
                                  "pack_error_code": "es.…", "original_production": "…",
                                  "corrected_production": "…" } ],
        "card_mutations": [ { "operation": "create", "client_operation_id": "op-…-c1",
                               "card": { "intent": "production", "prompt": "…",
                                          "answer": "…", "prompt_language": "ru",
                                          "creation_reason": "…" } } ],
    });
    serde_json::json!({
        "schema_version": tutor_contract::SCHEMA_VERSION,
        "submit": {
            "mcp_tool": "complete_lesson",
            "json_rpc_method": "complete_lesson"
        },
        "instructions": "В конце урока верни ровно один JSON-блок schema_version=2 по template. Завершение требует минимум 2 полностью измеренных diagnostic probes. Для каждого выбранного evidence/listening probe обязательны assistance_level, response_millis, probe_phase и конкретный context/context_tag; если измерение ненадёжно, проведи другой probe, не выдумывай значение. Для production/transfer обязательны pronunciation_intelligible и pronunciation_repeat_required. В interventions записывай только реально применённые методы, а не запланированные tutor_tasks. Для существующего материала используй learning_item_id; для материала этого урока — client_item_key. Для transfer используй canonical context_family. assistance_level=0 только при реально самостоятельном ответе; после подсказки используй prompted_production.",
        "enums": enums,
        "template": template,
    })
}

#[cfg(test)]
// Test fixtures intentionally fail fast and use compact tuple snapshots.
#[allow(clippy::unwrap_used, clippy::type_complexity)]
mod error_projection_tests {
    use super::*;

    const ERROR_ROW: &str = "SELECT 'err-1' AS id, 'llp-1' AS learner_language_profile_id,
        'es-ES' AS language_pack_id, '0.2.0' AS language_pack_version,
        'grammar' AS core_category, 'CODE' AS pack_error_code,
        'estoy médico' AS original_production, 'soy médico' AS corrected_production,
        'explain' AS explanation, 5 AS occurrence_count,
        1 AS method_change_required, 'recheck' AS status";

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
                "id":"err-1","language_profile_id":"llp-1","language_pack_id":"es-ES",
                "language_pack_version":"0.2.0","core_category":"grammar","pack_error_code":"CODE",
                "original_production":"estoy médico","corrected_production":"soy médico",
                "explanation":"explain","occurrence_count":5,"method_change_required":true,
                "status":"recheck"
            })
        );
    }

    #[test]
    fn active_error_projection_matches_context_fixture() {
        let db = rusqlite::Connection::open_in_memory().unwrap();
        let expected = serde_json::json!({
            "error_target_id":"err-1","core_category":"grammar","pack_error_code":"CODE",
            "original_production":"estoy médico","corrected_production":"soy médico",
            "occurrence_count":5,"method_change_required":true
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
            ERROR_ROW.replace(
                "'0.2.0' AS language_pack_version",
                "'nope' AS language_pack_version",
            ),
        ] {
            assert!(
                db.query_row(&invalid, [], project_error_wire)
                    .unwrap()
                    .is_none()
            );
        }
    }
}
