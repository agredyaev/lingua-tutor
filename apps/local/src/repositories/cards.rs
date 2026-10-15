//! `cards`, `card_tags`, `card_mutations` repositories (WBS 5.1–5.2).

use super::{RepoError, parse_enum};
use learning_core::{
    Card, CardActor, CardId, CardIntent, CardStatus, ErrorTargetId, LanguagePackId,
    LanguageProfileId, LanguageTag, LearningItemId, LessonId,
};
use serde::{Deserialize, Serialize};

/// Insert a card.
pub const INSERT_CARD: &str = "INSERT INTO cards
    (id, learner_language_profile_id, language_pack_id, language_pack_version,
     intent, status, prompt, prompt_language, answer, answer_language,
     accepted_answers_json, answer_evaluation_mode, explanation,
     pronunciation_note, source_learning_item_id, source_error_target_id,
     source_lesson_id, creation_reason, priority_score, content_hash,
     content_version, parent_card_id, suspended_reason, created_by,
     created_at, updated_at)
 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15,
         ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?25)";

const CARD_COLUMNS: &str = "id, learner_language_profile_id, language_pack_id,
    language_pack_version, intent, status, prompt, prompt_language, answer,
    answer_language, accepted_answers_json, answer_evaluation_mode, explanation,
    pronunciation_note, source_learning_item_id, source_error_target_id,
    source_lesson_id, creation_reason, priority_score, content_hash,
    content_version, parent_card_id, suspended_reason, created_by";

/// Card by ID.
#[must_use]
pub fn select_card_sql() -> String {
    format!("SELECT {CARD_COLUMNS} FROM cards WHERE id = ?1")
}

/// Non-retired card by duplicate-detection hash (uses `ix_cards_content_hash`).
#[must_use]
pub fn select_card_by_hash_sql() -> String {
    format!(
        "SELECT {CARD_COLUMNS} FROM cards
         WHERE learner_language_profile_id = ?1 AND content_hash = ?2
           AND status != 'retired'"
    )
}

/// Profile cards with optional status/intent filters, bounded.
#[must_use]
pub fn select_cards_sql() -> String {
    format!(
        "SELECT {CARD_COLUMNS} FROM cards
         WHERE learner_language_profile_id = ?1
           AND (?2 IS NULL OR status = ?2)
           AND (?3 IS NULL OR intent = ?3)
         ORDER BY created_at DESC LIMIT ?4"
    )
}

/// Content update, schedule preserved (cosmetic/clarifying edits).
pub const UPDATE_CARD_CONTENT: &str = "UPDATE cards SET
    prompt = ?2, explanation = ?3, pronunciation_note = ?4,
    accepted_answers_json = ?5, content_hash = ?6,
    content_version = content_version + 1, updated_at = ?7
 WHERE id = ?1";

/// Status transition (+ suspended_reason bookkeeping).
pub const UPDATE_CARD_STATUS: &str = "UPDATE cards SET
    status = ?2, suspended_reason = ?3, updated_at = ?4 WHERE id = ?1";

/// Accepted-answers replacement.
pub const UPDATE_ACCEPTED_ANSWERS: &str = "UPDATE cards SET
    accepted_answers_json = ?2, content_hash = ?3, updated_at = ?4 WHERE id = ?1";

/// Cards activated (created non-candidate) today — activation budget input.
pub const COUNT_ACTIVATED_TODAY: &str = "SELECT COUNT(*) AS n FROM cards
 WHERE learner_language_profile_id = ?1 AND status != 'candidate'
   AND created_at >= ?2";

/// Oldest deferred (`candidate`) cards for a profile, for activation-budget
/// promotion: a card that missed the daily mint budget at creation time has
/// no other path back to `new` — nothing else in the codebase ever revisits
/// it — so each queue fetch gives the oldest ones first claim on today's
/// remaining budget.
pub const SELECT_OLDEST_CANDIDATES: &str = "SELECT id FROM cards
 WHERE learner_language_profile_id = ?1 AND status = 'candidate'
 ORDER BY created_at ASC LIMIT ?2";

/// Audit row per attempted mutation (applied or not, 07 §3).
pub const INSERT_CARD_MUTATION: &str = "INSERT INTO card_mutations
    (id, card_id, operation, client_operation_id, before_json, after_json,
     source_lesson_id, actor, reason, language_pack_id, language_pack_version,
     validation_result, created_at)
 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
 ON CONFLICT(client_operation_id) DO NOTHING";

/// Tag insert.
pub const INSERT_CARD_TAG: &str = "INSERT INTO card_tags
    (card_id, tag_type, normalized_tag) VALUES (?1, ?2, ?3)
 ON CONFLICT DO NOTHING";

