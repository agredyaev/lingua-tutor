//! Cloudflare Worker API entry point: routing for `/api/v1`.
//! Owns HTTP concerns only; domain rules live in `application` (02-workspace.md §2).

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
    Router::new()
        .get("/api/v1/health", |_req, _ctx| {
            Response::from_json(&shared_schema::HealthResponse::ok(env!(
                "CARGO_PKG_VERSION"
            )))
        })
        .run(req, env)
        .await
}
