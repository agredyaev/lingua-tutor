//! Learning records repositories (WBS 4.3): `learning_items`,
//! `learning_item_evidence`, `error_targets`, `error_occurrences`,
//! `error_rechecks`, `listening_attempts`.

use super::{RepoError, enum_str, parse_enum};
use learning_core::{
    ErrorTarget, ErrorTargetId, ErrorTargetStatus, LanguagePackId, LanguageProfileId, LanguageTag,
    LearningItem, LearningItemId, LearningItemStatus, LearningItemType, LessonId,
};
use serde::{Deserialize, Serialize};

/// Insert a learning item.
pub const INSERT_ITEM: &str = "INSERT INTO learning_items
    (id, learner_language_profile_id, language_pack_id, item_type, target_text,
     normalized_target_text, prompt_text, prompt_language, status,
     language_metadata_json, introduced_lesson_id, created_at, updated_at)
 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?12)";

/// Update item status.
pub const UPDATE_ITEM_STATUS: &str =
    "UPDATE learning_items SET status = ?2, updated_at = ?3 WHERE id = ?1";

/// Item by ID.
pub const SELECT_ITEM: &str = "SELECT id, learner_language_profile_id,
    language_pack_id, item_type, target_text, normalized_target_text,
    prompt_text, prompt_language, status, language_metadata_json,
    introduced_lesson_id
 FROM learning_items WHERE id = ?1";

/// Duplicate check by canonical text (uses `ix_items_normalized`).
pub const SELECT_ITEM_BY_NORMALIZED: &str = "SELECT id, learner_language_profile_id,
    language_pack_id, item_type, target_text, normalized_target_text,
    prompt_text, prompt_language, status, language_metadata_json,
    introduced_lesson_id
 FROM learning_items
 WHERE learner_language_profile_id = ?1 AND normalized_target_text = ?2";

/// Profile items filtered by status (uses `ix_items_profile_status`).
pub const SELECT_ITEMS_BY_STATUS: &str = "SELECT id, learner_language_profile_id,
    language_pack_id, item_type, target_text, normalized_target_text,
    prompt_text, prompt_language, status, language_metadata_json,
    introduced_lesson_id
 FROM learning_items
 WHERE learner_language_profile_id = ?1 AND status = ?2 LIMIT ?3";

/// Insert evidence.
pub const INSERT_EVIDENCE: &str = "INSERT INTO learning_item_evidence
    (id, learning_item_id, lesson_id, kind, success, detail, pack_metadata_json,
     recorded_at)
 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)";

/// Evidence trail for one item (uses `ix_evidence_item`).
pub const SELECT_EVIDENCE_FOR_ITEM: &str = "SELECT id, learning_item_id, lesson_id,
    kind, success, detail, pack_metadata_json, recorded_at
 FROM learning_item_evidence WHERE learning_item_id = ?1 ORDER BY recorded_at";

/// Insert an error target.
pub const INSERT_ERROR_TARGET: &str = "INSERT INTO error_targets
    (id, learner_language_profile_id, language_pack_id, language_pack_version,
     core_category, pack_error_code, original_production, corrected_production,
     explanation, occurrence_count, method_change_required, status,
     created_at, updated_at)
 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?13)";

/// Update target counters/status after a sighting.
pub const UPDATE_ERROR_TARGET: &str = "UPDATE error_targets SET
    occurrence_count = ?2, method_change_required = ?3, status = ?4, updated_at = ?5
 WHERE id = ?1";

/// Target by ID.
pub const SELECT_ERROR_TARGET: &str = "SELECT id, learner_language_profile_id,
    language_pack_id, language_pack_version, core_category, pack_error_code,
    original_production, corrected_production, explanation, occurrence_count,
    method_change_required, status
 FROM error_targets WHERE id = ?1";

/// Profile targets by status (uses `ix_errors_profile_status`).
pub const SELECT_ERROR_TARGETS: &str = "SELECT id, learner_language_profile_id,
    language_pack_id, language_pack_version, core_category, pack_error_code,
    original_production, corrected_production, explanation, occurrence_count,
    method_change_required, status
 FROM error_targets
 WHERE learner_language_profile_id = ?1 AND (?2 IS NULL OR status = ?2)
 ORDER BY occurrence_count DESC LIMIT ?3";

/// Insert one sighting.
pub const INSERT_ERROR_OCCURRENCE: &str = "INSERT INTO error_occurrences
    (id, error_target_id, lesson_id, context, recorded_at)
 VALUES (?1, ?2, ?3, ?4, ?5)";

/// Insert a 1/3/7-day recheck slot.
pub const INSERT_ERROR_RECHECK: &str = "INSERT INTO error_rechecks
    (id, error_target_id, offset_days, due_at)
 VALUES (?1, ?2, ?3, ?4)
 ON CONFLICT(error_target_id, offset_days) DO NOTHING";

/// Complete a recheck with pass/fail.
pub const COMPLETE_ERROR_RECHECK: &str = "UPDATE error_rechecks SET
    completed_at = ?2, passed = ?3 WHERE id = ?1 AND completed_at IS NULL";

/// Due rechecks (uses partial index `ix_rechecks_due`).
pub const SELECT_DUE_RECHECKS: &str = "SELECT id, error_target_id, offset_days, due_at
 FROM error_rechecks WHERE completed_at IS NULL AND due_at <= ?1 ORDER BY due_at";

/// Insert a listening attempt (no audio, references only).
pub const INSERT_LISTENING_ATTEMPT: &str = "INSERT INTO listening_attempts
    (id, learner_language_profile_id, learning_item_id, lesson_id,
     source_reference, transcript, success, recorded_at)
 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)";

