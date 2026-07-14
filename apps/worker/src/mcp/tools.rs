//! MCP tool table: names, scopes, input schemas, dispatch (13-roadmap.md §3).
//! Tools reuse the HTTP handlers through an internal request bridge — same
//! validation, bounds, and error shapes (ADR-0006 §6).

use crate::handlers;
use worker::{Env, Method, Request, Response, Result};

/// One entry in the static tool table.
pub struct ToolDef {
    /// Tool name exposed via `tools/list`.
    pub name: &'static str,
    /// One-line description for the model.
    pub description: &'static str,
    /// Required OAuth scope (13-roadmap.md §3).
    pub scope: &'static str,
    /// Required string arguments (all become the input schema).
    pub required: &'static [&'static str],
}

/// Result of a tool call, ready for the MCP content envelope.
pub struct ToolOutcome {
    /// Response body (JSON text from the underlying handler).
    pub body: String,
    /// True when the underlying handler returned an error status.
    pub is_error: bool,
}

/// Static tool table (read tools; write tools land in WBS 12.1g).
pub const TOOLS: &[ToolDef] = &[
    ToolDef {
        name: "list_language_packs",
        description: "List installed language packs with their versions.",
        scope: "language_packs.read",
        required: &[],
    },
    ToolDef {
        name: "get_active_language_profile",
        description: "List the learner's language profiles (active pack, level, settings).",
        scope: "learning.read",
        required: &[],
    },
    ToolDef {
        name: "get_language_pack_context",
        description: "Get one language pack's manifest and metadata by pack id.",
        scope: "language_packs.read",
        required: &["pack_id"],
    },
    ToolDef {
        name: "get_learning_state",
        description: "Learning items with status and evidence counts for a profile.",
        scope: "learning.read",
        required: &["language_profile_id"],
    },
    ToolDef {
        name: "get_next_lesson",
        description: "Next lesson target: goal, scenario, due errors, review pressure.",
        scope: "learning.read",
        required: &["language_profile_id"],
    },
    ToolDef {
        name: "get_due_error_targets",
        description: "Open error targets for a profile, most urgent first.",
        scope: "learning.read",
        required: &["language_profile_id"],
    },
    ToolDef {
        name: "get_review_summary",
        description: "Review workload summary: due counts, recent grades, streaks.",
        scope: "learning.read",
        required: &["language_profile_id"],
    },
    ToolDef {
        name: "get_card_redesign_candidates",
        description: "Cards that keep failing and need redesign, with evidence.",
        scope: "learning.read",
        required: &["language_profile_id"],
    },
    ToolDef {
        name: "get_tutor_context",
        description: "Full tutor context: instructions, next goal, errors, review summary. \
                      Fetch this before starting a lesson.",
        scope: "exports.read",
        required: &["language_profile_id"],
    },
];

/// `tools/list` payload.
#[must_use]
pub fn list() -> Vec<serde_json::Value> {
    TOOLS
        .iter()
        .map(|t| {
            serde_json::json!({
                "name": t.name,
                "description": t.description,
                "inputSchema": input_schema(t),
            })
        })
        .collect()
}

/// Find a tool by name.
#[must_use]
pub fn find(name: &str) -> Option<&'static ToolDef> {
    TOOLS.iter().find(|t| t.name == name)
}

/// JSON Schema for a tool's arguments: flat object of required strings.
fn input_schema(tool: &ToolDef) -> serde_json::Value {
    let properties: serde_json::Map<String, serde_json::Value> = tool
        .required
        .iter()
        .map(|name| ((*name).to_owned(), serde_json::json!({ "type": "string" })))
        .collect();
    serde_json::json!({
        "type": "object",
        "properties": properties,
        "required": tool.required,
    })
}

/// Missing/blank required argument → tool error before dispatch.
fn missing_argument(tool: &ToolDef, arguments: &serde_json::Value) -> Option<String> {
    tool.required
        .iter()
        .find(|name| {
            arguments
                .get(**name)
                .and_then(serde_json::Value::as_str)
                .is_none_or(str::is_empty)
        })
        .map(|name| (*name).to_owned())
}

/// Synthetic GET request against an internal handler path; the bridge keeps
/// every existing query-param validation in force (ADR-0006 §6).
fn internal_get(path: &str, query: &[(&str, &str)]) -> Result<Request> {
    let mut url = worker::Url::parse("https://internal")
        .map_err(|e| worker::Error::RustError(e.to_string()))?;
    url.set_path(path);
    for (key, value) in query {
        url.query_pairs_mut().append_pair(key, value);
    }
    Request::new(url.as_str(), Method::Get)
}

