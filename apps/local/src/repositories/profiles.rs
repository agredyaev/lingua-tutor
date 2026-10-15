//! `learner_profile` + `learner_language_profiles` repositories (WBS 4.2).

use super::{RepoError, enum_str, parse_enum};
use learning_core::{
    LanguagePackId, LanguageProfileId, LanguageTag, LearnerId, LearnerLanguageProfile,
    LearnerProfile, ProfileStatus,
};
use serde::{Deserialize, Serialize};

/// Upsert for the single learner row.
pub const UPSERT_LEARNER: &str = "INSERT INTO learner_profile
    (id, timezone, daily_session_target, session_minutes_target, max_new_cards,
     desired_retention, max_review_minutes, created_at, updated_at)
 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)
 ON CONFLICT(id) DO UPDATE SET
    timezone = excluded.timezone,
    daily_session_target = excluded.daily_session_target,
    session_minutes_target = excluded.session_minutes_target,
    max_new_cards = excluded.max_new_cards,
    desired_retention = excluded.desired_retention,
    max_review_minutes = excluded.max_review_minutes,
    updated_at = excluded.updated_at";

/// Single learner row lookup.
pub const SELECT_LEARNER: &str = "SELECT id, timezone, daily_session_target,
    session_minutes_target, max_new_cards, desired_retention, max_review_minutes
 FROM learner_profile LIMIT 1";

/// Insert a language profile.
pub const INSERT_LANGUAGE_PROFILE: &str = "INSERT INTO learner_language_profiles
    (id, learner_id, language_pack_id, source_language, target_locale,
     estimated_level, current_lesson_number, status, is_default,
     preferences_json, created_at, updated_at)
 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?11)";

/// All language profiles, default first.
pub const SELECT_LANGUAGE_PROFILES: &str = "SELECT id, learner_id, language_pack_id,
    source_language, target_locale, estimated_level, current_lesson_number,
    status, is_default, preferences_json
 FROM learner_language_profiles ORDER BY is_default DESC, created_at";

/// One language profile by ID.
pub const SELECT_LANGUAGE_PROFILE: &str = "SELECT id, learner_id, language_pack_id,
    source_language, target_locale, estimated_level, current_lesson_number,
    status, is_default, preferences_json
 FROM learner_language_profiles WHERE id = ?1";

/// Patch mutable profile fields.
pub const UPDATE_LANGUAGE_PROFILE: &str = "UPDATE learner_language_profiles SET
    estimated_level = ?2, status = ?3, is_default = ?4, preferences_json = ?5,
    current_lesson_number = ?6, updated_at = ?7
 WHERE id = ?1";

/// `learner_language_profiles` row, column-shaped.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanguageProfileRow {
    /// `id`.
    pub id: String,
    /// `learner_id`.
    pub learner_id: String,
    /// `language_pack_id`.
    pub language_pack_id: String,
    /// `source_language`.
    pub source_language: String,
    /// `target_locale`.
    pub target_locale: String,
    /// `estimated_level`.
    pub estimated_level: Option<String>,
    /// `current_lesson_number`.
    pub current_lesson_number: u32,
    /// `status` TEXT.
    pub status: String,
    /// `is_default` 0/1.
    pub is_default: u8,
    /// `preferences_json` TEXT.
    pub preferences_json: String,
}

impl LanguageProfileRow {
    pub(crate) fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            learner_id: row.get("learner_id")?,
            language_pack_id: row.get("language_pack_id")?,
            source_language: row.get("source_language")?,
            target_locale: row.get("target_locale")?,
            estimated_level: row.get("estimated_level")?,
            current_lesson_number: row.get("current_lesson_number")?,
            status: row.get("status")?,
            is_default: row.get("is_default")?,
            preferences_json: row.get("preferences_json")?,
        })
    }
}

impl LanguageProfileRow {
    /// Column row → domain type.
    ///
    /// # Errors
    ///
    /// [`RepoError::CorruptRow`] on invalid status or preferences JSON.
    pub fn into_domain(self) -> Result<LearnerLanguageProfile, RepoError> {
        let status: ProfileStatus = parse_enum("status", &self.status)?;
        let preferences = serde_json::from_str(&self.preferences_json)
            .map_err(|e| RepoError::CorruptRow(format!("preferences_json: {e}")))?;
        Ok(LearnerLanguageProfile {
            id: LanguageProfileId(self.id),
            learner_id: LearnerId(self.learner_id),
            language_pack_id: LanguagePackId(self.language_pack_id),
            source_language: LanguageTag(self.source_language),
            target_locale: LanguageTag(self.target_locale),
            estimated_level: self.estimated_level,
            current_lesson_number: self.current_lesson_number,
            status,
            is_default: self.is_default != 0,
            preferences,
        })
    }

    /// Domain type → column row.
    #[must_use]
    pub fn from_domain(profile: &LearnerLanguageProfile) -> Self {
        Self {
            id: profile.id.0.clone(),
            learner_id: profile.learner_id.0.clone(),
            language_pack_id: profile.language_pack_id.0.clone(),
            source_language: profile.source_language.0.clone(),
            target_locale: profile.target_locale.0.clone(),
            estimated_level: profile.estimated_level.clone(),
            current_lesson_number: profile.current_lesson_number,
            status: enum_str(&profile.status).to_owned(),
            is_default: u8::from(profile.is_default),
            preferences_json: profile.preferences.to_string(),
        }
    }
}

/// `learner_profile` row, column-shaped.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearnerProfileRow {
    /// `id`.
    pub id: String,
    /// `timezone`.
    pub timezone: String,
    /// `daily_session_target`.
    pub daily_session_target: u8,
    /// `session_minutes_target`.
    pub session_minutes_target: u16,
    /// `max_new_cards`.
    pub max_new_cards: u8,
    /// `desired_retention`.
    pub desired_retention: f64,
    /// `max_review_minutes`.
    pub max_review_minutes: u16,
}

impl LearnerProfileRow {
    /// Column row → domain type.
    #[must_use]
    pub fn into_domain(self) -> LearnerProfile {
        LearnerProfile {
            id: LearnerId(self.id),
            timezone: self.timezone,
            daily_session_target: self.daily_session_target,
            session_minutes_target: self.session_minutes_target,
            max_new_cards: self.max_new_cards,
            desired_retention: self.desired_retention,
            max_review_minutes: self.max_review_minutes,
        }
    }
}
