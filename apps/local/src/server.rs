//! Loopback PWA server.
//!
//! Interactive browser operations use one JSON-RPC boundary at `/rpc`.
//! Full JSON/CSV exports remain direct download endpoints because their
//! response semantics are files, not application command/query calls.

use crate::{Env, Error, api, handlers};
use axum::Router;
use axum::body::{Body, to_bytes};
use axum::extract::State;
use axum::http::{Method as HttpMethod, Request as HttpRequest, Response as HttpResponse};
use axum::routing::any;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Clone)]
struct AppState {
    env: Env,
    registry: Arc<crate::BuiltInPackRegistry>,
    assets: PathBuf,
    port: u16,
}

fn reply(status: u16, content_type: &str, body: impl Into<Body>) -> HttpResponse<Body> {
    HttpResponse::builder()
        .status(status)
        .header("content-type", content_type)
        .body(body.into())
        .unwrap_or_else(|_| HttpResponse::new(Body::empty()))
}

#[derive(Deserialize)]
struct RpcRequest {
    jsonrpc: String,
    id: serde_json::Value,
    method: String,
    #[serde(default)]
    params: serde_json::Value,
}

fn rpc_param<'a>(params: &'a serde_json::Value, name: &str) -> Option<&'a str> {
    params.get(name).and_then(serde_json::Value::as_str)
}

fn rpc_response(id: serde_json::Value, result: serde_json::Value) -> HttpResponse<Body> {
    reply(
        200,
        "application/json",
        serde_json::json!({ "jsonrpc": "2.0", "id": id, "result": result }).to_string(),
    )
}

fn rpc_operation_error(id: serde_json::Value, error: &Error) -> HttpResponse<Body> {
    match error {
        Error::Operation {
            status,
            code,
            message,
            details,
        } => {
            let rpc_code = match *code {
                "method_not_found" => -32601,
                "invalid_params" => -32602,
                _ => -32000,
            };
            let body = serde_json::json!({
                "error": { "code": code, "message": message, "details": details }
            });
            reply(
                200,
                "application/json",
                serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": {
                        "code": rpc_code,
                        "message": message,
                        "data": { "status": status, "body": body }
                    }
                })
                .to_string(),
            )
        }
        Error::RustError(message) => rpc_error(id, -32603, message),
    }
}

fn rpc_error(id: serde_json::Value, code: i64, message: &str) -> HttpResponse<Body> {
    reply(
        200,
        "application/json",
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": code, "message": message }
        })
        .to_string(),
    )
}

