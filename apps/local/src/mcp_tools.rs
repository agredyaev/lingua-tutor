//! MCP tool table: names, input schemas, and thin transport dispatch.
//! MCP never constructs synthetic HTTP requests. Both MCP and browser
//! transports call the same typed application operations.

use crate::handlers;
use crate::{Env, Result};
use rusqlite::{OptionalExtension as _, TransactionBehavior, params};

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
        description: "Submit the finished lesson: the full lesson-result document (schema_version 1) with evidence, errors, metrics, card mutations. Applied atomically; safe to retry (client_operation_id).",
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
        description: "Record one recurring-error occurrence outside a lesson; creates the error target and 1/3/7-day rechecks when new.",
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
                    "description": "The full lesson-result document, schema_version \"1\"."
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

fn pack_version_of(
    registry: &language_pack_registry::LanguagePackRegistry,
    pack_id: &str,
) -> String {
    registry.get(pack_id).map_or_else(
        || "0.0.0".to_owned(),
        |pack| pack.manifest().version.clone(),
    )
}

fn upsert_cards(
    arguments: &serde_json::Value,
    env: &Env,
    registry: &language_pack_registry::LanguagePackRegistry,
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

    let mut database = env.connection()?;
    let transaction = database.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let profile_id = arg(arguments, "language_profile_id");
    let Some(profile_row) = handlers::profiles::fetch_profile(&transaction, profile_id)? else {
        return crate::api::operation_error(404, "not_found", "unknown profile");
    };
    let Ok(profile) = profile_row.into_domain() else {
        return crate::api::operation_error(500, "corrupt_row", "stored profile is invalid");
    };
    let context = application::LessonCompletionContext {
        profile,
        now: crate::api::now(),
        existing_lesson: None,
        duplicate_import: false,
        items_by_normalized: Vec::new(),
        normalized_new_items: Vec::new(),
        known_item_ids: Vec::new(),
        error_targets: Vec::new(),
        evidence_for_mastery: Vec::new(),
    };
    let mutation_ctx =
        handlers::lessons::build_mutation_context(&transaction, &mutations, &context)?;
    let planned = application::plan_card_mutations(&mutations, &mutation_ctx);
    let pack_version = pack_version_of(registry, &context.profile.language_pack_id.0);
    let at = crate::repositories::to_rfc3339(context.now);
    let summary = handlers::lessons::apply_card_mutations(
        &transaction,
        &mutations,
        &pack_version,
        &context,
        &planned,
        &mutation_ctx,
        None,
        &at,
    )?;
    transaction.commit()?;
    crate::json(&serde_json::json!({ "card_mutations": summary }))
}

fn suspend_cards(arguments: &serde_json::Value, env: &Env) -> Result<ToolOutcome> {
    let ids: Vec<String> = arguments
        .get("card_ids")
        .and_then(|value| serde_json::from_value(value.clone()).ok())
        .unwrap_or_default();
    if ids.is_empty() || ids.len() > 20 {
        return invalid_arguments("'card_ids' (1-20 strings) is required");
    }

    let mut database = env.connection()?;
    let transaction = database.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let reason = optional_arg(arguments, "reason");
    let mut results = Vec::with_capacity(ids.len());
    for id in &ids {
        let result = handlers::cards::set_status_on(
            &transaction,
            id,
            learning_core::CardStatus::Suspended,
            reason,
        )?;
        results.push(result);
    }
    transaction.commit()?;
    Ok(ToolOutcome {
        body: serde_json::json!({ "results": results }).to_string(),
        is_error: false,
    })
}

fn log_error_occurrence(
    arguments: &serde_json::Value,
    env: &Env,
    registry: &language_pack_registry::LanguagePackRegistry,
) -> Result<ToolOutcome> {
    use crate::repositories::records as records_repo;

    let mut database = env.connection()?;
    let transaction = database.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let profile_id = arg(arguments, "language_profile_id");
    let Some(profile_row) = handlers::profiles::fetch_profile(&transaction, profile_id)? else {
        return invalid_arguments("unknown language_profile_id");
    };
    let Ok(profile) = profile_row.into_domain() else {
        return invalid_arguments("stored profile is invalid");
    };
    let Ok(core_category) = serde_json::from_value::<learning_core::CoreErrorCategory>(
        serde_json::json!(arg(arguments, "core_category")),
    ) else {
        return invalid_arguments("unknown core_category");
    };

    let now = crate::api::now();
    let at = crate::repositories::to_rfc3339(now);
    let target_id = match optional_arg(arguments, "error_target_id") {
        Some(existing_id) => {
            let row = transaction
                .query_row(
                    records_repo::SELECT_ERROR_TARGET,
                    params![existing_id],
                    records_repo::ErrorTargetRow::from_row,
                )
                .optional()?;
            let Some(target) = row.and_then(|row| row.into_domain().ok()) else {
                return invalid_arguments("unknown error_target_id");
            };
            transaction.execute(
                records_repo::UPDATE_ERROR_TARGET,
                params![
                    target.id.0,
                    target.occurrence_count + 1,
                    u8::from(target.occurrence_count + 1 >= 3),
                    "recheck",
                    at,
                ],
            )?;
            target.id.0
        }
        None => {
            let target_id = crate::api::new_id("et");
            transaction.execute(
                records_repo::INSERT_ERROR_TARGET,
                params![
                    target_id,
                    profile.id.0,
                    profile.language_pack_id.0,
                    pack_version_of(registry, &profile.language_pack_id.0),
                    crate::repositories::enum_str(&core_category),
                    optional_arg(arguments, "pack_error_code"),
                    arg(arguments, "original_production"),
                    arg(arguments, "corrected_production"),
                    optional_arg(arguments, "explanation"),
                    1_u32,
                    0_u8,
                    "active",
                    at,
                ],
            )?;
            handlers::lessons::insert_rechecks(&transaction, &target_id, now)?;
            target_id
        }
    };
    transaction.execute(
        records_repo::INSERT_ERROR_OCCURRENCE,
        params![
            crate::api::new_id("eo"),
            target_id,
            Option::<String>::None,
            Option::<String>::None,
            at,
        ],
    )?;
    transaction.commit()?;
    Ok(ToolOutcome {
        body: serde_json::json!({ "error_target_id": target_id }).to_string(),
        is_error: false,
    })
}

fn correct_lesson_record(arguments: &serde_json::Value, env: &Env) -> Result<ToolOutcome> {
    let database = env.connection()?;
    let duration = arguments
        .get("duration_seconds")
        .and_then(serde_json::Value::as_i64)
        .filter(|duration| (0..=86_400).contains(duration));
    let changed = database.execute(
        crate::repositories::lessons::CORRECT_LESSON_RECORD,
        params![
            arg(arguments, "lesson_id"),
            optional_arg(arguments, "tutor_summary"),
            optional_arg(arguments, "next_goal"),
            duration,
            crate::repositories::to_rfc3339(crate::api::now()),
        ],
    )?;
    if changed == 0 {
        return invalid_arguments("unknown lesson_id or lesson not completed");
    }
    Ok(ToolOutcome {
        body: serde_json::json!({ "status": "corrected" }).to_string(),
        is_error: false,
    })
}

pub(crate) fn call(
    tool: &ToolDef,
    arguments: &serde_json::Value,
    env: &Env,
    registry: &language_pack_registry::LanguagePackRegistry,
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
            "start_lesson" => handlers::lessons::start_lesson_operation(
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
                handlers::lessons::complete_from_result(registry, env, &result)
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
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
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
