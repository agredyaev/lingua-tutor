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

#[derive(Clone)]
struct AppState {
    env: Env,
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

fn dispatch_rpc(call: RpcRequest, env: &Env) -> crate::Result<serde_json::Value> {
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
    let u8_param = |name: &str| {
        u32_param(name).and_then(|value| {
            u8::try_from(value).map_err(|_| {
                crate::Error::operation(
                    400,
                    "invalid_params",
                    format!("RPC param out of range: {name}"),
                    serde_json::json!({}),
                )
            })
        })
    };

    match call.method.as_str() {
        "health" => handlers::records::health(env),
        "list_language_profiles" => handlers::profiles::list_profiles(env),
        "create_language_profile" => crate::operations::profile_write::create_profile(
            env,
            required("target_locale")?,
            required("source_language")?,
        ),
        "get_dashboard" => handlers::records::dashboard_for(env, required("language_profile_id")?),
        "get_learning_state" => {
            handlers::records::learning_state_for(env, required("language_profile_id")?)
        }
        "get_next_lesson" => {
            handlers::records::next_lesson_for(env, required("language_profile_id")?)
        }
        "list_lessons" => {
            handlers::lessons::list_lessons_for(env, required("language_profile_id")?)
        }
        "validate_lesson_result" => handlers::lessons::validate_import_text(required("text")?, env),
        "apply_lesson_result" => handlers::lessons::apply_import_text(
            required("text")?,
            required("validation_token")?,
            env,
        ),
        "list_error_targets" => handlers::records::list_error_targets_for(
            env,
            required("language_profile_id")?,
            rpc_param(params, "status"),
        ),
        "complete_error_recheck" => crate::operations::error::complete_error_recheck(
            env,
            required("error_target_id")?,
            required("recheck_id")?,
            bool_param("passed")?,
            u8_param("assistance_level")?,
            required("context")?,
            bool_param("repair_applied")?,
            rpc_param(params, "repair_action"),
        ),
        "get_tutor_context" => {
            handlers::records::tutor_context_for(env, required("language_profile_id")?)
        }
        "list_cards" => handlers::cards::list_for(
            env,
            required("language_profile_id")?,
            rpc_param(params, "status"),
            rpc_param(params, "intent"),
        ),
        "get_card" => handlers::cards::get(env, required("card_id")?),
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
        return match dispatch_rpc(call, &state.env) {
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
pub async fn serve(env: Env, assets: &Path, port: u16) -> Result<(), String> {
    let assets = assets
        .canonicalize()
        .map_err(|error| format!("assets: {error}"))?;
    if !assets.join("index.html").is_file() {
        return Err("assets directory lacks index.html".into());
    }
    let state = AppState { env, assets, port };
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

    fn call(env: &Env, method: &str, params: serde_json::Value) -> serde_json::Value {
        dispatch_rpc(
            RpcRequest {
                jsonrpc: "2.0".to_owned(),
                id: serde_json::json!("test"),
                method: method.to_owned(),
                params,
            },
            env,
        )
        .unwrap()
    }

    #[test]
    fn health_rpc_operation_works() {
        let dir = std::env::temp_dir().join(format!("lingua-rpc-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join("lingua.sqlite3");
        let env = storage::environment(&db).unwrap();
        let result = dispatch_rpc(
            RpcRequest {
                jsonrpc: "2.0".to_owned(),
                id: serde_json::json!(1),
                method: "health".to_owned(),
                params: serde_json::json!({}),
            },
            &env,
        )
        .unwrap();

        assert_eq!(result["status"], "ok");
        assert_eq!(result["sqlite_connected"], true);

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
        let profile = call(
            &env,
            "create_language_profile",
            serde_json::json!({
                "target_locale": "es-ES",
                "source_language": "ru"
            }),
        );
        let profile_id = profile["id"].as_str().unwrap().to_owned();
        let session = call(
            &env,
            "create_review_session",
            serde_json::json!({
                "language_profile_id": profile_id,
                "source": "online"
            }),
        );
        let session_id = session["id"].as_str().unwrap().to_owned();
        let at = crate::sqlite::to_rfc3339(crate::api::now());
        let due_at = "2000-01-01T00:00:00Z";

        {
            let mut database = env.connection().unwrap();
            let transaction = database.transaction().unwrap();
            {
                let mut card = transaction
                    .prepare(
                        "INSERT INTO cards
                         (id,learner_language_profile_id,
                          intent,status,prompt,prompt_language,answer,answer_language,
                          accepted_answers_json,creation_reason,
                          content_hash,created_by,created_at,updated_at)
                         VALUES(?1,?2,'production','new',?3,'ru',?4,'es-ES',
                                '[]','performance fixture',?5,'system',?6,?6)",
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
        let dashboard = call(&env, "get_dashboard", dashboard_params.clone());
        assert_eq!(dashboard["reviews_today"], REVIEW_EVENT_COUNT);
        let queue = call(&env, "get_review_queue", queue_params.clone());
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
                let result = call(&env, "get_dashboard", dashboard_params.clone());
                std::hint::black_box(result);
            },
            SAMPLES,
        );
        let (queue_p50, queue_p95, queue_max) = sample(
            || {
                let result = call(&env, "get_review_queue", queue_params.clone());
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
#[cfg(fuzzing)]
#[allow(clippy::expect_used, missing_docs)]
pub(crate) fn fuzz_boundary(env: &Env, dir: &Path, args: &serde_json::Value, data: &[u8]) {
    let assets = dir.join("assets");
    std::fs::create_dir_all(&assets).expect("assets dir");
    for ext in [
        "html", "css", "js", "wasm", "json", "svg", "png", "ico", "txt",
    ] {
        std::fs::write(assets.join(format!("index.{ext}")), b"fuzz asset").expect("asset");
    }
    let state = AppState {
        env: env.clone(),
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
            "create_language_profile",
            "get_dashboard",
            "get_learning_state",
            "get_next_lesson",
            "list_lessons",
            "validate_lesson_result",
            "apply_lesson_result",
            "list_error_targets",
            "complete_error_recheck",
            "get_tutor_context",
            "list_cards",
            "get_card",
            "set_card_status",
            "create_review_session",
            "get_review_queue",
            "review_card",
            "undo_review",
            "complete_review_session",
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
