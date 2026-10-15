//! Language-neutral domain types and rules (03-domain-model.md). No IO, no
//! language-specific logic, no dependency on any concrete pack. Persistence
//! column mapping lives in `apps/local` repositories.

mod card;
mod ids;
mod mastery;

pub use card::{Card, CardActor, CardIntent, CardStatus};
pub use ids::{
    CardId, ErrorTargetId, EvidenceId, LanguagePackId, LanguageProfileId, LanguageTag, LearnerId,
    LearningItemId, LessonId, RecommendationId,
};
pub use language_pack_api::CoreErrorCategory;
pub use mastery::{MasteryAssessment, assess_mastery};

use serde::{Deserialize, Serialize};

/// UTC instant, unix seconds. RFC 3339 TEXT conversion at repository boundary
/// (04-database.md §1). Mirrors `review_engine::Timestamp`; `application`
/// converts between the two trivially.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Timestamp(pub i64);

/// Global learner settings (03-domain-model.md §2). Single row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LearnerProfile {
    /// Learner ID.
    pub id: LearnerId,
    /// IANA timezone name; default `Europe/Madrid`.
    pub timezone: String,
    /// Target study sessions per day (default 2).
    pub daily_session_target: u8,
    /// Target minutes per session (default 20).
    pub session_minutes_target: u16,
    /// New-card daily budget (default 8).
    pub max_new_cards: u8,
    /// Scheduler retention target (default 0.90).
    pub desired_retention: f64,
    /// Daily review-minutes cap (default 12).
    pub max_review_minutes: u16,
}

/// Language profile lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProfileStatus {
    /// In use.
    Active,
    /// Temporarily parked; history kept.
    Paused,
    /// Closed; history kept.
    Archived,
}

/// Per-language learning scope (03-domain-model.md §3): all lessons, items,
/// errors, cards, queues hang off one of these.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LearnerLanguageProfile {
    /// Profile ID.
    pub id: LanguageProfileId,
    /// Owning learner.
    pub learner_id: LearnerId,
    /// Selected pack, e.g. `es-ES`.
    pub language_pack_id: LanguagePackId,
    /// Instruction language, BCP 47.
    pub source_language: LanguageTag,
    /// Target locale, BCP 47.
    pub target_locale: LanguageTag,
    /// Level label from the pack framework, e.g. `A2`.
    pub estimated_level: Option<String>,
    /// Last assigned lesson number.
    pub current_lesson_number: u32,
    /// Lifecycle status.
    pub status: ProfileStatus,
    /// Default profile flag.
    pub is_default: bool,
    /// Free-form preferences; validated at service boundary.
    pub preferences: serde_json::Value,
}

/// Lesson lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LessonStatus {
    /// Proposed, not started.
    Planned,
    /// In progress.
    Active,
    /// Finished with results.
    Completed,
    /// Abandoned without results.
    Abandoned,
}

/// Where a lesson record came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LessonSource {
    /// Created in the PWA.
    Pwa,
    /// Pasted lesson-result import.
    Import,
    /// MCP tool call (post-MVP).
    Mcp,
}

/// One tutor lesson (03-domain-model.md §4).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Lesson {
    /// Lesson ID.
    pub id: LessonId,
    /// Owning language profile.
    pub language_profile_id: LanguageProfileId,
    /// Pack active when the lesson ran.
    pub language_pack_id: LanguagePackId,
    /// Pack version active when the lesson ran.
    pub language_pack_version: semver::Version,
    /// Unique within the profile.
    pub lesson_number: u32,
    /// Lifecycle status.
    pub status: LessonStatus,
    /// Scenario identifier or title.
    pub scenario: String,
    /// Goal set before the lesson.
    pub planned_goal: String,
    /// Origin.
    pub source: LessonSource,
    /// Start instant.
    pub started_at: Option<Timestamp>,
    /// Completion instant.
    pub completed_at: Option<Timestamp>,
    /// Active duration.
    pub duration_seconds: Option<u32>,
    /// Tutor summary text.
    pub tutor_summary: Option<String>,
    /// Tutor-proposed next goal.
    pub next_goal: Option<String>,
    /// Import idempotency key.
    pub idempotency_key: Option<String>,
}

