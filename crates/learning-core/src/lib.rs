//! Language-neutral scalar contracts and pure rules.
//!
//! Persistent entity rows live in SQLite; request-local processing storage lives in
//! the operation that owns it. This crate intentionally does not retain entity
//! records or object graphs.

mod card;

pub use card::{CardActor, CardIntent, CardStatus, card_content_hash, card_content_hash_refs};
pub use language_pack_api::CoreErrorCategory;

use serde::{Deserialize, Serialize};

/// UTC instant represented as Unix seconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Timestamp(pub i64);

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
    /// MCP tool call.
    Mcp,
}

/// Learning item kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LearningItemType {
    /// Useful phrase.
    Phrase,
    /// Word combination.
    Collocation,
    /// Grammatical/productive pattern.
    Pattern,
    /// Isolated word.
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
    /// Produced unaided once.
    ProvisionallyMastered,
    /// Used correctly in enough distinct transfer contexts.
    Mastered,
    /// Parked.
    Suspended,
}

/// Fixed evidence kinds.
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

/// Error target lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorTargetStatus {
    /// Being tracked.
    Active,
    /// In the recheck cycle.
    Recheck,
    /// No longer recurring.
    Resolved,
    /// Escalated to method change.
    Escalated,
}

/// Communicative impact of an observed error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CommunicativeImpact {
    /// Mostly cosmetic or low-risk to task completion.
    Low,
    /// Noticeable but normally recoverable.
    #[default]
    Medium,
    /// Likely to block, mislead, or materially damage the communicative task.
    High,
}

/// Occurrences after which a medium-impact mini-target is recommended.
pub const ERROR_ESCALATION_THRESHOLD: u32 = 3;
/// Unresolved occurrences after which method change is required.
pub const ERROR_METHOD_CHANGE_THRESHOLD: u32 = 5;
/// Recheck schedule for a newly tracked error, in whole days.
pub const ERROR_RECHECK_OFFSETS_DAYS: [u32; 3] = [1, 3, 7];

/// Impact-aware mini-target threshold. These are explicit v1 policy heuristics.
#[must_use]
pub const fn error_escalation_threshold(impact: CommunicativeImpact) -> u32 {
    match impact {
        CommunicativeImpact::High => 2,
        CommunicativeImpact::Medium => 3,
        CommunicativeImpact::Low => 4,
    }
}

/// Impact-aware method-change threshold.
#[must_use]
pub const fn error_method_change_threshold(impact: CommunicativeImpact) -> u32 {
    match impact {
        CommunicativeImpact::High => 4,
        CommunicativeImpact::Medium => 5,
        CommunicativeImpact::Low => 6,
    }
}

/// True when a recurring error warrants a mini-target at the given impact.
#[must_use]
pub fn error_escalation_due_for(
    occurrence_count: u32,
    resolved: bool,
    impact: CommunicativeImpact,
) -> bool {
    !resolved && occurrence_count >= error_escalation_threshold(impact)
}

/// True when unresolved repetitions require a method change at the given impact.
#[must_use]
pub fn error_method_change_due_for(
    occurrence_count: u32,
    resolved: bool,
    impact: CommunicativeImpact,
) -> bool {
    !resolved && occurrence_count >= error_method_change_threshold(impact)
}

/// True when a recurring error warrants a mini-target.
#[must_use]
pub fn error_escalation_due(occurrence_count: u32, resolved: bool) -> bool {
    error_escalation_due_for(occurrence_count, resolved, CommunicativeImpact::Medium)
}

/// True when unresolved repetitions require a different teaching method.
#[must_use]
pub fn error_method_change_due(occurrence_count: u32, resolved: bool) -> bool {
    error_method_change_due_for(occurrence_count, resolved, CommunicativeImpact::Medium)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn enum_db_encoding_matches_check_constraints() {
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
        assert!(!error_escalation_due(2, false));
        assert!(error_escalation_due(3, false));
        assert!(!error_escalation_due(3, true));
        assert!(!error_method_change_due(4, false));
        assert!(error_method_change_due(5, false));
        assert!(!error_method_change_due(5, true));
        assert!(error_escalation_due_for(
            2,
            false,
            CommunicativeImpact::High
        ));
        assert!(!error_escalation_due_for(
            2,
            false,
            CommunicativeImpact::Low
        ));
        assert!(error_method_change_due_for(
            4,
            false,
            CommunicativeImpact::High
        ));
        assert!(!error_method_change_due_for(
            5,
            false,
            CommunicativeImpact::Low
        ));
    }
}
