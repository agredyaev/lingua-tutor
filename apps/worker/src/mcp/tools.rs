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

/// Static tool table: the 14 roadmap §3 tools + `get_tutor_context`.
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
    ToolDef {
        name: "start_lesson",
        description: "Open a lesson record before teaching: profile, scenario, goal.",
        scope: "lessons.write",
        required: &["language_profile_id", "scenario"],
    },
    ToolDef {
        name: "complete_lesson",
        description: "Submit the finished lesson: the full lesson-result document \
                      (schema_version 1) with evidence, errors, metrics, card mutations. \
                      Applied atomically; safe to retry (client_operation_id).",
        scope: "lessons.write",
        required: &[],
    },
    ToolDef {
        name: "upsert_cards",
        description: "Create or update cards outside a lesson (max 20 mutations); \
                      quality rules and activation budget still apply.",
        scope: "cards.write",
        required: &["language_profile_id"],
    },
    ToolDef {
        name: "suspend_cards",
        description: "Suspend up to 20 cards by id (reversible; restore via the app).",
        scope: "cards.write",
        required: &[],
    },
    ToolDef {
        name: "log_error_occurrence",
        description: "Record one recurring-error occurrence outside a lesson; creates \
                      the error target and 1/3/7-day rechecks when new.",
        scope: "errors.write",
        required: &[
            "language_profile_id",
            "core_category",
            "original_production",
            "corrected_production",
        ],
    },
    ToolDef {
        name: "correct_lesson_record",
        description: "Fix a completed lesson's summary, next goal, or duration. \
                      Evidence and history stay immutable.",
        scope: "lessons.write",
        required: &["lesson_id"],
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

/// JSON Schema for a tool's arguments: flat object of required strings, plus
/// tool-specific non-string members.
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

fn invalid_arguments(message: &str) -> Result<ToolOutcome> {
    Ok(ToolOutcome {
        body: serde_json::json!({ "error": "invalid_arguments", "message": message }).to_string(),
        is_error: true,
    })
}

/// Synthetic POST request with a JSON body against an internal handler path.
fn internal_post(path: &str, body: &serde_json::Value) -> Result<Request> {
    let headers = worker::Headers::new();
    headers.set("Content-Type", "application/json")?;
    let init = worker::RequestInit {
        method: Method::Post,
        headers,
        body: Some(body.to_string().into()),
        ..worker::RequestInit::default()
    };
    Request::new_with_init(&format!("https://internal{path}"), &init)
}

/// `upsert_cards`: card mutations outside a lesson — same planner, quality
/// rules, activation budget, and audit rows as the lesson path (ADR-0006 §6).
async fn upsert_cards(arguments: &serde_json::Value, env: &Env) -> Result<Response> {
    let Some(raw) = arguments.get("card_mutations") else {
        return crate::api::error_response(400, "invalid_arguments", "card_mutations required");
    };
    let Ok(mutations) = serde_json::from_value::<Vec<tutor_contract::CardMutation>>(raw.clone())
    else {
        return crate::api::error_response(400, "invalid_arguments", "malformed card_mutations");
    };
    if mutations.is_empty() || mutations.len() > 20 {
        return crate::api::error_response(400, "invalid_arguments", "1-20 mutations allowed");
    }
    let database = crate::api::db(env)?;
    let profile_id = arg(arguments, "language_profile_id");
    let Some(profile_row) = handlers::profiles::fetch_profile(&database, profile_id).await? else {
        return crate::api::error_response(404, "not_found", "unknown profile");
    };
    let Ok(profile) = profile_row.into_domain() else {
        return crate::api::error_response(500, "corrupt_row", "stored profile is invalid");
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
        handlers::lessons::build_mutation_context(&database, &mutations, &context).await?;
    let planned = application::plan_card_mutations(&mutations, &mutation_ctx);
    let pack_version = pack_version_of(&context.profile.language_pack_id.0)?;
    let at = crate::repositories::to_rfc3339(context.now);
    let mut statements = Vec::new();
    let summary = handlers::lessons::push_card_mutation_statements(
        &database,
        &mut statements,
        &mutations,
        &pack_version,
        &context,
        &planned,
        &mutation_ctx,
        None,
        &at,
    )?;
    database.batch(statements).await?;
    Response::from_json(&serde_json::json!({ "card_mutations": summary }))
}

/// `suspend_cards`: bounded loop over the existing status handler.
async fn suspend_cards(arguments: &serde_json::Value, env: &Env) -> Result<ToolOutcome> {
    let ids: Vec<String> = arguments
        .get("card_ids")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    if ids.is_empty() || ids.len() > 20 {
        return invalid_arguments("'card_ids' (1-20 strings) is required");
    }
    let reason = arg(arguments, "reason");
    let mut results = Vec::with_capacity(ids.len());
    for id in &ids {
        let req = internal_post(
            &format!("/api/v1/cards/{id}/suspend"),
            &serde_json::json!({ "reason": reason }),
        )?;
        let mut response =
            handlers::cards::set_status(req, env, id, learning_core::CardStatus::Suspended).await?;
        results.push(serde_json::json!({
            "card_id": id,
            "status": response.status_code(),
            "body": response.text().await?,
        }));
    }
    Ok(ToolOutcome {
        body: serde_json::json!({ "results": results }).to_string(),
        is_error: false,
    })
}

/// `log_error_occurrence`: one occurrence outside a lesson. Existing target →
/// count bump + occurrence; new → target + occurrence + 1/3/7-day rechecks.
/// Same SQL constants as the lesson path — no duplicate queries.
async fn log_error_occurrence(arguments: &serde_json::Value, env: &Env) -> Result<ToolOutcome> {
    use crate::repositories::records as records_repo;
    let database = crate::api::db(env)?;
    let profile_id = arg(arguments, "language_profile_id");
    let Some(profile_row) = handlers::profiles::fetch_profile(&database, profile_id).await? else {
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
    let mut statements = Vec::new();
    let target_id = match arguments
        .get("error_target_id")
        .and_then(serde_json::Value::as_str)
        .filter(|s| !s.is_empty())
    {
        Some(existing_id) => {
            let row = database
                .prepare(records_repo::SELECT_ERROR_TARGET)
                .bind(&[existing_id.into()])?
                .first::<records_repo::ErrorTargetRow>(None)
                .await?;
            let Some(target) = row.and_then(|r| r.into_domain().ok()) else {
                return invalid_arguments("unknown error_target_id");
            };
            statements.push(database.prepare(records_repo::UPDATE_ERROR_TARGET).bind(&[
                target.id.0.clone().into(),
                (target.occurrence_count + 1).into(),
                u8::from(target.occurrence_count + 1 >= 3).into(),
                "recheck".into(),
                at.clone().into(),
            ])?);
            target.id.0
        }
        None => {
            let target_id = crate::api::new_id("et");
            statements.push(database.prepare(records_repo::INSERT_ERROR_TARGET).bind(&[
                target_id.clone().into(),
                profile.id.0.clone().into(),
                profile.language_pack_id.0.clone().into(),
                pack_version_of(&profile.language_pack_id.0)?.into(),
                crate::repositories::enum_str(&core_category).into(),
                optional_arg(arguments, "pack_error_code"),
                arg(arguments, "original_production").into(),
                arg(arguments, "corrected_production").into(),
                optional_arg(arguments, "explanation"),
                1.into(),
                0.into(),
                "active".into(),
                at.clone().into(),
            ])?);
            handlers::lessons::push_rechecks(&database, &mut statements, &target_id, now)?;
            target_id
        }
    };
    statements.push(
        database
            .prepare(records_repo::INSERT_ERROR_OCCURRENCE)
            .bind(&[
                crate::api::new_id("eo").into(),
                target_id.clone().into(),
                worker::wasm_bindgen::JsValue::NULL,
                worker::wasm_bindgen::JsValue::NULL,
                at.into(),
            ])?,
    );
    database.batch(statements).await?;
    Ok(ToolOutcome {
        body: serde_json::json!({ "error_target_id": target_id }).to_string(),
        is_error: false,
    })
}

/// Installed pack version for audit rows; unknown pack → "0.0.0".
fn pack_version_of(pack_id: &str) -> Result<String> {
    let registry =
        crate::built_in_language_packs().map_err(|e| worker::Error::RustError(e.to_string()))?;
    Ok(registry
        .get(pack_id)
        .map_or_else(|| "0.0.0".to_owned(), |p| p.manifest().version.clone()))
}

fn optional_arg(arguments: &serde_json::Value, name: &str) -> worker::wasm_bindgen::JsValue {
    arguments
        .get(name)
        .and_then(serde_json::Value::as_str)
        .filter(|s| !s.is_empty())
        .map_or(worker::wasm_bindgen::JsValue::NULL, Into::into)
}

/// `correct_lesson_record`: bounded UPDATE of a completed lesson's mutable
/// record fields; history and evidence untouched.
async fn correct_lesson_record(arguments: &serde_json::Value, env: &Env) -> Result<ToolOutcome> {
    let database = crate::api::db(env)?;
    let duration = arguments
        .get("duration_seconds")
        .and_then(serde_json::Value::as_i64)
        .filter(|d| (0..=86_400).contains(d));
    let result = database
        .prepare(crate::repositories::lessons::CORRECT_LESSON_RECORD)
        .bind(&[
            arg(arguments, "lesson_id").into(),
            optional_arg(arguments, "tutor_summary"),
            optional_arg(arguments, "next_goal"),
            duration.map_or(worker::wasm_bindgen::JsValue::NULL, Into::into),
            crate::repositories::to_rfc3339(crate::api::now()).into(),
        ])?
        .run()
        .await?;
    let changed = result
        .meta()
        .ok()
        .flatten()
        .and_then(|m| m.changes)
        .unwrap_or(0)
        > 0;
    if !changed {
        return invalid_arguments("unknown lesson_id or lesson not completed");
    }
    Ok(ToolOutcome {
        body: serde_json::json!({ "status": "corrected" }).to_string(),
        is_error: false,
    })
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
        "start_lesson" => {
            let body = serde_json::json!({
                "language_profile_id": arg(arguments, "language_profile_id"),
                "scenario": arg(arguments, "scenario"),
                "planned_goal": arg(arguments, "planned_goal"),
            });
            let req = internal_post("/api/v1/lessons/start", &body)?;
            handlers::lessons::start_lesson(&registry, req, env).await?
        }
        "complete_lesson" => {
            let Some(document) = arguments.get("lesson_result") else {
                return invalid_arguments("'lesson_result' (object) is required");
            };
            let text = document.to_string();
            match handlers::lessons::parse_and_validate(&text) {
                Ok((result, issues)) if issues.is_empty() => {
                    handlers::lessons::complete_from_result(env, &result).await?
                }
                Ok((_, issues)) => {
                    return Ok(ToolOutcome {
                        body: serde_json::json!({
                            "error": "validation_failed", "issues": issues
                        })
                        .to_string(),
                        is_error: true,
                    });
                }
                Err(response) => response?,
            }
        }
        "upsert_cards" => upsert_cards(arguments, env).await?,
        "suspend_cards" => return suspend_cards(arguments, env).await,
        "log_error_occurrence" => return log_error_occurrence(arguments, env).await,
        "correct_lesson_record" => return correct_lesson_record(arguments, env).await,
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
    fn every_roadmap_tool_exists_with_write_scopes() {
        // 13-roadmap.md §3 — the authoritative list.
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
        for (name, scope) in [
            ("start_lesson", "lessons.write"),
            ("complete_lesson", "lessons.write"),
            ("correct_lesson_record", "lessons.write"),
            ("upsert_cards", "cards.write"),
            ("suspend_cards", "cards.write"),
            ("log_error_occurrence", "errors.write"),
        ] {
            assert_eq!(find(name).unwrap().scope, scope, "{name}");
        }
        // Write tools carry non-trivial schemas.
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
