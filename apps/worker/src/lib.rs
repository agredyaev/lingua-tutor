//! Cloudflare Worker API entry point: routing for `/api/v1`.
//! Owns HTTP concerns only; domain rules live in `application` (02-workspace.md §2).

pub mod api;
pub mod handlers;
pub mod repositories;

use api::error_response;
use language_pack_registry::{LanguagePackRegistry, RegistryError};
use worker::{Context, Env, Request, Response, Result, Router, event};

/// Production pack composition (06-language-packs.md §4). Synthetic test
/// pack is intentionally absent from production builds.
///
/// # Errors
///
/// [`RegistryError`] when a pack's contract version mismatches or IDs
/// collide — startup must fail clearly, not limp.
pub fn built_in_language_packs() -> std::result::Result<LanguagePackRegistry, RegistryError> {
    LanguagePackRegistry::new([language_pack_es_es::register()])
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn production_composition_registers_es_es_only() {
        let registry = built_in_language_packs().unwrap();
        assert!(registry.get("es-ES").is_some(), "es-ES selectable");
        assert!(
            registry.get("zz-ZZ").is_none(),
            "synthetic pack must not ship"
        );
        assert_eq!(registry.manifests().count(), 1);
    }
}

/// Routes every incoming request through the `/api/v1` router.
#[event(fetch)]
async fn fetch(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    let registry = match built_in_language_packs() {
        Ok(registry) => registry,
        // Startup must fail clearly on contract mismatch (06 §4).
        Err(e) => return error_response(500, "pack_registry", &e.to_string()),
    };
    Router::with_data(registry)
        .get("/api/v1/health", |_req, _ctx| {
            Response::from_json(&shared_schema::HealthResponse::ok(env!(
                "CARGO_PKG_VERSION"
            )))
        })
        .get("/api/v1/language-packs", |_req, ctx| {
            handlers::profiles::list_packs(&ctx.data)
        })
        .get("/api/v1/language-packs/:pack_id", |_req, ctx| {
            let pack_id = ctx.param("pack_id").cloned().unwrap_or_default();
            handlers::profiles::get_pack(&ctx.data, &pack_id)
        })
        .get_async("/api/v1/language-profiles", |_req, ctx| async move {
            handlers::profiles::list_profiles(&ctx.env).await
        })
        .post_async("/api/v1/language-profiles", |req, ctx| async move {
            handlers::profiles::create_profile(&ctx.data, req, &ctx.env).await
        })
        .patch_async(
            "/api/v1/language-profiles/:profile_id",
            |req, ctx| async move {
                let profile_id = ctx.param("profile_id").cloned().unwrap_or_default();
                handlers::profiles::patch_profile(req, &ctx.env, &profile_id).await
            },
        )
        .get_async("/api/v1/learning-state", |req, ctx| async move {
            handlers::records::learning_state(&req, &ctx.env).await
        })
        .get_async("/api/v1/next-lesson", |req, ctx| async move {
            handlers::records::next_lesson(&req, &ctx.env).await
        })
        .post_async("/api/v1/lessons/start", |req, ctx| async move {
            handlers::lessons::start_lesson(req, &ctx.env).await
        })
        .post_async(
            "/api/v1/lessons/:lesson_id/complete",
            |req, ctx| async move {
                let lesson_id = ctx.param("lesson_id").cloned().unwrap_or_default();
                handlers::lessons::complete_lesson(req, &ctx.env, &lesson_id).await
            },
        )
        .get_async("/api/v1/lessons", |req, ctx| async move {
            handlers::lessons::list_lessons(&req, &ctx.env).await
        })
        .get_async("/api/v1/lessons/:lesson_id", |_req, ctx| async move {
            let lesson_id = ctx.param("lesson_id").cloned().unwrap_or_default();
            handlers::lessons::get_lesson(&ctx.env, &lesson_id).await
        })
        .get_async("/api/v1/error-targets", |req, ctx| async move {
            handlers::records::list_error_targets(&req, &ctx.env).await
        })
        .get_async("/api/v1/error-targets/:id", |_req, ctx| async move {
            let id = ctx.param("id").cloned().unwrap_or_default();
            handlers::records::get_error_target(&ctx.env, &id).await
        })
        .post_async(
            "/api/v1/error-targets/:id/rechecks/:recheck_id/complete",
            |req, ctx| async move {
                let id = ctx.param("id").cloned().unwrap_or_default();
                let recheck_id = ctx.param("recheck_id").cloned().unwrap_or_default();
                handlers::records::complete_recheck(req, &ctx.env, &id, &recheck_id).await
            },
        )
        .post_async("/api/v1/listening-attempts", |req, ctx| async move {
            handlers::records::create_listening_attempt(req, &ctx.env).await
        })
        .get_async("/api/v1/export/tutor-context", |req, ctx| async move {
            handlers::records::tutor_context(&ctx.data, &req, &ctx.env).await
        })
        .post_async(
            "/api/v1/import/lesson-result/validate",
            |req, _ctx| async move { handlers::lessons::validate_import(req).await },
        )
        .post_async(
            "/api/v1/import/lesson-result/apply",
            |req, ctx| async move { handlers::lessons::apply_import(req, &ctx.env).await },
        )
        .run(req, env)
        .await
}
