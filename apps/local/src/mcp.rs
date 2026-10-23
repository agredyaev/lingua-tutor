//! In-process MCP JSON-RPC dispatch for OpenAI Secure MCP Tunnel.

use crate::{Env, mcp_tools};

/// Result of one stateless JSON-RPC dispatch, used by the native tunnel.
pub enum DirectDispatch {
    /// JSON-RPC response body.
    Response(String),
    /// Notification accepted; no JSON-RPC response body.
    NotificationAck,
}

/// Dispatch one bounded MCP JSON-RPC message in-process.
pub fn dispatch_jsonrpc(
    env: &Env,
    registry: &language_pack_registry::LanguagePackRegistry,
    input: &[u8],
) -> Result<DirectDispatch, String> {
    if input.len() > 512 * 1024 {
        return Err("MCP request exceeds 512 KB".to_owned());
    }
    let request: serde_json::Value =
        serde_json::from_slice(input).map_err(|error| format!("invalid JSON-RPC: {error}"))?;
    let id = request.get("id").cloned();
    let method = request
        .get("method")
        .and_then(serde_json::Value::as_str)
        .ok_or("JSON-RPC method is required")?;

    if id.is_none() {
        return Ok(DirectDispatch::NotificationAck);
    }
    let id = id.unwrap_or(serde_json::Value::Null);
    let result = match method {
        "initialize" => {
            let protocol = request
                .pointer("/params/protocolVersion")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("2025-06-18");
            serde_json::json!({
                "protocolVersion": protocol,
                "capabilities": {"tools": {"listChanged": false}},
                "serverInfo": {"name": "lingua-tutor", "version": env!("CARGO_PKG_VERSION")}
            })
        }
        "ping" => serde_json::json!({}),
        "tools/list" => serde_json::json!({"tools": mcp_tools::list()}),
        "tools/call" => {
            let name = request
                .pointer("/params/name")
                .and_then(serde_json::Value::as_str)
                .ok_or("tools/call requires params.name")?;
            let Some(definition) = mcp_tools::find(name) else {
                return Ok(DirectDispatch::Response(
                    serde_json::json!({
                        "jsonrpc":"2.0","id":id,
                        "error":{"code":-32602,"message":format!("unknown tool: {name}")}
                    })
                    .to_string(),
                ));
            };
            let arguments = request
                .pointer("/params/arguments")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({}));
            let outcome = mcp_tools::call(definition, &arguments, env, registry)
                .map_err(|error| error.to_string())?;
            serde_json::json!({
                "content": [{"type":"text","text": outcome.body}],
                "isError": outcome.is_error
            })
        }
        _ => {
            return Ok(DirectDispatch::Response(
                serde_json::json!({
                    "jsonrpc":"2.0","id":id,
                    "error":{"code":-32601,"message":format!("method not found: {method}")}
                })
                .to_string(),
            ));
        }
    };
    Ok(DirectDispatch::Response(
        serde_json::json!({"jsonrpc":"2.0","id":id,"result":result}).to_string(),
    ))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::storage;
    use rusqlite::Connection;

    fn seeded_env() -> (std::path::PathBuf, Env) {
        let dir = std::env::temp_dir().join(format!("lingua-mcp-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join("lingua.sqlite3");
        let connection = storage::open(&db).unwrap();
        connection
            .execute_batch(
                "INSERT INTO language_packs
                   (id, package_name, version, contract_version, target_language,
                    target_locale, display_name, capabilities_json, enabled,
                    installed_at, updated_at)
                 VALUES
                   ('es-ES','language-pack-es-es','0.1.0',1,'es','es-ES',
                    'Spanish (Spain)','[]',1,'2026-01-01T00:00:00Z','2026-01-01T00:00:00Z');
                 INSERT INTO learner_profile
                   (id, created_at, updated_at)
                 VALUES
                   ('learner-1','2026-01-01T00:00:00Z','2026-01-01T00:00:00Z');
                 INSERT INTO learner_language_profiles
                   (id, learner_id, language_pack_id, source_language, target_locale,
                    status, is_default, preferences_json, created_at, updated_at)
                 VALUES
                   ('llp-1','learner-1','es-ES','ru','es-ES','active',1,'{}',
                    '2026-01-01T00:00:00Z','2026-01-01T00:00:00Z');",
            )
            .unwrap();
        drop(connection);
        let env = storage::environment(&db).unwrap();
        (db, env)
    }

    #[tokio::test]
    async fn direct_dispatch_lists_tools_and_writes_lesson() {
        let (db, env) = seeded_env();

        let registry = crate::built_in_language_packs().unwrap();
        let tools = dispatch_jsonrpc(
            &env,
            &registry,
            br#"{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}"#,
        )
        .unwrap();
        let DirectDispatch::Response(tools) = tools else {
            panic!("expected tools response");
        };
        let tools: serde_json::Value = serde_json::from_str(&tools).unwrap();
        assert_eq!(tools["result"]["tools"].as_array().map(Vec::len), Some(15));

        let start = dispatch_jsonrpc(
            &env,
            &registry,
            br#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"start_lesson","arguments":{"language_profile_id":"llp-1","scenario":"MCP smoke"}}}"#,
        )
        .unwrap();
        let DirectDispatch::Response(start) = start else {
            panic!("expected start response");
        };
        let start: serde_json::Value = serde_json::from_str(&start).unwrap();
        assert_eq!(start["result"]["isError"], false);

        let connection = Connection::open(&db).unwrap();
        let source: String = connection
            .query_row(
                "SELECT source FROM lessons ORDER BY created_at DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(source, "mcp");

        drop(connection);
        std::fs::remove_dir_all(db.parent().unwrap()).unwrap();
    }
}
