//! Card model (04-database.md `cards`, 05-review-engine.md §3, 07 §5).
//! Scheduling state lives in `review-engine`; this is content + lifecycle.

use crate::{
    CardId, ErrorTargetId, LanguagePackId, LanguageProfileId, LanguageTag, LearningItemId, LessonId,
};
use serde::{Deserialize, Serialize};

pub use language_pack_api::CardIntent;

/// Card lifecycle (05-review-engine.md §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CardStatus {
    /// Tutor proposal; not introduced (budget/duplication/quality pending).
    Candidate,
    /// Activated, never reviewed.
    New,
    /// In initial learning.
    Learning,
    /// Owned by the scheduler.
    Review,
    /// Failed a review; relearning.
    Relearning,
    /// Paused; restorable.
    Suspended,
    /// Permanently retired.
    Retired,
}

/// Who caused a card change (04-database.md `created_by`/`actor`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CardActor {
    /// ChatGPT tutor via lesson result / MCP.
    Tutor,
    /// Owner via UI.
    User,
    /// Deterministic system rule.
    System,
}

/// One review card.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Card {
    /// Card ID.
    pub id: CardId,
    /// Owning language profile.
    pub language_profile_id: LanguageProfileId,
    /// Pack that produced it.
    pub language_pack_id: LanguagePackId,
    /// Pack version that produced it.
    pub language_pack_version: semver::Version,
    /// Intent (production/listening/contrast/transfer).
    pub intent: CardIntent,
    /// Lifecycle status.
    pub status: CardStatus,
    /// Learner-facing prompt.
    pub prompt: String,
    /// Prompt language.
    pub prompt_language: LanguageTag,
    /// Preferred answer.
    pub answer: String,
    /// Answer language.
    pub answer_language: LanguageTag,
    /// Accepted alternatives.
    pub accepted_answers: Vec<String>,
    /// Pack-defined evaluation mode identifier.
    pub answer_evaluation_mode: String,
    /// Short explanation shown after reveal.
    pub explanation: Option<String>,
    /// Pronunciation note.
    pub pronunciation_note: Option<String>,
    /// Source learning item.
    pub source_learning_item_id: Option<LearningItemId>,
    /// Source error target.
    pub source_error_target_id: Option<ErrorTargetId>,
    /// Lesson that produced it.
    pub source_lesson_id: Option<LessonId>,
    /// Why this card exists (quality rule: mandatory).
    pub creation_reason: String,
    /// Tutor candidate score 0..=1.
    pub priority_score: Option<f64>,
    /// Duplicate-detection hash, see [`Card::content_hash_of`].
    pub content_hash: String,
    /// Bumped on clarifying/semantic edits.
    pub content_version: u32,
    /// Split/merge lineage.
    pub parent_card_id: Option<CardId>,
    /// Why suspended, when suspended.
    pub suspended_reason: Option<String>,
    /// Original author.
    pub created_by: CardActor,
}

impl Card {
    /// Deterministic content hash over the recall-relevant fields
    /// (intent, prompt, answer, accepted answers) — duplicate detection key
    /// (07-tutor-contract.md §4).
    #[must_use]
    pub fn content_hash_of(
        intent: CardIntent,
        prompt: &str,
        answer: &str,
        accepted_answers: &[String],
    ) -> String {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        let mut eat = |bytes: &[u8]| {
            for b in bytes {
                hash ^= u64::from(*b);
                hash = hash.wrapping_mul(0x100_0000_01b3);
            }
            hash ^= 0xfe;
            hash = hash.wrapping_mul(0x100_0000_01b3);
        };
        eat(format!("{intent:?}").as_bytes());
        eat(prompt.trim().to_lowercase().as_bytes());
        eat(answer.trim().to_lowercase().as_bytes());
        let mut sorted: Vec<String> = accepted_answers
            .iter()
            .map(|a| a.trim().to_lowercase())
            .collect();
        sorted.sort_unstable();
        for accepted in &sorted {
            eat(accepted.as_bytes());
        }
        format!("{hash:016x}")
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn content_hash_ignores_case_whitespace_and_accepted_order() {
        let a = Card::content_hash_of(
            CardIntent::Production,
            "Ask for water",
            "¿Me da agua?",
            &["dame agua".to_owned(), "quiero agua".to_owned()],
        );
        let b = Card::content_hash_of(
            CardIntent::Production,
            "  ask for WATER ",
            "¿me da agua? ",
            &["Quiero agua".to_owned(), "Dame agua".to_owned()],
        );
        assert_eq!(a, b);
    }

    #[test]
    fn content_hash_distinguishes_intent_and_answer() {
        let base = Card::content_hash_of(CardIntent::Production, "p", "a", &[]);
        assert_ne!(
            base,
            Card::content_hash_of(CardIntent::Listening, "p", "a", &[])
        );
        assert_ne!(
            base,
            Card::content_hash_of(CardIntent::Production, "p", "b", &[])
        );
    }

    #[test]
    fn status_encoding_matches_check_constraint() {
        for (status, text) in [
            (CardStatus::Candidate, "candidate"),
            (CardStatus::Relearning, "relearning"),
            (CardStatus::Retired, "retired"),
        ] {
            assert_eq!(
                serde_json::to_value(status).unwrap(),
                serde_json::json!(text)
            );
        }
    }
}
