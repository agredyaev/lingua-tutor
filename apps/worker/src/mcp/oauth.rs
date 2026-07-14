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

// --- authorize + token (security core, ADR-0006 §2-4) ---

/// Access-token TTL: 1 hour.
const ACCESS_TTL_SECS: i64 = 3600;
/// Refresh-token TTL: 30 days, rotated on every use.
const REFRESH_TTL_SECS: i64 = 30 * 86_400;
/// Authorization-code TTL: 10 minutes.
const CODE_TTL_SECS: i64 = 600;

/// SHA-256 hex digest (hash-at-rest for codes and tokens).
#[must_use]
pub fn sha256_hex(value: &str) -> String {
    use sha2::Digest as _;
    hex::encode(sha2::Sha256::digest(value.as_bytes()))
}

/// New opaque secret: prefix + 2×uuid4 (~244 bits of randomness).
fn new_secret(prefix: &str) -> String {
    format!(
        "{prefix}{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}

/// PKCE S256 check: `BASE64URL-NOPAD(SHA256(verifier)) == challenge`,
/// constant-time on the comparison.
#[must_use]
pub fn pkce_matches(verifier: &str, challenge: &str) -> bool {
    use base64::Engine as _;
    use sha2::Digest as _;
    let computed = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(sha2::Sha256::digest(verifier.as_bytes()));
    let mismatch = computed
        .as_bytes()
        .iter()
        .zip(challenge.as_bytes())
        .fold(computed.len() != challenge.len(), |acc, (a, b)| {
            acc | (a != b)
        });
    !mismatch
}

/// Requested scope string → validated space-joined subset of [`SCOPES`].
/// Empty request means "all scopes" (single-owner convenience).
#[must_use]
pub fn validated_scope(requested: &str) -> Option<String> {
    if requested.trim().is_empty() {
        return Some(SCOPES.join(" "));
    }
    let parts: Vec<&str> = requested.split_whitespace().collect();
    parts
        .iter()
        .all(|p| SCOPES.contains(p))
        .then(|| parts.join(" "))
}

fn form_params(text: &str) -> std::collections::HashMap<String, String> {
    // RFC 6749 token/consent bodies are x-www-form-urlencoded; reuse the URL
    // query parser rather than pulling a form crate.
    worker::Url::parse(&format!("https://x/?{text}"))
        .map(|u| {
            u.query_pairs()
                .map(|(k, v)| (k.into_owned(), v.into_owned()))
                .collect()
        })
        .unwrap_or_default()
}

fn oauth_error(status: u16, code: &str, description: &str) -> Result<Response> {
    Ok(
        Response::from_json(&serde_json::json!({"error": code, "error_description": description}))?
            .with_status(status),
    )
}

struct AuthorizeParams {
    client_id: String,
    redirect_uri: String,
    scope: String,
    state: String,
    code_challenge: String,
}

/// Validate authorize params against the client registry. Errors render as
/// pages (never redirect to an unvalidated URI — RFC 6749 §4.1.2.1).
async fn authorize_params(
    params: &std::collections::HashMap<String, String>,
    env: &Env,
) -> Result<std::result::Result<AuthorizeParams, Response>> {
    let get = |k: &str| params.get(k).cloned().unwrap_or_default();
    let client_id = get("client_id");
    let redirect_uri = get("redirect_uri");
    if get("response_type") != "code" {
        return Ok(Err(error_response(
            400,
            "unsupported_response_type",
            "response_type must be code",
        )?));
    }
    if get("code_challenge_method") != "S256" || get("code_challenge").is_empty() {
        return Ok(Err(error_response(
            400,
            "invalid_request",
            "PKCE S256 required",
        )?));
    }
    let Some(scope) = validated_scope(&get("scope")) else {
        return Ok(Err(error_response(400, "invalid_scope", "unknown scope")?));
    };
    let database = crate::api::db(env)?;
    let client = database
        .prepare(repo::SELECT_CLIENT)
        .bind(&[client_id.clone().into()])?
        .first::<repo::ClientRow>(None)
        .await?;
    let Some(client) = client else {
        return Ok(Err(error_response(
            400,
            "invalid_client",
            "unknown client",
        )?));
    };
    let registered = client
        .redirect_uris()
        .map_err(|e| worker::Error::RustError(e.to_string()))?;
    if !registered.iter().any(|u| u == &redirect_uri) {
        return Ok(Err(error_response(
            400,
            "invalid_redirect_uri",
            "redirect_uri not registered",
        )?));
    }
    Ok(Ok(AuthorizeParams {
        client_id,
        redirect_uri,
        scope,
        state: get("state"),
        code_challenge: get("code_challenge"),
    }))
}

fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// `GET /oauth/authorize` — owner-only consent page (behind Access at the
/// edge; re-verified here).
pub async fn authorize_form(req: Request, env: &Env) -> Result<Response> {
    match crate::auth::verify_owner(&req, env).await? {
        crate::auth::OwnerCheck::Owner => {}
        _ => return error_response(403, "access_denied", "owner login required"),
    }
    let url = req.url()?;
    let params: std::collections::HashMap<String, String> = url
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    let validated = match authorize_params(&params, env).await? {
        Ok(v) => v,
        Err(resp) => return Ok(resp),
    };
    let scope_items: String = validated
        .scope
        .split(' ')
        .map(|s| format!("<li>{}</li>", html_escape(s)))
        .collect();
    let hidden: String = [
        ("client_id", &validated.client_id),
        ("redirect_uri", &validated.redirect_uri),
        ("scope", &validated.scope),
        ("state", &validated.state),
        ("code_challenge", &validated.code_challenge),
        ("code_challenge_method", &"S256".to_owned()),
        ("response_type", &"code".to_owned()),
    ]
    .iter()
    .map(|(k, v)| {
        format!(
            "<input type=\"hidden\" name=\"{k}\" value=\"{}\">",
            html_escape(v)
        )
    })
    .collect();
    let page = format!(
        "<!doctype html><meta charset=\"utf-8\"><title>Authorize MCP client</title>\
         <h1>Authorize \u{201c}{}\u{201d}?</h1><p>Grants:</p><ul>{scope_items}</ul>\
         <form method=\"post\" action=\"/oauth/authorize\">{hidden}\
         <button type=\"submit\">Approve</button></form>",
        html_escape(&validated.client_id)
    );
    Response::from_html(page)
}

/// `POST /oauth/authorize` — issue the code and redirect back.
pub async fn authorize_approve(mut req: Request, env: &Env) -> Result<Response> {
    match crate::auth::verify_owner(&req, env).await? {
        crate::auth::OwnerCheck::Owner => {}
        _ => return error_response(403, "access_denied", "owner login required"),
    }
    let text = req.text().await?;
    if text.len() > 16 * 1024 {
        return error_response(413, "too_large", "body too large");
    }
    let params = form_params(&text);
    let validated = match authorize_params(&params, env).await? {
        Ok(v) => v,
        Err(resp) => return Ok(resp),
    };
    let code = new_secret("mcp_code_");
    let now = crate::api::now();
    let database = crate::api::db(env)?;
    database
        .prepare(repo::INSERT_CODE)
        .bind(&[
            sha256_hex(&code).into(),
            validated.client_id.clone().into(),
            validated.redirect_uri.clone().into(),
            validated.scope.clone().into(),
            validated.code_challenge.clone().into(),
            crate::repositories::to_rfc3339(learning_core::Timestamp(now.0 + CODE_TTL_SECS)).into(),
        ])?
        .run()
        .await?;
    let mut location = worker::Url::parse(&validated.redirect_uri)
        .map_err(|e| worker::Error::RustError(e.to_string()))?;
    location
        .query_pairs_mut()
        .append_pair("code", &code)
        .append_pair("state", &validated.state);
    Response::redirect(location)
}

async fn issue_token_pair(env: &Env, client_id: &str, scope: &str) -> Result<serde_json::Value> {
    let access = new_secret("mcp_at_");
    let refresh = new_secret("mcp_rt_");
    let now = crate::api::now();
    let created = crate::repositories::to_rfc3339(now);
    let database = crate::api::db(env)?;
    for (token, kind, ttl) in [
        (&access, "access", ACCESS_TTL_SECS),
        (&refresh, "refresh", REFRESH_TTL_SECS),
    ] {
        database
            .prepare(repo::INSERT_TOKEN)
            .bind(&[
                sha256_hex(token).into(),
                kind.into(),
                client_id.into(),
                scope.into(),
                crate::repositories::to_rfc3339(learning_core::Timestamp(now.0 + ttl)).into(),
                created.clone().into(),
            ])?
            .run()
            .await?;
    }
    Ok(serde_json::json!({
        "access_token": access,
        "token_type": "Bearer",
        "expires_in": ACCESS_TTL_SECS,
        "refresh_token": refresh,
        "scope": scope,
    }))
}

/// `POST /oauth/token` — authorization_code + refresh_token grants.
pub async fn token(mut req: Request, env: &Env) -> Result<Response> {
    let text = req.text().await?;
    if text.len() > 16 * 1024 {
        return oauth_error(413, "invalid_request", "body too large");
    }
    let params = form_params(&text);
    let get = |k: &str| params.get(k).cloned().unwrap_or_default();
    let database = crate::api::db(env)?;
    let now_ts = crate::repositories::to_rfc3339(crate::api::now());
    // Opportunistic cleanup (ADR-0006): cheap, keeps tables bounded.
    database
        .prepare(repo::PURGE_EXPIRED_TOKENS)
        .bind(&[now_ts.clone().into()])?
        .run()
        .await?;
    database
        .prepare(repo::PURGE_EXPIRED_CODES)
        .bind(&[now_ts.clone().into()])?
        .run()
        .await?;

    match get("grant_type").as_str() {
        "authorization_code" => {
            #[derive(serde::Deserialize)]
            struct CodeRow {
                client_id: String,
                redirect_uri: String,
                scope: String,
                code_challenge: String,
            }
            let consumed = database
                .prepare(repo::CONSUME_CODE)
                .bind(&[sha256_hex(&get("code")).into(), now_ts.into()])?
                .first::<CodeRow>(None)
                .await?;
            let Some(row) = consumed else {
                return oauth_error(400, "invalid_grant", "code invalid, expired, or used");
            };
            if row.client_id != get("client_id") || row.redirect_uri != get("redirect_uri") {
                return oauth_error(400, "invalid_grant", "client/redirect mismatch");
            }
            if !pkce_matches(&get("code_verifier"), &row.code_challenge) {
                return oauth_error(400, "invalid_grant", "PKCE verification failed");
            }
            Response::from_json(&issue_token_pair(env, &row.client_id, &row.scope).await?)
        }
        "refresh_token" => {
            #[derive(serde::Deserialize)]
            struct TokenRow {
                scope: String,
                client_id: String,
            }
            let hash = sha256_hex(&get("refresh_token"));
            let found = database
                .prepare(repo::SELECT_TOKEN)
                .bind(&[
                    hash.clone().into(),
                    "refresh".into(),
                    crate::repositories::to_rfc3339(crate::api::now()).into(),
                ])?
                .first::<TokenRow>(None)
                .await?;
            let Some(row) = found else {
                return oauth_error(400, "invalid_grant", "refresh token invalid");
            };
            // Rotation: old refresh token dies with this use.
            database
                .prepare(repo::REVOKE_TOKEN)
                .bind(&[hash.into()])?
                .run()
                .await?;
            Response::from_json(&issue_token_pair(env, &row.client_id, &row.scope).await?)
        }
        other => oauth_error(
            400,
            "unsupported_grant_type",
            &format!("unsupported grant_type '{other}'"),
        ),
    }
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

    #[test]
    fn pkce_s256_round_trip() {
        // RFC 7636 appendix B vector.
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let challenge = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";
        assert!(pkce_matches(verifier, challenge));
        assert!(!pkce_matches("wrong-verifier", challenge));
        assert!(!pkce_matches(
            verifier,
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM-longer"
        ));
    }

    #[test]
    fn secrets_are_prefixed_hashed_and_high_entropy() {
        let token = new_secret("mcp_at_");
        assert!(token.starts_with("mcp_at_"));
        assert_eq!(token.len(), "mcp_at_".len() + 64);
        assert_ne!(new_secret("mcp_at_"), token, "secrets must not repeat");
        assert_eq!(sha256_hex("abc").len(), 64);
        assert_ne!(sha256_hex(&token), token, "storage sees only the hash");
    }

    #[test]
    fn scope_validation_subsets_only() {
        assert_eq!(validated_scope(""), Some(SCOPES.join(" ")));
        assert_eq!(
            validated_scope("learning.read lessons.write"),
            Some("learning.read lessons.write".to_owned())
        );
        assert_eq!(validated_scope("admin"), None);
        assert_eq!(validated_scope("learning.read admin"), None);
    }

    #[test]
    fn form_params_parses_urlencoded() {
        let p = form_params("grant_type=authorization_code&code=abc%20def&state=");
        assert_eq!(p["grant_type"], "authorization_code");
        assert_eq!(p["code"], "abc def");
        assert_eq!(p["state"], "");
    }

    #[test]
    fn html_escape_neutralizes_markup() {
        assert_eq!(html_escape("<script>\"&'"), "&lt;script&gt;&quot;&amp;'");
        // value lands inside a double-quoted attribute: quotes must be dead.
        assert!(!html_escape("\" onmouseover=\"x").contains('"'));
    }
}
