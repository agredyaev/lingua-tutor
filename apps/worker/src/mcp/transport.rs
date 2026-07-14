//! Streamable HTTP transport for MCP: JSON-RPC 2.0 over `POST /mcp`
//! (ADR-0006 §1). Stateless — every request carries its own bearer token.

use super::{oauth, tools};
use worker::{Env, Request, Response, Result};

/// MCP protocol revision we implement.
const PROTOCOL_VERSION: &str = "2025-06-18";
/// Same body bound as the import path (10-security.md input bounds).
const MAX_BODY_BYTES: usize = 256 * 1024;

/// Parsed JSON-RPC request envelope.
pub struct RpcRequest {
    /// `id` — absent for notifications.
    pub id: Option<serde_json::Value>,
    /// Method name (`initialize`, `tools/list`, `tools/call`, ...).
    pub method: String,
    /// Method params object (empty object when absent).
    pub params: serde_json::Value,
}

/// Parse a JSON-RPC 2.0 request body.
///
/// # Errors
/// Human-readable reason when the envelope is not JSON-RPC 2.0.
pub fn parse_rpc(body: &str) -> std::result::Result<RpcRequest, String> {
    let value: serde_json::Value =
        serde_json::from_str(body).map_err(|e| format!("parse error: {e}"))?;
    if value.get("jsonrpc").and_then(serde_json::Value::as_str) != Some("2.0") {
        return Err("jsonrpc must be \"2.0\"".to_owned());
    }
    let Some(method) = value.get("method").and_then(serde_json::Value::as_str) else {
        return Err("method required".to_owned());
    };
    Ok(RpcRequest {
        id: value.get("id").cloned(),
        method: method.to_owned(),
        params: value
            .get("params")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({})),
    })
}

fn rpc_result(id: &Option<serde_json::Value>, result: serde_json::Value) -> Result<Response> {
    Response::from_json(&serde_json::json!({
        "jsonrpc": "2.0",
        "id": id.clone().unwrap_or(serde_json::Value::Null),
        "result": result,
    }))
}

fn rpc_error(id: &Option<serde_json::Value>, code: i64, message: &str) -> Result<Response> {
    Response::from_json(&serde_json::json!({
        "jsonrpc": "2.0",
        "id": id.clone().unwrap_or(serde_json::Value::Null),
        "error": { "code": code, "message": message },
    }))
}

/// Scopes granted to the presented bearer token.
pub struct Grant {
    /// Space-separated scopes from the token row.
    pub scopes: Vec<String>,
}

impl Grant {
    /// True when the grant covers `scope`.
    #[must_use]
    pub fn allows(&self, scope: &str) -> bool {
        self.scopes.iter().any(|s| s == scope)
    }
}

/// Resolve the bearer token to its grant; `None` = respond 401.
async fn bearer_grant(req: &Request, env: &Env) -> Result<Option<Grant>> {
    let Some(header) = req.headers().get("Authorization")? else {
        return Ok(None);
    };
    let Some(token) = header.strip_prefix("Bearer ") else {
        return Ok(None);
    };
    #[derive(serde::Deserialize)]
    struct TokenRow {
        scope: String,
    }
    let database = crate::api::db(env)?;
    let found = database
        .prepare(crate::repositories::oauth::SELECT_TOKEN)
        .bind(&[
            oauth::sha256_hex(token).into(),
            "access".into(),
            crate::repositories::to_rfc3339(crate::api::now()).into(),
        ])?
        .first::<TokenRow>(None)
        .await?;
    Ok(found.map(|row| Grant {
        scopes: row.scope.split(' ').map(str::to_owned).collect(),
    }))
}

/// Handle `POST /mcp`.
pub async fn handle(mut req: Request, env: &Env) -> Result<Response> {
    let origin = {
        let url = req.url()?;
        url.origin().ascii_serialization()
    };
    let Some(grant) = bearer_grant(&req, env).await? else {
        return oauth::unauthorized(&origin);
    };
    let body = req.text().await?;
    if body.len() > MAX_BODY_BYTES {
        return rpc_error(&None, -32600, "request body exceeds 256 KB");
    }
    let rpc = match parse_rpc(&body) {
        Ok(rpc) => rpc,
        Err(reason) => return rpc_error(&None, -32700, &reason),
    };
    match rpc.method.as_str() {
        "initialize" => rpc_result(
            &rpc.id,
            serde_json::json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "lingua-tutor", "version": env!("CARGO_PKG_VERSION") },
            }),
        ),
        // Notifications get an empty 202 (Streamable HTTP).
        "notifications/initialized" | "notifications/cancelled" => {
            Ok(Response::empty()?.with_status(202))
        }
        "ping" => rpc_result(&rpc.id, serde_json::json!({})),
        "tools/list" => rpc_result(&rpc.id, serde_json::json!({ "tools": tools::list() })),
        "tools/call" => {
            let name = rpc
                .params
                .get("name")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            let Some(tool) = tools::find(name) else {
                return rpc_error(&rpc.id, -32602, &format!("unknown tool '{name}'"));
            };
            if !grant.allows(tool.scope) {
                return rpc_error(
                    &rpc.id,
                    -32602,
                    &format!(
                        "insufficient_scope: '{}' requires {}",
                        tool.name, tool.scope
                    ),
                );
            }
            let arguments = rpc
                .params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({}));
            let outcome = tools::call(tool, &arguments, env).await?;
            rpc_result(
                &rpc.id,
                serde_json::json!({
                    "content": [{ "type": "text", "text": outcome.body }],
                    "isError": outcome.is_error,
                }),
            )
        }
        other => rpc_error(&rpc.id, -32601, &format!("method '{other}' not found")),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn parse_rpc_accepts_valid_envelope() {
        let rpc =
            parse_rpc(r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"x"}}"#)
                .unwrap();
        assert_eq!(rpc.method, "tools/call");
        assert_eq!(rpc.id, Some(serde_json::json!(1)));
        assert_eq!(rpc.params["name"], "x");
    }

    #[test]
    fn parse_rpc_rejects_bad_envelopes() {
        assert!(parse_rpc("not json").is_err());
        assert!(parse_rpc(r#"{"jsonrpc":"1.0","method":"m"}"#).is_err());
        assert!(parse_rpc(r#"{"jsonrpc":"2.0"}"#).is_err());
    }

    #[test]
    fn grant_scope_check() {
        let grant = Grant {
            scopes: vec!["learning.read".to_owned(), "lessons.write".to_owned()],
        };
        assert!(grant.allows("learning.read"));
        assert!(!grant.allows("cards.write"));
        assert!(!grant.allows("learning"));
    }
}