/// Learning item kind (03-domain-model.md §5). Phrase-first: `Word` only
/// when independently useful and a phrase card would add complexity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LearningItemType {
    /// Useful phrase (primary unit).
    Phrase,
    /// Word combination.
    Collocation,
    /// Grammatical/productive pattern.
    Pattern,
    /// Isolated word (exception, see above).
    Word,
    /// Conversational response frame.
    ResponseFrame,
}

/// Learning item progress states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LearningItemStatus {
    /// Proposed, not yet introduced.
    Candidate,
    /// Introduced in a lesson.
    Introduced,
    /// Under active practice.
    Learning,
    /// Awaiting transfer to new contexts.
    TransferPending,
    /// Produced unaided once (mastery rules below).
    ProvisionallyMastered,
    /// Used correctly in ≥ 3 distinct contexts.
    Mastered,
    /// Parked.
    Suspended,
}

/// One learnable unit (03-domain-model.md §5).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LearningItem {
    /// Item ID.
    pub id: LearningItemId,
    /// Owning language profile.
    pub language_profile_id: LanguageProfileId,
    /// Pack that produced/normalized it.
    pub language_pack_id: LanguagePackId,
    /// Kind.
    pub item_type: LearningItemType,
    /// Target-language text as taught.
    pub target_text: String,
    /// Canonical form via active pack normalization.
    pub normalized_target_text: String,
    /// Learner-facing prompt.
    pub prompt_text: Option<String>,
    /// Prompt language.
    pub prompt_language: Option<LanguageTag>,
    /// Progress status.
    pub status: LearningItemStatus,
    /// Pack-owned metadata; schema is pack-versioned.
    pub language_metadata: serde_json::Value,
    /// Lesson that introduced it.
    pub introduced_lesson_id: Option<LessonId>,
}

/// Fixed evidence kinds (03-domain-model.md §6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceKind {
    /// Understood in text.
    Recognition,
    /// Understood in audio.
    Listening,
    /// Produced with hint/prompt.
    PromptedProduction,
    /// Produced unaided in conversation.
    SpontaneousProduction,
    /// Adapted to a new context.
    Transfer,
    /// Corrected form accepted.
    Correction,
}

/// One evidence observation for a learning item.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LearningItemEvidence {
    /// Evidence ID.
    pub id: EvidenceId,
    /// Observed item.
    pub learning_item_id: LearningItemId,
    /// Lesson context, when applicable.
    pub lesson_id: Option<LessonId>,
    /// Kind.
    pub kind: EvidenceKind,
    /// Outcome.
    pub success: bool,
    /// Free-form detail.
    pub detail: Option<String>,
    /// Bounded pack metadata.
    pub pack_metadata: Option<serde_json::Value>,
    /// Observation instant.
    pub recorded_at: Timestamp,
}

/// Error target lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorTargetStatus {
    /// Being tracked.
    Active,
    /// In 1/3/7-day recheck cycle.
    Recheck,
    /// No longer recurring.
    Resolved,
    /// Escalated to method change.
    Escalated,
}

/// Recurring-error tracking target (03-domain-model.md §7).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ErrorTarget {
    /// Target ID.
    pub id: ErrorTargetId,
    /// Owning language profile.
    pub language_profile_id: LanguageProfileId,
    /// Pack that classified it.
    pub language_pack_id: LanguagePackId,
    /// Pack version that classified it.
    pub language_pack_version: semver::Version,
    /// Core category (closed set, `language-pack-api`).
    pub core_category: CoreErrorCategory,
    /// Stable pack taxonomy code.
    pub pack_error_code: Option<String>,
    /// What the learner produced.
    pub original_production: String,
    /// Corrected form.
    pub corrected_production: String,
    /// Explanation shown to learner.
    pub explanation: Option<String>,
    /// Sightings so far.
    pub occurrence_count: u32,
    /// Set at 5 unresolved occurrences.
    pub method_change_required: bool,
    /// Lifecycle status.
    pub status: ErrorTargetStatus,
}

