//! Versioned tutor lesson-result and card-mutation schemas + validation
//! (07-tutor-contract.md §2–3). Schema and bounds checking only — no
//! business logic; applying results is `application`'s job.
//!
//! This is the untrusted-input boundary: payloads arrive as pasted ChatGPT
//! output. Parsing/validation must never panic (fuzzed in `fuzz/`).

mod validate;

pub use validate::{ValidationIssue, validate};

use language_pack_api::CardIntent;
use learning_core::{CoreErrorCategory, EvidenceKind, LearningItemStatus, LearningItemType};
use serde::{Deserialize, Serialize};

/// Supported lesson-result schema version.
pub const SCHEMA_VERSION: &str = "1";

/// Hard size bounds (07-tutor-contract.md §2 "schema- and size-bounded").
pub mod bounds {
    /// Max new learning items per lesson result.
    pub const MAX_NEW_ITEMS: usize = 50;
    /// Max evidence entries.
    pub const MAX_EVIDENCE: usize = 200;
    /// Max error occurrences.
    pub const MAX_ERROR_OCCURRENCES: usize = 50;
    /// Max listening attempts.
    pub const MAX_LISTENING_ATTEMPTS: usize = 100;
    /// Max mastery changes.
    pub const MAX_MASTERY_CHANGES: usize = 100;
    /// Max card mutations.
    pub const MAX_CARD_MUTATIONS: usize = 50;
    /// Max accepted answers per card.
    pub const MAX_ACCEPTED_ANSWERS: usize = 10;
    /// Max tags per tag list.
    pub const MAX_TAGS: usize = 10;
    /// Max length of short text fields (prompts, answers, productions).
    pub const MAX_SHORT_TEXT: usize = 500;
    /// Max length of long text fields (summaries, explanations).
    pub const MAX_LONG_TEXT: usize = 4_000;
    /// Max length of IDs and keys.
    pub const MAX_ID: usize = 128;
}

/// Parses a lesson-result JSON document. Schema check only; call
/// [`validate`] afterwards for bounds + cross-field rules.
///
/// # Errors
///
/// `serde_json::Error` when the document does not match the schema
/// (unknown fields rejected).
pub fn parse_lesson_result(json: &str) -> Result<LessonResult, serde_json::Error> {
    serde_json::from_str(json)
}

/// Versioned block returned by the tutor after a lesson (§2). Applied
/// atomically under `client_operation_id` idempotency.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LessonResult {
    /// Must equal [`SCHEMA_VERSION`].
    pub schema_version: String,
    /// Idempotency key for the whole import.
    pub client_operation_id: String,
    /// Target language profile.
    pub language_profile_id: String,
    /// Pack the lesson ran under.
    pub language_pack_id: String,
    /// Pack version the lesson ran under.
    pub language_pack_version: String,
    /// Lesson block.
    pub lesson: LessonBlock,
    /// Lesson metrics.
    #[serde(default)]
    pub metrics: Option<MetricsBlock>,
    /// Items introduced this lesson.
    #[serde(default)]
    pub new_learning_items: Vec<NewLearningItem>,
    /// Evidence observed this lesson.
    #[serde(default)]
    pub evidence: Vec<EvidenceEntry>,
    /// Errors observed this lesson.
    #[serde(default)]
    pub error_occurrences: Vec<ErrorOccurrenceEntry>,
    /// Listening attempts (no audio, references only).
    #[serde(default)]
    pub listening_attempts: Vec<ListeningAttemptEntry>,
    /// Tutor-proposed status changes.
    #[serde(default)]
    pub mastery_changes: Vec<MasteryChange>,
    /// Proposed card mutations.
    #[serde(default)]
    pub card_mutations: Vec<CardMutation>,
}

/// Lesson identification + outcome text (§2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LessonBlock {
    /// Existing lesson to complete; `None` → import creates the lesson.
    #[serde(default)]
    pub lesson_id: Option<String>,
    /// Scenario identifier or title.
    pub scenario: String,
    /// Active lesson duration.
    #[serde(default)]
    pub duration_seconds: Option<u32>,
    /// Tutor summary.
    #[serde(default)]
    pub tutor_summary: Option<String>,
    /// Proposed next goal.
    #[serde(default)]
    pub next_goal: Option<String>,
}

/// Lesson metrics (maps to `lesson_metrics`, 04-database.md).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetricsBlock {
    /// Active speaking time.
    #[serde(default)]
    pub speaking_seconds: u32,
    /// Unaided recalls.
    #[serde(default)]
    pub unaided_recall_count: u32,
    /// Aided recalls.
    #[serde(default)]
    pub aided_recall_count: u32,
    /// Listening comprehension 0..=1.
    #[serde(default)]
    pub listening_score: Option<f64>,
    /// Mean answer latency.
    #[serde(default)]
    pub avg_answer_millis: Option<u32>,
    /// Long pauses observed.
    #[serde(default)]
    pub long_pause_count: u32,
    /// Tutor-estimated confidence 0..=1.
    #[serde(default)]
    pub confidence: Option<f64>,
}

