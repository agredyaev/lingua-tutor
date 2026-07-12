//! DTOs shared between `apps/web` and `apps/worker`: serde types mirroring
//! the `/api/v1` contract ([08-http-api.md]). No domain logic, no IO.
//!
//! [08-http-api.md]: https://github.com/lingua-tutor/docs/spec/08-http-api.md

use serde::{Deserialize, Serialize};

/// Response body of `GET /api/v1/health`.
///
/// Stub shape for M0; extended with D1 connectivity, schema version,
/// scheduler version, migration and deployment metadata at WBS 10.5.
///
/// # Examples
///
/// ```
/// let health = shared_schema::HealthResponse::ok("0.1.0");
/// assert_eq!(health.status, "ok");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HealthResponse {
    /// `"ok"` when the Worker is serving requests.
    pub status: String,
    /// Deployment version (crate version at build time).
    pub version: String,
}

impl HealthResponse {
    /// Healthy response for the given deployment version.
    #[must_use]
    pub fn ok(version: &str) -> Self {
        Self {
            status: "ok".to_owned(),
            version: version.to_owned(),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)] // tests exempt (16-rust-guidelines.md §2)
mod tests {
    use super::*;

    #[test]
    fn health_response_serializes_to_contract_shape() {
        let health = HealthResponse::ok("0.1.0");
        let json = serde_json::to_value(&health).unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "status": "ok", "version": "0.1.0" })
        );
    }

    #[test]
    fn health_response_round_trips() {
        let health = HealthResponse::ok("0.1.0");
        let text = serde_json::to_string(&health).unwrap();
        let back: HealthResponse = serde_json::from_str(&text).unwrap();
        assert_eq!(back, health);
    }
}
