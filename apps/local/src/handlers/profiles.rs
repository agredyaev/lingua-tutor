//! Language-pack and learner-profile operations.

use crate::api::{new_id, now, operation_error};
use crate::repositories::{packs, parse_enum, profiles, to_rfc3339};
use crate::{Env, Result};
use application::validate_profile_creation;
use rusqlite::{OptionalExtension as _, params};
use serde::Deserialize;

pub(crate) fn list_packs(
    registry: &language_pack_registry::LanguagePackRegistry,
) -> Result<serde_json::Value> {
    let manifests: Vec<_> = registry.manifests().collect();
    crate::json(&manifests)
}

pub(crate) fn get_pack(
    registry: &language_pack_registry::LanguagePackRegistry,
    pack_id: &str,
) -> Result<serde_json::Value> {
    match registry.get(pack_id) {
        Some(pack) => crate::json(pack.manifest()),
        None => operation_error(404, "not_found", "unknown language pack"),
    }
}

pub(crate) fn list_profiles(env: &Env) -> Result<serde_json::Value> {
    let connection = env.connection()?;
    let mut statement = connection.prepare(profiles::SELECT_LANGUAGE_PROFILES)?;
    let rows = statement
        .query_map([], profiles::LanguageProfileRow::from_row)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let domain: Vec<_> = rows
        .into_iter()
        .filter_map(|row| row.into_domain().ok())
        .collect();
    crate::json(&domain)
}

pub(crate) fn create_profile(
    registry: &language_pack_registry::LanguagePackRegistry,
    env: &Env,
    language_pack_id: &str,
    source_language: &str,
) -> Result<serde_json::Value> {
    let Some(pack) = registry.get(language_pack_id) else {
        return operation_error(404, "not_found", "unknown language pack");
    };
    let manifest = pack.manifest();
    if let Err(error) = validate_profile_creation(manifest, source_language) {
        return operation_error(409, "pack_incompatible", &error.to_string());
    }

    let at = to_rfc3339(now());
    let learner_id = "learner-1";
    let pack_row = packs::PackRow::from_manifest(manifest)
        .map_err(|error| crate::Error::RustError(error.to_string()))?;
    let profile_id = new_id("llp");

    let mut connection = env.connection()?;
    let transaction =
        connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let result = (|| -> rusqlite::Result<()> {
        transaction.execute(
            packs::UPSERT_PACK,
            params![
                pack_row.id,
                pack_row.package_name,
                pack_row.version,
                pack_row.contract_version,
                pack_row.target_language,
                pack_row.target_locale,
                pack_row.display_name,
                pack_row.capabilities_json,
                at,
            ],
        )?;
        transaction.execute(
            profiles::UPSERT_LEARNER,
            params![learner_id, "Europe/Madrid", 2, 20, 8, 0.90, 12, at],
        )?;
        transaction.execute(
            profiles::INSERT_LANGUAGE_PROFILE,
            params![
                profile_id,
                learner_id,
                manifest.id,
                source_language,
                manifest.target_locale,
                Option::<String>::None,
                0,
                "active",
                1,
                "{}",
                at,
            ],
        )?;
        Ok(())
    })();
    if let Err(error) = result {
        let text = error.to_string();
        if text.contains("UNIQUE") {
            return operation_error(409, "profile_exists", "active profile for this pack exists");
        }
        return Err(error.into());
    }
    transaction.commit()?;
    crate::json(&serde_json::json!({ "id": profile_id }))
}

#[derive(Deserialize)]
pub(crate) struct PatchProfileParams {
    pub estimated_level: Option<String>,
    pub status: Option<String>,
    pub is_default: Option<bool>,
    pub preferences: Option<serde_json::Value>,
}

pub(crate) fn patch_profile(
    env: &Env,
    profile_id: &str,
    body: PatchProfileParams,
) -> Result<serde_json::Value> {
    let connection = env.connection()?;
    let Some(row) = fetch_profile(&connection, profile_id)? else {
        return operation_error(404, "not_found", "unknown profile");
    };
    let Ok(mut domain) = row.into_domain() else {
        return operation_error(500, "corrupt_row", "stored profile is invalid");
    };
    if let Some(level) = body.estimated_level {
        domain.estimated_level = Some(level);
    }
    if let Some(status) = body.status {
        match parse_enum("status", &status) {
            Ok(parsed) => domain.status = parsed,
            Err(_) => return operation_error(400, "invalid_field", "status outside closed set"),
        }
    }
    if let Some(default) = body.is_default {
        domain.is_default = default;
    }
    if let Some(preferences) = body.preferences {
        domain.preferences = preferences;
    }
    let row = profiles::LanguageProfileRow::from_domain(&domain);
    connection.execute(
        profiles::UPDATE_LANGUAGE_PROFILE,
        params![
            row.id,
            row.estimated_level,
            row.status,
            row.is_default,
            row.preferences_json,
            row.current_lesson_number,
            to_rfc3339(now()),
        ],
    )?;
    crate::json(&domain)
}

pub(crate) fn fetch_profile(
    connection: &rusqlite::Connection,
    profile_id: &str,
) -> Result<Option<profiles::LanguageProfileRow>> {
    Ok(connection
        .query_row(
            profiles::SELECT_LANGUAGE_PROFILE,
            params![profile_id],
            profiles::LanguageProfileRow::from_row,
        )
        .optional()?)
}
