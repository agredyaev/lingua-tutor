//! Fixed-capacity SoA writes for recommendations.

use crate::Result;
use crate::repositories::recommendations as repo;
use rusqlite::{Connection, OptionalExtension as _, params};

/// Insert unless the same open recommendation already exists.
pub(crate) fn insert_recommendation(
    database: &Connection,
    profile_id: &str,
    subject_type: &str,
    subject_id: Option<&str>,
    action: &str,
    detail_json: &str,
    created_at: &str,
) -> Result<()> {
    let duplicate = database
        .query_row(
            repo::SELECT_OPEN_DUPLICATE,
            params![profile_id, subject_type, subject_id, action],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    if duplicate.is_none() {
        database.execute(
            repo::INSERT,
            params![
                crate::api::new_id("rec"),
                profile_id,
                subject_type,
                subject_id,
                action,
                detail_json,
                created_at,
            ],
        )?;
    }
    Ok(())
}
