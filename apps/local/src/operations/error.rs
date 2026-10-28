//! Error mutation SQL passes. Bind source values directly without staging.

use crate::Result;
use crate::repositories::records;
use rusqlite::{Connection, params};

/// Bind admitted values directly to the SQLite statement (no staging allocation).
#[allow(clippy::too_many_arguments)]
pub(crate) fn insert_error_occurrence(
    database: &Connection,
    id: &str,
    target_id: &str,
    lesson_id: Option<&str>,
    context: Option<&str>,
    communicative_impact: &str,
    recorded_at: &str,
) -> Result<()> {
    let mut statement = database.prepare_cached(records::INSERT_ERROR_OCCURRENCE)?;
    statement.execute(params![
        id,
        target_id,
        lesson_id,
        context,
        communicative_impact,
        recorded_at,
    ])?;
    Ok(())
}

/// Bind admitted values directly to the SQLite statement (no staging allocation).
#[allow(clippy::too_many_arguments)]
pub(crate) fn insert_error_target(
    database: &Connection,
    id: &str,
    profile_id: &str,
    pack_id: &str,
    pack_version: &str,
    core_category: &str,
    pack_error_code: Option<&str>,
    original_production: &str,
    corrected_production: &str,
    explanation: Option<&str>,
    occurrence_count: u32,
    method_change_required: bool,
    status: &str,
    at: &str,
) -> Result<()> {
    let mut statement = database.prepare_cached(records::INSERT_ERROR_TARGET)?;
    statement.execute(params![
        id,
        profile_id,
        pack_id,
        pack_version,
        core_category,
        pack_error_code,
        original_production,
        corrected_production,
        explanation,
        occurrence_count,
        u8::from(method_change_required),
        status,
        at,
    ])?;
    Ok(())
}

/// Bind admitted values directly to the SQLite statement (no staging allocation).
#[allow(clippy::too_many_arguments)]
pub(crate) fn update_error_target(
    database: &Connection,
    id: &str,
    occurrence_count: u32,
    method_change_required: bool,
    status: &str,
    at: &str,
) -> Result<()> {
    let mut statement = database.prepare_cached(records::UPDATE_ERROR_TARGET)?;
    statement.execute(params![
        id,
        occurrence_count,
        u8::from(method_change_required),
        status,
        at,
    ])?;
    Ok(())
}

/// Bind admitted values directly to the SQLite statement (no staging allocation).
#[allow(clippy::too_many_arguments)]
pub(crate) fn insert_error_recheck(
    database: &Connection,
    id: &str,
    target_id: &str,
    offset_days: u32,
    due_at: &str,
) -> Result<()> {
    let mut statement = database.prepare_cached(records::INSERT_ERROR_RECHECK)?;
    statement.execute(params![id, target_id, offset_days, due_at,])?;
    Ok(())
}

#[cfg(any(test, feature = "fuzzing"))]
fn scratch_database() -> rusqlite::Result<Connection> {
    let connection = Connection::open_in_memory()?;
    connection.execute_batch(
        "PRAGMA foreign_keys=ON;
         CREATE TABLE error_targets (
            id TEXT PRIMARY KEY,
            learner_language_profile_id TEXT NOT NULL,
            language_pack_id TEXT NOT NULL,
            language_pack_version TEXT NOT NULL,
            core_category TEXT NOT NULL,
            pack_error_code TEXT,
            original_production TEXT NOT NULL,
            corrected_production TEXT NOT NULL,
            explanation TEXT,
            occurrence_count INTEGER NOT NULL,
            method_change_required INTEGER NOT NULL,
            status TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
         );
         CREATE TABLE error_occurrences (
            id TEXT PRIMARY KEY,
            error_target_id TEXT NOT NULL REFERENCES error_targets(id),
            lesson_id TEXT,
            context TEXT,
            communicative_impact TEXT NOT NULL DEFAULT 'medium',
            recorded_at TEXT NOT NULL
         );
         CREATE TABLE error_rechecks (
            id TEXT PRIMARY KEY,
            error_target_id TEXT NOT NULL REFERENCES error_targets(id),
            offset_days INTEGER NOT NULL,
            due_at TEXT NOT NULL,
            completed_at TEXT,
            passed INTEGER,
            UNIQUE(error_target_id, offset_days)
         );",
    )?;
    Ok(connection)
}

