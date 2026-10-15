//! Shared operation errors, IDs, and clock.

use crate::{Error, Result};

/// Return a structured application-operation failure.
pub fn operation_error<T>(status: u16, code: &'static str, message: &str) -> Result<T> {
    operation_error_with(status, code, message, serde_json::json!({}))
}

/// Return a structured application-operation failure with machine-readable details.
pub fn operation_error_with<T>(
    status: u16,
    code: &'static str,
    message: &str,
    details: serde_json::Value,
) -> Result<T> {
    Err(Error::operation(status, code, message, details))
}

/// Request instant, unix seconds.
#[must_use]
pub fn now() -> learning_core::Timestamp {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    learning_core::Timestamp(i64::try_from(seconds).unwrap_or(i64::MAX))
}

/// New opaque entity ID: UUID v4 with a short type prefix.
#[must_use]
pub fn new_id(prefix: &str) -> String {
    format!("{prefix}_{}", uuid::Uuid::new_v4().simple())
}
