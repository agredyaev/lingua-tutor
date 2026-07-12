//! D1 repositories: the only place SQL lives (16-rust-guidelines.md §4).
//! Row structs mirror 04-database.md columns exactly; conversion to domain
//! types happens here, including RFC 3339 ⇄ unix-seconds timestamps.
//! SQL constants are exercised against scratch SQLite in `tests/`.

pub mod cards;
pub mod lessons;
pub mod packs;
pub mod profiles;
pub mod recommendations;
pub mod records;
pub mod review;

use learning_core::Timestamp;

/// Repository failure: constraint violation, mapping error, or D1 error.
#[derive(Debug, thiserror::Error)]
pub enum RepoError {
    /// Row exists but violates expected shape (bad enum value, bad timestamp).
    #[error("corrupt row: {0}")]
    CorruptRow(String),
    /// Underlying database error.
    #[error("database: {0}")]
    Database(String),
}

impl From<worker::Error> for RepoError {
    fn from(e: worker::Error) -> Self {
        Self::Database(e.to_string())
    }
}

/// Unix seconds → RFC 3339 TEXT (04-database.md §1).
#[must_use]
pub fn to_rfc3339(t: Timestamp) -> String {
    chrono::DateTime::from_timestamp(t.0, 0)
        .unwrap_or_default()
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

/// RFC 3339 TEXT → unix seconds.
///
/// # Errors
///
/// [`RepoError::CorruptRow`] when the stored text is not RFC 3339.
pub fn from_rfc3339(text: &str) -> Result<Timestamp, RepoError> {
    chrono::DateTime::parse_from_rfc3339(text)
        .map(|dt| Timestamp(dt.timestamp()))
        .map_err(|e| RepoError::CorruptRow(format!("timestamp '{text}': {e}")))
}

/// Parses a serde snake_case enum stored as TEXT.
///
/// # Errors
///
/// [`RepoError::CorruptRow`] when the value is outside the closed set.
pub fn parse_enum<T: serde::de::DeserializeOwned>(
    column: &str,
    value: &str,
) -> Result<T, RepoError> {
    serde_json::from_value(serde_json::Value::String(value.to_owned()))
        .map_err(|_| RepoError::CorruptRow(format!("{column}: invalid value '{value}'")))
}

/// Serializes a serde snake_case enum to its TEXT form.
#[must_use]
pub fn enum_str<T: serde::Serialize>(value: &T) -> String {
    match serde_json::to_value(value) {
        Ok(serde_json::Value::String(s)) => s,
        _ => String::new(), // unreachable for unit enums; empty fails CHECK loudly
    }
}
