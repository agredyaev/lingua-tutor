//! MCP tool table: names, input schemas, and thin transport dispatch.
//! MCP never constructs synthetic HTTP requests. Both MCP and browser
//! transports call the same typed application operations.

use crate::handlers;
use crate::{Env, Result};

pub(crate) struct ToolDef {
    pub name: &'static str,
    pub description: &'static str,
    pub required: &'static [&'static str],
}

pub(crate) struct ToolOutcome {
    pub body: String,
    pub is_error: bool,
}

pub(crate) const TOOLS: &[ToolDef] = &[
    ToolDef {
        name: "get_active_language_profile",
        description: "List the learner's language profiles (target locale, level, settings).",
        required: &[],
    },
    ToolDef {
        name: "get_tutor_context",
        description: "Authoritative adaptive tutor context: methodology, ranked plan/tasks, persisted learner state, recent signals, preferences, errors, review/redesign evidence, planner history, and lesson-result contract. tutor_tasks is the ordered item-level execution queue; each task carries success_criterion and measurement_requirements. Fetch this before every lesson.",
        required: &["language_profile_id"],
    },
    ToolDef {
        name: "start_lesson",
        description: "After get_tutor_context, open the lesson record before teaching with an explicit scenario and planned goal fixed before instruction. Use the adaptive_lesson_plan and the ordered tutor_tasks returned by start_lesson as the final authoritative execution snapshot for this lesson; consume selected tasks in returned order and instrument them from measurement_requirements before teaching.",
        required: &["language_profile_id", "scenario", "planned_goal"],
    },
    ToolDef {
        name: "complete_lesson",
        description: "Submit the finished lesson using the lesson_id returned by start_lesson plus the full lesson-result document (schema_version 2). Applied atomically; safe to retry (client_operation_id).",
        required: &["lesson_id"],
    },
];

#[must_use]
pub(crate) fn list() -> Vec<serde_json::Value> {
    TOOLS
        .iter()
        .map(|tool| {
            serde_json::json!({
                "name": tool.name,
                "description": tool.description,
                "inputSchema": input_schema(tool),
            })
        })
        .collect()
}

#[must_use]
pub(crate) fn find(name: &str) -> Option<&'static ToolDef> {
    TOOLS.iter().find(|tool| tool.name == name)
}

fn input_schema(tool: &ToolDef) -> serde_json::Value {
    let mut properties: serde_json::Map<String, serde_json::Value> = tool
        .required
        .iter()
        .map(|name| ((*name).to_owned(), serde_json::json!({ "type": "string" })))
        .collect();
    let mut required: Vec<&str> = tool.required.to_vec();
    if tool.name == "complete_lesson" {
        properties.insert(
            "lesson_result".to_owned(),
            serde_json::json!({
                "type": "object",
                "description": "The full lesson-result document, schema_version \"2\"."
            }),
        );
        required.push("lesson_result");
    }
    serde_json::json!({
        "type": "object",
        "properties": properties,
        "required": required,
    })
}

fn missing_argument(tool: &ToolDef, arguments: &serde_json::Value) -> Option<String> {
    tool.required
        .iter()
        .find(|name| {
            arguments
                .get(**name)
                .and_then(serde_json::Value::as_str)
                .is_none_or(|value| value.trim().is_empty())
        })
        .map(|name| (*name).to_owned())
}

fn outcome(value: serde_json::Value) -> ToolOutcome {
    ToolOutcome {
        is_error: false,
        body: value.to_string(),
    }
}

fn arg<'a>(arguments: &'a serde_json::Value, name: &str) -> &'a str {
    arguments
        .get(name)
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
}

