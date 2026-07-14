//! Cloudflare Access JWT verification (10-security.md; WBS 10.2).
//! Defense in depth: Access already gates the hostname; the Worker still
//! verifies `Cf-Access-Jwt-Assertion` — RS256 signature against the team's
//! JWKS, `aud`, expiry, and the owner email claim.
//!
//! Config via Wrangler vars: `AUTH_ENFORCED` ("true"/"false"),
//! `ACCESS_TEAM_DOMAIN` (e.g. `myteam.cloudflareaccess.com`),
//! `ACCESS_AUD`, `OWNER_EMAIL`. Local dev runs with enforcement off
//! (ADR-0003); production sets all four.

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use rsa::pkcs1v15::{Signature, VerifyingKey};
use rsa::sha2::Sha256;
use rsa::signature::Verifier as _;
use rsa::{BigUint, RsaPublicKey};
use worker::{Env, Request, Response, Result};

/// Verification outcome; `Deny` carries the ready 401/403 response.
pub enum AuthOutcome {
    /// Request may proceed.
    Allow,
    /// Request must stop with this response.
    Deny(Response),
}

/// Enforces Access when `AUTH_ENFORCED=true`; otherwise passes (local dev).
///
/// # Errors
///
/// Propagates Worker/JWKS fetch failures as 500s at the call site.
pub async fn enforce(req: &Request, env: &Env) -> Result<AuthOutcome> {
    if !enforced(env) {
        return Ok(AuthOutcome::Allow);
    }
    let Some(token) = req.headers().get("Cf-Access-Jwt-Assertion")? else {
        return deny(401, "missing Access token");
    };
    let team = env.var("ACCESS_TEAM_DOMAIN")?.to_string();
    let aud = env.var("ACCESS_AUD")?.to_string();
    let owner = env.var("OWNER_EMAIL")?.to_string();

    let claims = match verify(&token, &team, &aud).await {
        Ok(claims) => claims,
        Err(reason) => return deny(401, &reason),
    };
    if claims.email.as_deref() != Some(owner.as_str()) {
        return deny(403, "not the owner");
    }
    Ok(AuthOutcome::Allow)
}

/// True when Access enforcement is on (`AUTH_ENFORCED=true`); off = local dev
/// (ADR-0003). Shared with the MCP OAuth surface (ADR-0006).
#[must_use]
pub fn enforced(env: &Env) -> bool {
    env.var("AUTH_ENFORCED")
        .map(|v| v.to_string() == "true")
        .unwrap_or(false)
}

fn deny(status: u16, message: &str) -> Result<AuthOutcome> {
    crate::api::error_response(status, "access_denied", message).map(AuthOutcome::Deny)
}

#[derive(serde::Deserialize)]
struct Claims {
    aud: serde_json::Value,
    exp: i64,
    email: Option<String>,
}

#[derive(serde::Deserialize)]
struct Jwks {
    keys: Vec<Jwk>,
}

#[derive(serde::Deserialize)]
struct Jwk {
    kid: Option<String>,
    n: String,
    e: String,
}

/// RS256 verification: JWKS fetch (per-request; Workers cache the response
/// at the edge via Cache-Control on the certs endpoint), kid match,
/// signature, aud, exp.
async fn verify(
    token: &str,
    team: &str,
    expected_aud: &str,
) -> std::result::Result<Claims, String> {
    let mut parts = token.split('.');
    let (Some(header_b64), Some(payload_b64), Some(signature_b64), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err("malformed JWT".to_owned());
    };

    #[derive(serde::Deserialize)]
    struct Header {
        kid: Option<String>,
        alg: String,
    }
    let header: Header = decode_json(header_b64)?;
    if header.alg != "RS256" {
        return Err("unexpected JWT alg".to_owned());
    }

    let jwks = fetch_jwks(team).await?;
    let key = jwks
        .keys
        .iter()
        .find(|k| header.kid.is_none() || k.kid == header.kid)
        .ok_or_else(|| "no matching JWKS key".to_owned())?;
    let public_key = RsaPublicKey::new(
        BigUint::from_bytes_be(&decode_b64(&key.n)?),
        BigUint::from_bytes_be(&decode_b64(&key.e)?),
    )
    .map_err(|e| format!("bad JWKS key: {e}"))?;

    let signature = Signature::try_from(decode_b64(signature_b64)?.as_slice())
        .map_err(|e| format!("bad signature encoding: {e}"))?;
    let signed = format!("{header_b64}.{payload_b64}");
    VerifyingKey::<Sha256>::new(public_key)
        .verify(signed.as_bytes(), &signature)
        .map_err(|_| "signature verification failed".to_owned())?;

    let claims: Claims = decode_json(payload_b64)?;
    let now = crate::api::now().0;
    if claims.exp < now {
        return Err("token expired".to_owned());
    }
    let aud_ok = match &claims.aud {
        serde_json::Value::String(aud) => aud == expected_aud,
        serde_json::Value::Array(list) => list.iter().any(|a| a == expected_aud),
        _ => false,
    };
    if !aud_ok {
        return Err("aud mismatch".to_owned());
    }
    Ok(claims)
}

async fn fetch_jwks(team: &str) -> std::result::Result<Jwks, String> {
    let url = format!("https://{team}/cdn-cgi/access/certs");
    let mut response = worker::Fetch::Url(url.parse().map_err(|_| "bad team domain".to_owned())?)
        .send()
        .await
        .map_err(|e| format!("JWKS fetch: {e}"))?;
    response
        .json::<Jwks>()
        .await
        .map_err(|e| format!("JWKS parse: {e}"))
}

fn decode_b64(text: &str) -> std::result::Result<Vec<u8>, String> {
    URL_SAFE_NO_PAD
        .decode(text)
        .map_err(|e| format!("base64: {e}"))
}

fn decode_json<T: serde::de::DeserializeOwned>(b64: &str) -> std::result::Result<T, String> {
    serde_json::from_slice(&decode_b64(b64)?).map_err(|e| format!("json: {e}"))
}
