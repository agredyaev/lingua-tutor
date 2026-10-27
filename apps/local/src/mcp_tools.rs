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
        name: "list_language_packs",
        description: "List installed language packs with their versions.",
        required: &[],
    },
    ToolDef {
        name: "get_active_language_profile",
        description: "List the learner's language profiles (active pack, level, settings).",
        required: &[],
    },
    ToolDef {
        name: "get_language_pack_context",
        description: "Get one language pack's manifest and metadata by pack id.",
        required: &["pack_id"],
    },
    ToolDef {
        name: "get_learning_state",
        description: "Learning items with status and evidence counts for a profile.",
        required: &["language_profile_id"],
    },
    ToolDef {
        name: "get_next_lesson",
        description: "Structured adaptive lesson plan from recent metrics, review pressure, errors, recommendations, and learner preferences.",
        required: &["language_profile_id"],
    },
    ToolDef {
        name: "get_due_error_targets",
        description: "Open error targets for a profile, most urgent first.",
        required: &["language_profile_id"],
    },
    ToolDef {
        name: "get_review_summary",
        description: "Review workload summary: due counts, recent grades, streaks.",
        required: &["language_profile_id"],
    },
    ToolDef {
        name: "get_card_redesign_candidates",
        description: "Cards that keep failing and need redesign, with evidence.",
        required: &["language_profile_id"],
    },
    ToolDef {
        name: "get_tutor_context",
        description: "Full adaptive tutor context: methodology, ranked lesson plan, recent skill signals, preferences, errors, review/redesign evidence, and curriculum candidates. Fetch this before every lesson.",
        required: &["language_profile_id"],
    },
    ToolDef {
        name: "start_lesson",
        description: "Open a lesson record before teaching: profile, scenario, goal.",
        required: &["language_profile_id", "scenario"],
    },
    ToolDef {
        name: "complete_lesson",
        description: "Submit the finished lesson: the full lesson-result document (schema_version 2) with evidence, errors, metrics, card mutations. Applied atomically; safe to retry (client_operation_id).",
        required: &[],
    },
    ToolDef {
        name: "upsert_cards",
        description: "Create or update cards outside a lesson (max 20 mutations); quality rules and activation budget still apply.",
        required: &["language_profile_id"],
    },
    ToolDef {
        name: "suspend_cards",
        description: "Suspend up to 20 cards by id (reversible; restore via the app).",
        required: &[],
    },
    ToolDef {
        name: "log_error_occurrence",
        description: "Record one recurring-error occurrence outside a lesson; optional communicative_impact=low|medium|high controls escalation timing; creates 1/3/7-day rechecks when new.",
        required: &[
            "language_profile_id",
            "core_category",
            "original_production",
            "corrected_production",
        ],
    },
    ToolDef {
        name: "correct_lesson_record",
        description: "Fix a completed lesson's summary, next goal, or duration. Evidence and history stay immutable.",
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
    match tool.name {
        "start_lesson" => {
            properties.insert(
                "planned_goal".to_owned(),
                serde_json::json!({"type": "string"}),
            );
        }
        "complete_lesson" => {
            properties.insert(
                "lesson_result".to_owned(),
                serde_json::json!({
                    "type": "object",
                    "description": "The full lesson-result document, schema_version \"2\"."
                }),
            );
            required.push("lesson_result");
        }
        "upsert_cards" => {
            properties.insert(
                "card_mutations".to_owned(),
                serde_json::json!({
                    "type": "array", "maxItems": 20,
                    "items": { "type": "object" },
                    "description": "Card mutation objects (07-tutor-contract.md §3)."
                }),
            );
            required.push("card_mutations");
        }
        "suspend_cards" => {
            properties.insert(
                "card_ids".to_owned(),
                serde_json::json!({
                    "type": "array", "maxItems": 20, "items": { "type": "string" }
                }),
            );
            properties.insert("reason".to_owned(), serde_json::json!({"type": "string"}));
            required.push("card_ids");
        }
        "log_error_occurrence" => {
            for optional in ["pack_error_code", "explanation", "error_target_id"] {
                properties.insert(optional.to_owned(), serde_json::json!({"type": "string"}));
            }
        }
        "correct_lesson_record" => {
            for optional in ["tutor_summary", "next_goal"] {
                properties.insert(optional.to_owned(), serde_json::json!({"type": "string"}));
            }
            properties.insert(
                "duration_seconds".to_owned(),
                serde_json::json!({"type": "integer", "minimum": 0}),
            );
        }
        _ => {}
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
                .is_none_or(str::is_empty)
        })
        .map(|name| (*name).to_owned())
}

