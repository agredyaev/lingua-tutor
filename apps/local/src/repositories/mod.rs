//! SQLite repositories: the only place SQL lives.
//! Row structs mirror database columns; conversion to domain types happens here.

pub mod cards;
pub mod lessons;
pub mod packs;
pub mod profiles;
pub mod recommendations;
pub mod records;
pub mod review;

use learning_core::Timestamp;

/// Repository failure: constraint violation, mapping error, or SQLite error.
#[derive(Debug, thiserror::Error)]
pub enum RepoError {
    /// Row exists but violates expected shape.
    #[error("corrupt row: {0}")]
    CorruptRow(String),
    /// Underlying database error.
    #[error("database: {0}")]
    Database(String),
}

impl From<crate::Error> for RepoError {
    fn from(e: crate::Error) -> Self {
        Self::Database(e.to_string())
    }
}

impl From<rusqlite::Error> for RepoError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Database(e.to_string())
    }
}

// Explicit, statically dispatched SQLite TEXT passes; no trait implementation,
// dynamic dispatch, or intermediate allocated text values.
macro_rules! db_enum_passes {
    ($type:path, $parse:ident, $format:ident, { $($variant:ident => $text:literal),+ $(,)? }) => {
        /// Decode one closed SQLite TEXT value; invalid values are rejected.
        #[must_use]
        pub fn $parse(value: &str) -> Option<$type> {
            use $type as Enum;
            match value {
                $($text => Some(Enum::$variant),)+
                _ => None,
            }
        }

        /// Encode a closed enum directly into its stable SQLite TEXT spelling.
        #[must_use]
        pub fn $format(value: &$type) -> &'static str {
            use $type as Enum;
            match value {
                $(Enum::$variant => $text,)+
            }
        }
    };
}

db_enum_passes!(learning_core::ProfileStatus, profile_status_from_db, profile_status_db, {
    Active => "active",
    Paused => "paused",
    Archived => "archived",
});
db_enum_passes!(learning_core::LessonStatus, lesson_status_from_db, lesson_status_db, {
    Planned => "planned",
    Active => "active",
    Completed => "completed",
    Abandoned => "abandoned",
});
db_enum_passes!(learning_core::LessonSource, lesson_source_from_db, lesson_source_db, {
    Pwa => "pwa",
    Import => "import",
    Mcp => "mcp",
});
db_enum_passes!(learning_core::LearningItemType, learning_item_type_from_db, learning_item_type_db, {
    Phrase => "phrase",
    Collocation => "collocation",
    Pattern => "pattern",
    Word => "word",
    ResponseFrame => "response_frame",
});
db_enum_passes!(learning_core::LearningItemStatus, learning_item_status_from_db, learning_item_status_db, {
    Candidate => "candidate",
    Introduced => "introduced",
    Learning => "learning",
    TransferPending => "transfer_pending",
    ProvisionallyMastered => "provisionally_mastered",
    Mastered => "mastered",
    Suspended => "suspended",
});
db_enum_passes!(learning_core::EvidenceKind, evidence_kind_from_db, evidence_kind_db, {
    Recognition => "recognition",
    Listening => "listening",
    PromptedProduction => "prompted_production",
    SpontaneousProduction => "spontaneous_production",
    Transfer => "transfer",
    Correction => "correction",
});
db_enum_passes!(learning_core::ErrorTargetStatus, error_target_status_from_db, error_target_status_db, {
    Active => "active",
    Recheck => "recheck",
    Resolved => "resolved",
    Escalated => "escalated",
});
db_enum_passes!(learning_core::CardStatus, card_status_from_db, card_status_db, {
    Candidate => "candidate",
    New => "new",
    Learning => "learning",
    Review => "review",
    Relearning => "relearning",
    Suspended => "suspended",
    Retired => "retired",
});
db_enum_passes!(learning_core::CardActor, card_actor_from_db, card_actor_db, {
    Tutor => "tutor",
    User => "user",
    System => "system",
});
db_enum_passes!(language_pack_api::CoreErrorCategory, core_error_category_from_db, core_error_category_db, {
    Grammar => "grammar",
    Vocabulary => "vocabulary",
    WordOrder => "word_order",
    Register => "register",
    Pronunciation => "pronunciation",
    Orthography => "orthography",
    Other => "other",
});
db_enum_passes!(language_pack_api::CardIntent, card_intent_from_db, card_intent_db, {
    Production => "production",
    Listening => "listening",
    Contrast => "contrast",
    Transfer => "transfer",
});
db_enum_passes!(tutor_contract::MutationOperation, mutation_operation_from_db, mutation_operation_db, {
    Create => "create",
    Update => "update",
    Split => "split",
    Merge => "merge",
    Suspend => "suspend",
    Restore => "restore",
    Retire => "retire",
    AddAcceptedAnswer => "add_accepted_answer",
    RemoveAcceptedAnswer => "remove_accepted_answer",
    AddListeningVariant => "add_listening_variant",
    Link => "link",
});

pub(crate) fn communicative_impact_db(value: &learning_core::CommunicativeImpact) -> &'static str {
    match value {
        learning_core::CommunicativeImpact::Low => "low",
        learning_core::CommunicativeImpact::Medium => "medium",
        learning_core::CommunicativeImpact::High => "high",
    }
}

/// Unix seconds → RFC 3339 TEXT.
#[must_use]
pub fn to_rfc3339(t: Timestamp) -> String {
    chrono::DateTime::from_timestamp(t.0, 0)
        .unwrap_or_default()
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

/// RFC 3339 TEXT → unix seconds.
pub fn from_rfc3339(text: &str) -> Result<Timestamp, RepoError> {
    chrono::DateTime::parse_from_rfc3339(text)
        .map(|dt| Timestamp(dt.timestamp()))
        .map_err(|e| RepoError::CorruptRow(format!("timestamp '{text}': {e}")))
}

#[cfg(test)]
// Test fixtures intentionally fail fast and use compact tuple snapshots.
#[allow(clippy::unwrap_used, clippy::type_complexity)]
mod enum_contract_tests {
    use super::*;

    #[test]
    fn mutation_operations_preserve_all_database_spellings() {
        use tutor_contract::MutationOperation as M;
        for (value, text) in [
            (M::Create, "create"),
            (M::Update, "update"),
            (M::Split, "split"),
            (M::Merge, "merge"),
            (M::Suspend, "suspend"),
            (M::Restore, "restore"),
            (M::Retire, "retire"),
            (M::AddAcceptedAnswer, "add_accepted_answer"),
            (M::RemoveAcceptedAnswer, "remove_accepted_answer"),
            (M::AddListeningVariant, "add_listening_variant"),
            (M::Link, "link"),
        ] {
            assert_eq!(mutation_operation_db(&value), text);
            assert_eq!(mutation_operation_from_db(text), Some(value));
        }
        assert_eq!(mutation_operation_from_db("invalid-enum-variant"), None);
    }
}
