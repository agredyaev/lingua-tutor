//! Canonical bounded planner-signal reads for both active lessons and tutor queries.
//! Returns borrowed preferences and scalar derived metrics, without owned DTO hops.
use crate::Result;
use crate::repositories::to_rfc3339;
use rusqlite::{Connection, params};

const SIGNIFICANTLY_OVERDUE_SECONDS: i64 = 86_400;
const RECENT_SIGNAL_WINDOW_SECONDS: i64 = 28 * 86_400;

pub(crate) fn read_planner_input<'a>(
    database: &Connection,
    profile_id: &str,
    at: learning_core::Timestamp,
    preferences: adaptation::PlannerPreferencesView<'a>,
) -> Result<adaptation::LessonPlannerInputView<'a>> {
    let at_text = to_rfc3339(at);
    let now_text = at_text.as_str();

    let due_reviews: u32 = database.query_row(
        "SELECT COUNT(*) FROM card_schedule_state s
         JOIN cards c ON c.id=s.card_id
         WHERE c.learner_language_profile_id=?1
           AND c.status IN ('new','learning','review','relearning')
           AND s.due_at IS NOT NULL AND s.due_at<=?2",
        [profile_id, now_text],
        |row| row.get(0),
    )?;

    let overdue_at = to_rfc3339(learning_core::Timestamp(
        at.0 - SIGNIFICANTLY_OVERDUE_SECONDS,
    ));
    let overdue_reviews: u32 = database.query_row(
        crate::repositories::review::COUNT_OVERDUE,
        [profile_id, overdue_at.as_str()],
        |row| row.get(0),
    )?;

    let active_errors: u32 = database.query_row(
        "SELECT COUNT(*) FROM error_targets
         WHERE learner_language_profile_id=?1
           AND status IN ('active','recheck','escalated')",
        [profile_id],
        |row| row.get(0),
    )?;

    let method_change_errors: u32 = database.query_row(
        "SELECT COUNT(*) FROM error_targets
         WHERE learner_language_profile_id=?1
           AND method_change_required=1 AND status!='resolved'",
        [profile_id],
        |row| row.get(0),
    )?;

    let due_error_rechecks: u32 = database.query_row(
        "SELECT COUNT(*) FROM error_rechecks r
         JOIN error_targets t ON t.id=r.error_target_id
         WHERE t.learner_language_profile_id=?1
           AND r.completed_at IS NULL AND r.due_at<=?2",
        [profile_id, now_text],
        |row| row.get(0),
    )?;

    let open_recommendations: u32 = database.query_row(
        "SELECT COUNT(*) FROM recommendations
         WHERE learner_language_profile_id=?1 AND status='open'",
        [profile_id],
        |row| row.get(0),
    )?;

    let recent = recent_signals(database, profile_id, at)?;

    Ok(adaptation::LessonPlannerInputView {
        due_reviews,
        overdue_reviews,
        due_error_rechecks,
        active_errors,
        method_change_errors,
        open_recommendations,
        recent,
        preferences,
    })
}
fn recent_signals(
    database: &rusqlite::Connection,
    profile_id: &str,
    at: learning_core::Timestamp,
) -> Result<adaptation::RecentLearningSignals> {
    let cutoff = to_rfc3339(learning_core::Timestamp(
        at.0.saturating_sub(RECENT_SIGNAL_WINDOW_SECONDS),
    ));
    let mut lessons = 0_u8;
    let mut speaking = 0_u64;
    let mut duration = 0_u64;
    let mut task_attempts = 0_u64;
    let mut task_lessons = 0_u8;
    let mut task_rate_sum = 0.0_f64;
    let mut confidence_sum = 0.0;
    let mut confidence_count = 0_u32;

    let mut statement = database.prepare(
        "SELECT l.duration_seconds,m.speaking_seconds,
                m.task_attempt_count,m.task_success_count,
                m.task_success_rubric,m.confidence
         FROM lessons l
         JOIN lesson_metrics m ON m.lesson_id=l.id
         WHERE l.learner_language_profile_id=?1
           AND l.status='completed'
           AND l.completed_at IS NOT NULL
           AND l.completed_at>=?2
         ORDER BY l.lesson_number DESC
         LIMIT 5",
    )?;
    let mut rows = statement.query(params![profile_id, cutoff.as_str()])?;
    while let Some(row) = rows.next()? {
        lessons = lessons.saturating_add(1);
        let duration_seconds: Option<u32> = row.get(0)?;
        let speaking_seconds: u32 = row.get(1)?;
        let task_attempt_count: u32 = row.get(2)?;
        let task_success_count: u32 = row.get(3)?;
        let task_success_rubric: Option<String> = row.get(4)?;
        let confidence: Option<f64> = row.get(5)?;

        speaking += u64::from(speaking_seconds);
        duration += u64::from(duration_seconds.unwrap_or_default());
        if task_success_rubric.as_deref()
            == Some(tutor_contract::COMMUNICATIVE_TASK_SUCCESS_RUBRIC_V2)
            && task_attempt_count > 0
        {
            task_attempts += u64::from(task_attempt_count);
            task_rate_sum += f64::from(task_success_count) / f64::from(task_attempt_count);
            task_lessons = task_lessons.saturating_add(1);
        }
        if let Some(value) = confidence {
            confidence_sum += value;
            confidence_count += 1;
        }
    }

    let mut recall_attempts = 0_u64;
    let mut recall_lessons = 0_u8;
    let mut recall_rate_sum = 0.0_f64;
    let mut recall_statement = database.prepare(
        "WITH recent_lessons AS (
             SELECT id FROM lessons
             WHERE learner_language_profile_id=?1
               AND status='completed'
               AND completed_at IS NOT NULL
               AND completed_at>=?2
             ORDER BY lesson_number DESC
             LIMIT 5
         )
         SELECT e.lesson_id,COUNT(*),
                COALESCE(SUM(CASE
                    WHEN e.success=1
                     AND CAST(json_extract(e.metadata_json,'$.assistance_level') AS INTEGER)=0
                    THEN 1 ELSE 0 END),0)
         FROM learning_item_evidence e
         JOIN recent_lessons r ON r.id=e.lesson_id
         WHERE e.kind IN ('prompted_production','spontaneous_production','transfer')
           AND json_type(e.metadata_json,'$.assistance_level') IN ('integer','real')
           AND COALESCE(json_extract(e.metadata_json,'$.probe_phase'),'legacy') != 'practice'
         GROUP BY e.lesson_id",
    )?;
    let mut recall_rows = recall_statement.query(params![profile_id, cutoff.as_str()])?;
    while let Some(row) = recall_rows.next()? {
        let attempts: u64 = row.get(1)?;
        let unaided_successes: u64 = row.get(2)?;
        if attempts == 0 {
            continue;
        }
        recall_attempts = recall_attempts.saturating_add(attempts);
        recall_lessons = recall_lessons.saturating_add(1);
        #[allow(clippy::cast_precision_loss)]
        {
            recall_rate_sum += unaided_successes as f64 / attempts as f64;
        }
    }

    #[allow(clippy::cast_precision_loss)]
    Ok(adaptation::RecentLearningSignals {
        lessons,
        speaking_ratio: (duration > 0).then_some(speaking as f64 / duration as f64),
        recall_attempts: u16::try_from(recall_attempts).unwrap_or(u16::MAX),
        recall_lessons,
        unaided_recall_rate: (recall_lessons > 0)
            .then_some(recall_rate_sum / f64::from(recall_lessons)),
        communicative_task_attempts: u16::try_from(task_attempts).unwrap_or(u16::MAX),
        communicative_task_lessons: task_lessons,
        communicative_task_success_rate: (task_lessons > 0)
            .then_some(task_rate_sum / f64::from(task_lessons)),
        confidence: (confidence_count > 0).then_some(confidence_sum / f64::from(confidence_count)),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn recall_planner_signal_comes_from_persisted_evidence_not_tutor_counters() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE lessons(
                id TEXT PRIMARY KEY,
                learner_language_profile_id TEXT NOT NULL,
                status TEXT NOT NULL,
                lesson_number INTEGER NOT NULL,
                duration_seconds INTEGER,
                completed_at TEXT
             );
             CREATE TABLE lesson_metrics(
                lesson_id TEXT PRIMARY KEY,
                speaking_seconds INTEGER NOT NULL,
                unaided_recall_count INTEGER NOT NULL,
                aided_recall_count INTEGER NOT NULL,
                task_attempt_count INTEGER NOT NULL,
                task_success_count INTEGER NOT NULL,
                task_success_rubric TEXT,
                confidence REAL
             );
             CREATE TABLE learning_item_evidence(
                learning_item_id TEXT NOT NULL,
                lesson_id TEXT,
                kind TEXT NOT NULL,
                success INTEGER NOT NULL,
                metadata_json TEXT
             );
             INSERT INTO lessons VALUES(
                'les-1','llp-1','completed',1,1200,'2026-11-01T12:00:00Z'
             );
             INSERT INTO lesson_metrics VALUES(
                'les-1',600,10,0,0,0,NULL,0.8
             );
             INSERT INTO learning_item_evidence VALUES
                ('li-1','les-1','spontaneous_production',0,
                 '{\"assistance_level\":0}'),
                ('li-1','les-1','prompted_production',1,
                 '{\"assistance_level\":1}');",
        )
        .unwrap();

        let signals = recent_signals(
            &db,
            "llp-1",
            learning_core::Timestamp(
                crate::repositories::from_rfc3339("2026-11-06T12:00:00Z")
                    .unwrap()
                    .0,
            ),
        )
        .unwrap();
        assert_eq!(signals.recall_attempts, 2);
        assert_eq!(signals.recall_lessons, 1);
        assert_eq!(signals.unaided_recall_rate, Some(0.0));
    }

    #[test]
    fn stale_lessons_do_not_authorize_current_stretch_signals() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE lessons(
                id TEXT PRIMARY KEY,
                learner_language_profile_id TEXT NOT NULL,
                status TEXT NOT NULL,
                lesson_number INTEGER NOT NULL,
                duration_seconds INTEGER,
                completed_at TEXT
             );
             CREATE TABLE lesson_metrics(
                lesson_id TEXT PRIMARY KEY,
                speaking_seconds INTEGER NOT NULL,
                unaided_recall_count INTEGER NOT NULL,
                aided_recall_count INTEGER NOT NULL,
                task_attempt_count INTEGER NOT NULL,
                task_success_count INTEGER NOT NULL,
                task_success_rubric TEXT,
                confidence REAL
             );
             CREATE TABLE learning_item_evidence(
                learning_item_id TEXT NOT NULL,
                lesson_id TEXT,
                kind TEXT NOT NULL,
                success INTEGER NOT NULL,
                metadata_json TEXT
             );
             INSERT INTO lessons VALUES(
                'les-old','llp-1','completed',1,1200,'2026-01-01T12:00:00Z'
             );
             INSERT INTO lesson_metrics VALUES(
                'les-old',600,0,0,3,3,'goal_completed_independently_v2',0.9
             );
             INSERT INTO learning_item_evidence VALUES
                ('li-1','les-old','spontaneous_production',1,
                 '{\"assistance_level\":0}');",
        )
        .unwrap();

        let now = crate::repositories::from_rfc3339("2026-11-06T12:00:00Z")
            .unwrap()
            .0;
        let signals = recent_signals(&db, "llp-1", learning_core::Timestamp(now)).unwrap();
        assert_eq!(signals.lessons, 0);
        assert_eq!(signals.recall_attempts, 0);
        assert_eq!(signals.communicative_task_attempts, 0);
    }

    #[test]
    fn practice_evidence_does_not_inflate_profile_recall_signal() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE lessons(
                id TEXT PRIMARY KEY,
                learner_language_profile_id TEXT NOT NULL,
                status TEXT NOT NULL,
                lesson_number INTEGER NOT NULL,
                duration_seconds INTEGER,
                completed_at TEXT
             );
             CREATE TABLE lesson_metrics(
                lesson_id TEXT PRIMARY KEY,
                speaking_seconds INTEGER NOT NULL,
                unaided_recall_count INTEGER NOT NULL,
                aided_recall_count INTEGER NOT NULL,
                task_attempt_count INTEGER NOT NULL,
                task_success_count INTEGER NOT NULL,
                task_success_rubric TEXT,
                confidence REAL
             );
             CREATE TABLE learning_item_evidence(
                learning_item_id TEXT NOT NULL,
                lesson_id TEXT,
                kind TEXT NOT NULL,
                success INTEGER NOT NULL,
                metadata_json TEXT
             );
             INSERT INTO lessons VALUES(
                'les-1','llp-1','completed',1,1200,'2026-11-01T12:00:00Z'
             );
             INSERT INTO lesson_metrics VALUES(
                'les-1',600,0,0,0,0,NULL,0.8
             );
             INSERT INTO learning_item_evidence VALUES
                ('li-1','les-1','spontaneous_production',0,
                 '{\"assistance_level\":0,\"probe_phase\":\"diagnostic\"}'),
                ('li-1','les-1','spontaneous_production',1,
                 '{\"assistance_level\":0,\"probe_phase\":\"practice\"}');",
        )
        .unwrap();

        let signals = recent_signals(
            &db,
            "llp-1",
            learning_core::Timestamp(
                crate::repositories::from_rfc3339("2026-11-06T12:00:00Z")
                    .unwrap()
                    .0,
            ),
        )
        .unwrap();

        assert_eq!(signals.recall_attempts, 1);
        assert_eq!(signals.recall_lessons, 1);
        assert_eq!(signals.unaided_recall_rate, Some(0.0));
    }

    #[test]
    fn recent_rates_are_balanced_across_lessons_not_raw_attempts() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE lessons(
                id TEXT PRIMARY KEY,
                learner_language_profile_id TEXT NOT NULL,
                status TEXT NOT NULL,
                lesson_number INTEGER NOT NULL,
                duration_seconds INTEGER,
                completed_at TEXT
             );
             CREATE TABLE lesson_metrics(
                lesson_id TEXT PRIMARY KEY,
                speaking_seconds INTEGER NOT NULL,
                unaided_recall_count INTEGER NOT NULL,
                aided_recall_count INTEGER NOT NULL,
                task_attempt_count INTEGER NOT NULL,
                task_success_count INTEGER NOT NULL,
                task_success_rubric TEXT,
                confidence REAL
             );
             CREATE TABLE learning_item_evidence(
                learning_item_id TEXT NOT NULL,
                lesson_id TEXT,
                kind TEXT NOT NULL,
                success INTEGER NOT NULL,
                metadata_json TEXT
             );
             INSERT INTO lessons VALUES
                ('les-1','llp-1','completed',1,1200,'2026-11-01T12:00:00Z'),
                ('les-2','llp-1','completed',2,1200,'2026-11-02T12:00:00Z');
             INSERT INTO lesson_metrics VALUES
                ('les-1',600,0,0,10,10,'goal_completed_independently_v2',0.8),
                ('les-2',600,0,0,1,0,'goal_completed_independently_v2',0.8);
             WITH RECURSIVE n(x) AS (
                SELECT 1 UNION ALL SELECT x+1 FROM n WHERE x<10
             )
             INSERT INTO learning_item_evidence
             SELECT 'li-' || x,'les-1','spontaneous_production',1,
                    '{\"assistance_level\":0}' FROM n;
             INSERT INTO learning_item_evidence VALUES
                ('li-fail','les-2','spontaneous_production',0,
                 '{\"assistance_level\":0}');",
        )
        .unwrap();

        let signals = recent_signals(
            &db,
            "llp-1",
            learning_core::Timestamp(
                crate::repositories::from_rfc3339("2026-11-06T12:00:00Z")
                    .unwrap()
                    .0,
            ),
        )
        .unwrap();
        assert_eq!(signals.recall_attempts, 11);
        assert_eq!(signals.recall_lessons, 2);
        assert_eq!(signals.unaided_recall_rate, Some(0.5));
        assert_eq!(signals.communicative_task_attempts, 11);
        assert_eq!(signals.communicative_task_lessons, 2);
        assert_eq!(signals.communicative_task_success_rate, Some(0.5));
    }
}