fn dispatch_rpc(
    call: RpcRequest,
    env: &Env,
    registry: &crate::BuiltInPackRegistry,
) -> crate::Result<serde_json::Value> {
    let params = &call.params;
    let required = |name: &str| {
        rpc_param(params, name).ok_or_else(|| {
            crate::Error::operation(
                400,
                "invalid_params",
                format!("missing RPC param: {name}"),
                serde_json::json!({}),
            )
        })
    };
    let u32_param = |name: &str| {
        params
            .get(name)
            .and_then(serde_json::Value::as_u64)
            .and_then(|value| u32::try_from(value).ok())
            .ok_or_else(|| {
                crate::Error::operation(
                    400,
                    "invalid_params",
                    format!("missing or invalid RPC param: {name}"),
                    serde_json::json!({}),
                )
            })
    };
    let bool_param = |name: &str| {
        params
            .get(name)
            .and_then(serde_json::Value::as_bool)
            .ok_or_else(|| {
                crate::Error::operation(
                    400,
                    "invalid_params",
                    format!("missing or invalid RPC param: {name}"),
                    serde_json::json!({}),
                )
            })
    };

    match call.method.as_str() {
        "health" => handlers::records::health(env),
        "list_language_profiles" => handlers::profiles::list_profiles(env),
        "list_language_packs" => handlers::profiles::list_packs(registry),
        "get_language_pack" => handlers::profiles::get_pack(registry, required("pack_id")?),
        "create_language_profile" => crate::operations::profile_write::create_profile(
            registry,
            env,
            required("language_pack_id")?,
            required("source_language")?,
        ),
        "update_profile" => {
            let profile_id = required("language_profile_id")?.to_owned();
            let patch: handlers::profiles::PatchProfileParams =
                serde_json::from_value(params.clone())?;
            crate::operations::profile_write::patch_profile(
                env,
                &profile_id,
                crate::operations::profile_write::PatchProfileInput {
                    estimated_level: patch.estimated_level.as_deref(),
                    status: patch.status.as_deref(),
                    is_default: patch.is_default,
                    preferences: patch.preferences.as_ref(),
                },
            )
        }
        "get_dashboard" => handlers::records::dashboard_for(env, required("language_profile_id")?),
        "get_learning_state" => {
            handlers::records::learning_state_for(env, required("language_profile_id")?)
        }
        "get_next_lesson" => {
            handlers::records::next_lesson_for(env, required("language_profile_id")?)
        }
        "start_lesson" => crate::operations::start_lesson::start_lesson(
            registry,
            env,
            required("language_profile_id")?,
            required("scenario")?,
            rpc_param(params, "planned_goal").unwrap_or_default(),
            "pwa",
        ),
        "list_lessons" => {
            handlers::lessons::list_lessons_for(env, required("language_profile_id")?)
        }
        "get_lesson" => handlers::lessons::get_lesson(env, required("lesson_id")?),
        "complete_lesson" => {
            let lesson_id = required("lesson_id")?.to_owned();
            let document = params.get("lesson_result").ok_or_else(|| {
                crate::Error::operation(
                    400,
                    "invalid_params",
                    "missing RPC param: lesson_result",
                    serde_json::json!({}),
                )
            })?;
            let result: tutor_contract::LessonResult = serde_json::from_value(document.clone())
                .map_err(|error| {
                    crate::Error::operation(
                        400,
                        "parse_error",
                        "not a valid lesson-result document",
                        serde_json::json!({ "detail": error.to_string() }),
                    )
                })?;
            handlers::lessons::complete_lesson_result(registry, env, &lesson_id, result)
        }
        "validate_lesson_result" => handlers::lessons::validate_import_text(required("text")?, env),
        "apply_lesson_result" => handlers::lessons::apply_import_text(
            registry,
            required("text")?,
            required("validation_token")?,
            env,
        ),
        "list_error_targets" => handlers::records::list_error_targets_for(
            env,
            required("language_profile_id")?,
            rpc_param(params, "status"),
        ),
        "get_error_target" => {
            handlers::records::get_error_target(env, required("error_target_id")?)
        }
        "complete_error_recheck" => crate::operations::records_write::complete_error_recheck(
            env,
            required("error_target_id")?,
            required("recheck_id")?,
            bool_param("passed")?,
        ),
        "create_listening_attempt" => {
            let attempt: handlers::records::ListeningAttemptParams =
                serde_json::from_value(params.clone())?;
            crate::operations::records_write::create_listening_attempt(
                env,
                crate::operations::records_write::ListeningAttemptInput {
                    language_profile_id: &attempt.language_profile_id,
                    learning_item_id: attempt.learning_item_id.as_deref(),
                    lesson_id: attempt.lesson_id.as_deref(),
                    source_reference: attempt.source_reference.as_deref(),
                    source_variant_id: attempt.source_variant_id.as_deref(),
                    talker_key: attempt.talker_key.as_deref(),
                    context_tag: &attempt.context_tag,
                    transcript: attempt.transcript.as_deref(),
                    success: attempt.success,
                    assistance_level: attempt.assistance_level,
                    response_millis: attempt.response_millis,
                    probe_phase: attempt.probe_phase,
                },
            )
        }
        "get_tutor_context" => {
            handlers::records::tutor_context_for(registry, env, required("language_profile_id")?)
        }
        "list_cards" => handlers::cards::list_for(
            env,
            required("language_profile_id")?,
            rpc_param(params, "status"),
            rpc_param(params, "intent"),
        ),
        "get_card" => handlers::cards::get(env, required("card_id")?),
        "patch_card" => {
            let card_id = required("card_id")?.to_owned();
            let patch: handlers::cards::CardPatchParams = serde_json::from_value(params.clone())?;
            crate::operations::card_direct::patch_card(
                env,
                &card_id,
                crate::operations::card_direct::CardPatchInput {
                    edit_class: &patch.edit_class,
                    prompt: patch.prompt.as_deref(),
                    answer: patch.answer.as_deref(),
                    explanation: patch.explanation.as_deref(),
                    pronunciation_note: patch.pronunciation_note.as_deref(),
                    accepted_answers: patch.accepted_answers.as_deref(),
                },
            )
        }
        "set_card_status" => {
            let status = match required("status")? {
                "suspended" => learning_core::CardStatus::Suspended,
                "review" => learning_core::CardStatus::Review,
                "retired" => learning_core::CardStatus::Retired,
                _ => {
                    return api::operation_error(400, "invalid_field", "unsupported card status");
                }
            };
            crate::operations::card_direct::set_card_status(
                env,
                required("card_id")?,
                status,
                rpc_param(params, "reason"),
            )
        }
        "list_recommendations" => {
            handlers::recommendations::list_for(env, required("language_profile_id")?)
        }
        "resolve_recommendation" => {
            let status = required("status")?;
            if !matches!(status, "accepted" | "dismissed") {
                return api::operation_error(
                    400,
                    "invalid_field",
                    "status must be accepted|dismissed",
                );
            }
            crate::operations::recommendation_ops::resolve(
                env,
                required("recommendation_id")?,
                status,
            )
        }
        "get_card_redesign_candidates" => handlers::recommendations::redesign_candidates_for(
            env,
            required("language_profile_id")?,
        ),
        "create_review_session" => crate::operations::review_ops::create_session(
            env,
            required("language_profile_id")?,
            rpc_param(params, "source").unwrap_or("online"),
        ),
        "get_review_queue" => crate::operations::review_queue::queue(env, required("session_id")?),
        "review_card" => {
            let session_id = required("session_id")?.to_owned();
            let event: handlers::review::ReviewEventParams =
                serde_json::from_value(params.clone())?;
            crate::operations::review_ops::submit_event(
                env,
                &session_id,
                crate::operations::review_ops::ReviewEventInputData {
                    client_operation_id: &event.client_operation_id,
                    card_id: &event.card_id,
                    grade: event.grade,
                    presented_at: &event.presented_at,
                    answered_at: &event.answered_at,
                    response_millis: event.response_millis,
                    answer_revealed: event.answer_revealed,
                    learner_note: event.learner_note.as_deref(),
                    prior_state_hash: &event.prior_state_hash,
                    source: &event.source,
                },
            )
        }
        "undo_review" => crate::operations::review_ops::undo_last(env, required("session_id")?),
        "complete_review_session" => crate::operations::review_ops::complete_session(
            env,
            required("session_id")?,
            u32_param("card_count")?,
            u32_param("review_seconds")?,
        ),
        "evaluate_review_answer" => handlers::review::evaluate(
            registry,
            env,
            required("card_id")?,
            required("learner_answer")?,
        ),
        "get_review_summary" => {
            handlers::review::summary_for(env, required("language_profile_id")?)
        }
        "get_review_forecast" => handlers::review::forecast_for(
            env,
            required("language_profile_id")?,
            params
                .get("days")
                .and_then(serde_json::Value::as_i64)
                .unwrap_or(7),
        ),
        "get_review_retention" => handlers::review::retention_for(
            env,
            required("language_profile_id")?,
            params
                .get("days")
                .and_then(serde_json::Value::as_i64)
                .unwrap_or(28),
        ),
        _ => api::operation_error(404, "method_not_found", "unknown RPC method"),
    }
}

