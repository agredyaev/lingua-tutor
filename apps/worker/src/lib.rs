//! Cloudflare Worker API entry point: routing for `/api/v1`.
//! Owns HTTP concerns only; domain rules live in `application` (02-workspace.md §2).

use worker::{Context, Env, Request, Response, Result, Router, event};

/// Routes every incoming request through the `/api/v1` router.
#[event(fetch)]
async fn fetch(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    Router::new()
        .get("/api/v1/health", |_req, _ctx| {
            Response::from_json(&shared_schema::HealthResponse::ok(env!(
                "CARGO_PKG_VERSION"
            )))
        })
        .run(req, env)
        .await
}
