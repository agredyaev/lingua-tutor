//! Language-pack and learner-profile operations.

use crate::api::operation_error;
use crate::repositories::profiles;
use crate::{Env, Result};
use serde::Deserialize;

pub(crate) fn list_packs(registry: &crate::BuiltInPackRegistry) -> Result<serde_json::Value> {
    let manifests: Vec<_> = language_pack_registry::manifests(registry).collect();
    crate::json(&manifests)
}

pub(crate) fn get_pack(
    registry: &crate::BuiltInPackRegistry,
    pack_id: &str,
) -> Result<serde_json::Value> {
    match language_pack_registry::manifest(registry, pack_id) {
        Some(manifest) => crate::json(&manifest),
        None => operation_error(404, "not_found", "unknown language pack"),
    }
}

// Streaming SQLite -> JSON without an intermediate row/domain graph.
fn project_profile_wire(row: &rusqlite::Row<'_>) -> rusqlite::Result<Option<serde_json::Value>> {
    let status: String = row.get("status")?;
    let preferences_text: String = row.get("preferences_json")?;
    let (Ok(_), Ok(preferences)) = (
        serde_json::from_value::<learning_core::ProfileStatus>(serde_json::json!(status)),
        serde_json::from_str::<serde_json::Value>(&preferences_text),
    ) else {
        return Ok(None);
    };
    Ok(Some(serde_json::json!({
        "id": row.get::<_, String>("id")?,
        "learner_id": row.get::<_, String>("learner_id")?,
        "language_pack_id": row.get::<_, String>("language_pack_id")?,
        "source_language": row.get::<_, String>("source_language")?,
        "target_locale": row.get::<_, String>("target_locale")?,
        "estimated_level": row.get::<_, Option<String>>("estimated_level")?,
        "current_lesson_number": row.get::<_, u32>("current_lesson_number")?,
        "status": status,
        "is_default": row.get::<_, u8>("is_default")? != 0,
        "preferences": preferences,
    })))
}

pub(crate) fn list_profiles(env: &Env) -> Result<serde_json::Value> {
    let connection = env.connection()?;
    let mut statement = connection.prepare(profiles::SELECT_LANGUAGE_PROFILES)?;
    let mut rows = statement.query([])?;
    let mut output = Vec::new();
    while let Some(row) = rows.next()? {
        if let Some(wire) = project_profile_wire(row)? {
            output.push(wire);
        }
    }
    crate::json(&output)
}

#[derive(Deserialize)]
pub(crate) struct PatchProfileParams {
    pub estimated_level: Option<String>,
    pub status: Option<String>,
    pub is_default: Option<bool>,
    pub preferences: Option<serde_json::Value>,
}

#[cfg(test)]
// Test fixtures intentionally fail fast and use compact tuple snapshots.
#[allow(clippy::unwrap_used, clippy::type_complexity)]
mod dod_profile_projection_tests {
    use super::*;

    const PROFILE_ROW: &str = "SELECT 'llp-1' AS id, 'learner-1' AS learner_id,
        'es-ES' AS language_pack_id, 'ru' AS source_language, 'es-ES' AS target_locale,
        'A1' AS estimated_level, 7 AS current_lesson_number,
        'active' AS status, 1 AS is_default,
        '{\"priority_scenarios\":[\"pharmacy\"]}' AS preferences_json";

    #[test]
    fn profile_sql_projection_matches_wire_fixture() {
        let db = rusqlite::Connection::open_in_memory().unwrap();
        let wire = db
            .query_row(PROFILE_ROW, [], project_profile_wire)
            .unwrap()
            .unwrap();
        assert_eq!(
            wire,
            serde_json::json!({
                "id":"llp-1","learner_id":"learner-1","language_pack_id":"es-ES",
                "source_language":"ru","target_locale":"es-ES","estimated_level":"A1",
                "current_lesson_number":7,"status":"active","is_default":true,
                "preferences":{"priority_scenarios":["pharmacy"]}
            })
        );
    }

    #[test]
    fn profile_sql_projection_rejects_corrupt_data() {
        let db = rusqlite::Connection::open_in_memory().unwrap();
        for invalid in [
            PROFILE_ROW.replace("'active' AS status", "'oops' AS status"),
            PROFILE_ROW.replace(
                "'{\"priority_scenarios\":[\"pharmacy\"]}' AS preferences_json",
                "'not-json' AS preferences_json",
            ),
        ] {
            assert!(
                db.query_row(&invalid, [], project_profile_wire)
                    .unwrap()
                    .is_none()
            );
        }
    }
}
