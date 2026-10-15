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

/// Exact SQLite TEXT encoding for closed enums stored in schema columns.
pub trait DbText: Sized {
    /// Stable database representation.
    fn as_db_text(&self) -> &'static str;
    /// Parse the stable database representation.
    fn from_db_text(value: &str) -> Option<Self>;
}

macro_rules! impl_db_text {
    ($ty:path, { $($variant:ident => $text:literal),+ $(,)? }) => {
        impl DbText for $ty {
            fn as_db_text(&self) -> &'static str {
                match self {
                    $(Self::$variant => $text,)+
                }
            }

            fn from_db_text(value: &str) -> Option<Self> {
                match value {
                    $($text => Some(Self::$variant),)+
                    _ => None,
                }
            }
        }
    };
}

impl_db_text!(learning_core::ProfileStatus, {
    Active => "active",
    Paused => "paused",
    Archived => "archived",
});
impl_db_text!(learning_core::LessonStatus, {
    Planned => "planned",
    Active => "active",
    Completed => "completed",
    Abandoned => "abandoned",
});
impl_db_text!(learning_core::LessonSource, {
    Pwa => "pwa",
    Import => "import",
    Mcp => "mcp",
});
impl_db_text!(learning_core::LearningItemType, {
    Phrase => "phrase",
    Collocation => "collocation",
    Pattern => "pattern",
    Word => "word",
    ResponseFrame => "response_frame",
});
impl_db_text!(learning_core::LearningItemStatus, {
    Candidate => "candidate",
    Introduced => "introduced",
    Learning => "learning",
    TransferPending => "transfer_pending",
    ProvisionallyMastered => "provisionally_mastered",
    Mastered => "mastered",
    Suspended => "suspended",
});
impl_db_text!(learning_core::EvidenceKind, {
    Recognition => "recognition",
    Listening => "listening",
    PromptedProduction => "prompted_production",
    SpontaneousProduction => "spontaneous_production",
    Transfer => "transfer",
    Correction => "correction",
});
impl_db_text!(learning_core::ErrorTargetStatus, {
    Active => "active",
    Recheck => "recheck",
    Resolved => "resolved",
    Escalated => "escalated",
});
impl_db_text!(learning_core::CardStatus, {
    Candidate => "candidate",
    New => "new",
    Learning => "learning",
    Review => "review",
    Relearning => "relearning",
    Suspended => "suspended",
    Retired => "retired",
});
impl_db_text!(learning_core::CardActor, {
    Tutor => "tutor",
    User => "user",
    System => "system",
});
impl_db_text!(language_pack_api::CoreErrorCategory, {
    Grammar => "grammar",
    Vocabulary => "vocabulary",
    WordOrder => "word_order",
    Register => "register",
    Pronunciation => "pronunciation",
    Orthography => "orthography",
    Other => "other",
});
impl_db_text!(language_pack_api::CardIntent, {
    Production => "production",
    Listening => "listening",
    Contrast => "contrast",
    Transfer => "transfer",
});
impl_db_text!(tutor_contract::MutationOperation, {
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

/// Parses a closed enum stored as SQLite TEXT.
pub fn parse_enum<T: DbText>(column: &str, value: &str) -> Result<T, RepoError> {
    T::from_db_text(value)
        .ok_or_else(|| RepoError::CorruptRow(format!("{column}: invalid value '{value}'")))
}

/// Returns the stable SQLite TEXT encoding for a closed enum.
#[must_use]
pub fn enum_str<T: DbText>(value: &T) -> &'static str {
    value.as_db_text()
}
