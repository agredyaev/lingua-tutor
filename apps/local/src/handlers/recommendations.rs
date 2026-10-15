//! Recommendation operations and persistence helper.

use crate::api::{new_id, now};
use crate::repositories::{recommendations as repo, to_rfc3339};
use crate::{Env, Result};
use adaptation::Recommendation;
use rusqlite::{OptionalExtension as _, params};

/// Persists recommendations, skipping ones already open for the same
/// subject + action (no spam on every session).
pub fn insert_all(
    connection: &rusqlite::Connection,
    profile_id: &str,
    recommendations: &[Recommendation],
) -> Result<u32> {
    let mut inserted = 0;
    for recommendation in recommendations {
        let duplicate: Option<String> = connection
            .query_row(
                repo::SELECT_OPEN_DUPLICATE,
                params![
                    profile_id,
                    recommendation.subject_type,
                    recommendation.subject_id,
                    recommendation.action,
                ],
                |row| row.get(0),
            )
            .optional()?;
        if duplicate.is_some() {
            continue;
        }
        connection.execute(
            repo::INSERT,
            params![
                new_id("rec"),
                profile_id,
                recommendation.subject_type,
                recommendation.subject_id,
                recommendation.action,
                serde_json::json!({ "detail": recommendation.detail }).to_string(),
                to_rfc3339(now()),
            ],
        )?;
        inserted += 1;
    }
    Ok(inserted)
}

pub(crate) fn list_for(env: &Env, profile_id: &str) -> Result<serde_json::Value> {
    let connection = env.connection()?;
    let mut statement = connection.prepare(repo::SELECT_OPEN)?;
    let rows = statement.query_map(params![profile_id, 50], |row| {
        Ok(serde_json::json!({
            "id": row.get::<_, String>("id")?,
            "subject_type": row.get::<_, String>("subject_type")?,
            "subject_id": row.get::<_, Option<String>>("subject_id")?,
            "action": row.get::<_, String>("action")?,
            "detail_json": row.get::<_, String>("detail_json")?,
            "created_at": row.get::<_, String>("created_at")?,
        }))
    })?;
    let rows = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    crate::json(&rows)
}

/// Resolve a recommendation as accepted or dismissed.
pub(crate) fn resolve(
    env: &Env,
    recommendation_id: &str,
    status: &str,
) -> Result<serde_json::Value> {
    let connection = env.connection()?;
    connection.execute(
        repo::UPDATE_STATUS,
        params![recommendation_id, status, to_rfc3339(now())],
    )?;
    crate::json(&serde_json::json!({ "id": recommendation_id, "status": status }))
}

pub(crate) fn redesign_candidates_for(env: &Env, profile_id: &str) -> Result<serde_json::Value> {
    let connection = env.connection()?;
    let mut statement = connection.prepare(repo::SELECT_REDESIGN_CANDIDATES)?;
    let rows = statement.query_map(params![profile_id], |row| {
        Ok(serde_json::json!({
            "recommendation_id": row.get::<_, String>("recommendation_id")?,
            "detail_json": row.get::<_, String>("detail_json")?,
            "card_id": row.get::<_, String>("card_id")?,
            "prompt": row.get::<_, String>("prompt")?,
            "answer": row.get::<_, String>("answer")?,
            "card_status": row.get::<_, String>("card_status")?,
        }))
    })?;
    let rows = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    crate::json(&rows)
}