/// `cards` row, column-shaped.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CardRow {
    /// `id`.
    pub id: String,
    /// `learner_language_profile_id`.
    pub learner_language_profile_id: String,
    /// `language_pack_id`.
    pub language_pack_id: String,
    /// `language_pack_version`.
    pub language_pack_version: String,
    /// `intent` TEXT.
    pub intent: String,
    /// `status` TEXT.
    pub status: String,
    /// `prompt`.
    pub prompt: String,
    /// `prompt_language`.
    pub prompt_language: String,
    /// `answer`.
    pub answer: String,
    /// `answer_language`.
    pub answer_language: String,
    /// `accepted_answers_json` TEXT.
    pub accepted_answers_json: String,
    /// `answer_evaluation_mode`.
    pub answer_evaluation_mode: String,
    /// `explanation`.
    pub explanation: Option<String>,
    /// `pronunciation_note`.
    pub pronunciation_note: Option<String>,
    /// `source_learning_item_id`.
    pub source_learning_item_id: Option<String>,
    /// `source_error_target_id`.
    pub source_error_target_id: Option<String>,
    /// `source_lesson_id`.
    pub source_lesson_id: Option<String>,
    /// `creation_reason`.
    pub creation_reason: String,
    /// `priority_score`.
    pub priority_score: Option<f64>,
    /// `content_hash`.
    pub content_hash: String,
    /// `content_version`.
    pub content_version: u32,
    /// `parent_card_id`.
    pub parent_card_id: Option<String>,
    /// `suspended_reason`.
    pub suspended_reason: Option<String>,
    /// `created_by` TEXT.
    pub created_by: String,
}

impl CardRow {
    pub(crate) fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            learner_language_profile_id: row.get("learner_language_profile_id")?,
            language_pack_id: row.get("language_pack_id")?,
            language_pack_version: row.get("language_pack_version")?,
            intent: row.get("intent")?,
            status: row.get("status")?,
            prompt: row.get("prompt")?,
            prompt_language: row.get("prompt_language")?,
            answer: row.get("answer")?,
            answer_language: row.get("answer_language")?,
            accepted_answers_json: row.get("accepted_answers_json")?,
            answer_evaluation_mode: row.get("answer_evaluation_mode")?,
            explanation: row.get("explanation")?,
            pronunciation_note: row.get("pronunciation_note")?,
            source_learning_item_id: row.get("source_learning_item_id")?,
            source_error_target_id: row.get("source_error_target_id")?,
            source_lesson_id: row.get("source_lesson_id")?,
            creation_reason: row.get("creation_reason")?,
            priority_score: row.get("priority_score")?,
            content_hash: row.get("content_hash")?,
            content_version: row.get("content_version")?,
            parent_card_id: row.get("parent_card_id")?,
            suspended_reason: row.get("suspended_reason")?,
            created_by: row.get("created_by")?,
        })
    }
}

impl CardRow {
    /// Column row → domain type.
    ///
    /// # Errors
    ///
    /// [`RepoError::CorruptRow`] on invalid enum/version/JSON.
    pub fn into_domain(self) -> Result<Card, RepoError> {
        let intent: CardIntent = parse_enum("intent", &self.intent)?;
        let status: CardStatus = parse_enum("status", &self.status)?;
        let created_by: CardActor = parse_enum("created_by", &self.created_by)?;
        let version = semver::Version::parse(&self.language_pack_version)
            .map_err(|e| RepoError::CorruptRow(format!("language_pack_version: {e}")))?;
        let accepted_answers: Vec<String> = serde_json::from_str(&self.accepted_answers_json)
            .map_err(|e| RepoError::CorruptRow(format!("accepted_answers_json: {e}")))?;
        Ok(Card {
            id: CardId(self.id),
            language_profile_id: LanguageProfileId(self.learner_language_profile_id),
            language_pack_id: LanguagePackId(self.language_pack_id),
            language_pack_version: version,
            intent,
            status,
            prompt: self.prompt,
            prompt_language: LanguageTag(self.prompt_language),
            answer: self.answer,
            answer_language: LanguageTag(self.answer_language),
            accepted_answers,
            answer_evaluation_mode: self.answer_evaluation_mode,
            explanation: self.explanation,
            pronunciation_note: self.pronunciation_note,
            source_learning_item_id: self.source_learning_item_id.map(LearningItemId),
            source_error_target_id: self.source_error_target_id.map(ErrorTargetId),
            source_lesson_id: self.source_lesson_id.map(LessonId),
            creation_reason: self.creation_reason,
            priority_score: self.priority_score,
            content_hash: self.content_hash,
            content_version: self.content_version,
            parent_card_id: self.parent_card_id.map(CardId),
            suspended_reason: self.suspended_reason,
            created_by,
        })
    }
}