async fn outcome_from(mut response: Response) -> Result<ToolOutcome> {
    let status = response.status_code();
    Ok(ToolOutcome {
        body: response.text().await?,
        is_error: status >= 400,
    })
}

fn arg<'a>(arguments: &'a serde_json::Value, name: &str) -> &'a str {
    arguments
        .get(name)
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
}

/// Dispatch a validated call.
///
/// # Errors
/// Propagates Worker runtime failures; handler-level errors become
/// `is_error` outcomes.
pub async fn call(tool: &ToolDef, arguments: &serde_json::Value, env: &Env) -> Result<ToolOutcome> {
    if let Some(name) = missing_argument(tool, arguments) {
        return Ok(ToolOutcome {
            body: serde_json::json!({
                "error": "invalid_arguments",
                "message": format!("'{name}' (string) is required"),
            })
            .to_string(),
            is_error: true,
        });
    }
    let registry =
        crate::built_in_language_packs().map_err(|e| worker::Error::RustError(e.to_string()))?;
    let profile_query = [("language_profile_id", arg(arguments, "language_profile_id"))];
    let response = match tool.name {
        "list_language_packs" => handlers::profiles::list_packs(&registry)?,
        "get_active_language_profile" => handlers::profiles::list_profiles(env).await?,
        "get_language_pack_context" => {
            handlers::profiles::get_pack(&registry, arg(arguments, "pack_id"))?
        }
        "get_learning_state" => {
            let req = internal_get("/api/v1/learning-state", &profile_query)?;
            handlers::records::learning_state(&req, env).await?
        }
        "get_next_lesson" => {
            let req = internal_get("/api/v1/next-lesson", &profile_query)?;
            handlers::records::next_lesson(&req, env).await?
        }
        "get_due_error_targets" => {
            let req = internal_get("/api/v1/error-targets", &profile_query)?;
            handlers::records::list_error_targets(&req, env).await?
        }
        "get_review_summary" => {
            let req = internal_get("/api/v1/review/summary", &profile_query)?;
            handlers::review::summary(&req, env).await?
        }
        "get_card_redesign_candidates" => {
            let req = internal_get("/api/v1/cards/redesign-candidates", &profile_query)?;
            handlers::recommendations::redesign_candidates(&req, env).await?
        }
        "get_tutor_context" => {
            let req = internal_get("/api/v1/export/tutor-context", &profile_query)?;
            handlers::records::tutor_context(&registry, &req, env).await?
        }
        other => {
            return Ok(ToolOutcome {
                body: format!("{{\"error\":\"tool '{other}' not implemented\"}}"),
                is_error: true,
            });
        }
    };
    outcome_from(response).await
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn read_tools_have_scopes_and_valid_schemas() {
        let read_tools = [
            ("list_language_packs", "language_packs.read"),
            ("get_active_language_profile", "learning.read"),
            ("get_language_pack_context", "language_packs.read"),
            ("get_learning_state", "learning.read"),
            ("get_next_lesson", "learning.read"),
            ("get_due_error_targets", "learning.read"),
            ("get_review_summary", "learning.read"),
            ("get_card_redesign_candidates", "learning.read"),
            ("get_tutor_context", "exports.read"),
        ];
        for (name, scope) in read_tools {
            let tool = find(name).unwrap_or_else(|| panic!("tool {name} missing"));
            assert_eq!(tool.scope, scope, "{name}");
            assert!(
                super::super::oauth::SCOPES.contains(&tool.scope),
                "{name}: scope must be one of the six"
            );
            let schema = input_schema(tool);
            assert_eq!(schema["type"], "object");
            assert!(schema["properties"].is_object());
            assert!(schema["required"].is_array());
        }
    }

    #[test]
    fn tool_names_are_unique() {
        let mut names: Vec<&str> = TOOLS.iter().map(|t| t.name).collect();
        names.sort_unstable();
        let before = names.len();
        names.dedup();
        assert_eq!(before, names.len());
    }

    #[test]
    fn missing_argument_detected() {
        let tool = find("get_learning_state").unwrap();
        assert_eq!(
            missing_argument(tool, &serde_json::json!({})),
            Some("language_profile_id".to_owned())
        );
        assert_eq!(
            missing_argument(tool, &serde_json::json!({"language_profile_id": ""})),
            Some("language_profile_id".to_owned())
        );
        assert_eq!(
            missing_argument(tool, &serde_json::json!({"language_profile_id": "llp-1"})),
            None
        );
    }
}
