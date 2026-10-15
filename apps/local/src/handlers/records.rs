//! Learning state, next lesson, errors, listening, and tutor-context operations.

use crate::api::{new_id, now, operation_error};
use crate::handlers::profiles::fetch_profile;
use crate::repositories::{records, to_rfc3339};
use crate::{Env, Result};
use rusqlite::{OptionalExtension as _, params};
use std::collections::HashMap;

const SIGNIFICANTLY_OVERDUE_SECONDS: i64 = 86_400;

/// Learning-state query — item counts by
/// status + active error targets + due rechecks.
pub(crate) fn learning_state_for(env: &Env, profile_id: &str) -> Result<serde_json::Value> {
    struct CountRow {
        status: String,
        n: u32,
    }
    impl CountRow {
        fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
            Ok(Self {
                status: row.get("status")?,
                n: row.get("n")?,
            })
        }
    }
    let database = env.connection()?;
    let counts: Vec<CountRow> = {
        let mut statement = database.prepare(
            "SELECT status, COUNT(*) AS n FROM learning_items
         WHERE learner_language_profile_id = ?1 GROUP BY status",
        )?;
        statement
            .query_map(params![profile_id], CountRow::from_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };
    let items: serde_json::Map<String, serde_json::Value> = counts
        .into_iter()
        .map(|row| (row.status, serde_json::json!(row.n)))
        .collect();
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

#[derive(serde::Deserialize)]
struct PlannerMetricRow {
    duration_seconds: Option<u32>,
    speaking_seconds: u32,
    unaided_recall_count: u32,
    aided_recall_count: u32,
    listening_score: Option<f64>,
    avg_answer_millis: Option<u32>,
    confidence: Option<f64>,
}
impl PlannerMetricRow {
    fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            duration_seconds: row.get("duration_seconds")?,
            speaking_seconds: row.get("speaking_seconds")?,
            unaided_recall_count: row.get("unaided_recall_count")?,
            aided_recall_count: row.get("aided_recall_count")?,
            listening_score: row.get("listening_score")?,
            avg_answer_millis: row.get("avg_answer_millis")?,
            confidence: row.get("confidence")?,
        })
    }
}

