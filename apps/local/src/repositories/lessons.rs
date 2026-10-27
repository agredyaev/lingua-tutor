//! `lessons` + `lesson_metrics` repositories (WBS 4.3).

/// Insert a lesson.
pub const INSERT_LESSON: &str = "INSERT INTO lessons
    (id, learner_language_profile_id, language_pack_id, language_pack_version,
     lesson_number, status, scenario, planned_goal, source, started_at,
     completed_at, duration_seconds, tutor_summary, next_goal, idempotency_key,
     created_at, updated_at)
 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?16)";

/// Complete a lesson (status/outcome fields only).
pub const COMPLETE_LESSON: &str = "UPDATE lessons SET
    status = 'completed', completed_at = ?2, duration_seconds = ?3,
    tutor_summary = ?4, next_goal = ?5, idempotency_key = ?6, updated_at = ?2
 WHERE id = ?1 AND status IN ('planned','active')";

/// Correct a completed lesson's record fields (13-roadmap.md §3
/// `correct_lesson_record`): summary/goal/duration only — evidence and
/// history stay immutable. NULL argument keeps the stored value.
pub const CORRECT_LESSON_RECORD: &str = "UPDATE lessons SET
    tutor_summary = COALESCE(?2, tutor_summary),
    next_goal = COALESCE(?3, next_goal),
    duration_seconds = COALESCE(?4, duration_seconds),
    updated_at = ?5
 WHERE id = ?1 AND status = 'completed'";

/// Lesson by ID.
pub const SELECT_LESSON: &str = "SELECT id, learner_language_profile_id,
    language_pack_id, language_pack_version, lesson_number, status, scenario,
    planned_goal, source, started_at, completed_at, duration_seconds,
    tutor_summary, next_goal, idempotency_key
 FROM lessons WHERE id = ?1";

/// Lesson by import idempotency key.
pub const SELECT_LESSON_BY_KEY: &str = "SELECT id, learner_language_profile_id,
    language_pack_id, language_pack_version, lesson_number, status, scenario,
    planned_goal, source, started_at, completed_at, duration_seconds,
    tutor_summary, next_goal, idempotency_key
 FROM lessons WHERE idempotency_key = ?1";

/// Most recent open (started but not yet completed) lesson for a profile —
/// completion-without-`lesson_id` fallback: without this, a completion
/// call that omits `lesson_id` always tries to *create* a new lesson row
/// numbered `current_lesson_number + 1`, colliding (UNIQUE constraint) with
/// the `active` row `start_lesson` already reserved that same number for.
pub const SELECT_LATEST_OPEN_LESSON: &str = "SELECT id, learner_language_profile_id,
    language_pack_id, language_pack_version, lesson_number, status, scenario,
    planned_goal, source, started_at, completed_at, duration_seconds,
    tutor_summary, next_goal, idempotency_key
 FROM lessons WHERE learner_language_profile_id = ?1 AND status IN ('planned','active')
 ORDER BY started_at DESC LIMIT 1";

/// Profile lessons, newest first, bounded.
pub const SELECT_LESSONS: &str = "SELECT id, learner_language_profile_id,
    language_pack_id, language_pack_version, lesson_number, status, scenario,
    planned_goal, source, started_at, completed_at, duration_seconds,
    tutor_summary, next_goal, idempotency_key
 FROM lessons WHERE learner_language_profile_id = ?1
 ORDER BY lesson_number DESC LIMIT ?2";

/// Insert lesson metrics (one row per lesson).
pub const INSERT_METRICS: &str = "INSERT INTO lesson_metrics
    (lesson_id, speaking_seconds, unaided_recall_count, aided_recall_count,
     listening_score, avg_answer_millis, long_pause_count,
     task_attempt_count, task_success_count, task_success_rubric, confidence, created_at)
 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)";
