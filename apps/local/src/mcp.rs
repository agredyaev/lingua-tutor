//! In-process MCP JSON-RPC dispatch used by the built-in secure tunnel.

use crate::{Env, mcp_tools};

/// Result of one stateless JSON-RPC dispatch, used by the native tunnel.
pub(crate) enum DirectDispatch {
    /// JSON-RPC response body.
    Response(String),
    /// Notification accepted; no JSON-RPC response body.
    NotificationAck,
}

/// Dispatch one bounded MCP JSON-RPC message in-process.
pub(crate) fn dispatch_jsonrpc(env: &Env, input: &[u8]) -> Result<DirectDispatch, String> {
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
            let outcome =
                mcp_tools::call(definition, &arguments, env).map_err(|error| error.to_string())?;
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
// Test fixtures intentionally fail fast and use compact tuple snapshots.
#[allow(clippy::unwrap_used, clippy::panic, clippy::type_complexity)]
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
                "
                 INSERT INTO learner_profile
                   (id, created_at, updated_at)
                 VALUES
                   ('learner-1','2026-01-01T00:00:00Z','2026-01-01T00:00:00Z');
                 INSERT INTO learner_language_profiles
                   (id, learner_id, source_language, target_locale,
                    status, is_default, preferences_json, created_at, updated_at)
                 VALUES
                   ('llp-1','learner-1', 'ru','es-ES','active',1,'{}',
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

        let tools = dispatch_jsonrpc(
            &env,
            br#"{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}"#,
        )
        .unwrap();
        let DirectDispatch::Response(tools) = tools else {
            panic!("expected tools response");
        };
        let tools: serde_json::Value = serde_json::from_str(&tools).unwrap();
        assert_eq!(tools["result"]["tools"].as_array().map(Vec::len), Some(4));

        let context = dispatch_jsonrpc(
            &env,
            br#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"get_tutor_context","arguments":{"language_profile_id":"llp-1"}}}"#,
        )
        .unwrap();
        let DirectDispatch::Response(context) = context else {
            panic!("expected tutor context response");
        };
        let context: serde_json::Value = serde_json::from_str(&context).unwrap();
        assert_eq!(context["result"]["isError"], false);
        let context_body: serde_json::Value =
            serde_json::from_str(context["result"]["content"][0]["text"].as_str().unwrap())
                .unwrap();
        assert!(context_body["tutor_instructions"]["instructions"].is_string());
        assert!(context_body["adaptive_lesson_plan"].is_object());
        assert!(context_body["tutor_tasks"].is_array());
        assert!(
            context_body["lesson_result_contract"]["instructions"]
                .as_str()
                .unwrap()
                .contains(
                    "Для calibration-eligible outcome нужен минимум 1 diagnostic baseline и минимум 2 measurement-eligible probes overall"
                )
        );

        let start = dispatch_jsonrpc(
            &env,
            br#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"start_lesson","arguments":{"language_profile_id":"llp-1","scenario":"MCP smoke","planned_goal":"complete the smoke-test communicative task"}}}"#,
        )
        .unwrap();
        let DirectDispatch::Response(start) = start else {
            panic!("expected start response");
        };
        let start: serde_json::Value = serde_json::from_str(&start).unwrap();
        assert_eq!(start["result"]["isError"], false);
        let start_body: serde_json::Value =
            serde_json::from_str(start["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(start_body["planner_version"], "planner-v3");
        assert!(start_body["adaptive_lesson_plan"].is_object());
        assert!(start_body["tutor_tasks"].is_array());

        let connection = Connection::open(&db).unwrap();
        let persisted_plan: String = connection
            .query_row(
                "SELECT plan_json FROM planner_decisions ORDER BY created_at DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let persisted_plan: serde_json::Value = serde_json::from_str(&persisted_plan).unwrap();
        assert_eq!(start_body["adaptive_lesson_plan"], persisted_plan);
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