pub(crate) fn adaptive_lesson_planning(
    database: &rusqlite::Connection,
    profile: &learning_core::LearnerLanguageProfile,
) -> Result<(
    adaptation::LessonPlannerInput,
    adaptation::AdaptiveLessonPlan,
)> {
    fn count<P: rusqlite::Params>(
        database: &rusqlite::Connection,
        sql: &str,
        params: P,
    ) -> Result<u32> {
        Ok(database.query_row(sql, params, |row| row.get(0))?)
    }

    let at = now();
    let now_text = to_rfc3339(at);
    let due_reviews = count(
        database,
        "SELECT COUNT(*) FROM card_schedule_state s
         JOIN cards c ON c.id = s.card_id
         WHERE c.learner_language_profile_id = ?1
           AND c.status IN ('new','learning','review','relearning')
           AND s.due_at IS NOT NULL AND s.due_at <= ?2",
        params![profile.id.0, now_text],
    )?;
    let significantly_overdue_at = to_rfc3339(learning_core::Timestamp(
        at.0 - SIGNIFICANTLY_OVERDUE_SECONDS,
    ));
    let overdue_reviews = count(
        database,
        crate::repositories::review::COUNT_OVERDUE,
        params![profile.id.0, significantly_overdue_at],
    )?;
    let active_errors = count(
        database,
        "SELECT COUNT(*) FROM error_targets
         WHERE learner_language_profile_id = ?1
           AND status IN ('active','recheck','escalated')",
        params![profile.id.0],
    )?;
    let method_change_errors = count(
        database,
        "SELECT COUNT(*) FROM error_targets
         WHERE learner_language_profile_id = ?1
           AND method_change_required = 1 AND status != 'resolved'",
        params![profile.id.0],
    )?;
    let due_error_rechecks = count(
        database,
        "SELECT COUNT(*) FROM error_rechecks r
         JOIN error_targets t ON t.id = r.error_target_id
         WHERE t.learner_language_profile_id = ?1
           AND r.completed_at IS NULL AND r.due_at <= ?2",
        params![profile.id.0, now_text],
    )?;
    let open_recommendations = count(
        database,
        "SELECT COUNT(*) FROM recommendations
         WHERE learner_language_profile_id = ?1 AND status = 'open'",
        params![profile.id.0],
    )?;
    let rows: Vec<PlannerMetricRow> = {
        let mut statement = database.prepare(
            "SELECT l.duration_seconds, m.speaking_seconds, m.unaided_recall_count,
                m.aided_recall_count, m.listening_score, m.avg_answer_millis,
                m.confidence
         FROM lessons l
         JOIN lesson_metrics m ON m.lesson_id = l.id
         WHERE l.learner_language_profile_id = ?1 AND l.status = 'completed'
         ORDER BY l.lesson_number DESC LIMIT 5",
        )?;
        statement
            .query_map(params![profile.id.0], PlannerMetricRow::from_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };

    let recent = aggregate_recent_signals(&rows);
    let preferences = planner_preferences(&profile.preferences);
    let new_card_budget = crate::handlers::review::effective_budget(database, &profile.id.0, at)?;
    let input = adaptation::LessonPlannerInput {
        due_reviews,
        overdue_reviews,
        due_error_rechecks,
        active_errors,
        method_change_errors,
        open_recommendations,
        new_card_budget,
        recent,
        preferences,
    };
    let plan = adaptation::plan_next_lesson(&input);
    Ok((input, plan))
}

#[allow(clippy::cast_precision_loss)]
fn aggregate_recent_signals(rows: &[PlannerMetricRow]) -> adaptation::RecentLearningSignals {
    let lessons = u8::try_from(rows.len()).unwrap_or(u8::MAX);
    let speaking: u64 = rows.iter().map(|row| u64::from(row.speaking_seconds)).sum();
    let duration: u64 = rows
        .iter()
        .filter_map(|row| row.duration_seconds)
        .map(u64::from)
        .sum();
    let unaided: u64 = rows
        .iter()
        .map(|row| u64::from(row.unaided_recall_count))
        .sum();
    let aided: u64 = rows
        .iter()
        .map(|row| u64::from(row.aided_recall_count))
        .sum();

    let average_f64 = |values: Vec<f64>| {
        if values.is_empty() {
            None
        } else {
            Some(values.iter().sum::<f64>() / values.len() as f64)
        }
    };
    let average_u32 = |values: Vec<u32>| {
        if values.is_empty() {
            None
        } else {
            let sum: u64 = values.iter().map(|value| u64::from(*value)).sum();
            u32::try_from(sum / values.len() as u64).ok()
        }
    };

    adaptation::RecentLearningSignals {
        lessons,
        speaking_ratio: (duration > 0).then_some(speaking as f64 / duration as f64),
        unaided_recall_rate: (unaided + aided > 0)
            .then_some(unaided as f64 / (unaided + aided) as f64),
        listening_score: average_f64(rows.iter().filter_map(|row| row.listening_score).collect()),
        avg_answer_millis: average_u32(
            rows.iter()
                .filter_map(|row| row.avg_answer_millis)
                .collect(),
        ),
        confidence: average_f64(rows.iter().filter_map(|row| row.confidence).collect()),
    }
}

fn planner_preferences(value: &serde_json::Value) -> adaptation::PlannerPreferences {
    adaptation::PlannerPreferences {
        max_new_items_per_lesson: value
            .get("max_new_items_per_lesson")
            .and_then(serde_json::Value::as_u64)
            .and_then(|value| u8::try_from(value).ok()),
        preferred_difficulty: value
            .get("preferred_difficulty")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned),
        priority_scenarios: value
            .get("priority_scenarios")
            .and_then(serde_json::Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .filter_map(serde_json::Value::as_str)
                    .map(str::to_owned)
                    .take(20)
                    .collect()
            })
            .unwrap_or_default(),
    }
}

struct ItemCapabilityRow {
    id: String,
    target_text: String,
    status: String,
}
impl ItemCapabilityRow {
    fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            target_text: row.get("target_text")?,
            status: row.get("status")?,
        })
    }
}

struct EvidenceCapabilityRow {
    learning_item_id: String,
    kind: String,
    success: u8,
    pack_metadata_json: Option<String>,
    recorded_at: String,
}
impl EvidenceCapabilityRow {
    fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            learning_item_id: row.get("learning_item_id")?,
            kind: row.get("kind")?,
            success: row.get("success")?,
            pack_metadata_json: row.get("pack_metadata_json")?,
            recorded_at: row.get("recorded_at")?,
        })
    }
}

