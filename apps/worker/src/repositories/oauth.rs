//! OAuth 2.1 storage for the MCP surface (WBS 12.1, ADR-0006).
//!
//! Codes and tokens are stored as SHA-256 hashes only; consume/lookup
//! statements push expiry, single-use and revocation checks into SQL so
//! callers cannot forget them.

use super::RepoError;
use serde::Deserialize;

/// Register a dynamic client (RFC 7591 subset).
pub const INSERT_CLIENT: &str = "INSERT INTO oauth_clients
    (client_id, client_name, redirect_uris_json, created_at)
 VALUES (?1, ?2, ?3, ?4)";

/// Client by ID.
pub const SELECT_CLIENT: &str = "SELECT client_id, client_name, redirect_uris_json
 FROM oauth_clients WHERE client_id = ?1";

/// Registered-client count, for the DCR cap (ADR-0006: max 2 — CI needs
/// none here, this is only the ChatGPT connector slot plus one spare for
/// re-registration during rotation).
pub const COUNT_CLIENTS: &str = "SELECT COUNT(*) AS n FROM oauth_clients";

/// Store an authorization code (hashed), bound to client + redirect + PKCE.
pub const INSERT_CODE: &str = "INSERT INTO oauth_codes
    (code_hash, client_id, redirect_uri, scope, code_challenge, expires_at)
 VALUES (?1, ?2, ?3, ?4, ?5, ?6)";

/// Atomically consume an unexpired, unused code. Returns the bound fields;
/// no row means invalid, expired, or already used (single statement — no
/// check-then-act race).
pub const CONSUME_CODE: &str = "UPDATE oauth_codes SET used = 1
 WHERE code_hash = ?1 AND used = 0 AND expires_at > ?2
 RETURNING client_id, redirect_uri, scope, code_challenge";

/// Store a token hash (kind: access | refresh).
pub const INSERT_TOKEN: &str = "INSERT INTO oauth_tokens
    (token_hash, kind, client_id, scope, expires_at, created_at)
 VALUES (?1, ?2, ?3, ?4, ?5, ?6)";

/// Live-token lookup: expiry and revocation filtered in SQL.
pub const SELECT_TOKEN: &str = "SELECT scope, client_id
 FROM oauth_tokens
 WHERE token_hash = ?1 AND kind = ?2 AND revoked = 0 AND expires_at > ?3";

/// Revoke one token (refresh rotation, logout, kill-switch).
pub const REVOKE_TOKEN: &str = "UPDATE oauth_tokens SET revoked = 1 WHERE token_hash = ?1";

/// Opportunistic cleanup, run from the token endpoint.
pub const PURGE_EXPIRED_TOKENS: &str = "DELETE FROM oauth_tokens WHERE expires_at <= ?1";
/// Opportunistic cleanup, run from the token endpoint.
pub const PURGE_EXPIRED_CODES: &str = "DELETE FROM oauth_codes WHERE expires_at <= ?1";

/// `oauth_clients` row.
#[derive(Debug, Deserialize)]
pub struct ClientRow {
    /// Client identifier issued at registration.
    pub client_id: String,
    /// Human-readable name from registration.
    pub client_name: String,
    /// JSON array of registered redirect URIs.
    pub redirect_uris_json: String,
}

impl ClientRow {
    /// Parse the stored redirect-URI list.
    ///
    /// # Errors
    /// `RepoError::CorruptRow` when the stored JSON is not a string array.
    pub fn redirect_uris(&self) -> Result<Vec<String>, RepoError> {
        serde_json::from_str(&self.redirect_uris_json)
            .map_err(|e| RepoError::CorruptRow(format!("oauth_clients.redirect_uris_json: {e}")))
    }
}