fn download_profile_id(query: Option<&str>) -> Option<String> {
    url::form_urlencoded::parse(query?.as_bytes())
        .find_map(|(key, value)| (key == "language_profile_id").then(|| value.into_owned()))
        .filter(|value| !value.is_empty())
}

fn static_file(assets: &Path, path: &str) -> HttpResponse<Body> {
    if path.contains('%') || path.contains("..") || path.contains('\\') {
        return reply(400, "text/plain", "invalid path");
    }
    let wanted = assets.join(path.trim_start_matches('/'));
    let file = if wanted.is_file() {
        wanted
    } else if Path::new(path).extension().is_none() {
        assets.join("index.html")
    } else {
        return reply(404, "text/plain", "not found");
    };
    let Ok(real) = file.canonicalize() else {
        return reply(404, "text/plain", "not found");
    };
    if !real.starts_with(assets) {
        return reply(403, "text/plain", "forbidden");
    }
    let Ok(bytes) = std::fs::read(&real) else {
        return reply(404, "text/plain", "not found");
    };
    let kind = match real
        .extension()
        .and_then(|suffix| suffix.to_str())
        .unwrap_or_default()
    {
        "html" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "wasm" => "application/wasm",
        "json" | "webmanifest" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "ico" => "image/x-icon",
        _ => "application/octet-stream",
    };
    reply(200, kind, Body::from(bytes))
}

