//! HTTP plumbing shared by handlers: error shape (08-http-api.md §1),
//! D1 access, IDs, clock.

use worker::{D1Database, Env, Response, Result};

/// Standard error body: `{ "error": { "code", "message", "details" } }`.
pub fn error_response(status: u16, code: &str, message: &str) -> Result<Response> {
    error_response_with(status, code, message, serde_json::json!({}))
}

/// Error body with structured details (e.g. validation issues).
pub fn error_response_with(
    status: u16,
    code: &str,
    message: &str,
    details: serde_json::Value,
) -> Result<Response> {
    Response::from_json(&serde_json::json!({
        "error": { "code": code, "message": message, "details": details }
    }))
    .map(|r| r.with_status(status))
}

/// The single D1 binding.
pub fn db(env: &Env) -> Result<D1Database> {
    env.d1("DB")
}

/// Request instant, unix seconds (Workers clock).
#[must_use]
pub fn now() -> learning_core::Timestamp {
    #[allow(clippy::cast_possible_wrap)] // ms since 1970 « i64::MAX
    learning_core::Timestamp((worker::Date::now().as_millis() / 1000) as i64)
}

/// New opaque entity ID: UUID v4 with a short type prefix (03-domain-model.md §1).
#[must_use]
pub fn new_id(prefix: &str) -> String {
    format!("{prefix}_{}", uuid::Uuid::new_v4().simple())
}

/// Required query parameter.
pub fn query_param(url: &worker::Url, name: &str) -> Option<String> {
    url.query_pairs()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.into_owned())
}
