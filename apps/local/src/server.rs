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
    registry: Arc<language_pack_registry::LanguagePackRegistry>,
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
    registry: &language_pack_registry::LanguagePackRegistry,
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
        "create_language_profile" => handlers::profiles::create_profile(
            registry,
            env,
            required("language_pack_id")?,
            required("source_language")?,
        ),
        "update_profile" => {
            let profile_id = required("language_profile_id")?.to_owned();
            let patch: handlers::profiles::PatchProfileParams =
                serde_json::from_value(params.clone())?;
            handlers::profiles::patch_profile(env, &profile_id, patch)
        }
        "get_dashboard" => handlers::records::dashboard_for(env, required("language_profile_id")?),
        "get_learning_state" => {
            handlers::records::learning_state_for(env, required("language_profile_id")?)
        }
        "get_next_lesson" => {
            handlers::records::next_lesson_for(env, required("language_profile_id")?)
        }
        "start_lesson" => handlers::lessons::start_lesson_operation(
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
        "complete_error_recheck" => handlers::records::complete_recheck(
            env,
            required("error_target_id")?,
            required("recheck_id")?,
            bool_param("passed")?,
        ),
        "create_listening_attempt" => {
            let attempt: handlers::records::ListeningAttemptParams =
                serde_json::from_value(params.clone())?;
            handlers::records::create_listening_attempt(env, attempt)
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
            handlers::cards::patch(env, &card_id, patch)
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
            handlers::cards::set_status_with_reason(
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
            handlers::recommendations::resolve(env, required("recommendation_id")?, status)
        }
        "get_card_redesign_candidates" => handlers::recommendations::redesign_candidates_for(
            env,
            required("language_profile_id")?,
        ),
        "create_review_session" => handlers::review::create_session_for(
            env,
            required("language_profile_id")?,
            rpc_param(params, "source").unwrap_or("online"),
        ),
        "get_review_queue" => handlers::review::queue(env, required("session_id")?),
        "review_card" => {
            let session_id = required("session_id")?.to_owned();
            let event: handlers::review::ReviewEventParams =
                serde_json::from_value(params.clone())?;
            handlers::review::submit_event_params(env, &session_id, event)
        }
        "undo_review" => handlers::review::undo_last(env, required("session_id")?),
        "complete_review_session" => handlers::review::complete_session(
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
    registry: Arc<language_pack_registry::LanguagePackRegistry>,
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
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::storage;

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
}