struct ListeningCapabilityRow {
    learning_item_id: String,
    success: u8,
    recorded_at: String,
}
impl ListeningCapabilityRow {
    fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            learning_item_id: row.get("learning_item_id")?,
            success: row.get("success")?,
            recorded_at: row.get("recorded_at")?,
        })
    }
}

pub(crate) fn item_capability_states(
    database: &rusqlite::Connection,
    profile_id: &str,
) -> Result<Vec<adaptation::ItemCapabilityState>> {
    let items: Vec<ItemCapabilityRow> = {
        let mut statement = database.prepare(
            "SELECT id, target_text, status
         FROM learning_items
         WHERE learner_language_profile_id = ?1
           AND status IN ('introduced','learning','transfer_pending',
                          'provisionally_mastered','mastered')
         ORDER BY updated_at DESC LIMIT 200",
        )?;
        statement
            .query_map(params![profile_id], ItemCapabilityRow::from_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };
    let evidence_rows: Vec<EvidenceCapabilityRow> = {
        let mut statement = database.prepare(
            "WITH ranked AS (
            SELECT e.learning_item_id, e.kind, e.success, e.pack_metadata_json,
                   e.recorded_at,
                   ROW_NUMBER() OVER (
                       PARTITION BY e.learning_item_id, e.kind
                       ORDER BY e.recorded_at DESC, e.id DESC
                   ) AS rn
            FROM learning_item_evidence e
            JOIN learning_items i ON i.id = e.learning_item_id
            WHERE i.learner_language_profile_id = ?1
              AND i.status IN ('introduced','learning','transfer_pending',
                               'provisionally_mastered','mastered')
              AND e.kind != 'listening'
         )
         SELECT learning_item_id, kind, success, pack_metadata_json, recorded_at
         FROM ranked
         WHERE rn <= 12
         ORDER BY recorded_at DESC",
        )?;
        statement
            .query_map(params![profile_id], EvidenceCapabilityRow::from_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };
    let listening_rows: Vec<ListeningCapabilityRow> = {
        let mut statement = database.prepare(
            "WITH ranked AS (
            SELECT learning_item_id, success, recorded_at,
                   ROW_NUMBER() OVER (
                       PARTITION BY learning_item_id
                       ORDER BY recorded_at DESC, id DESC
                   ) AS rn
            FROM listening_attempts
            WHERE learner_language_profile_id = ?1
              AND learning_item_id IS NOT NULL
         )
         SELECT learning_item_id, success, recorded_at
         FROM ranked
         WHERE rn <= 12
         ORDER BY recorded_at DESC",
        )?;
        statement
            .query_map(params![profile_id], ListeningCapabilityRow::from_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };

    let mut observations: HashMap<String, Vec<adaptation::EvidenceObservation>> = HashMap::new();
    for row in evidence_rows {
        let Some(capability) = capability_kind(&row.kind) else {
            continue;
        };
        let metadata = row
            .pack_metadata_json
            .as_deref()
            .and_then(|value| serde_json::from_str::<serde_json::Value>(value).ok());
        let assistance_level = metadata
            .as_ref()
            .and_then(|value| value.get("assistance_level"))
            .and_then(serde_json::Value::as_u64)
            .and_then(|value| u8::try_from(value).ok());
        let response_millis = metadata
            .as_ref()
            .and_then(|value| value.get("response_millis"))
            .and_then(serde_json::Value::as_u64)
            .and_then(|value| u32::try_from(value).ok());
        let context_key = metadata
            .as_ref()
            .and_then(|value| value.get("context_key"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned);
        let context_family = metadata
            .as_ref()
            .and_then(|value| value.get("context_family"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned);
        let recorded_at_unix = crate::repositories::from_rfc3339(&row.recorded_at)
            .map(|value| value.0)
            .unwrap_or_default();
        observations.entry(row.learning_item_id).or_default().push(
            adaptation::EvidenceObservation {
                capability,
                success: row.success != 0,
                assistance_level,
                response_millis,
                context_key,
                context_family,
                recorded_at_unix,
            },
        );
    }
    for row in listening_rows {
        let recorded_at_unix = crate::repositories::from_rfc3339(&row.recorded_at)
            .map(|value| value.0)
            .unwrap_or_default();
        observations.entry(row.learning_item_id).or_default().push(
            adaptation::EvidenceObservation {
                capability: adaptation::CapabilityKind::Listening,
                success: row.success != 0,
                assistance_level: None,
                response_millis: None,
                context_key: None,
                context_family: None,
                recorded_at_unix,
            },
        );
    }

    let now_unix = now().0;
    Ok(items
        .into_iter()
        .map(|item| {
            let evidence = observations.remove(&item.id).unwrap_or_default();
            adaptation::project_item_capabilities(
                item.id,
                item.target_text,
                item.status,
                &evidence,
                now_unix,
            )
        })
        .collect())
}

fn capability_kind(kind: &str) -> Option<adaptation::CapabilityKind> {
    match kind {
        "recognition" => Some(adaptation::CapabilityKind::Recognition),
        // listening_attempts is canonical; generic listening evidence is ignored.
        "listening" => None,
        "prompted_production" => Some(adaptation::CapabilityKind::PromptedProduction),
        "spontaneous_production" => Some(adaptation::CapabilityKind::SpontaneousProduction),
        "transfer" => Some(adaptation::CapabilityKind::Transfer),
        "correction" => None,
        _ => None,
    }
}

struct AdaptiveLessonSnapshot {
    planner_input: adaptation::LessonPlannerInput,
    plan: adaptation::AdaptiveLessonPlan,
    states: Vec<adaptation::ItemCapabilityState>,
    tutor_tasks: Vec<adaptation::TutorTask>,
}

fn adaptive_lesson_snapshot(
    database: &rusqlite::Connection,
    profile: &learning_core::LearnerLanguageProfile,
) -> Result<AdaptiveLessonSnapshot> {
    let (planner_input, plan) = adaptive_lesson_planning(database, profile)?;
    let states = item_capability_states(database, &profile.id.0)?;
    let task_candidates = adaptation::score_task_candidates(&states, &plan, 24);
    let focus_threads = adaptation::build_tutor_focus_threads(&states, now().0, 32);
    let tutor_tasks = adaptation::build_tutor_task_queue(&focus_threads, &task_candidates, 16);
    Ok(AdaptiveLessonSnapshot {
        planner_input,
        plan,
        states,
        tutor_tasks,
    })
}

/// Next-lesson query.
pub(crate) fn next_lesson_for(env: &Env, profile_id: &str) -> Result<serde_json::Value> {
    struct GoalRow {
        next_goal: Option<String>,
        scenario: String,
    }
    impl GoalRow {
        fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
            Ok(Self {
                next_goal: row.get("next_goal")?,
                scenario: row.get("scenario")?,
            })
        }
    }
    let database = env.connection()?;
    let Some(profile_row) = fetch_profile(&database, profile_id)? else {
        return operation_error(404, "not_found", "unknown profile");
    };
    let Ok(profile) = profile_row.into_domain() else {
        return operation_error(500, "corrupt_row", "stored profile is invalid");
    };
    let last = database
        .query_row(
            "SELECT next_goal, scenario FROM lessons
             WHERE learner_language_profile_id = ?1 AND status = 'completed'
             ORDER BY lesson_number DESC LIMIT 1",
            params![profile.id.0],
            GoalRow::from_row,
        )
        .optional()?;
    let snapshot = adaptive_lesson_snapshot(&database, &profile)?;
    crate::json(&serde_json::json!({
        "lesson_number": profile.current_lesson_number + 1,
        "estimated_level": profile.estimated_level,
        "learner_preferences": profile.preferences,
        "previous_goal": last.as_ref().and_then(|row| row.next_goal.clone()),
        "previous_scenario": last.map(|row| row.scenario),
        "planner_input": snapshot.planner_input,
        "adaptive_plan": snapshot.plan,
        "tutor_tasks": snapshot.tutor_tasks,
        "tracked_item_count": snapshot.states.len(),
    }))
}

/// Error-target query with optional status filter.
pub(crate) fn list_error_targets_for(
    env: &Env,
    profile_id: &str,
    status: Option<&str>,
) -> Result<serde_json::Value> {
    let database = env.connection()?;
    let mut statement = database.prepare(records::SELECT_ERROR_TARGETS)?;
    let rows = statement
        .query_map(
            params![profile_id, status, 100],
            records::ErrorTargetRow::from_row,
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let domain: Vec<_> = rows
        .into_iter()
        .filter_map(|row| row.into_domain().ok())
        .collect();
    crate::json(&domain)
}

pub(crate) fn get_error_target(env: &Env, target_id: &str) -> Result<serde_json::Value> {
    let database = env.connection()?;
    let row = database
        .query_row(
            records::SELECT_ERROR_TARGET,
            params![target_id],
            records::ErrorTargetRow::from_row,
        )
        .optional()?;
    match row.and_then(|row| row.into_domain().ok()) {
        Some(target) => crate::json(&target),
        None => operation_error(404, "not_found", "unknown error target"),
    }
}

pub(crate) fn complete_recheck(
    env: &Env,
    target_id: &str,
    recheck_id: &str,
    passed: bool,
) -> Result<serde_json::Value> {
    let mut database = env.connection()?;
    let transaction =
        database.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let at = to_rfc3339(now());

    transaction.execute(
        records::COMPLETE_ERROR_RECHECK,
        params![recheck_id, at, u8::from(passed)],
    )?;
    let new_status = if passed { "recheck" } else { "active" };
    transaction.execute(
        "UPDATE error_targets SET status = ?2, updated_at = ?3 WHERE id = ?1",
        params![target_id, new_status, at],
    )?;

    if !passed {
        let profile_id: Option<String> = transaction
            .query_row(
                "SELECT learner_language_profile_id FROM error_targets WHERE id = ?1",
                params![target_id],
                |row| row.get(0),
            )
            .optional()?;
        let offset_days = transaction
            .query_row(
                "SELECT offset_days FROM error_rechecks WHERE id = ?1",
                params![recheck_id],
                |row| row.get::<_, u32>(0),
            )
            .optional()?
            .unwrap_or(1);
        if let Some(profile_id) = profile_id {
            let recommendation = adaptation::recheck_failed(target_id, offset_days);
            crate::handlers::recommendations::insert_all(
                &transaction,
                &profile_id,
                &[recommendation],
            )?;
        }
    }

    transaction.commit()?;
    crate::json(&serde_json::json!({ "recheck_id": recheck_id, "passed": passed }))
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
    pub transcript: Option<String>,
    pub success: bool,
}

pub(crate) fn create_listening_attempt(
    env: &Env,
    body: ListeningAttemptParams,
) -> Result<serde_json::Value> {
    let id = new_id("la");
    let database = env.connection()?;
    database.execute(
        records::INSERT_LISTENING_ATTEMPT,
        params![
            id,
            body.language_profile_id,
            body.learning_item_id,
            body.lesson_id,
            body.source_reference,
            body.transcript,
            u8::from(body.success),
            to_rfc3339(now()),
        ],
    )?;
    crate::json(&serde_json::json!({ "id": id }))
}

fn active_error_context(
    database: &rusqlite::Connection,
    profile_id: &str,
) -> Result<Vec<serde_json::Value>> {
    let rows = {
        let mut statement = database.prepare(records::SELECT_ERROR_TARGETS)?;
        statement
            .query_map(
                params![profile_id, Option::<String>::None, 20],
                records::ErrorTargetRow::from_row,
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };
    Ok(rows
        .into_iter()
        .filter_map(|row| row.into_domain().ok())
        .filter(|target| target.status != learning_core::ErrorTargetStatus::Resolved)
        .map(|target| {
            serde_json::json!({
                "error_target_id": target.id.0,
                "core_category": target.core_category,
                "pack_error_code": target.pack_error_code,
                "original_production": target.original_production,
                "corrected_production": target.corrected_production,
                "occurrence_count": target.occurrence_count,
                "method_change_required": target.method_change_required,
            })
        })
        .collect())
}

struct PlannerHistoryRow {
    planner_version: String,
    input_json: String,
    plan_json: String,
    task_candidates_json: String,
    capability_state_json: String,
    outcome_json: Option<String>,
    created_at: String,
}

impl PlannerHistoryRow {
    fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            planner_version: row.get("planner_version")?,
            input_json: row.get("input_json")?,
            plan_json: row.get("plan_json")?,
            task_candidates_json: row.get("task_candidates_json")?,
            capability_state_json: row.get("capability_state_json")?,
            outcome_json: row.get("outcome_json")?,
            created_at: row.get("created_at")?,
        })
    }
}

struct PlannerHistoryContext {
    summaries: Vec<serde_json::Value>,
    intervention_effectiveness: Vec<adaptation::InterventionEffect>,
}

fn planner_history_context(
    database: &rusqlite::Connection,
    profile_id: &str,
) -> Result<PlannerHistoryContext> {
    let rows = {
        let mut statement = database.prepare(
            "SELECT planner_version, input_json, plan_json, task_candidates_json,
                    capability_state_json, outcome_json, created_at
             FROM planner_decisions
             WHERE learner_language_profile_id = ?1
             ORDER BY created_at DESC LIMIT 10",
        )?;
        statement
            .query_map(params![profile_id], PlannerHistoryRow::from_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };

    let summaries = rows
        .iter()
        .take(5)
        .map(|row| {
            let input =
                serde_json::from_str::<serde_json::Value>(&row.input_json).unwrap_or_default();
            let plan =
                serde_json::from_str::<serde_json::Value>(&row.plan_json).unwrap_or_default();
            let outcome = row
                .outcome_json
                .as_deref()
                .and_then(|value| serde_json::from_str::<serde_json::Value>(value).ok());
            serde_json::json!({
                "planner_version": row.planner_version,
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
                    "error_count": value
                        .get("error_occurrences")
                        .and_then(serde_json::Value::as_array)
                        .map_or(0, Vec::len),
                    "next_goal": value.get("next_goal"),
                })),
                "created_at": row.created_at,
            })
        })
        .collect();

    let calibration_snapshots = rows
        .iter()
        .filter_map(|row| {
            let item_states = serde_json::from_str::<Vec<adaptation::ItemCapabilityState>>(
                &row.capability_state_json,
            )
            .ok()?;
            let task_candidates =
                serde_json::from_str::<Vec<adaptation::TaskCandidate>>(&row.task_candidates_json)
                    .ok()?;
            let at_unix = crate::repositories::from_rfc3339(&row.created_at).ok()?.0;
            Some(adaptation::CalibrationSnapshot {
                at_unix,
                item_states,
                task_candidates,
            })
        })
        .collect::<Vec<_>>();

    Ok(PlannerHistoryContext {
        summaries,
        intervention_effectiveness: adaptation::summarize_intervention_effects(
            &calibration_snapshots,
            6 * 60 * 60,
        ),
    })
}

