//! Recommendation operations and persistence helper.

use crate::repositories::recommendations as repo;
use crate::{Env, Result};
use rusqlite::params;

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