/// One item introduced this lesson.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewLearningItem {
    /// Item kind.
    pub item_type: LearningItemType,
    /// Target-language text.
    pub target_text: String,
    /// Learner-facing prompt.
    #[serde(default)]
    pub prompt_text: Option<String>,
    /// Prompt language tag.
    #[serde(default)]
    pub prompt_language: Option<String>,
}

/// One evidence observation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceEntry {
    /// Existing learning item.
    pub learning_item_id: String,
    /// Evidence kind.
    pub kind: EvidenceKind,
    /// Outcome.
    pub success: bool,
    /// Free-form detail.
    #[serde(default)]
    pub detail: Option<String>,
}

/// One error sighting; `error_target_id = None` → new target.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ErrorOccurrenceEntry {
    /// Existing target, when recurring.
    #[serde(default)]
    pub error_target_id: Option<String>,
    /// Core category (closed set).
    pub core_category: CoreErrorCategory,
    /// Stable pack taxonomy code.
    #[serde(default)]
    pub pack_error_code: Option<String>,
    /// What the learner produced.
    pub original_production: String,
    /// Corrected form.
    pub corrected_production: String,
    /// Explanation.
    #[serde(default)]
    pub explanation: Option<String>,
}

/// One listening attempt; never audio, references only (00-overview.md §3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListeningAttemptEntry {
    /// Item listened to, when known.
    #[serde(default)]
    pub learning_item_id: Option<String>,
    /// Outcome.
    pub success: bool,
    /// Reference into lesson material.
    #[serde(default)]
    pub source_reference: Option<String>,
    /// What was heard/said, as text.
    #[serde(default)]
    pub transcript: Option<String>,
}

/// Tutor-proposed item status change; server re-checks against mastery rules.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MasteryChange {
    /// Item to change.
    pub learning_item_id: String,
    /// Proposed status.
    pub new_status: LearningItemStatus,
}

/// Card mutation operations (§3, `card_mutations` CHECK set).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MutationOperation {
    /// New card proposal.
    Create,
    /// Edit existing card (edit class decided server-side).
    Update,
    /// Split into smaller cards.
    Split,
    /// Merge into a survivor.
    Merge,
    /// Pause reviews.
    Suspend,
    /// Resume reviews.
    Restore,
    /// Permanently retire.
    Retire,
    /// Add an accepted alternative.
    AddAcceptedAnswer,
    /// Remove an accepted alternative.
    RemoveAcceptedAnswer,
    /// Add a listening variant.
    AddListeningVariant,
    /// Link to phrase / error target / listening item / scenario.
    Link,
}

/// One proposed mutation. Field requirements per operation enforced by
/// [`validate`]; every attempt lands in the `card_mutations` audit either way.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CardMutation {
    /// Operation kind.
    pub operation: MutationOperation,
    /// Per-mutation idempotency key.
    pub client_operation_id: String,
    /// Target card for non-create operations.
    #[serde(default)]
    pub card_id: Option<String>,
    /// Card payload for `create`/`update`/`split`.
    #[serde(default)]
    pub card: Option<CardPayload>,
    /// Accepted answer for add/remove operations.
    #[serde(default)]
    pub accepted_answer: Option<String>,
    /// Why the tutor proposes this.
    #[serde(default)]
    pub reason: Option<String>,
}

/// Card content payload for create-like operations (§3 create payload).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CardPayload {
    /// Card intent.
    pub intent: CardIntent,
    /// Learner-facing prompt.
    pub prompt: String,
    /// Preferred answer.
    pub answer: String,
    /// Accepted alternatives.
    #[serde(default)]
    pub accepted_answers: Vec<String>,
    /// Prompt language; defaults to profile source language server-side.
    #[serde(default)]
    pub prompt_language: Option<String>,
    /// Short explanation.
    #[serde(default)]
    pub explanation: Option<String>,
    /// Scenario tags.
    #[serde(default)]
    pub scenario_tags: Vec<String>,
    /// Skill tags.
    #[serde(default)]
    pub skill_tags: Vec<String>,
    /// Source learning item.
    #[serde(default)]
    pub source_learning_item_id: Option<String>,
    /// Source error target.
    #[serde(default)]
    pub source_error_target_id: Option<String>,
    /// Why this card exists (quality rule: mandatory).
    pub creation_reason: String,
    /// Tutor candidate score.
    #[serde(default)]
    pub priority_score: Option<f64>,
}
