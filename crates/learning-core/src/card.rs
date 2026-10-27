//! Card scalar contracts and duplicate-detection passes.

use serde::{Deserialize, Serialize};

pub use language_pack_api::CardIntent;

/// Card lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CardStatus {
    /// Tutor proposal; not introduced.
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

/// Who caused a card change.
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

/// Deterministic duplicate-detection hash over recall fields.
#[must_use]
pub fn card_content_hash(
    intent: CardIntent,
    prompt: &str,
    answer: &str,
    accepted_answers: &[String],
) -> String {
    card_content_hash_refs(
        intent,
        prompt,
        answer,
        accepted_answers.iter().map(String::as_str),
    )
}

/// Same canonical hash with borrowed accepted-answer text.
#[must_use]
pub fn card_content_hash_refs<'a>(
    intent: CardIntent,
    prompt: &str,
    answer: &str,
    accepted_answers: impl IntoIterator<Item = &'a str>,
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
    eat(match intent {
        CardIntent::Production => b"Production",
        CardIntent::Listening => b"Listening",
        CardIntent::Contrast => b"Contrast",
        CardIntent::Transfer => b"Transfer",
    });
    eat(prompt.trim().to_lowercase().as_bytes());
    eat(answer.trim().to_lowercase().as_bytes());
    let mut sorted: Vec<String> = accepted_answers
        .into_iter()
        .map(|answer| answer.trim().to_lowercase())
        .collect();
    sorted.sort_unstable();
    for accepted in &sorted {
        eat(accepted.as_bytes());
    }
    format!("{hash:016x}")
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn content_hash_golden_vector_preserves_existing_card_identity() {
        assert_eq!(
            card_content_hash(
                CardIntent::Production,
                "hello",
                "hola",
                &["ok".to_owned(), "y".to_owned()]
            ),
            "3948c0e05c8fe581"
        );
    }

    #[test]
    fn content_hash_ignores_case_whitespace_and_accepted_order() {
        let a = card_content_hash(
            CardIntent::Production,
            "Ask for water",
            "¿Me da agua?",
            &["dame agua".to_owned(), "quiero agua".to_owned()],
        );
        let b = card_content_hash(
            CardIntent::Production,
            "  ask for WATER ",
            "¿me da agua? ",
            &["Quiero agua".to_owned(), "Dame agua".to_owned()],
        );
        assert_eq!(a, b);
    }

    #[test]
    fn content_hash_distinguishes_intent_and_answer() {
        let base = card_content_hash(CardIntent::Production, "p", "a", &[]);
        assert_ne!(
            base,
            card_content_hash(CardIntent::Listening, "p", "a", &[])
        );
        assert_ne!(
            base,
            card_content_hash(CardIntent::Production, "p", "b", &[])
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