pub(crate) fn tutor_context_for(
    registry: &language_pack_registry::LanguagePackRegistry,
    env: &Env,
    profile_id: &str,
) -> Result<serde_json::Value> {
    let database = env.connection()?;
    let Some(profile_row) = fetch_profile(&database, profile_id)? else {
        return operation_error(404, "not_found", "unknown profile");
    };
    let Ok(profile) = profile_row.into_domain() else {
        return operation_error(500, "corrupt_row", "stored profile is invalid");
    };
    let Some(pack) = registry.get(&profile.language_pack_id.0) else {
        return operation_error(409, "pack_incompatible", "profile pack not registered");
    };
    let errors = active_error_context(&database, &profile.id.0)?;

    let learner_view = language_pack_api::LearnerLanguageProfile {
        source_language: profile.source_language.0.clone(),
        target_locale: profile.target_locale.0.clone(),
        estimated_level: profile.estimated_level.clone(),
    };
    let instructions = pack.tutor_instructions(&learner_view);
    let snapshot = adaptive_lesson_snapshot(&database, &profile)?;

    let planner_history = planner_history_context(&database, &profile.id.0)?;

    struct RecommendationRow {
        id: String,
        subject_type: String,
        subject_id: Option<String>,
        action: String,
        detail_json: String,
        created_at: String,
    }
    impl RecommendationRow {
        fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
            Ok(Self {
                id: row.get("id")?,
                subject_type: row.get("subject_type")?,
                subject_id: row.get("subject_id")?,
                action: row.get("action")?,
                detail_json: row.get("detail_json")?,
                created_at: row.get("created_at")?,
            })
        }
        fn to_json(&self) -> serde_json::Value {
            serde_json::json!({
                "id": self.id,
                "subject_type": self.subject_type,
                "subject_id": self.subject_id,
                "action": self.action,
                "detail_json": self.detail_json,
                "created_at": self.created_at,
            })
        }
    }
    let open_recommendations = {
        let mut statement = database.prepare(crate::repositories::recommendations::SELECT_OPEN)?;
        statement
            .query_map(params![profile.id.0, 20], RecommendationRow::from_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?
            .iter()
            .map(RecommendationRow::to_json)
            .collect::<Vec<_>>()
    };

    struct RedesignRow {
        recommendation_id: String,
        detail_json: String,
        card_id: String,
        prompt: String,
        answer: String,
        card_status: String,
    }
    impl RedesignRow {
        fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
            Ok(Self {
                recommendation_id: row.get("recommendation_id")?,
                detail_json: row.get("detail_json")?,
                card_id: row.get("card_id")?,
                prompt: row.get("prompt")?,
                answer: row.get("answer")?,
                card_status: row.get("card_status")?,
            })
        }
        fn to_json(&self) -> serde_json::Value {
            serde_json::json!({
                "recommendation_id": self.recommendation_id,
                "detail_json": self.detail_json,
                "card_id": self.card_id,
                "prompt": self.prompt,
                "answer": self.answer,
                "card_status": self.card_status,
            })
        }
    }
    let redesign = {
        let mut statement =
            database.prepare(crate::repositories::recommendations::SELECT_REDESIGN_CANDIDATES)?;
        statement
            .query_map(params![profile.id.0], RedesignRow::from_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?
            .iter()
            .map(RedesignRow::to_json)
            .collect::<Vec<_>>()
    };

    let week_ago = to_rfc3339(learning_core::Timestamp(now().0 - 7 * 86_400));
    let week_events = {
        let mut statement = database.prepare(crate::repositories::review::SELECT_EVENTS_SINCE)?;
        statement
            .query_map(params![profile.id.0, week_ago], |row| {
                row.get::<_, u8>("grade")
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };
    let again_count = week_events.iter().filter(|grade| **grade == 1).count();

    struct ItemRowSlim {
        id: String,
        target_text: String,
        normalized_target_text: String,
        status: String,
    }
    impl ItemRowSlim {
        fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
            Ok(Self {
                id: row.get("id")?,
                target_text: row.get("target_text")?,
                normalized_target_text: row.get("normalized_target_text")?,
                status: row.get("status")?,
            })
        }
    }
    let items = {
        let mut statement = database.prepare(
            "SELECT id, target_text, normalized_target_text, status
             FROM learning_items
             WHERE learner_language_profile_id = ?1
               AND status NOT IN ('candidate','suspended')
             ORDER BY updated_at DESC LIMIT 60",
        )?;
        statement
            .query_map(params![profile.id.0], ItemRowSlim::from_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };
    let items_json: Vec<serde_json::Value> = items
        .iter()
        .map(|item| {
            serde_json::json!({
                "learning_item_id": item.id,
                "target_text": item.target_text,
                "status": item.status
            })
        })
        .collect();

    struct DueRecheckRow {
        recheck_id: String,
        offset_days: u32,
        error_target_id: String,
        original_production: String,
        corrected_production: String,
    }
    impl DueRecheckRow {
        fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
            Ok(Self {
                recheck_id: row.get("recheck_id")?,
                offset_days: row.get("offset_days")?,
                error_target_id: row.get("error_target_id")?,
                original_production: row.get("original_production")?,
                corrected_production: row.get("corrected_production")?,
            })
        }
        fn to_json(&self) -> serde_json::Value {
            serde_json::json!({
                "recheck_id": self.recheck_id,
                "offset_days": self.offset_days,
                "error_target_id": self.error_target_id,
                "original_production": self.original_production,
                "corrected_production": self.corrected_production,
            })
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
            .query_map(
                params![profile.id.0, to_rfc3339(now())],
                DueRecheckRow::from_row,
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?
            .iter()
            .map(DueRecheckRow::to_json)
            .collect::<Vec<_>>()
    };

    let goal: Option<String> = database
        .query_row(
            "SELECT next_goal FROM lessons
             WHERE learner_language_profile_id = ?1 AND status = 'completed'
             ORDER BY lesson_number DESC LIMIT 1",
            params![profile.id.0],
            |row| row.get(0),
        )
        .optional()?
        .flatten();

    let known: Vec<&str> = items
        .iter()
        .map(|item| item.normalized_target_text.as_str())
        .collect();
    let mut seed_candidates: Vec<&language_pack_api::PhraseSeed> = pack
        .curriculum_catalog()
        .seeds
        .iter()
        .filter(|seed| {
            let canonical = pack
                .normalize_target_text(
                    &seed.target_text,
                    language_pack_api::NormalizationMode::Canonical,
                )
                .0;
            !known.contains(&canonical.as_str())
        })
        .collect();
    seed_candidates.sort_by_key(|seed| {
        snapshot
            .plan
            .priority_scenarios
            .iter()
            .position(|scenario| scenario == &seed.scenario_id)
            .unwrap_or(usize::MAX)
    });
    let suggested_seeds: Vec<serde_json::Value> = seed_candidates
        .into_iter()
        .take(10)
        .map(|seed| {
            serde_json::json!({
                "target_text": seed.target_text,
                "prompt": seed.prompt,
                "scenario_id": seed.scenario_id,
                "preferred_scenario": snapshot
                    .plan
                    .priority_scenarios
                    .contains(&seed.scenario_id)
            })
        })
        .collect();

    struct CardSlim {
        answer: String,
        intent: String,
    }
    impl CardSlim {
        fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
            Ok(Self {
                answer: row.get("answer")?,
                intent: row.get("intent")?,
            })
        }
    }
    let existing_cards = {
        let mut statement = database.prepare(
            "SELECT answer, intent FROM cards
             WHERE learner_language_profile_id = ?1 AND status != 'retired'
             ORDER BY created_at DESC LIMIT 50",
        )?;
        statement
            .query_map(params![profile.id.0], CardSlim::from_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };
    let existing_card_answers: Vec<serde_json::Value> = existing_cards
        .iter()
        .map(|card| serde_json::json!({ "answer": card.answer, "intent": card.intent }))
        .collect();

    crate::json(&serde_json::json!({
        "schema_version": "1",
        "language_profile_id": profile.id.0,
        "language_pack": pack.manifest(),
        "next_lesson_number": profile.current_lesson_number + 1,
        "estimated_level": profile.estimated_level,
        "learner_preferences": profile.preferences,
        "tutor_instructions": instructions,
        "planner_input": snapshot.planner_input,
        "adaptive_lesson_plan": snapshot.plan,
        "tutor_tasks": snapshot.tutor_tasks,
        "tracked_item_count": snapshot.states.len(),
        "planner_history": planner_history.summaries,
        "intervention_effectiveness": planner_history.intervention_effectiveness,
        "active_error_targets": errors,
        "previous_next_goal_hypothesis": goal,
        "learning_items": items_json,
        "due_error_rechecks": due_rechecks,
        "suggested_new_phrases": suggested_seeds,
        "existing_card_answers": existing_card_answers,
        "review_summary_7d": {
            "reviews": week_events.len(),
            "again_count": again_count,
        },
        "redesign_candidates": redesign,
        "open_recommendations": open_recommendations,
        "curriculum_scenarios": pack.curriculum_catalog().scenarios,
        "lesson_result_contract": lesson_result_contract(&profile, pack),
    }))
}

pub(crate) fn dashboard_for(env: &Env, profile_id: &str) -> Result<serde_json::Value> {
    let database = env.connection()?;
    let Some(profile_row) = fetch_profile(&database, profile_id)? else {
        return operation_error(404, "not_found", "unknown profile");
    };
    let Ok(profile) = profile_row.into_domain() else {
        return operation_error(500, "corrupt_row", "stored profile is invalid");
    };
    let at = crate::api::now();
    let now_text = to_rfc3339(at);
    let day_start = to_rfc3339(learning_core::Timestamp(at.0 - at.0 % 86_400));

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

    let response_millis = {
        let mut statement = database.prepare(crate::repositories::review::SELECT_EVENTS_SINCE)?;
        statement
            .query_map(params![profile_id, day_start], |row| {
                row.get::<_, u32>("response_millis")
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };
    let review_seconds_today: u64 = response_millis
        .iter()
        .map(|value| u64::from(*value))
        .sum::<u64>()
        / 1000;

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

    let budget = crate::handlers::review::effective_budget(&database, profile_id, at)?;

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
        reviews_today: u32::try_from(response_millis.len()).unwrap_or(u32::MAX),
        review_seconds_today: u32::try_from(review_seconds_today).unwrap_or(u32::MAX),
        active_error_targets: active_errors,
        due_error_rechecks: due_rechecks,
        next_lesson_number: profile.current_lesson_number + 1,
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
    profile: &learning_core::LearnerLanguageProfile,
    pack: &dyn language_pack_api::LanguagePack,
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
        "schema_version": "1",
        "client_operation_id": "op-<уникальный-id-урока>",
        "language_profile_id": profile.id.0,
        "language_pack_id": profile.language_pack_id.0,
        "language_pack_version": pack.manifest().version,
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
                         "probe_phase": "delayed_retrieval" } ],
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
        "instructions": "В конце урока верни ровно один JSON-блок по template. Для существующего материала используй learning_item_id; для материала, введённого в этом же уроке, задай client_item_key. В evidence указывай конкретный context; для transfer обязательно stable context_family вида domain.communicative_function. assistance_level=0 ставь только при реально самостоятельном ответе; после подсказки используй prompted_production. По возможности фиксируй response_millis и probe_phase; error code — из таксономии пака.",
        "enums": enums,
        "template": template,
    })
}
