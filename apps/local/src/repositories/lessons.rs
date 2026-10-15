//! `lessons` + `lesson_metrics` repositories (WBS 4.3).

use super::{RepoError, from_rfc3339, parse_enum};
use learning_core::{
    LanguagePackId, LanguageProfileId, Lesson, LessonId, LessonSource, LessonStatus,
};
use serde::{Deserialize, Serialize};

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
    tutor_summary = ?4, next_goal = ?5, updated_at = ?2
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
     listening_score, avg_answer_millis, long_pause_count, confidence, created_at)
 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)";

/// `lessons` row, column-shaped.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LessonRow {
    /// `id`.
    pub id: String,
    /// `learner_language_profile_id`.
    pub learner_language_profile_id: String,
    /// `language_pack_id`.
    pub language_pack_id: String,
    /// `language_pack_version`.
    pub language_pack_version: String,
    /// `lesson_number`.
    pub lesson_number: u32,
    /// `status` TEXT.
    pub status: String,
    /// `scenario`.
    pub scenario: String,
    /// `planned_goal`.
    pub planned_goal: String,
    /// `source` TEXT.
    pub source: String,
    /// `started_at` RFC 3339.
    pub started_at: Option<String>,
    /// `completed_at` RFC 3339.
    pub completed_at: Option<String>,
    /// `duration_seconds`.
    pub duration_seconds: Option<u32>,
    /// `tutor_summary`.
    pub tutor_summary: Option<String>,
    /// `next_goal`.
    pub next_goal: Option<String>,
    /// `idempotency_key`.
    pub idempotency_key: Option<String>,
}

impl LessonRow {
    pub(crate) fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            learner_language_profile_id: row.get("learner_language_profile_id")?,
            language_pack_id: row.get("language_pack_id")?,
            language_pack_version: row.get("language_pack_version")?,
            lesson_number: row.get("lesson_number")?,
            status: row.get("status")?,
            scenario: row.get("scenario")?,
            planned_goal: row.get("planned_goal")?,
            source: row.get("source")?,
            started_at: row.get("started_at")?,
            completed_at: row.get("completed_at")?,
            duration_seconds: row.get("duration_seconds")?,
            tutor_summary: row.get("tutor_summary")?,
            next_goal: row.get("next_goal")?,
            idempotency_key: row.get("idempotency_key")?,
        })
    }
}

impl LessonRow {
    /// Column row → domain type.
    ///
    /// # Errors
    ///
    /// [`RepoError::CorruptRow`] on invalid enum, version, or timestamp text.
    pub fn into_domain(self) -> Result<Lesson, RepoError> {
        let status: LessonStatus = parse_enum("status", &self.status)?;
        let source: LessonSource = parse_enum("source", &self.source)?;
        let version = semver::Version::parse(&self.language_pack_version)
            .map_err(|e| RepoError::CorruptRow(format!("language_pack_version: {e}")))?;
        Ok(Lesson {
            id: LessonId(self.id),
            language_profile_id: LanguageProfileId(self.learner_language_profile_id),
            language_pack_id: LanguagePackId(self.language_pack_id),
            language_pack_version: version,
            lesson_number: self.lesson_number,
            status,
            scenario: self.scenario,
            planned_goal: self.planned_goal,
            source,
            started_at: self.started_at.as_deref().map(from_rfc3339).transpose()?,
            completed_at: self.completed_at.as_deref().map(from_rfc3339).transpose()?,
            duration_seconds: self.duration_seconds,
            tutor_summary: self.tutor_summary,
            next_goal: self.next_goal,
            idempotency_key: self.idempotency_key,
        })
    }
}
