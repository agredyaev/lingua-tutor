//! Embedded OAuth 2.1 authorization server (ADR-0006): discovery metadata,
//! dynamic client registration, authorize + token endpoints.

use crate::api::error_response;
use crate::repositories::oauth as repo;
use worker::{Env, Request, Response, Result};

/// The six MCP scopes (13-roadmap.md §3).
pub const SCOPES: [&str; 6] = [
    "language_packs.read",
    "learning.read",
    "lessons.write",
    "cards.write",
    "errors.write",
    "exports.read",
];

/// Registered-redirect host allowlist: ChatGPT connector callbacks only
/// (10-security.md §2 — narrow surface; widen deliberately, never wildcard).
const REDIRECT_HOST_SUFFIXES: [&str; 2] = ["chatgpt.com", "openai.com"];

/// `GET /.well-known/oauth-protected-resource` body.
#[must_use]
pub fn protected_resource_metadata(origin: &str) -> serde_json::Value {
    serde_json::json!({
        "resource": origin,
        "authorization_servers": [origin],
        "scopes_supported": SCOPES,
        "bearer_methods_supported": ["header"],
    })
}

/// `GET /.well-known/oauth-authorization-server` body.
#[must_use]
pub fn auth_server_metadata(origin: &str) -> serde_json::Value {
    serde_json::json!({
        "issuer": origin,
        "authorization_endpoint": format!("{origin}/oauth/authorize"),
        "token_endpoint": format!("{origin}/oauth/token"),
        "registration_endpoint": format!("{origin}/oauth/register"),
        "response_types_supported": ["code"],
        "grant_types_supported": ["authorization_code", "refresh_token"],
        "code_challenge_methods_supported": ["S256"],
        "token_endpoint_auth_methods_supported": ["none"],
        "scopes_supported": SCOPES,
    })
}

/// Validate one redirect URI: https + allowlisted host, or plain-http
/// localhost when `allow_localhost` (local dev smoke only).
#[must_use]
pub fn redirect_uri_allowed(uri: &str, allow_localhost: bool) -> bool {
    let Ok(parsed) = worker::Url::parse(uri) else {
        return false;
    };
    let Some(host) = parsed.host_str() else {
        return false;
    };
    if allow_localhost && parsed.scheme() == "http" && (host == "localhost" || host == "127.0.0.1")
    {
        return true;
    }
    parsed.scheme() == "https"
        && REDIRECT_HOST_SUFFIXES
            .iter()
            .any(|s| host == *s || host.ends_with(&format!(".{s}")))
}

/// Origin (`scheme://host[:port]`) of the incoming request.
fn request_origin(req: &Request) -> Result<String> {
    let url = req.url()?;
    Ok(url.origin().ascii_serialization())
}

/// 401 with the `WWW-Authenticate` pointer clients use to discover the AS.
pub fn unauthorized(origin: &str) -> Result<Response> {
    let headers = worker::Headers::new();
    headers.set(
        "WWW-Authenticate",
        &format!("Bearer resource_metadata=\"{origin}/.well-known/oauth-protected-resource\""),
    )?;
    Ok(
        Response::from_json(&serde_json::json!({"error": "unauthorized"}))?
            .with_status(401)
            .with_headers(headers),
    )
}

/// `GET /.well-known/oauth-protected-resource`.
pub fn serve_protected_resource_metadata(req: &Request) -> Result<Response> {
    Response::from_json(&protected_resource_metadata(&request_origin(req)?))
}

/// `GET /.well-known/oauth-authorization-server`.
pub fn serve_auth_server_metadata(req: &Request) -> Result<Response> {
    Response::from_json(&auth_server_metadata(&request_origin(req)?))
}

/// RFC 7591 registration request (subset we accept).
#[derive(serde::Deserialize)]
struct RegistrationRequest {
    #[serde(default)]
    client_name: Option<String>,
    redirect_uris: Vec<String>,
}

/// `POST /oauth/register` — dynamic client registration (public client,
/// PKCE-only; no secret issued).
pub async fn register(mut req: Request, env: &Env) -> Result<Response> {
    let text = req.text().await?;
    if text.len() > 16 * 1024 {
        return error_response(413, "too_large", "registration body too large");
    }
    let Ok(body) = serde_json::from_str::<RegistrationRequest>(&text) else {
        return error_response(400, "invalid_client_metadata", "malformed registration");
    };
    if body.redirect_uris.is_empty() || body.redirect_uris.len() > 5 {
        return error_response(400, "invalid_redirect_uri", "1-5 redirect_uris required");
    }
    let allow_localhost = !crate::auth::enforced(env);
    if !body
        .redirect_uris
        .iter()
        .all(|u| redirect_uri_allowed(u, allow_localhost))
    {
        return error_response(400, "invalid_redirect_uri", "redirect host not allowed");
    }
    let client_id = uuid::Uuid::new_v4().to_string();
    let client_name = body.client_name.unwrap_or_else(|| "unnamed".to_owned());
    let client_name: String = client_name.chars().take(120).collect();
    let uris_json = serde_json::to_string(&body.redirect_uris)
        .map_err(|e| worker::Error::RustError(e.to_string()))?;
    let database = crate::api::db(env)?;
    database
        .prepare(repo::INSERT_CLIENT)
        .bind(&[
            client_id.clone().into(),
            client_name.clone().into(),
            uris_json.into(),
            crate::repositories::to_rfc3339(crate::api::now()).into(),
        ])?
        .run()
        .await?;
    Ok(Response::from_json(&serde_json::json!({
        "client_id": client_id,
        "client_name": client_name,
        "redirect_uris": body.redirect_uris,
        "token_endpoint_auth_method": "none",
        "grant_types": ["authorization_code", "refresh_token"],
        "response_types": ["code"],
    }))?
    .with_status(201))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn metadata_carries_endpoints_and_scopes() {
        let m = auth_server_metadata("https://example.com");
        assert_eq!(m["issuer"], "https://example.com");
        assert_eq!(
            m["authorization_endpoint"],
            "https://example.com/oauth/authorize"
        );
        assert_eq!(m["token_endpoint"], "https://example.com/oauth/token");
        assert_eq!(
            m["registration_endpoint"],
            "https://example.com/oauth/register"
        );
        assert_eq!(m["code_challenge_methods_supported"][0], "S256");
        assert_eq!(m["scopes_supported"].as_array().unwrap().len(), 6);
        let p = protected_resource_metadata("https://example.com");
        assert_eq!(p["authorization_servers"][0], "https://example.com");
    }

    #[test]
    fn redirect_validator_allowlist() {
        // Allowed: ChatGPT/OpenAI over https.
        for uri in [
            "https://chatgpt.com/connector_platform_oauth_redirect",
            "https://chat.openai.com/aip/callback",
        ] {
            assert!(redirect_uri_allowed(uri, false), "{uri}");
        }
        // Rejected: http, foreign hosts, suffix tricks, garbage.
        for uri in [
            "http://chatgpt.com/cb",
            "https://evil.com/cb",
            "https://notchatgpt.com/cb",
            "https://chatgpt.com.evil.com/cb",
            "not a url",
            "https://localhost/cb",
        ] {
            assert!(!redirect_uri_allowed(uri, false), "{uri}");
        }
        // Local dev only: plain-http loopback.
        assert!(redirect_uri_allowed("http://localhost:8788/cb", true));
        assert!(redirect_uri_allowed("http://127.0.0.1:9/cb", true));
        assert!(!redirect_uri_allowed("http://localhost:8788/cb", false));
    }
}
