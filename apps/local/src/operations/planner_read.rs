//! Canonical bounded planner-signal reads for both active lessons and tutor queries.
//! Returns borrowed preferences and scalar derived metrics, without owned DTO hops.
use crate::Result;
use crate::repositories::to_rfc3339;
use rusqlite::{Connection, params};

const SIGNIFICANTLY_OVERDUE_SECONDS: i64 = 86_400;

pub(crate) fn read_planner_input<'a>(
    database: &Connection,
    profile_id: &str,
    at: learning_core::Timestamp,
    preferences: adaptation::PlannerPreferencesView<'a>,
) -> Result<adaptation::LessonPlannerInputView<'a>> {
    let at_text = to_rfc3339(at);
    let now_text = at_text.as_str();
    let count = |sql: &str, bind: &[&dyn rusqlite::ToSql]| -> Result<u32> {
        Ok(database.query_row(sql, bind, |row| row.get(0))?)
    };

    let due_reviews = count(
        "SELECT COUNT(*) FROM card_schedule_state s
         JOIN cards c ON c.id=s.card_id
         WHERE c.learner_language_profile_id=?1
           AND c.status IN ('new','learning','review','relearning')
           AND s.due_at IS NOT NULL AND s.due_at<=?2",
        &[&profile_id, &now_text],
    )?;
    let overdue_at = to_rfc3339(learning_core::Timestamp(
        at.0 - SIGNIFICANTLY_OVERDUE_SECONDS,
    ));
    let overdue_reviews = count(
        crate::repositories::review::COUNT_OVERDUE,
        &[&profile_id, &overdue_at],
    )?;
    let active_errors = count(
        "SELECT COUNT(*) FROM error_targets
         WHERE learner_language_profile_id=?1
           AND status IN ('active','recheck','escalated')",
        &[&profile_id],
    )?;
    let method_change_errors = count(
        "SELECT COUNT(*) FROM error_targets
         WHERE learner_language_profile_id=?1
           AND method_change_required=1 AND status!='resolved'",
        &[&profile_id],
    )?;
    let due_error_rechecks = count(
        "SELECT COUNT(*) FROM error_rechecks r
         JOIN error_targets t ON t.id=r.error_target_id
         WHERE t.learner_language_profile_id=?1
           AND r.completed_at IS NULL AND r.due_at<=?2",
        &[&profile_id, &now_text],
    )?;
    let open_recommendations = count(
        "SELECT COUNT(*) FROM recommendations
         WHERE learner_language_profile_id=?1 AND status='open'",
        &[&profile_id],
    )?;

    let recent = recent_signals(database, profile_id)?;

    let new_card_budget = super::card_budget::remaining_budget(
        database,
        profile_id,
        at,
        crate::repositories::cards::COUNT_ACTIVATED_TODAY,
    )?;
    Ok(adaptation::LessonPlannerInputView {
        due_reviews,
        overdue_reviews,
        due_error_rechecks,
        active_errors,
        method_change_errors,
        open_recommendations,
        new_card_budget,
        recent,
        preferences,
    })
}
fn recent_signals(
    database: &rusqlite::Connection,
    profile_id: &str,
) -> Result<adaptation::RecentLearningSignals> {
    let mut lessons = 0_u8;
    let mut speaking = 0_u64;
    let mut duration = 0_u64;
    let mut unaided = 0_u64;
    let mut aided = 0_u64;
    let mut listening_sum = 0.0;
    let mut listening_count = 0_u32;
    let mut latency_sum = 0_u64;
    let mut latency_count = 0_u32;
    let mut task_attempts = 0_u64;
    let mut task_successes = 0_u64;
    let mut confidence_sum = 0.0;
    let mut confidence_count = 0_u32;

    let mut statement = database.prepare(
        "SELECT l.duration_seconds,m.speaking_seconds,m.unaided_recall_count,
                m.aided_recall_count,m.listening_score,m.avg_answer_millis,
                m.task_attempt_count,m.task_success_count,m.task_success_rubric,m.confidence
         FROM lessons l
         JOIN lesson_metrics m ON m.lesson_id=l.id
         WHERE l.learner_language_profile_id=?1 AND l.status='completed'
         ORDER BY l.lesson_number DESC
         LIMIT 5",
    )?;
    let mut rows = statement.query(params![profile_id])?;
    while let Some(row) = rows.next()? {
        lessons = lessons.saturating_add(1);
        let duration_seconds: Option<u32> = row.get(0)?;
        let speaking_seconds: u32 = row.get(1)?;
        let unaided_count: u32 = row.get(2)?;
        let aided_count: u32 = row.get(3)?;
        let listening_score: Option<f64> = row.get(4)?;
        let avg_answer_millis: Option<u32> = row.get(5)?;
        let task_attempt_count: u32 = row.get(6)?;
        let task_success_count: u32 = row.get(7)?;
        let task_success_rubric: Option<String> = row.get(8)?;
        let confidence: Option<f64> = row.get(9)?;

        speaking += u64::from(speaking_seconds);
        duration += u64::from(duration_seconds.unwrap_or_default());
        unaided += u64::from(unaided_count);
        aided += u64::from(aided_count);
        if let Some(value) = listening_score {
            listening_sum += value;
            listening_count += 1;
        }
        if let Some(value) = avg_answer_millis {
            latency_sum += u64::from(value);
            latency_count += 1;
        }
        if task_success_rubric.as_deref()
            == Some(tutor_contract::COMMUNICATIVE_TASK_SUCCESS_RUBRIC_V1)
        {
            task_attempts += u64::from(task_attempt_count);
            task_successes += u64::from(task_success_count);
        }
        if let Some(value) = confidence {
            confidence_sum += value;
            confidence_count += 1;
        }
    }

    #[allow(clippy::cast_precision_loss)]
    Ok(adaptation::RecentLearningSignals {
        lessons,
        speaking_ratio: (duration > 0).then_some(speaking as f64 / duration as f64),
        unaided_recall_rate: (unaided + aided > 0)
            .then_some(unaided as f64 / (unaided + aided) as f64),
        listening_score: (listening_count > 0)
            .then_some(listening_sum / f64::from(listening_count)),
        avg_answer_millis: (latency_count > 0)
            .then(|| u32::try_from(latency_sum / u64::from(latency_count)).unwrap_or(u32::MAX)),
        communicative_task_success_rate: (task_attempts > 0)
            .then_some(task_successes as f64 / task_attempts as f64),
        confidence: (confidence_count > 0).then_some(confidence_sum / f64::from(confidence_count)),
    })
}
