//! Scalar recommendation state transitions.

use crate::{Env, Result};
use rusqlite::params;

pub(crate) fn resolve(
    env: &Env,
    recommendation_id: &str,
    status: &str,
) -> Result<serde_json::Value> {
    let database = env.connection()?;
    database.execute(
        crate::repositories::recommendations::UPDATE_STATUS,
        params![
            recommendation_id,
            status,
            crate::repositories::to_rfc3339(crate::api::now())
        ],
    )?;
    crate::json(&serde_json::json!({"id":recommendation_id,"status":status}))
}