#[cfg(feature = "fuzzing")]
/// Exercise direct error writes with arbitrary bytes and transaction rollback.
pub fn fuzz_error_occurrence_sequence(data: &[u8]) {
    let Ok(mut database) = scratch_database() else {
        return;
    };
    let Ok(tx) = database.transaction() else {
        return;
    };
    if insert_error_target(
        &tx,
        "target",
        "profile",
        "pack",
        "version",
        "grammar",
        None,
        "original",
        "corrected",
        None,
        1,
        false,
        "active",
        "now",
    )
    .is_err()
    {
        return;
    }
    for (i, byte) in data.iter().take(200).enumerate() {
        let id = format!("occurrence-{i}");
        let context = if byte % 2 == 0 { Some("context") } else { None };
        if insert_error_occurrence(&tx, &id, "target", None, context, "medium", "now").is_err() {
            return;
        }
    }
    if data.first().is_some_and(|byte| byte % 2 == 0) {
        drop(tx);
        let count: i64 = database
            .query_row("SELECT COUNT(*) FROM error_occurrences", [], |row| {
                row.get(0)
            })
            .unwrap_or(-1);
        assert_eq!(count, 0);
    } else {
        let expected = data.len().min(200) as i64;
        if tx.commit().is_err() {
            return;
        }
        let count: i64 = database
            .query_row("SELECT COUNT(*) FROM error_occurrences", [], |row| {
                row.get(0)
            })
            .unwrap_or(-1);
        assert_eq!(count, expected);
    }
}

#[cfg(test)]
// Test fixtures intentionally fail fast and use compact tuple snapshots.
#[allow(clippy::unwrap_used, clippy::type_complexity)]
mod tests {
    use super::*;

    #[test]
    fn direct_writes_preserve_nullable_values_and_conflicts() {
        let database = scratch_database().unwrap();
        insert_error_target(
            &database,
            "target",
            "profile",
            "pack",
            "v1",
            "grammar",
            None,
            "original",
            "corrected",
            Some("explanation"),
            1,
            false,
            "active",
            "t0",
        )
        .unwrap();
        insert_error_occurrence(&database, "occ", "target", None, None, "medium", "t0").unwrap();
        insert_error_recheck(&database, "rc1", "target", 1, "t1").unwrap();
        insert_error_recheck(&database, "rc2", "target", 1, "t2").unwrap();
        update_error_target(&database, "target", 3, true, "recheck", "t3").unwrap();
        let (count, changed, category, explanation): (u32, u8, String, Option<String>) = database
            .query_row(
                "SELECT occurrence_count, method_change_required, core_category, explanation
                   FROM error_targets WHERE id='target'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(
            (count, changed, category.as_str(), explanation.as_deref()),
            (3, 1, "grammar", Some("explanation"))
        );
        let rechecks: i64 = database
            .query_row("SELECT COUNT(*) FROM error_rechecks", [], |row| row.get(0))
            .unwrap();
        assert_eq!(rechecks, 1);
        let occurrence_null: (Option<String>, Option<String>) = database
            .query_row(
                "SELECT lesson_id, context FROM error_occurrences",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(occurrence_null, (None, None));
    }

    #[test]
    fn direct_writes_roll_back_on_foreign_key_error() {
        let mut database = scratch_database().unwrap();
        let tx = database.transaction().unwrap();
        insert_error_target(
            &tx, "target", "profile", "pack", "v1", "grammar", None, "a", "b", None, 1, false,
            "active", "t0",
        )
        .unwrap();
        assert!(
            insert_error_occurrence(&tx, "bad", "missing", None, None, "medium", "t0").is_err()
        );
        drop(tx);
        let remaining: i64 = database
            .query_row("SELECT COUNT(*) FROM error_targets", [], |row| row.get(0))
            .unwrap();
        assert_eq!(remaining, 0);
    }
}