fn invalid_arguments(message: &str) -> Result<ToolOutcome> {
    Ok(ToolOutcome {
        body: serde_json::json!({ "error": "invalid_arguments", "message": message }).to_string(),
        is_error: true,
    })
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

fn optional_arg<'a>(arguments: &'a serde_json::Value, name: &str) -> Option<&'a str> {
    arguments
        .get(name)
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
}

fn upsert_cards(
    arguments: &serde_json::Value,
    env: &Env,
    registry: &crate::BuiltInPackRegistry,
) -> Result<serde_json::Value> {
    let Some(raw) = arguments.get("card_mutations") else {
        return crate::api::operation_error(400, "invalid_arguments", "card_mutations required");
    };
    let Ok(mutations) = serde_json::from_value::<Vec<tutor_contract::CardMutation>>(raw.clone())
    else {
        return crate::api::operation_error(400, "invalid_arguments", "malformed card_mutations");
    };
    if mutations.is_empty() || mutations.len() > 20 {
        return crate::api::operation_error(400, "invalid_arguments", "1-20 mutations allowed");
    }

    crate::operations_v2::upsert_cards::upsert_cards(
        registry,
        env,
        arg(arguments, "language_profile_id"),
        &mutations,
    )
}

fn suspend_cards(arguments: &serde_json::Value, env: &Env) -> Result<ToolOutcome> {
    match crate::operations_v2::suspend_cards::suspend_cards_json(
        env,
        arguments
            .get("card_ids")
            .unwrap_or(&serde_json::Value::Null),
        optional_arg(arguments, "reason"),
    ) {
        Ok(value) => Ok(ToolOutcome {
            body: value.to_string(),
            is_error: false,
        }),
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

fn log_error_occurrence(
    arguments: &serde_json::Value,
    env: &Env,
    registry: &crate::BuiltInPackRegistry,
) -> Result<ToolOutcome> {
    let Ok(core_category) = serde_json::from_value::<learning_core::CoreErrorCategory>(
        serde_json::json!(arg(arguments, "core_category")),
    ) else {
        return invalid_arguments("unknown core_category");
    };

    let communicative_impact = match optional_arg(arguments, "communicative_impact") {
        Some(value) => {
            let Ok(impact) = serde_json::from_value::<learning_core::CommunicativeImpact>(
                serde_json::json!(value),
            ) else {
                return invalid_arguments("unknown communicative_impact");
            };
            impact
        }
        None => learning_core::CommunicativeImpact::Medium,
    };

    let outcome = crate::operations_v2::log_error::log_error_occurrence(
        registry,
        env,
        crate::operations_v2::log_error::LogErrorOccurrenceInput {
            language_profile_id: arg(arguments, "language_profile_id").to_owned(),
            core_category,
            original_production: arg(arguments, "original_production").to_owned(),
            corrected_production: arg(arguments, "corrected_production").to_owned(),
            pack_error_code: optional_arg(arguments, "pack_error_code").map(str::to_owned),
            explanation: optional_arg(arguments, "explanation").map(str::to_owned),
            error_target_id: optional_arg(arguments, "error_target_id").map(str::to_owned),
            communicative_impact,
        },
    );

    match outcome {
        Ok(error_target_id) => Ok(ToolOutcome {
            body: serde_json::json!({ "error_target_id": error_target_id }).to_string(),
            is_error: false,
        }),
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

fn correct_lesson_record(arguments: &serde_json::Value, env: &Env) -> Result<ToolOutcome> {
    let duration = arguments
        .get("duration_seconds")
        .and_then(serde_json::Value::as_i64);

    match crate::operations_v2::lesson_correction::correct_lesson_record(
        env,
        arg(arguments, "lesson_id"),
        optional_arg(arguments, "tutor_summary"),
        optional_arg(arguments, "next_goal"),
        duration,
    ) {
        Ok(value) => Ok(ToolOutcome {
            body: value.to_string(),
            is_error: false,
        }),
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

pub(crate) fn call(
    tool: &ToolDef,
    arguments: &serde_json::Value,
    env: &Env,
    registry: &crate::BuiltInPackRegistry,
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

    match tool.name {
        "suspend_cards" => return suspend_cards(arguments, env),
        "log_error_occurrence" => return log_error_occurrence(arguments, env, registry),
        "correct_lesson_record" => return correct_lesson_record(arguments, env),
        _ => {}
    }

    let response = (|| -> Result<serde_json::Value> {
        match tool.name {
            "list_language_packs" => handlers::profiles::list_packs(registry),
            "get_active_language_profile" => handlers::profiles::list_profiles(env),
            "get_language_pack_context" => {
                handlers::profiles::get_pack(registry, arg(arguments, "pack_id"))
            }
            "get_learning_state" => {
                handlers::records::learning_state_for(env, arg(arguments, "language_profile_id"))
            }
            "get_next_lesson" => {
                handlers::records::next_lesson_for(env, arg(arguments, "language_profile_id"))
            }
            "get_due_error_targets" => handlers::records::list_error_targets_for(
                env,
                arg(arguments, "language_profile_id"),
                None,
            ),
            "get_review_summary" => {
                handlers::review::summary_for(env, arg(arguments, "language_profile_id"))
            }
            "get_card_redesign_candidates" => handlers::recommendations::redesign_candidates_for(
                env,
                arg(arguments, "language_profile_id"),
            ),
            "get_tutor_context" => handlers::records::tutor_context_for(
                registry,
                env,
                arg(arguments, "language_profile_id"),
            ),
            "start_lesson" => crate::operations_v2::start_lesson::start_lesson(
                registry,
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
                let result: tutor_contract::LessonResult = serde_json::from_value(document.clone())
                    .map_err(|error| {
                        crate::Error::operation(
                            400,
                            "parse_error",
                            "not a valid lesson-result document",
                            serde_json::json!({ "detail": error.to_string() }),
                        )
                    })?;
                let result = handlers::lessons::validate_lesson_result(result)?;
                crate::operations_v2::complete_lesson::complete_lesson(registry, env, &result)
            }
            "upsert_cards" => upsert_cards(arguments, env, registry),
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
        for name in [
            "list_language_packs",
            "get_active_language_profile",
            "get_language_pack_context",
            "get_learning_state",
            "get_next_lesson",
            "get_due_error_targets",
            "get_review_summary",
            "get_card_redesign_candidates",
            "get_tutor_context",
        ] {
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
            "list_language_packs",
            "get_active_language_profile",
            "get_language_pack_context",
            "get_learning_state",
            "get_next_lesson",
            "get_due_error_targets",
            "get_review_summary",
            "get_card_redesign_candidates",
            "start_lesson",
            "complete_lesson",
            "upsert_cards",
            "suspend_cards",
            "log_error_occurrence",
            "correct_lesson_record",
        ];
        for name in roadmap {
            assert!(find(name).is_some(), "roadmap tool {name} missing");
        }
        let schema = input_schema(find("complete_lesson").unwrap());
        assert!(schema["properties"]["lesson_result"].is_object());
        assert!(
            schema["required"]
                .as_array()
                .unwrap()
                .contains(&serde_json::json!("lesson_result"))
        );
        let schema = input_schema(find("suspend_cards").unwrap());
        assert_eq!(schema["properties"]["card_ids"]["maxItems"], 20);
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