pub(crate) fn call(
    tool: &ToolDef,
    arguments: &serde_json::Value,
    env: &Env,
) -> Result<ToolOutcome> {
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

    let response = (|| -> Result<serde_json::Value> {
        match tool.name {
            "get_active_language_profile" => handlers::profiles::list_profiles(env),
            "get_tutor_context" => {
                handlers::records::tutor_context_for(env, arg(arguments, "language_profile_id"))
            }
            "start_lesson" => crate::operations::start_lesson::start_lesson(
                env,
                arg(arguments, "language_profile_id"),
                arg(arguments, "scenario"),
                arg(arguments, "planned_goal"),
                "mcp",
            ),
            "complete_lesson" => {
                let Some(document) = arguments.get("lesson_result") else {
                    return crate::api::operation_error(
                        400,
                        "invalid_arguments",
                        "'lesson_result' (object) is required",
                    );
                };
                let mut result: tutor_contract::LessonResult =
                    serde_json::from_value(document.clone()).map_err(|error| {
                        crate::Error::operation(
                            400,
                            "parse_error",
                            "not a valid lesson-result document",
                            serde_json::json!({ "detail": error.to_string() }),
                        )
                    })?;
                result = handlers::lessons::validate_lesson_result(result)?;
                result.lesson.lesson_id = Some(arg(arguments, "lesson_id").to_owned());
                crate::operations::complete_lesson::complete_lesson(env, &result)
            }
            other => crate::api::operation_error(
                500,
                "tool_not_implemented",
                &format!("tool '{other}' not implemented"),
            ),
        }
    })();

    match response {
        Ok(value) => Ok(outcome(value)),
        Err(crate::Error::Operation {
            code,
            message,
            details,
            ..
        }) => Ok(ToolOutcome {
            body: serde_json::json!({
                "error": code,
                "message": message,
                "details": details,
            })
            .to_string(),
            is_error: true,
        }),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
// Test fixtures intentionally fail fast and use compact tuple snapshots.
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::type_complexity
)]
mod tests {
    use super::*;

    #[test]
    fn read_tools_have_valid_schemas() {
        for name in ["get_active_language_profile", "get_tutor_context"] {
            let tool = find(name).unwrap_or_else(|| panic!("tool {name} missing"));
            let schema = input_schema(tool);
            assert_eq!(schema["type"], "object");
            assert!(schema["properties"].is_object());
            assert!(schema["required"].is_array());
        }
    }

    #[test]
    fn every_roadmap_tool_exists_with_write_schemas() {
        let roadmap = [
            "get_active_language_profile",
            "get_tutor_context",
            "start_lesson",
            "complete_lesson",
        ];
        for name in roadmap {
            assert!(find(name).is_some(), "roadmap tool {name} missing");
        }
        let schema = input_schema(find("complete_lesson").unwrap());
        assert!(schema["properties"]["lesson_result"].is_object());
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&serde_json::json!("lesson_result")));
        assert!(
            required.contains(&serde_json::json!("lesson_id")),
            "live MCP completion must bind explicitly to the lesson returned by start_lesson"
        );
    }

    #[test]
    fn start_lesson_requires_explicit_scenario_and_planned_goal() {
        let schema = input_schema(find("start_lesson").unwrap());
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&serde_json::json!("scenario")));
        assert!(
            required.contains(&serde_json::json!("planned_goal")),
            "communicative outcome must be anchored to a goal fixed before teaching"
        );
    }

    #[test]
    fn lesson_tools_make_context_order_explicit_for_the_llm() {
        let context = find("get_tutor_context").unwrap();
        let start = find("start_lesson").unwrap();
        assert!(context.description.contains("Authoritative"));
        assert!(context.description.contains("before every lesson"));
        assert!(
            start.description.contains("get_tutor_context")
                && start
                    .description
                    .contains("final authoritative execution snapshot")
                && start.description.contains("measurement_requirements")
                && context
                    .description
                    .contains("ordered item-level execution queue"),
            "start_lesson must bind the chat LLM to context-first flow and its persisted executable plan"
        );
    }

    #[test]
    fn tool_names_are_unique() {
        let mut names: Vec<&str> = TOOLS.iter().map(|tool| tool.name).collect();
        names.sort_unstable();
        let before = names.len();
        names.dedup();
        assert_eq!(before, names.len());
    }

    #[test]
    fn missing_argument_detected() {
        let tool = find("get_tutor_context").unwrap();
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