/// Occurrences after which a mini-target (+ contrast card when appropriate)
/// is recommended (03-domain-model.md §7).
pub const ERROR_ESCALATION_THRESHOLD: u32 = 3;
/// Unresolved occurrences after which method change is required.
pub const ERROR_METHOD_CHANGE_THRESHOLD: u32 = 5;

impl ErrorTarget {
    /// Registers one more sighting: bumps the count and flags method change
    /// at [`ERROR_METHOD_CHANGE_THRESHOLD`] unresolved occurrences.
    pub fn register_occurrence(&mut self) {
        self.occurrence_count += 1;
        if self.occurrence_count >= ERROR_METHOD_CHANGE_THRESHOLD
            && self.status != ErrorTargetStatus::Resolved
        {
            self.method_change_required = true;
        }
    }

    /// True once the occurrence count warrants a mini-target escalation.
    #[must_use]
    pub fn needs_escalation(&self) -> bool {
        self.occurrence_count >= ERROR_ESCALATION_THRESHOLD
            && self.status != ErrorTargetStatus::Resolved
    }
}

/// What an adaptive recommendation is about (03-domain-model.md §8).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "subject_type",
    content = "subject_id",
    rename_all = "snake_case"
)]
pub enum RecommendationSubject {
    /// A learning item.
    LearningItem(LearningItemId),
    /// An error target.
    ErrorTarget(ErrorTargetId),
    /// A card.
    Card(CardId),
    /// A listening-weak item.
    ListeningItem(LearningItemId),
    /// Overall review workload for a profile.
    ReviewLoad(LanguageProfileId),
    /// The profile itself (e.g. level change).
    LanguageProfile(LanguageProfileId),
    /// Pack upgrade caution.
    PackUpgradeWarning(LanguagePackId),
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn enum_db_encoding_matches_check_constraints() {
        // serde snake_case strings must equal 04-database.md CHECK values.
        let cases = [
            (
                serde_json::to_value(LessonStatus::Planned).unwrap(),
                "planned",
            ),
            (serde_json::to_value(LessonSource::Pwa).unwrap(), "pwa"),
            (
                serde_json::to_value(LearningItemStatus::TransferPending).unwrap(),
                "transfer_pending",
            ),
            (
                serde_json::to_value(EvidenceKind::SpontaneousProduction).unwrap(),
                "spontaneous_production",
            ),
            (
                serde_json::to_value(ErrorTargetStatus::Recheck).unwrap(),
                "recheck",
            ),
            (
                serde_json::to_value(LearningItemType::ResponseFrame).unwrap(),
                "response_frame",
            ),
        ];
        for (actual, expected) in cases {
            assert_eq!(actual, serde_json::json!(expected));
        }
    }

    #[test]
    fn error_target_escalation_thresholds() {
        let mut target = ErrorTarget {
            id: ErrorTargetId("e-1".to_owned()),
            language_profile_id: LanguageProfileId("llp-1".to_owned()),
            language_pack_id: LanguagePackId("zz-ZZ".to_owned()),
            language_pack_version: semver::Version::new(0, 1, 0),
            core_category: CoreErrorCategory::Grammar,
            pack_error_code: None,
            original_production: "a".to_owned(),
            corrected_production: "b".to_owned(),
            explanation: None,
            occurrence_count: 1,
            method_change_required: false,
            status: ErrorTargetStatus::Active,
        };
        assert!(!target.needs_escalation());
        target.register_occurrence();
        target.register_occurrence();
        assert_eq!(target.occurrence_count, 3);
        assert!(target.needs_escalation(), "3 occurrences → mini-target");
        assert!(!target.method_change_required);
        target.register_occurrence();
        target.register_occurrence();
        assert!(
            target.method_change_required,
            "5 unresolved → method change"
        );
    }

    #[test]
    fn recommendation_subject_serializes_with_tag_and_id() {
        let subject = RecommendationSubject::Card(CardId("c-1".to_owned()));
        assert_eq!(
            serde_json::to_value(&subject).unwrap(),
            serde_json::json!({"subject_type": "card", "subject_id": "c-1"})
        );
    }
}