/// Profile listening attempts, newest first, bounded.
pub const SELECT_LISTENING_ATTEMPTS: &str = "SELECT id, learner_language_profile_id,
    learning_item_id, lesson_id, source_reference, transcript, success, recorded_at
 FROM listening_attempts WHERE learner_language_profile_id = ?1
 ORDER BY recorded_at DESC LIMIT ?2";

/// `learning_items` row, column-shaped.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemRow {
    /// `id`.
    pub id: String,
    /// `learner_language_profile_id`.
    pub learner_language_profile_id: String,
    /// `language_pack_id`.
    pub language_pack_id: String,
    /// `item_type` TEXT.
    pub item_type: String,
    /// `target_text`.
    pub target_text: String,
    /// `normalized_target_text`.
    pub normalized_target_text: String,
    /// `prompt_text`.
    pub prompt_text: Option<String>,
    /// `prompt_language`.
    pub prompt_language: Option<String>,
    /// `status` TEXT.
    pub status: String,
    /// `language_metadata_json` TEXT.
    pub language_metadata_json: String,
    /// `introduced_lesson_id`.
    pub introduced_lesson_id: Option<String>,
}

impl ItemRow {
    /// Column row → domain type.
    ///
    /// # Errors
    ///
    /// [`RepoError::CorruptRow`] on invalid enum or metadata JSON.
    pub fn into_domain(self) -> Result<LearningItem, RepoError> {
        let item_type: LearningItemType = parse_enum("item_type", &self.item_type)?;
        let status: LearningItemStatus = parse_enum("status", &self.status)?;
        let language_metadata = serde_json::from_str(&self.language_metadata_json)
            .map_err(|e| RepoError::CorruptRow(format!("language_metadata_json: {e}")))?;
        Ok(LearningItem {
            id: LearningItemId(self.id),
            language_profile_id: LanguageProfileId(self.learner_language_profile_id),
            language_pack_id: LanguagePackId(self.language_pack_id),
            item_type,
            target_text: self.target_text,
            normalized_target_text: self.normalized_target_text,
            prompt_text: self.prompt_text,
            prompt_language: self.prompt_language.map(LanguageTag),
            status,
            language_metadata,
            introduced_lesson_id: self.introduced_lesson_id.map(LessonId),
        })
    }

    /// Domain type → column row.
    #[must_use]
    pub fn from_domain(item: &LearningItem) -> Self {
        Self {
            id: item.id.0.clone(),
            learner_language_profile_id: item.language_profile_id.0.clone(),
            language_pack_id: item.language_pack_id.0.clone(),
            item_type: enum_str(&item.item_type),
            target_text: item.target_text.clone(),
            normalized_target_text: item.normalized_target_text.clone(),
            prompt_text: item.prompt_text.clone(),
            prompt_language: item.prompt_language.as_ref().map(|t| t.0.clone()),
            status: enum_str(&item.status),
            language_metadata_json: item.language_metadata.to_string(),
            introduced_lesson_id: item.introduced_lesson_id.as_ref().map(|l| l.0.clone()),
        }
    }
}

/// `error_targets` row, column-shaped.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorTargetRow {
    /// `id`.
    pub id: String,
    /// `learner_language_profile_id`.
    pub learner_language_profile_id: String,
    /// `language_pack_id`.
    pub language_pack_id: String,
    /// `language_pack_version`.
    pub language_pack_version: String,
    /// `core_category` TEXT.
    pub core_category: String,
    /// `pack_error_code`.
    pub pack_error_code: Option<String>,
    /// `original_production`.
    pub original_production: String,
    /// `corrected_production`.
    pub corrected_production: String,
    /// `explanation`.
    pub explanation: Option<String>,
    /// `occurrence_count`.
    pub occurrence_count: u32,
    /// `method_change_required` 0/1.
    pub method_change_required: u8,
    /// `status` TEXT.
    pub status: String,
}

impl ErrorTargetRow {
    /// Column row → domain type.
    ///
    /// # Errors
    ///
    /// [`RepoError::CorruptRow`] on invalid enum or version text.
    pub fn into_domain(self) -> Result<ErrorTarget, RepoError> {
        let core_category = parse_enum("core_category", &self.core_category)?;
        let status: ErrorTargetStatus = parse_enum("status", &self.status)?;
        let version = semver::Version::parse(&self.language_pack_version)
            .map_err(|e| RepoError::CorruptRow(format!("language_pack_version: {e}")))?;
        Ok(ErrorTarget {
            id: ErrorTargetId(self.id),
            language_profile_id: LanguageProfileId(self.learner_language_profile_id),
            language_pack_id: LanguagePackId(self.language_pack_id),
            language_pack_version: version,
            core_category,
            pack_error_code: self.pack_error_code,
            original_production: self.original_production,
            corrected_production: self.corrected_production,
            explanation: self.explanation,
            occurrence_count: self.occurrence_count,
            method_change_required: self.method_change_required != 0,
            status,
        })
    }

    /// Domain type → column row.
    #[must_use]
    pub fn from_domain(target: &ErrorTarget) -> Self {
        Self {
            id: target.id.0.clone(),
            learner_language_profile_id: target.language_profile_id.0.clone(),
            language_pack_id: target.language_pack_id.0.clone(),
            language_pack_version: target.language_pack_version.to_string(),
            core_category: enum_str(&target.core_category),
            pack_error_code: target.pack_error_code.clone(),
            original_production: target.original_production.clone(),
            corrected_production: target.corrected_production.clone(),
            explanation: target.explanation.clone(),
            occurrence_count: target.occurrence_count,
            method_change_required: u8::from(target.method_change_required),
            status: enum_str(&target.status),
        }
    }
}
