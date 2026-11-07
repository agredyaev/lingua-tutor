//! Canonical bounded planner-signal reads for both active lessons and tutor queries.
//! Returns borrowed preferences and scalar derived metrics, without owned DTO hops.
use crate::Result;
use crate::sqlite::to_rfc3339;
use rusqlite::{Connection, params};

const SIGNIFICANTLY_OVERDUE_SECONDS: i64 = 86_400;
const RECENT_SIGNAL_WINDOW_SECONDS: i64 = 28 * 86_400;
const RECENT_SIGNAL_MAX_LESSONS: u8 = 5;

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
        crate::sqlite::COUNT_OVERDUE,
        [profile_id, overdue_at.as_str()],
        |row| row.get(0),
    )?;

    let active_errors: u32 = database.query_row(
        "SELECT COUNT(*) FROM error_targets
         WHERE learner_language_profile_id=?1
           AND status IN ('active','escalated')",
        [profile_id],
        |row| row.get(0),
    )?;

    let method_change_errors: u32 = database.query_row(
        "SELECT COUNT(*) FROM error_targets
         WHERE learner_language_profile_id=?1
           AND method_change_required=1 AND status IN ('active','escalated')",
        [profile_id],
        |row| row.get(0),
    )?;

    let due_error_rechecks: u32 = database.query_row(
        "SELECT COUNT(*) FROM error_rechecks r
         JOIN error_targets t ON t.id=r.error_target_id
         WHERE t.learner_language_profile_id=?1
           AND r.completed_at IS NULL AND r.due_at<=?2
           AND r.offset_days = (
               SELECT MIN(r2.offset_days)
               FROM error_rechecks r2
               WHERE r2.error_target_id=r.error_target_id
                 AND r2.cycle=r.cycle
                 AND r2.completed_at IS NULL
           )",
        [profile_id, now_text],
        |row| row.get(0),
    )?;

    let open_recommendations: u32 = database.query_row(
        "SELECT COUNT(*)
         FROM recommendations r
         WHERE r.learner_language_profile_id = ?1 AND r.status = 'open'
           AND (
               r.subject_type NOT IN ('learning_item','error_target','card')
               OR (
                   r.subject_type='learning_item'
                   AND EXISTS (
                       SELECT 1 FROM learning_items i
                       WHERE i.id=r.subject_id
                         AND i.learner_language_profile_id=r.learner_language_profile_id
                         AND i.status IN ('introduced','learning','transfer_pending',
                                          'provisionally_mastered','mastered')
                   )
               )
               OR (
                   r.subject_type='error_target'
                   AND EXISTS (
                       SELECT 1 FROM error_targets t
                       WHERE t.id=r.subject_id
                         AND t.learner_language_profile_id=r.learner_language_profile_id
                         AND t.status IN ('active','escalated')
                   )
               )
               OR (
                   r.subject_type='card'
                   AND EXISTS (
                       SELECT 1 FROM cards c
                       WHERE c.id=r.subject_id
                         AND c.learner_language_profile_id=r.learner_language_profile_id
                         AND c.status IN ('new','learning','review','relearning')
                   )
               )
           )",
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
    let mut task_attempts = 0_u64;
    let mut task_lessons = 0_u8;
    let mut task_rate_sum = 0.0_f64;
    let mut task_oldest_at: Option<i64> = None;
    let mut task_newest_at: Option<i64> = None;
    let mut confidence_sum = 0.0;
    let mut confidence_count = 0_u32;

    let mut statement = database.prepare(
        "SELECT m.task_attempt_count,m.task_success_count,
                m.task_success_rubric,m.confidence,l.completed_at,l.planned_goal
         FROM lessons l
         LEFT JOIN lesson_metrics m ON m.lesson_id=l.id
         WHERE l.learner_language_profile_id=?1
           AND l.status='completed'
           AND l.completed_at IS NOT NULL
           AND l.completed_at>=?2
         ORDER BY l.lesson_number DESC
         LIMIT ?3",
    )?;
    let mut rows = statement.query(params![
        profile_id,
        cutoff.as_str(),
        RECENT_SIGNAL_MAX_LESSONS,
    ])?;
    while let Some(row) = rows.next()? {
        lessons = lessons.saturating_add(1);
        let task_attempt_count: Option<u32> = row.get(0)?;
        let task_success_count: Option<u32> = row.get(1)?;
        let task_success_rubric: Option<String> = row.get(2)?;
        let confidence: Option<f64> = row.get(3)?;
        let completed_at: String = row.get(4)?;
        let planned_goal: String = row.get(5)?;
        let task_attempt_count = task_attempt_count.unwrap_or_default();
        let task_success_count = task_success_count.unwrap_or_default();

        if task_success_rubric.as_deref()
            == Some(tutor_contract::COMMUNICATIVE_TASK_SUCCESS_RUBRIC_V2)
            && task_attempt_count > 0
            && !planned_goal.trim().is_empty()
        {
            task_attempts += u64::from(task_attempt_count);
            task_rate_sum += f64::from(task_success_count) / f64::from(task_attempt_count);
            task_lessons = task_lessons.saturating_add(1);
            let observed_at = crate::sqlite::from_rfc3339(&completed_at)
                .map_err(crate::Error::RustError)?
                .0;
            task_oldest_at =
                Some(task_oldest_at.map_or(observed_at, |value| value.min(observed_at)));
            task_newest_at =
                Some(task_newest_at.map_or(observed_at, |value| value.max(observed_at)));
        }
        if let Some(value) = confidence {
            confidence_sum += value;
            confidence_count += 1;
        }
    }

    let mut recall_attempts = 0_u64;
    let mut recall_lessons = 0_u8;
    let mut recall_rate_sum = 0.0_f64;
    let mut recall_oldest_at: Option<i64> = None;
    let mut recall_newest_at: Option<i64> = None;
    let mut recall_statement = database.prepare(
        "WITH recent_lessons AS (
             SELECT id,completed_at FROM lessons
             WHERE learner_language_profile_id=?1
               AND status='completed'
               AND completed_at IS NOT NULL
               AND completed_at>=?2
             ORDER BY lesson_number DESC
             LIMIT ?3
         )
         SELECT e.lesson_id,r.completed_at,COUNT(*),
                COALESCE(SUM(CASE
                    WHEN e.success=1
                     AND CAST(json_extract(e.metadata_json,'$.assistance_level') AS INTEGER)=0
                    THEN 1 ELSE 0 END),0)
         FROM learning_item_evidence e
         JOIN recent_lessons r ON r.id=e.lesson_id
         WHERE e.kind='spontaneous_production'
           AND json_type(e.metadata_json,'$.assistance_level') IN ('integer','real')
           AND COALESCE(json_extract(e.metadata_json,'$.probe_phase'),'legacy') != 'practice'
         GROUP BY e.lesson_id,r.completed_at",
    )?;
    let mut recall_rows = recall_statement.query(params![
        profile_id,
        cutoff.as_str(),
        RECENT_SIGNAL_MAX_LESSONS,
    ])?;
    while let Some(row) = recall_rows.next()? {
        let completed_at: String = row.get(1)?;
        let attempts: u64 = row.get(2)?;
        let unaided_successes: u64 = row.get(3)?;
        if attempts == 0 {
            continue;
        }
        recall_attempts = recall_attempts.saturating_add(attempts);
        recall_lessons = recall_lessons.saturating_add(1);
        let observed_at = crate::sqlite::from_rfc3339(&completed_at)
            .map_err(crate::Error::RustError)?
            .0;
        recall_oldest_at =
            Some(recall_oldest_at.map_or(observed_at, |value| value.min(observed_at)));
        recall_newest_at =
            Some(recall_newest_at.map_or(observed_at, |value| value.max(observed_at)));
        #[allow(clippy::cast_precision_loss)]
        {
            recall_rate_sum += unaided_successes as f64 / attempts as f64;
        }
    }

    #[allow(clippy::cast_precision_loss)]
    Ok(adaptation::RecentLearningSignals {
        lessons,
        recall_attempts: u16::try_from(recall_attempts).unwrap_or(u16::MAX),
        recall_lessons,
        unaided_recall_rate: (recall_lessons > 0)
            .then_some(recall_rate_sum / f64::from(recall_lessons)),
        recall_temporally_distributed: recall_oldest_at.zip(recall_newest_at).is_some_and(
            |(oldest, newest)| {
                newest.saturating_sub(oldest) >= adaptation::MIN_DISTRIBUTED_SUCCESS_GAP_SECONDS
            },
        ),
        communicative_task_attempts: u16::try_from(task_attempts).unwrap_or(u16::MAX),
        communicative_task_lessons: task_lessons,
        communicative_task_success_rate: (task_lessons > 0)
            .then_some(task_rate_sum / f64::from(task_lessons)),
        communicative_task_temporally_distributed: task_oldest_at.zip(task_newest_at).is_some_and(
            |(oldest, newest)| {
                newest.saturating_sub(oldest) >= adaptation::MIN_DISTRIBUTED_SUCCESS_GAP_SECONDS
            },
        ),
        confidence: (confidence_count > 0).then_some(confidence_sum / f64::from(confidence_count)),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn recheck_state_does_not_create_active_repair_pressure() {
        let path = std::env::temp_dir().join(format!(
            "lingua-planner-recheck-state-{}.sqlite3",
            uuid::Uuid::new_v4()
        ));
        let db = crate::storage::open(&path).unwrap();
        db.execute_batch(
            "INSERT INTO learner_profile (id,created_at,updated_at)
             VALUES ('learner-1','2026-11-01T00:00:00Z','2026-11-01T00:00:00Z');
             INSERT INTO learner_language_profiles
                (id,learner_id,source_language,target_locale,created_at,updated_at)
             VALUES
                ('llp-1','learner-1','ru','es-ES','2026-11-01T00:00:00Z','2026-11-01T00:00:00Z');
             INSERT INTO error_targets
                (id,learner_language_profile_id,core_category,original_production,
                 corrected_production,occurrence_count,method_change_required,status,
                 created_at,updated_at)
             VALUES
                ('et-1','llp-1','grammar','wrong','right',3,0,'recheck',
                 '2026-11-01T00:00:00Z','2026-11-01T00:00:00Z');",
        )
        .unwrap();

        let preferences = adaptation::PlannerPreferencesView {
            max_new_items_per_lesson: None,
            preferred_difficulty: None,
        };
        let at = crate::sqlite::from_rfc3339("2026-11-06T12:00:00Z").unwrap();
        let input = read_planner_input(&db, "llp-1", at, preferences).unwrap();
        assert_eq!(
            input.active_errors, 0,
            "recheck state is retention measurement, not active repair pressure"
        );

        db.execute(
            "UPDATE error_targets SET status='active' WHERE id='et-1'",
            [],
        )
        .unwrap();
        let input = read_planner_input(&db, "llp-1", at, preferences).unwrap();
        assert_eq!(input.active_errors, 1);

        db.execute(
            "UPDATE error_targets
             SET status='escalated',method_change_required=1
             WHERE id='et-1'",
            [],
        )
        .unwrap();
        let input = read_planner_input(&db, "llp-1", at, preferences).unwrap();
        assert_eq!(input.active_errors, 1);
        assert_eq!(
            input.method_change_errors, 1,
            "legacy escalated state is an active repair state and must preserve method-change pressure"
        );

        drop(db);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn stale_open_recommendations_do_not_create_planner_or_tutor_pressure() {
        let path = std::env::temp_dir().join(format!(
            "lingua-planner-stale-recommendations-{}.sqlite3",
            uuid::Uuid::new_v4()
        ));
        let db = crate::storage::open(&path).unwrap();
        db.execute_batch(
            "INSERT INTO learner_profile(id,created_at,updated_at)
             VALUES('learner-1','2026-11-01T00:00:00Z','2026-11-01T00:00:00Z');
             INSERT INTO learner_language_profiles
              (id,learner_id,source_language,target_locale,created_at,updated_at)
             VALUES('llp-1','learner-1','ru','es-ES',
                    '2026-11-01T00:00:00Z','2026-11-01T00:00:00Z');

             INSERT INTO learning_items
              (id,learner_language_profile_id,item_type,target_text,
               normalized_target_text,status,created_at,updated_at)
             VALUES
              ('li-suspended','llp-1','phrase','s','s','suspended',
               '2026-11-01T00:00:00Z','2026-11-01T00:00:00Z');

             INSERT INTO cards
              (id,learner_language_profile_id,intent,status,prompt,prompt_language,
               answer,answer_language,accepted_answers_json,creation_reason,content_hash,
               content_version,created_by,created_at,updated_at)
             VALUES
              ('card-suspended','llp-1','production','suspended','p','ru','a','es-ES',
               '[]','seed','hash-s',1,'tutor',
               '2026-11-01T00:00:00Z','2026-11-01T00:00:00Z');

             INSERT INTO error_targets
              (id,learner_language_profile_id,core_category,original_production,
               corrected_production,occurrence_count,method_change_required,status,
               created_at,updated_at)
             VALUES
              ('et-recheck','llp-1','grammar','wrong','right',3,0,'recheck',
               '2026-11-01T00:00:00Z','2026-11-01T00:00:00Z');

             INSERT INTO recommendations
              (id,learner_language_profile_id,subject_type,subject_id,action,
               detail_json,status,created_at)
             VALUES
              ('rec-item-stale','llp-1','learning_item','li-suspended',
               'practice_in_lesson','{}','open','2026-11-01T00:00:00Z'),
              ('rec-card-stale','llp-1','card','card-suspended',
               'redesign_card','{}','open','2026-11-01T00:00:01Z'),
              ('rec-error-stale','llp-1','error_target','et-recheck',
               'activate_mini_target','{}','open','2026-11-01T00:00:02Z'),
              ('rec-valid','llp-1','language_profile',NULL,
               'review_strategy','{}','open','2026-11-01T00:00:03Z');",
        )
        .unwrap();

        let preferences = adaptation::PlannerPreferencesView {
            max_new_items_per_lesson: None,
            preferred_difficulty: None,
        };
        let at = crate::sqlite::from_rfc3339("2026-11-06T12:00:00Z").unwrap();
        let input = read_planner_input(&db, "llp-1", at, preferences).unwrap();
        assert_eq!(
            input.open_recommendations, 1,
            "legacy stale rows must not create adaptive recommendation pressure"
        );

        let mut statement = db.prepare(crate::sqlite::SELECT_OPEN).unwrap();
        let visible = statement
            .query_map(rusqlite::params!["llp-1", 20], |row| {
                row.get::<_, String>(0)
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        assert_eq!(
            visible,
            vec!["rec-valid".to_owned()],
            "Tutor-facing recommendation list must use the same actionability boundary as planner count"
        );
        drop(statement);

        drop(db);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn task_metrics_without_preplanned_goal_have_no_strong_planner_authority() {
        let path = std::env::temp_dir().join(format!(
            "lingua-planner-task-without-goal-{}.sqlite3",
            uuid::Uuid::new_v4()
        ));
        let db = crate::storage::open(&path).unwrap();
        db.execute_batch(
            "INSERT INTO learner_profile(id,created_at,updated_at)
             VALUES('learner-1','2026-11-01T00:00:00Z','2026-11-01T00:00:00Z');
             INSERT INTO learner_language_profiles
              (id,learner_id,source_language,target_locale,created_at,updated_at)
             VALUES('llp-1','learner-1','ru','es-ES',
                    '2026-11-01T00:00:00Z','2026-11-01T00:00:00Z');
             INSERT INTO lessons
              (id,learner_language_profile_id,lesson_number,status,scenario,planned_goal,
               source,started_at,completed_at,created_at,updated_at)
             VALUES('les-import','llp-1',1,'completed','pharmacy','',
                    'import','2026-11-05T10:00:00Z','2026-11-05T10:20:00Z',
                    '2026-11-05T10:00:00Z','2026-11-05T10:20:00Z');
             INSERT INTO lesson_metrics
              (lesson_id,speaking_seconds,unaided_recall_count,aided_recall_count,
               task_attempt_count,task_success_count,task_success_rubric,confidence,created_at)
             VALUES('les-import',0,0,0,5,5,'goal_completed_independently_v2',1.0,
                    '2026-11-05T10:20:00Z');",
        )
        .unwrap();

        let signals = recent_signals(
            &db,
            "llp-1",
            crate::sqlite::from_rfc3339("2026-11-06T12:00:00Z").unwrap(),
        )
        .unwrap();
        assert_eq!(signals.communicative_task_attempts, 0);
        assert_eq!(signals.communicative_task_lessons, 0);
        assert_eq!(signals.communicative_task_success_rate, None);

        drop(db);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn planner_signal_schema_excludes_uninstrumented_speaking_proxy() {
        let value = serde_json::to_value(adaptation::RecentLearningSignals::default()).unwrap();
        assert!(
            value.get("speaking_ratio").is_none(),
            "uninstrumented speaking time must not become adaptive learner state"
        );
    }

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
                planned_goal TEXT NOT NULL,
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
                'les-1','llp-1','completed',1,1200,'goal','2026-11-01T12:00:00Z'
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
                crate::sqlite::from_rfc3339("2026-11-06T12:00:00Z")
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
    fn stale_lessons_do_not_authorize_current_stretch_signals() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE lessons(
                id TEXT PRIMARY KEY,
                learner_language_profile_id TEXT NOT NULL,
                status TEXT NOT NULL,
                lesson_number INTEGER NOT NULL,
                duration_seconds INTEGER,
                planned_goal TEXT NOT NULL,
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
                'les-old','llp-1','completed',1,1200,'goal','2026-01-01T12:00:00Z'
             );
             INSERT INTO lesson_metrics VALUES(
                'les-old',600,0,0,3,3,'goal_completed_independently_v2',0.9
             );
             INSERT INTO learning_item_evidence VALUES
                ('li-1','les-old','spontaneous_production',1,
                 '{\"assistance_level\":0}');",
        )
        .unwrap();

        let now = crate::sqlite::from_rfc3339("2026-11-06T12:00:00Z")
            .unwrap()
            .0;
        let signals = recent_signals(&db, "llp-1", learning_core::Timestamp(now)).unwrap();
        assert_eq!(signals.lessons, 0);
        assert_eq!(signals.recall_attempts, 0);
        assert_eq!(signals.communicative_task_attempts, 0);
    }

    #[test]
    fn completed_lessons_without_metrics_still_count_as_learning_history() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE lessons(
                id TEXT PRIMARY KEY,
                learner_language_profile_id TEXT NOT NULL,
                status TEXT NOT NULL,
                lesson_number INTEGER NOT NULL,
                duration_seconds INTEGER,
                planned_goal TEXT NOT NULL,
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
                'les-1','llp-1','completed',1,1200,'goal','2026-11-01T12:00:00Z'
             );",
        )
        .unwrap();

        let signals = recent_signals(
            &db,
            "llp-1",
            learning_core::Timestamp(
                crate::sqlite::from_rfc3339("2026-11-06T12:00:00Z")
                    .unwrap()
                    .0,
            ),
        )
        .unwrap();

        assert_eq!(signals.lessons, 1);
        assert_eq!(signals.communicative_task_attempts, 0);
        assert_eq!(signals.confidence, None);
    }

    #[test]
    fn prompted_attempts_do_not_dilute_spontaneous_recall_rate() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE lessons(
                id TEXT PRIMARY KEY,
                learner_language_profile_id TEXT NOT NULL,
                status TEXT NOT NULL,
                lesson_number INTEGER NOT NULL,
                duration_seconds INTEGER,
                planned_goal TEXT NOT NULL,
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
                'les-1','llp-1','completed',1,1200,'goal','2026-11-01T12:00:00Z'
             );
             INSERT INTO lesson_metrics VALUES(
                'les-1',600,0,0,0,0,NULL,0.8
             );
             INSERT INTO learning_item_evidence VALUES
                ('li-1','les-1','spontaneous_production',1,
                 '{\"assistance_level\":0,\"probe_phase\":\"diagnostic\"}'),
                ('li-1','les-1','prompted_production',1,
                 '{\"assistance_level\":2,\"probe_phase\":\"diagnostic\"}');",
        )
        .unwrap();

        let signals = recent_signals(
            &db,
            "llp-1",
            learning_core::Timestamp(
                crate::sqlite::from_rfc3339("2026-11-06T12:00:00Z")
                    .unwrap()
                    .0,
            ),
        )
        .unwrap();

        assert_eq!(signals.recall_attempts, 1);
        assert_eq!(signals.recall_lessons, 1);
        assert_eq!(signals.unaided_recall_rate, Some(1.0));
    }

    #[test]
    fn transfer_failures_do_not_masquerade_as_profile_prompt_dependence() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE lessons(
                id TEXT PRIMARY KEY,
                learner_language_profile_id TEXT NOT NULL,
                status TEXT NOT NULL,
                lesson_number INTEGER NOT NULL,
                duration_seconds INTEGER,
                planned_goal TEXT NOT NULL,
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
                'les-1','llp-1','completed',1,1200,'goal','2026-11-01T12:00:00Z'
             );
             INSERT INTO lesson_metrics VALUES(
                'les-1',600,0,0,0,0,NULL,0.8
             );
             INSERT INTO learning_item_evidence VALUES
                ('li-1','les-1','spontaneous_production',1,
                 '{\"assistance_level\":0,\"probe_phase\":\"diagnostic\"}'),
                ('li-1','les-1','transfer',0,
                 '{\"assistance_level\":0,\"probe_phase\":\"transfer\",\"context_family\":\"service.ask_help\"}');",
        )
        .unwrap();

        let signals = recent_signals(
            &db,
            "llp-1",
            learning_core::Timestamp(
                crate::sqlite::from_rfc3339("2026-11-06T12:00:00Z")
                    .unwrap()
                    .0,
            ),
        )
        .unwrap();

        assert_eq!(signals.recall_attempts, 1);
        assert_eq!(signals.recall_lessons, 1);
        assert_eq!(signals.unaided_recall_rate, Some(1.0));
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
                planned_goal TEXT NOT NULL,
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
                'les-1','llp-1','completed',1,1200,'goal','2026-11-01T12:00:00Z'
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
                crate::sqlite::from_rfc3339("2026-11-06T12:00:00Z")
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
                planned_goal TEXT NOT NULL,
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
                ('les-1','llp-1','completed',1,1200,'goal','2026-11-01T12:00:00Z'),
                ('les-2','llp-1','completed',2,1200,'goal','2026-11-02T12:00:00Z');
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
                crate::sqlite::from_rfc3339("2026-11-06T12:00:00Z")
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
        assert!(signals.recall_temporally_distributed);
        assert!(signals.communicative_task_temporally_distributed);
    }

    #[test]
    fn massed_lesson_ids_do_not_create_stretch_authority() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE lessons(
                id TEXT PRIMARY KEY,
                learner_language_profile_id TEXT NOT NULL,
                status TEXT NOT NULL,
                lesson_number INTEGER NOT NULL,
                duration_seconds INTEGER,
                planned_goal TEXT NOT NULL,
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
                ('les-1','llp-1','completed',1,1200,'goal','2026-11-01T12:00:00Z'),
                ('les-2','llp-1','completed',2,1200,'goal','2026-11-01T12:10:00Z');
             INSERT INTO lesson_metrics VALUES
                ('les-1',600,0,0,2,2,'goal_completed_independently_v2',0.8),
                ('les-2',600,0,0,2,2,'goal_completed_independently_v2',0.8);
             INSERT INTO learning_item_evidence VALUES
                ('li-1','les-1','spontaneous_production',1,
                 '{\"assistance_level\":0,\"probe_phase\":\"diagnostic\"}'),
                ('li-1','les-2','spontaneous_production',1,
                 '{\"assistance_level\":0,\"probe_phase\":\"diagnostic\"}');",
        )
        .unwrap();

        let signals = recent_signals(
            &db,
            "llp-1",
            learning_core::Timestamp(
                crate::sqlite::from_rfc3339("2026-11-06T12:00:00Z")
                    .unwrap()
                    .0,
            ),
        )
        .unwrap();

        assert_eq!(signals.recall_lessons, 2);
        assert_eq!(signals.communicative_task_lessons, 2);
        assert!(!signals.recall_temporally_distributed);
        assert!(!signals.communicative_task_temporally_distributed);
    }
}
