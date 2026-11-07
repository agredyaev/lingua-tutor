//! Fixed-capacity SoA writes for recommendations.

use crate::Result;
use rusqlite::{Connection, OptionalExtension as _, params};

const SELECT_OPEN_DUPLICATE: &str = "SELECT id FROM recommendations
 WHERE learner_language_profile_id=?1 AND subject_type=?2
   AND (subject_id=?3 OR (subject_id IS NULL AND ?3 IS NULL))
   AND action=?4 AND status='open' LIMIT 1";
const INSERT_RECOMMENDATION: &str = "INSERT INTO recommendations
 (id,learner_language_profile_id,subject_type,subject_id,action,detail_json,status,created_at)
 VALUES (?1,?2,?3,?4,?5,?6,'open',?7)";

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
            SELECT_OPEN_DUPLICATE,
            params![profile_id, subject_type, subject_id, action],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    if duplicate.is_none() {
        database.execute(
            INSERT_RECOMMENDATION,
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
