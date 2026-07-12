//! Packs + language profiles (08-http-api.md §2).

use crate::api::{db, error_response, new_id, now};
use crate::repositories::{packs, profiles, to_rfc3339};
use application::validate_profile_creation;
use serde::Deserialize;
use worker::{Env, Request, Response, Result};

/// `GET /api/v1/language-packs` — registry manifests (source of truth).
pub fn list_packs(registry: &language_pack_registry::LanguagePackRegistry) -> Result<Response> {
    let manifests: Vec<_> = registry.manifests().collect();
    Response::from_json(&manifests)
}

/// `GET /api/v1/language-packs/{pack_id}`.
pub fn get_pack(
    registry: &language_pack_registry::LanguagePackRegistry,
    pack_id: &str,
) -> Result<Response> {
    match registry.get(pack_id) {
        Some(pack) => Response::from_json(pack.manifest()),
        None => error_response(404, "not_found", "unknown language pack"),
    }
}

/// `GET /api/v1/language-profiles`.
pub async fn list_profiles(env: &Env) -> Result<Response> {
    let rows: Vec<profiles::LanguageProfileRow> = db(env)?
        .prepare(profiles::SELECT_LANGUAGE_PROFILES)
        .all()
        .await?
        .results()?;
    let domain: Vec<_> = rows
        .into_iter()
        .filter_map(|r| r.into_domain().ok())
        .collect();
    Response::from_json(&domain)
}

#[derive(Deserialize)]
struct CreateProfileBody {
    language_pack_id: String,
    source_language: String,
}

/// `POST /api/v1/language-profiles` — validates against the pack manifest,
/// persists pack metadata + learner row on first use.
pub async fn create_profile(
    registry: &language_pack_registry::LanguagePackRegistry,
    mut req: Request,
    env: &Env,
) -> Result<Response> {
    let Ok(body) = req.json::<CreateProfileBody>().await else {
        return error_response(
            400,
            "invalid_body",
            "expected language_pack_id + source_language",
        );
    };
    let Some(pack) = registry.get(&body.language_pack_id) else {
        return error_response(404, "not_found", "unknown language pack");
    };
    let manifest = pack.manifest();
    if let Err(e) = validate_profile_creation(manifest, &body.source_language) {
        return error_response(409, "pack_incompatible", &e.to_string());
    }

    let database = db(env)?;
    let at = to_rfc3339(now());
    let learner_id = "learner-1"; // single-owner app (00-overview.md §1)
    let pack_row = packs::PackRow::from_manifest(manifest)
        .map_err(|e| worker::Error::RustError(e.to_string()))?;
    let profile_id = new_id("llp");

    let statements = vec![
        database.prepare(packs::UPSERT_PACK).bind(&[
            pack_row.id.into(),
            pack_row.package_name.into(),
            pack_row.version.into(),
            pack_row.contract_version.into(),
            pack_row.target_language.into(),
            pack_row.target_locale.into(),
            pack_row.display_name.into(),
            pack_row.capabilities_json.into(),
            at.clone().into(),
        ])?,
        database.prepare(profiles::UPSERT_LEARNER).bind(&[
            learner_id.into(),
            "Europe/Madrid".into(),
            2.into(),
            20.into(),
            8.into(),
            0.90.into(),
            12.into(),
            at.clone().into(),
        ])?,
        database.prepare(profiles::INSERT_LANGUAGE_PROFILE).bind(&[
            profile_id.clone().into(),
            learner_id.into(),
            manifest.id.clone().into(),
            body.source_language.clone().into(),
            manifest.target_locale.clone().into(),
            worker::wasm_bindgen::JsValue::NULL,
            0.into(),
            "active".into(),
            1.into(),
            "{}".into(),
            at.into(),
        ])?,
    ];
    if let Err(e) = database.batch(statements).await {
        let text = e.to_string();
        if text.contains("UNIQUE") {
            return error_response(409, "profile_exists", "active profile for this pack exists");
        }
        return Err(e);
    }
    Response::from_json(&serde_json::json!({ "id": profile_id })).map(|r| r.with_status(201))
}

#[derive(Deserialize)]
struct PatchProfileBody {
    estimated_level: Option<String>,
    status: Option<String>,
    is_default: Option<bool>,
    preferences: Option<serde_json::Value>,
}

/// `PATCH /api/v1/language-profiles/{profile_id}`.
pub async fn patch_profile(mut req: Request, env: &Env, profile_id: &str) -> Result<Response> {
    let Ok(body) = req.json::<PatchProfileBody>().await else {
        return error_response(400, "invalid_body", "malformed patch body");
    };
    let database = db(env)?;
    let Some(row) = fetch_profile(&database, profile_id).await? else {
        return error_response(404, "not_found", "unknown profile");
    };
    let Ok(mut domain) = row.into_domain() else {
        return error_response(500, "corrupt_row", "stored profile is invalid");
    };
    if let Some(level) = body.estimated_level {
        domain.estimated_level = Some(level);
    }
    if let Some(status) = body.status {
        match serde_json::from_value(serde_json::Value::String(status)) {
            Ok(parsed) => domain.status = parsed,
            Err(_) => return error_response(400, "invalid_field", "status outside closed set"),
        }
    }
    if let Some(default) = body.is_default {
        domain.is_default = default;
    }
    if let Some(preferences) = body.preferences {
        domain.preferences = preferences;
    }
    let back = profiles::LanguageProfileRow::from_domain(&domain);
    database
        .prepare(profiles::UPDATE_LANGUAGE_PROFILE)
        .bind(&[
            back.id.into(),
            back.estimated_level
                .map_or(worker::wasm_bindgen::JsValue::NULL, Into::into),
            back.status.into(),
            back.is_default.into(),
            back.preferences_json.into(),
            back.current_lesson_number.into(),
            to_rfc3339(now()).into(),
        ])?
        .run()
        .await?;
    Response::from_json(&domain)
}

/// Loads one profile row.
pub async fn fetch_profile(
    database: &worker::D1Database,
    profile_id: &str,
) -> Result<Option<profiles::LanguageProfileRow>> {
    database
        .prepare(profiles::SELECT_LANGUAGE_PROFILE)
        .bind(&[profile_id.into()])?
        .first::<profiles::LanguageProfileRow>(None)
        .await
}
