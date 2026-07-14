//! MCP tool table: names, scopes, input schemas, dispatch (13-roadmap.md §3).
//! Tools reuse the HTTP handlers through an internal request bridge — same
//! validation, bounds, and error shapes (ADR-0006 §6).

use worker::{Env, Result};

/// One entry in the static tool table.
pub struct ToolDef {
    /// Tool name exposed via `tools/list`.
    pub name: &'static str,
    /// One-line description for the model.
    pub description: &'static str,
    /// Required OAuth scope (13-roadmap.md §3).
    pub scope: &'static str,
}

/// Result of a tool call, ready for the MCP content envelope.
pub struct ToolOutcome {
    /// Response body (JSON text from the underlying handler).
    pub body: String,
    /// True when the underlying handler returned an error status.
    pub is_error: bool,
}

/// Static tool table. Read tools land in WBS 12.1f, write tools in 12.1g.
const TOOLS: [ToolDef; 0] = [];

/// `tools/list` payload.
#[must_use]
pub fn list() -> Vec<serde_json::Value> {
    TOOLS
        .iter()
        .map(|t| {
            serde_json::json!({
                "name": t.name,
                "description": t.description,
                "inputSchema": input_schema(t.name),
            })
        })
        .collect()
}

/// Find a tool by name.
#[must_use]
pub fn find(name: &str) -> Option<&'static ToolDef> {
    TOOLS.iter().find(|t| t.name == name)
}

/// JSON Schema for a tool's arguments.
fn input_schema(name: &str) -> serde_json::Value {
    let _ = name;
    serde_json::json!({ "type": "object", "properties": {}, "required": [] })
}

/// Dispatch a validated call.
///
/// # Errors
/// Propagates Worker runtime failures; handler-level errors become
/// `is_error` outcomes.
pub async fn call(tool: &ToolDef, arguments: &serde_json::Value, env: &Env) -> Result<ToolOutcome> {
    let _ = (arguments, env);
    Ok(ToolOutcome {
        body: format!("{{\"error\":\"tool '{}' not implemented\"}}", tool.name),
        is_error: true,
    })
}