#[axum::debug_handler]
async fn handle(State(state): State<AppState>, request: HttpRequest<Body>) -> HttpResponse<Body> {
    let (head, body) = request.into_parts();
    let host = head
        .headers
        .get("host")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    let expected = format!("127.0.0.1:{}", state.port);
    let alternate = format!("localhost:{}", state.port);
    if host != expected && host != alternate {
        return reply(403, "text/plain", "forbidden host");
    }
    if let Some(origin) = head
        .headers
        .get("origin")
        .and_then(|value| value.to_str().ok())
        && origin != format!("http://{host}")
    {
        return reply(403, "text/plain", "forbidden origin");
    }

    if head.uri.path() == "/rpc" {
        if head.method != HttpMethod::POST {
            return reply(405, "text/plain", "method not allowed");
        }
        if !head
            .headers
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.starts_with("application/json"))
        {
            return reply(415, "text/plain", "JSON required");
        }
        let Ok(bytes) = to_bytes(body, 256 * 1024).await else {
            return reply(413, "text/plain", "request too large");
        };
        let Ok(call) = serde_json::from_slice::<RpcRequest>(&bytes) else {
            return rpc_error(serde_json::Value::Null, -32700, "parse error");
        };
        if call.jsonrpc != "2.0" || call.method.is_empty() {
            return rpc_error(call.id, -32600, "invalid request");
        }
        let id = call.id.clone();
        return match dispatch_rpc(call, &state.env, &state.registry) {
            Ok(result) => rpc_response(id, result),
            Err(error) => {
                if matches!(error, Error::RustError(_)) {
                    eprintln!("RPC error: {error}");
                }
                rpc_operation_error(id, &error)
            }
        };
    }

    if matches!(
        head.uri.path(),
        "/api/v1/export/full" | "/api/v1/export/cards.csv"
    ) {
        if head.method != HttpMethod::GET {
            return reply(405, "text/plain", "method not allowed");
        }
        let Some(profile_id) = download_profile_id(head.uri.query()) else {
            return reply(400, "text/plain", "language_profile_id required");
        };
        let response = if head.uri.path().ends_with("cards.csv") {
            handlers::export::cards_csv_for(&state.env, &profile_id)
        } else {
            handlers::export::full_for(&state.env, &profile_id)
        };
        return match response {
            Ok(download) => HttpResponse::builder()
                .status(200)
                .header("content-type", download.content_type)
                .header(
                    "content-disposition",
                    format!("attachment; filename={}", download.filename),
                )
                .body(Body::from(download.body))
                .unwrap_or_else(|_| HttpResponse::new(Body::empty())),
            Err(error) => {
                eprintln!("export error: {error}");
                reply(500, "text/plain", "export failed")
            }
        };
    }

    if head.method == HttpMethod::GET {
        static_file(&state.assets, head.uri.path())
    } else {
        reply(404, "text/plain", "not found")
    }
}

/// Serve the PWA and JSON-RPC boundary on a loopback socket.
pub async fn serve(
    env: Env,
    registry: Arc<crate::BuiltInPackRegistry>,
    assets: &Path,
    port: u16,
) -> Result<(), String> {
    let assets = assets
        .canonicalize()
        .map_err(|error| format!("assets: {error}"))?;
    if !assets.join("index.html").is_file() {
        return Err("assets directory lacks index.html".into());
    }
    let state = AppState {
        env,
        registry,
        assets,
        port,
    };
    let app = Router::new().fallback(any(handle)).with_state(state);
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port))
        .await
        .map_err(|error| error.to_string())?;
    eprintln!("Lingua Tutor: http://127.0.0.1:{port}");
    axum::serve(listener, app)
        .await
        .map_err(|error| error.to_string())
}

#[cfg(test)]
// Test fixtures intentionally fail fast and use compact tuple snapshots.
#[allow(clippy::unwrap_used, clippy::type_complexity)]
mod tests {
    use super::*;
    use crate::storage;

    fn call(
        env: &Env,
        registry: &crate::BuiltInPackRegistry,
        method: &str,
        params: serde_json::Value,
    ) -> serde_json::Value {
        dispatch_rpc(
            RpcRequest {
                jsonrpc: "2.0".to_owned(),
                id: serde_json::json!("test"),
                method: method.to_owned(),
                params,
            },
            env,
            registry,
        )
        .unwrap()
    }

    #[test]
    fn health_rpc_operation_works() {
        let dir = std::env::temp_dir().join(format!("lingua-rpc-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join("lingua.sqlite3");
        let env = storage::environment(&db).unwrap();
        let registry = crate::built_in_language_packs().unwrap();

        let result = dispatch_rpc(
            RpcRequest {
                jsonrpc: "2.0".to_owned(),
                id: serde_json::json!(1),
                method: "health".to_owned(),
                params: serde_json::json!({}),
            },
            &env,
            &registry,
        )
        .unwrap();

        assert_eq!(result["status"], "ok");
        assert_eq!(result["sqlite_connected"], true);

        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn lesson_review_next_lesson_rpc_cycle_works_on_fresh_sqlite() {
        let dir = std::env::temp_dir().join(format!("lingua-rpc-cycle-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join("lingua.sqlite3");
        let env = storage::environment(&db).unwrap();
        let registry = crate::built_in_language_packs().unwrap();

        let profile = call(
            &env,
            &registry,
            "create_language_profile",
            serde_json::json!({
                "language_pack_id": "es-ES",
                "source_language": "ru"
            }),
        );
        let profile_id = profile["id"].as_str().unwrap();

        let before = call(
            &env,
            &registry,
            "get_next_lesson",
            serde_json::json!({ "language_profile_id": profile_id }),
        );
        assert_eq!(before["lesson_number"], 1);

        let started = call(
            &env,
            &registry,
            "start_lesson",
            serde_json::json!({
                "language_profile_id": profile_id,
                "scenario": "restaurant",
                "planned_goal": "order a simple meal"
            }),
        );
        assert_eq!(started["lesson_number"], 1);
        let lesson_id = started["id"].as_str().unwrap();

        let completed = call(
            &env,
            &registry,
            "complete_lesson",
            serde_json::json!({
                "lesson_id": lesson_id,
                "lesson_result": {
                    "schema_version": "2",
                    "client_operation_id": "op-rpc-cycle-lesson",
                    "language_profile_id": profile_id,
                    "language_pack_id": "es-ES",
                    "language_pack_version": "0.2.0",
                    "lesson": {
                        "scenario": "restaurant",
                        "duration_seconds": 720,
                        "tutor_summary": "Practised ordering a meal.",
                        "next_goal": "ask for the bill"
                    },
                    "metrics": {
                        "speaking_seconds": 300,
                        "unaided_recall_count": 4,
                        "aided_recall_count": 1,
                        "listening_score": 0.8,
                        "avg_answer_millis": 1800,
                        "long_pause_count": 1,
                        "confidence": 0.75
                    },
                    "new_learning_items": [{
                        "client_item_key": "bill",
                        "item_type": "phrase",
                        "target_text": "La cuenta, por favor."
                    }],
                    "evidence": [
                        {
                            "client_item_key": "bill",
                            "kind": "prompted_production",
                            "success": true,
                            "assistance_level": 1,
                            "response_millis": 1800,
                            "probe_phase": "diagnostic",
                            "context": "restaurant:bill",
                            "pronunciation_intelligible": true,
                            "pronunciation_repeat_required": false
                        },
                        {
                            "client_item_key": "bill",
                            "kind": "recognition",
                            "success": true,
                            "assistance_level": 0,
                            "response_millis": 900,
                            "probe_phase": "diagnostic",
                            "context": "restaurant:bill-recognition"
                        }
                    ],
                    "interventions": [{
                        "action": "guided_retrieval",
                        "client_item_key": "bill",
                        "target_capability": "prompted_production",
                        "evidence_index": 0
                    }],
                    "card_mutations": [{
                        "operation": "create",
                        "client_operation_id": "op-rpc-cycle-card",
                        "card": {
                            "intent": "production",
                            "prompt": "Попросите счёт",
                            "answer": "La cuenta, por favor.",
                            "prompt_language": "ru",
                            "creation_reason": "rpc lifecycle test"
                        }
                    }]
                }
            }),
        );
        assert_eq!(completed["lesson_id"], lesson_id);

        let cards = call(
            &env,
            &registry,
            "list_cards",
            serde_json::json!({ "language_profile_id": profile_id }),
        );
        let cards = cards.as_array().unwrap();
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0]["answer"], "La cuenta, por favor.");

        let session = call(
            &env,
            &registry,
            "create_review_session",
            serde_json::json!({
                "language_profile_id": profile_id,
                "source": "online"
            }),
        );
        let session_id = session["id"].as_str().unwrap();
        let queue = call(
            &env,
            &registry,
            "get_review_queue",
            serde_json::json!({ "session_id": session_id }),
        );
        let queued = queue["queue"].as_array().unwrap();
        assert_eq!(queued.len(), 1);
        assert_eq!(queued[0]["answer"], "La cuenta, por favor.");
        let card_id = queued[0]["card_id"].as_str().unwrap();
        let prior_state_hash = queued[0]["prior_state_hash"].as_str().unwrap();
        let at = crate::repositories::to_rfc3339(crate::api::now());

        let graded = call(
            &env,
            &registry,
            "review_card",
            serde_json::json!({
                "session_id": session_id,
                "client_operation_id": "op-rpc-cycle-review",
                "card_id": card_id,
                "grade": 3,
                "presented_at": at,
                "answered_at": at,
                "response_millis": 1500,
                "answer_revealed": true,
                "prior_state_hash": prior_state_hash,
                "source": "online"
            }),
        );
        assert_eq!(graded["card_id"], card_id);
        assert_eq!(graded["duplicate"], serde_json::Value::Null);

        call(
            &env,
            &registry,
            "complete_review_session",
            serde_json::json!({
                "session_id": session_id,
                "card_count": 1,
                "review_seconds": 20
            }),
        );

        let next = call(
            &env,
            &registry,
            "get_next_lesson",
            serde_json::json!({ "language_profile_id": profile_id }),
        );
        assert_eq!(next["lesson_number"], 2);
        assert_eq!(next["previous_scenario"], "restaurant");

        let dashboard = call(
            &env,
            &registry,
            "get_dashboard",
            serde_json::json!({ "language_profile_id": profile_id }),
        );
        assert_eq!(dashboard["next_lesson_number"], 2);
        assert_eq!(dashboard["reviews_today"], 1);

        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    #[ignore = "representative performance gate; run with --release --ignored --nocapture"]
    fn representative_single_learner_latency_stays_within_budget() {
        use std::time::{Duration, Instant};

        const CARD_COUNT: u32 = 5_000;
        const REVIEW_EVENT_COUNT: u32 = 20_000;
        const SAMPLES: usize = 30;

        let dir = std::env::temp_dir().join(format!("lingua-perf-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join("lingua.sqlite3");
        let env = storage::environment(&db).unwrap();
        let registry = crate::built_in_language_packs().unwrap();

        let profile = call(
            &env,
            &registry,
            "create_language_profile",
            serde_json::json!({
                "language_pack_id": "es-ES",
                "source_language": "ru"
            }),
        );
        let profile_id = profile["id"].as_str().unwrap().to_owned();
        let session = call(
            &env,
            &registry,
            "create_review_session",
            serde_json::json!({
                "language_profile_id": profile_id,
                "source": "online"
            }),
        );
        let session_id = session["id"].as_str().unwrap().to_owned();
        let at = crate::repositories::to_rfc3339(crate::api::now());
        let due_at = "2000-01-01T00:00:00Z";

        {
            let mut database = env.connection().unwrap();
            let transaction = database.transaction().unwrap();
            {
                let mut card = transaction
                    .prepare(
                        "INSERT INTO cards
                         (id,learner_language_profile_id,language_pack_id,language_pack_version,
                          intent,status,prompt,prompt_language,answer,answer_language,
                          accepted_answers_json,answer_evaluation_mode,creation_reason,
                          content_hash,created_by,created_at,updated_at)
                         VALUES(?1,?2,'es-ES','0.2.0','production','new',?3,'ru',?4,'es',
                                '[]','canonical','performance fixture',?5,'system',?6,?6)",
                    )
                    .unwrap();
                let mut schedule = transaction
                    .prepare(
                        "INSERT INTO card_schedule_state
                         (card_id,algorithm,algorithm_version,difficulty,stability,last_review_at,
                          due_at,scheduled_interval_seconds,lapse_count,review_count,state_version,
                          state_hash,updated_at)
                         VALUES(?1,'fsrs','bench',5.0,10.0,NULL,?2,0,0,0,1,'bench-hash',?3)",
                    )
                    .unwrap();
                for index in 0..CARD_COUNT {
                    let card_id = format!("perf-card-{index:05}");
                    card.execute(rusqlite::params![
                        card_id,
                        profile_id,
                        format!("prompt {index}"),
                        format!("answer {index}"),
                        format!("hash-{index}"),
                        at,
                    ])
                    .unwrap();
                    schedule
                        .execute(rusqlite::params![card_id, due_at, at])
                        .unwrap();
                }
            }
            {
                let mut event = transaction
                    .prepare(
                        "INSERT INTO review_events
                         (id,card_id,review_session_id,client_operation_id,presented_at,answered_at,
                          grade,response_millis,answer_revealed,learner_note,prior_state_hash,
                          result_state_hash,algorithm_version,source,compensates_event_id,created_at)
                         VALUES(?1,?2,?3,?4,?5,?5,3,1500,1,NULL,'before','after','bench',
                                'online',NULL,?5)",
                    )
                    .unwrap();
                for index in 0..REVIEW_EVENT_COUNT {
                    let card_id = format!("perf-card-{:05}", index % CARD_COUNT);
                    event
                        .execute(rusqlite::params![
                            format!("perf-event-{index:05}"),
                            card_id,
                            session_id,
                            format!("perf-op-{index:05}"),
                            at,
                        ])
                        .unwrap();
                }
            }
            transaction.commit().unwrap();
        }

        // Structural guard: the due-card query must stay on indexed access.
        {
            let database = env.connection().unwrap();
            let mut plan = database
                .prepare(
                    "EXPLAIN QUERY PLAN
                     SELECT COUNT(*) FROM card_schedule_state s
                     JOIN cards c ON c.id=s.card_id
                     WHERE c.learner_language_profile_id=?1
                       AND c.status IN ('new','learning','review','relearning')
                       AND s.due_at <= ?2",
                )
                .unwrap();
            let details = plan
                .query_map(rusqlite::params![profile_id, at], |row| {
                    row.get::<_, String>(3)
                })
                .unwrap()
                .collect::<rusqlite::Result<Vec<_>>>()
                .unwrap();
            assert!(
                details.iter().any(|detail| detail.contains("USING INDEX")),
                "due-card query lost indexed access: {details:?}"
            );
        }

        let dashboard_params = serde_json::json!({ "language_profile_id": profile_id });
        let queue_params = serde_json::json!({ "session_id": session_id });

        // Warm caches and prepared SQLite pages before collecting samples.
        let dashboard = call(&env, &registry, "get_dashboard", dashboard_params.clone());
        assert_eq!(dashboard["reviews_today"], REVIEW_EVENT_COUNT);
        let queue = call(&env, &registry, "get_review_queue", queue_params.clone());
        assert_eq!(queue["queue"].as_array().unwrap().len(), 8);

        fn sample(mut operation: impl FnMut(), count: usize) -> (Duration, Duration, Duration) {
            let mut values = Vec::with_capacity(count);
            for _ in 0..count {
                let started = Instant::now();
                operation();
                values.push(started.elapsed());
            }
            values.sort_unstable();
            let p50 = values[count / 2];
            let p95 = values[((count * 95).div_ceil(100)).saturating_sub(1)];
            let max = *values.last().unwrap();
            (p50, p95, max)
        }

        let (dashboard_p50, dashboard_p95, dashboard_max) = sample(
            || {
                let result = call(&env, &registry, "get_dashboard", dashboard_params.clone());
                std::hint::black_box(result);
            },
            SAMPLES,
        );
        let (queue_p50, queue_p95, queue_max) = sample(
            || {
                let result = call(&env, &registry, "get_review_queue", queue_params.clone());
                std::hint::black_box(result);
            },
            SAMPLES,
        );

        eprintln!(
            "representative latency: cards={CARD_COUNT}, events={REVIEW_EVENT_COUNT}; \
             dashboard p50={dashboard_p50:?} p95={dashboard_p95:?} max={dashboard_max:?}; \
             review_queue p50={queue_p50:?} p95={queue_p95:?} max={queue_max:?}"
        );

        assert!(
            dashboard_p95 < Duration::from_millis(500),
            "dashboard p95 {dashboard_p95:?} exceeds 500 ms target"
        );
        assert!(
            queue_p95 < Duration::from_millis(300),
            "review queue p95 {queue_p95:?} exceeds 300 ms target"
        );

        std::fs::remove_dir_all(dir).unwrap();
    }
}

// FUZZ SUPPORT BELOW: excluded from production line coverage.

/// Drive the real HTTP handler without binding a socket.
#[cfg(feature = "fuzzing")]
#[allow(clippy::expect_used, missing_docs)]
pub(crate) fn fuzz_boundary(
    env: &Env,
    registry: &crate::BuiltInPackRegistry,
    dir: &Path,
    args: &serde_json::Value,
    data: &[u8],
) {
    let _ = registry;
    let assets = dir.join("assets");
    std::fs::create_dir_all(&assets).expect("assets dir");
    for ext in [
        "html", "css", "js", "wasm", "json", "svg", "png", "ico", "txt",
    ] {
        std::fs::write(assets.join(format!("index.{ext}")), b"fuzz asset").expect("asset");
    }
    let state = AppState {
        env: env.clone(),
        registry: Arc::new(crate::built_in_language_packs().expect("registry")),
        assets: assets.canonicalize().expect("assets path"),
        port: 7777,
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    runtime.block_on(async {
        let methods = [
            "health",
            "list_language_profiles",
            "list_language_packs",
            "get_language_pack",
            "create_language_profile",
            "update_profile",
            "get_dashboard",
            "get_learning_state",
            "get_next_lesson",
            "start_lesson",
            "list_lessons",
            "get_lesson",
            "complete_lesson",
            "validate_lesson_result",
            "apply_lesson_result",
            "list_error_targets",
            "get_error_target",
            "complete_error_recheck",
            "create_listening_attempt",
            "get_tutor_context",
            "list_cards",
            "get_card",
            "patch_card",
            "set_card_status",
            "list_recommendations",
            "resolve_recommendation",
            "get_card_redesign_candidates",
            "create_review_session",
            "get_review_queue",
            "review_card",
            "undo_review",
            "complete_review_session",
            "evaluate_review_answer",
            "get_review_summary",
            "get_review_forecast",
            "get_review_retention",
            "unknown",
        ];
        for method in methods {
            for params in [args.clone(), serde_json::json!({})] {
                let body =
                    serde_json::json!({"jsonrpc":"2.0","id":9,"method":method,"params":params})
                        .to_string();
                let request = HttpRequest::builder()
                    .method("POST")
                    .uri("/rpc")
                    .header("host", "127.0.0.1:7777")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .expect("request");
                let response = handle(State(state.clone()), request).await;
                let bytes = to_bytes(response.into_body(), 1024 * 1024)
                    .await
                    .expect("response");
                let json: serde_json::Value =
                    serde_json::from_slice(&bytes).expect("RPC response JSON");
                assert_eq!(json["id"], 9);
            }
        }
        for (method, path, host, origin, ctype, body, expected) in [
            ("GET", "/rpc", "evil", "", "", b"".as_slice(), 403),
            (
                "POST",
                "/rpc",
                "localhost:7777",
                "http://evil",
                "application/json",
                b"",
                403,
            ),
            ("GET", "/rpc", "localhost:7777", "", "", b"", 405),
            ("POST", "/rpc", "localhost:7777", "", "text/plain", b"", 415),
            (
                "POST",
                "/rpc",
                "localhost:7777",
                "",
                "application/json",
                b"{",
                200,
            ),
            (
                "POST",
                "/rpc",
                "localhost:7777",
                "",
                "application/json",
                b"{\"jsonrpc\":\"1.0\",\"id\":1,\"method\":\"health\"}",
                200,
            ),
            (
                "GET",
                "/api/v1/export/full",
                "localhost:7777",
                "",
                "",
                b"",
                400,
            ),
            (
                "POST",
                "/api/v1/export/full",
                "localhost:7777",
                "",
                "",
                b"",
                405,
            ),
            (
                "GET",
                "/api/v1/export/full?language_profile_id=missing",
                "localhost:7777",
                "",
                "",
                b"",
                200,
            ),
            (
                "GET",
                "/api/v1/export/cards.csv?language_profile_id=missing",
                "localhost:7777",
                "",
                "",
                b"",
                200,
            ),
            ("POST", "/other", "localhost:7777", "", "", b"", 404),
        ] {
            let mut builder = HttpRequest::builder()
                .method(method)
                .uri(path)
                .header("host", host)
                .header("content-type", ctype);
            if !origin.is_empty() {
                builder = builder.header("origin", origin);
            }
            let response = handle(
                State(state.clone()),
                builder.body(Body::from(body.to_vec())).expect("request"),
            )
            .await;
            assert_eq!(response.status().as_u16(), expected);
        }
        for path in [
            "/",
            "/route",
            "/missing.js",
            "/%2e",
            "/../escape",
            "/index.html",
            "/index.css",
            "/index.js",
            "/index.wasm",
            "/index.json",
            "/index.svg",
            "/index.png",
            "/index.ico",
            "/index.txt",
        ] {
            let request = HttpRequest::builder()
                .uri(path)
                .header("host", "localhost:7777")
                .body(Body::empty())
                .expect("request");
            let _ = handle(State(state.clone()), request).await;
        }
        let request = HttpRequest::builder()
            .method("POST")
            .uri("/rpc")
            .header("host", "localhost:7777")
            .header("content-type", "application/json")
            .body(Body::from(data.to_vec()))
            .expect("fuzz HTTP request");
        let _ = handle(State(state.clone()), request).await;
    });
}
