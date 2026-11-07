//! Error mutation SQL passes. Bind source values directly without staging.

use crate::sqlite::to_rfc3339;
use crate::{Env, Result};
use rusqlite::{Connection, OptionalExtension as _, TransactionBehavior, params};

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
    let mut statement = database.prepare_cached(
        "INSERT INTO error_occurrences
         (id,error_target_id,lesson_id,context,communicative_impact,recorded_at)
         VALUES (?1,?2,?3,?4,?5,?6)",
    )?;
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
    core_category: &str,
    error_code: Option<&str>,
    original_production: &str,
    corrected_production: &str,
    explanation: Option<&str>,
    occurrence_count: u32,
    method_change_required: bool,
    status: &str,
    at: &str,
) -> Result<()> {
    let mut statement = database.prepare_cached(
        "INSERT INTO error_targets
         (id,learner_language_profile_id,core_category,error_code,original_production,
          corrected_production,explanation,occurrence_count,method_change_required,status,
          created_at,updated_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?11)",
    )?;
    statement.execute(params![
        id,
        profile_id,
        core_category,
        error_code,
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
    let mut statement = database.prepare_cached(
        "UPDATE error_targets SET
         occurrence_count=?2,method_change_required=?3,status=?4,updated_at=?5
         WHERE id=?1",
    )?;
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
    cycle: u32,
    offset_days: u32,
    due_at: &str,
) -> Result<()> {
    let mut statement = database.prepare_cached(
        "INSERT INTO error_rechecks
         (id,error_target_id,cycle,offset_days,due_at)
         VALUES (?1,?2,?3,?4,?5)",
    )?;
    statement.execute(params![id, target_id, cycle, offset_days, due_at])?;
    Ok(())
}

/// Start one fresh 1/3/7-day recheck series while preserving prior completed cycles.
pub(crate) fn schedule_error_rechecks(
    database: &Connection,
    target_id: &str,
    now: learning_core::Timestamp,
) -> Result<()> {
    let latest: Option<(u32, u32)> = database
        .query_row(
            "SELECT cycle,
                    SUM(CASE WHEN completed_at IS NOT NULL THEN 1 ELSE 0 END)
             FROM error_rechecks
             WHERE error_target_id=?1
             GROUP BY cycle
             ORDER BY cycle DESC
             LIMIT 1",
            params![target_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;

    let (cycle, reuse_pending_cycle) = match latest {
        None => (0, false),
        Some((cycle, 0)) => (cycle, true),
        Some((cycle, _)) => {
            database.execute(
                "UPDATE error_rechecks
                 SET completed_at=?3,passed=NULL
                 WHERE error_target_id=?1 AND cycle=?2 AND completed_at IS NULL",
                params![target_id, cycle, to_rfc3339(now)],
            )?;
            (cycle.saturating_add(1), false)
        }
    };

    for offset in learning_core::ERROR_RECHECK_OFFSETS_DAYS {
        let due = learning_core::Timestamp(now.0 + i64::from(offset) * 86_400);
        let due = to_rfc3339(due);
        if reuse_pending_cycle {
            let changed = database.execute(
                "UPDATE error_rechecks SET due_at=?4
                 WHERE error_target_id=?1 AND cycle=?2 AND offset_days=?3
                   AND completed_at IS NULL",
                params![target_id, cycle, offset, due],
            )?;
            if changed != 0 {
                continue;
            }
        }
        insert_error_recheck(
            database,
            &crate::api::new_id("rc"),
            target_id,
            cycle,
            offset,
            &due,
        )?;
    }
    database.execute(
        "UPDATE error_targets SET status='recheck',updated_at=?2 WHERE id=?1",
        params![target_id, to_rfc3339(now)],
    )?;
    Ok(())
}

#[cfg(test)]
fn scratch_database() -> rusqlite::Result<Connection> {
    let connection = Connection::open_in_memory()?;
    connection.execute_batch(
        "PRAGMA foreign_keys=ON;
         CREATE TABLE error_targets (
            id TEXT PRIMARY KEY,
            learner_language_profile_id TEXT NOT NULL,
            core_category TEXT NOT NULL,
            error_code TEXT,
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
            cycle INTEGER NOT NULL DEFAULT 0,
            offset_days INTEGER NOT NULL,
            due_at TEXT NOT NULL,
            completed_at TEXT,
            passed INTEGER,
            assistance_level INTEGER,
            context TEXT,
            repair_applied INTEGER,
            repair_action TEXT,
            UNIQUE(error_target_id, cycle, offset_days)
         );",
    )?;
    Ok(connection)
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
        insert_error_recheck(&database, "rc1", "target", 0, 1, "t1").unwrap();
        assert!(insert_error_recheck(&database, "rc2", "target", 0, 1, "t2").is_err());
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
            &tx, "target", "profile", "grammar", None, "a", "b", None, 1, false, "active", "t0",
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

pub(crate) fn apply_error_recheck(
    database: &Connection,
    expected_profile_id: Option<&str>,
    outcome: &tutor_contract::ErrorRecheckResult,
    now: learning_core::Timestamp,
) -> Result<()> {
    let target_id = outcome.error_target_id.as_str();
    let recheck_id = outcome.recheck_id.as_str();
    let passed = outcome.passed;
    let assistance_level = Some(outcome.assistance_level);
    let context = Some(outcome.context.as_str());
    let repair_applied = outcome.repair_applied;
    let repair_action = outcome.repair_action.as_deref();

    if repair_applied && repair_action.is_none_or(|action| action.trim().is_empty()) {
        return Err(crate::Error::operation(
            400,
            "recheck_repair_action_missing",
            "applied recheck repair requires a stable repair_action",
            serde_json::json!({ "recheck_id": recheck_id }),
        ));
    }
    if !repair_applied && repair_action.is_some() {
        return Err(crate::Error::operation(
            400,
            "recheck_repair_action_inconsistent",
            "repair_action is valid only when repair_applied=true",
            serde_json::json!({ "recheck_id": recheck_id }),
        ));
    }
    if repair_action.is_some_and(|action| action.len() > tutor_contract::bounds::MAX_ID) {
        return Err(crate::Error::operation(
            400,
            "recheck_repair_action_invalid",
            "repair_action is too long",
            serde_json::json!({ "recheck_id": recheck_id }),
        ));
    }
    if repair_action.is_some_and(|action| !learning_core::stable_context_family_id(action)) {
        return Err(crate::Error::operation(
            400,
            "recheck_repair_action_invalid",
            "repair_action must use canonical lowercase identifier spelling",
            serde_json::json!({ "recheck_id": recheck_id }),
        ));
    }

    let Some((
        actual_target_id,
        profile_id,
        cycle,
        offset_days,
        due_at,
        completed_at,
        prior_passed,
        method_change_required,
    )) = database
        .query_row(
            "SELECT r.error_target_id,t.learner_language_profile_id,r.cycle,r.offset_days,
                        r.due_at,r.completed_at,r.passed,t.method_change_required
                 FROM error_rechecks r
                 JOIN error_targets t ON t.id=r.error_target_id
                 WHERE r.id=?1",
            params![recheck_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, u32>(2)?,
                    row.get::<_, u32>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, Option<u8>>(6)?,
                    row.get::<_, u8>(7)? != 0,
                ))
            },
        )
        .optional()?
    else {
        return Err(crate::Error::operation(
            404,
            "recheck_not_found",
            "unknown error recheck",
            serde_json::json!({ "recheck_id": recheck_id }),
        ));
    };

    if actual_target_id != target_id {
        return Err(crate::Error::operation(
            409,
            "recheck_target_mismatch",
            "error recheck does not belong to the supplied target",
            serde_json::json!({
                "recheck_id": recheck_id,
                "expected_error_target_id": actual_target_id,
                "supplied_error_target_id": target_id
            }),
        ));
    }
    if expected_profile_id.is_some_and(|expected| expected != profile_id) {
        return Err(crate::Error::operation(
            409,
            "recheck_profile_mismatch",
            "error recheck does not belong to the lesson language profile",
            serde_json::json!({ "recheck_id": recheck_id }),
        ));
    }
    if assistance_level != Some(0) {
        return Err(crate::Error::operation(
            400,
            "recheck_not_unaided",
            "scheduled error recheck must be elicited without assistance",
            serde_json::json!({ "recheck_id": recheck_id }),
        ));
    }
    if context.is_none_or(|value| value.trim().is_empty()) {
        return Err(crate::Error::operation(
            400,
            "recheck_context_missing",
            "scheduled error recheck requires a concrete communicative context",
            serde_json::json!({ "recheck_id": recheck_id }),
        ));
    }
    if passed && repair_applied {
        return Err(crate::Error::operation(
            400,
            "recheck_repair_inconsistent",
            "passed error recheck must not be recorded as corrective repair",
            serde_json::json!({ "recheck_id": recheck_id }),
        ));
    }
    if !passed && !repair_applied {
        return Err(crate::Error::operation(
            400,
            "recheck_repair_missing",
            "failed error recheck requires corrective repair before rescheduling",
            serde_json::json!({ "recheck_id": recheck_id }),
        ));
    }

    if completed_at.is_some() {
        if prior_passed == Some(u8::from(passed)) {
            return Ok(());
        }
        return Err(crate::Error::operation(
            409,
            "recheck_already_completed",
            "error recheck was already completed with a different outcome",
            serde_json::json!({ "recheck_id": recheck_id }),
        ));
    }

    if let Some((previous_offset, previous_completed_at, previous_passed)) = database
        .query_row(
            "SELECT offset_days,completed_at,passed
             FROM error_rechecks
             WHERE error_target_id=?1 AND cycle=?2 AND offset_days<?3
             ORDER BY offset_days DESC
             LIMIT 1",
            params![target_id, cycle, offset_days],
            |row| {
                Ok((
                    row.get::<_, u32>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<u8>>(2)?,
                ))
            },
        )
        .optional()?
    {
        let Some(previous_completed_at) = previous_completed_at else {
            return Err(crate::Error::operation(
                409,
                "recheck_sequence_incomplete",
                "previous error recheck stage must pass before this stage",
                serde_json::json!({ "recheck_id": recheck_id }),
            ));
        };
        if previous_passed != Some(1) {
            return Err(crate::Error::operation(
                409,
                "recheck_sequence_incomplete",
                "previous error recheck stage must pass before this stage",
                serde_json::json!({ "recheck_id": recheck_id }),
            ));
        }
        let previous_at =
            crate::sqlite::from_rfc3339(&previous_completed_at).map_err(crate::Error::RustError)?;
        let required_gap_seconds = i64::from(offset_days.saturating_sub(previous_offset)) * 86_400;
        let available_at =
            learning_core::Timestamp(previous_at.0.saturating_add(required_gap_seconds));
        if now < available_at {
            return Err(crate::Error::operation(
                409,
                "recheck_not_due",
                "error recheck requires elapsed spacing after the previous actual check",
                serde_json::json!({
                    "recheck_id": recheck_id,
                    "due_at": to_rfc3339(available_at)
                }),
            ));
        }
    }

    let due = crate::sqlite::from_rfc3339(&due_at).map_err(crate::Error::RustError)?;
    if due > now {
        return Err(crate::Error::operation(
            409,
            "recheck_not_due",
            "error recheck is not due yet",
            serde_json::json!({ "recheck_id": recheck_id, "due_at": due_at }),
        ));
    }

    if !passed && repair_applied && method_change_required {
        let previous_action: Option<String> = database
            .query_row(
                "SELECT repair_action
                 FROM (
                     SELECT r.repair_action AS repair_action,
                            r.completed_at AS repaired_at
                     FROM error_rechecks r
                     WHERE r.error_target_id=?1
                       AND r.id!=?2
                       AND r.completed_at IS NOT NULL
                       AND r.repair_action IS NOT NULL
                     UNION ALL
                     SELECT json_extract(rec.detail_json,'$.repair_action') AS repair_action,
                            rec.resolved_at AS repaired_at
                     FROM recommendations rec
                     WHERE rec.subject_type='error_target'
                       AND rec.subject_id=?1
                       AND rec.status='accepted'
                       AND json_valid(rec.detail_json)
                       AND json_type(rec.detail_json,'$.repair_action')='text'
                 )
                 WHERE repair_action IS NOT NULL AND TRIM(repair_action)!=''
                 ORDER BY repaired_at DESC
                 LIMIT 1",
                params![target_id, recheck_id],
                |row| row.get(0),
            )
            .optional()?;
        if previous_action.as_deref() == repair_action {
            return Err(crate::Error::operation(
                409,
                "repair_method_not_changed",
                "sticky method change requires a different named repair_action after a failed recheck",
                serde_json::json!({
                    "error_target_id": target_id,
                    "repair_action": repair_action
                }),
            ));
        }
    }

    let at = to_rfc3339(now);
    let changed = database.execute(
        "UPDATE error_rechecks
         SET completed_at=?2,passed=?3,assistance_level=?4,context=?5,
             repair_applied=?6,repair_action=?7
         WHERE id=?1 AND completed_at IS NULL",
        params![
            recheck_id,
            at,
            u8::from(passed),
            assistance_level,
            context,
            u8::from(repair_applied),
            repair_action
        ],
    )?;
    if changed != 1 {
        return Err(crate::Error::operation(
            409,
            "recheck_state_conflict",
            "error recheck changed before completion",
            serde_json::json!({ "recheck_id": recheck_id }),
        ));
    }

    if passed {
        for later_offset in learning_core::ERROR_RECHECK_OFFSETS_DAYS
            .into_iter()
            .filter(|candidate| *candidate > offset_days)
        {
            let gap_days = later_offset.saturating_sub(offset_days);
            let next_due =
                learning_core::Timestamp(now.0.saturating_add(i64::from(gap_days) * 86_400));
            database.execute(
                "UPDATE error_rechecks
                 SET due_at=?4
                 WHERE error_target_id=?1 AND cycle=?2 AND offset_days=?3
                   AND completed_at IS NULL",
                params![target_id, cycle, later_offset, to_rfc3339(next_due)],
            )?;
        }

        let (total, incomplete): (u32, u32) = database.query_row(
            "SELECT COUNT(*),
                    SUM(CASE WHEN completed_at IS NULL OR passed IS NOT 1 THEN 1 ELSE 0 END)
             FROM error_rechecks
             WHERE error_target_id=?1 AND cycle=?2",
            params![target_id, cycle],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let full_series =
            total == learning_core::ERROR_RECHECK_OFFSETS_DAYS.len() as u32 && incomplete == 0;
        if full_series {
            database.execute(
                "UPDATE error_targets
                 SET status='resolved',method_change_required=0,updated_at=?2
                 WHERE id=?1",
                params![target_id, at],
            )?;
            database.execute(
                "UPDATE recommendations
                 SET status='expired',resolved_at=?3
                 WHERE learner_language_profile_id=?1
                   AND subject_type='error_target' AND subject_id=?2
                   AND status='open'",
                params![profile_id, target_id, at],
            )?;
        } else {
            database.execute(
                "UPDATE error_targets SET status='recheck',updated_at=?2 WHERE id=?1",
                params![target_id, at],
            )?;
        }
        return Ok(());
    }

    database.execute(
        "UPDATE error_rechecks
         SET completed_at=?3,passed=NULL
         WHERE error_target_id=?1 AND cycle=?2 AND completed_at IS NULL",
        params![target_id, cycle, at],
    )?;
    database.execute(
        "UPDATE error_targets SET status='active',updated_at=?2 WHERE id=?1",
        params![target_id, at],
    )?;

    if repair_applied {
        schedule_error_rechecks(database, target_id, now)?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)] // Direct compatibility RPC keeps flat wire fields explicit; grouping them would add wrapper-only state.
pub(crate) fn complete_error_recheck(
    env: &Env,
    target_id: &str,
    recheck_id: &str,
    passed: bool,
    assistance_level: u8,
    context: &str,
    repair_applied: bool,
    repair_action: Option<&str>,
) -> Result<serde_json::Value> {
    let mut database = env.connection()?;
    let transaction = database.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let outcome = tutor_contract::ErrorRecheckResult {
        error_target_id: target_id.to_owned(),
        recheck_id: recheck_id.to_owned(),
        passed,
        assistance_level,
        context: context.to_owned(),
        repair_applied,
        repair_action: repair_action.map(str::to_owned),
    };
    apply_error_recheck(&transaction, None, &outcome, crate::api::now())?;
    transaction.commit()?;
    crate::json(&serde_json::json!({
        "recheck_id": recheck_id,
        "passed": passed,
        "assistance_level": assistance_level,
        "context": context,
        "repair_applied": repair_applied,
        "repair_action": repair_action
    }))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::type_complexity)]
mod recheck_tests {
    use super::*;

    fn create_env(label: &str) -> (std::path::PathBuf, Env) {
        let path = std::env::temp_dir().join(format!(
            "lingua-recheck-{label}-{}.sqlite3",
            uuid::Uuid::new_v4()
        ));
        let db = crate::storage::open(&path).unwrap();
        db.execute_batch(
            r#"
             INSERT INTO learner_profile (id, created_at, updated_at)
             VALUES ('learner-1', '2026-10-16T00:00:00Z', '2026-10-16T00:00:00Z');
             INSERT INTO learner_language_profiles
                (id, learner_id, source_language, target_locale, created_at, updated_at)
             VALUES
                ('llp-1', 'learner-1', 'ru', 'es-ES',
                 '2026-10-16T00:00:00Z', '2026-10-16T00:00:00Z');
             INSERT INTO error_targets
                (id, learner_language_profile_id, core_category, original_production,
                 corrected_production, occurrence_count, method_change_required,
                 status, created_at, updated_at)
             VALUES
                ('et-1', 'llp-1', 'grammar', 'wrong', 'right', 5, 1, 'recheck',
                 '2026-10-16T00:00:00Z', '2026-10-16T00:00:00Z'),
                ('et-2', 'llp-1', 'grammar', 'other wrong', 'other right', 1, 0, 'active',
                 '2026-10-16T00:00:00Z', '2026-10-16T00:00:00Z');
             INSERT INTO error_rechecks (id, error_target_id, offset_days, due_at)
             VALUES
                ('rc-1', 'et-1', 1, '2020-01-01T00:00:00Z'),
                ('rc-3', 'et-1', 3, '2020-01-03T00:00:00Z'),
                ('rc-7', 'et-1', 7, '2020-01-07T00:00:00Z');"#,
        )
        .unwrap();
        drop(db);
        (path.clone(), Env::new(path, "test-signing-key".to_owned()))
    }

    fn complete(
        env: &Env,
        target_id: &str,
        recheck_id: &str,
        passed: bool,
    ) -> Result<serde_json::Value> {
        complete_error_recheck(
            env,
            target_id,
            recheck_id,
            passed,
            0,
            "test:scheduled-recheck",
            !passed,
            (!passed).then_some("test_repair"),
        )
    }

    fn complete_at(
        env: &Env,
        target_id: &str,
        recheck_id: &str,
        passed: bool,
        at: i64,
    ) -> Result<()> {
        let mut database = env.connection()?;
        let transaction = database.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let outcome = tutor_contract::ErrorRecheckResult {
            error_target_id: target_id.to_owned(),
            recheck_id: recheck_id.to_owned(),
            passed,
            assistance_level: 0,
            context: "test:scheduled-recheck".to_owned(),
            repair_applied: !passed,
            repair_action: (!passed).then(|| "test_repair".to_owned()),
        };
        apply_error_recheck(&transaction, None, &outcome, learning_core::Timestamp(at))?;
        transaction.commit()?;
        Ok(())
    }

    fn snapshot(env: &Env) -> (String, Option<u8>, bool, Vec<(String, String, String)>) {
        let db = env.connection().unwrap();
        let status = db
            .query_row(
                "SELECT status FROM error_targets WHERE id='et-1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let (passed, completed_at): (Option<u8>, Option<String>) = db
            .query_row(
                "SELECT passed, completed_at FROM error_rechecks WHERE id='rc-3'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        let recommendations = db
            .prepare("SELECT action,detail_json,status FROM recommendations ORDER BY action")
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        (status, passed, completed_at.is_some(), recommendations)
    }

    #[test]
    fn passed_first_recheck_keeps_target_in_recheck_state() {
        let (path, env) = create_env("passed");
        complete(&env, "et-1", "rc-1", true).unwrap();
        let db = env.connection().unwrap();
        let state: (String, Option<u8>, bool) = db
            .query_row(
                "SELECT t.status,r.passed,r.completed_at IS NOT NULL
                 FROM error_targets t
                 JOIN error_rechecks r ON r.error_target_id=t.id
                 WHERE t.id='et-1' AND r.id='rc-1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(state, ("recheck".to_owned(), Some(1), true));
        drop(db);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn overdue_recheck_stages_cannot_be_completed_back_to_back() {
        let (path, env) = create_env("massed-stages");
        complete(&env, "et-1", "rc-1", true).unwrap();
        assert!(
            complete(&env, "et-1", "rc-3", true).is_err(),
            "a later overdue stage must still wait for elapsed spacing after the prior actual check"
        );

        let db = env.connection().unwrap();
        let rc3_completed: Option<String> = db
            .query_row(
                "SELECT completed_at FROM error_rechecks WHERE id='rc-3'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(rc3_completed.is_none());
        drop(db);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn successful_spaced_recheck_series_resolves_target_and_clears_method_change() {
        let (path, env) = create_env("resolved");
        env.connection()
            .unwrap()
            .execute(
                "INSERT INTO recommendations
                 (id,learner_language_profile_id,subject_type,subject_id,action,detail_json,status,created_at)
                 VALUES('rec-stale','llp-1','error_target','et-1','practice_again','{}','open',
                        '2026-10-16T00:00:00Z')",
                [],
            )
            .unwrap();

        const T0: i64 = 1_800_000_000;
        complete_at(&env, "et-1", "rc-1", true, T0).unwrap();
        complete_at(&env, "et-1", "rc-3", true, T0 + 2 * 86_400).unwrap();
        complete_at(&env, "et-1", "rc-7", true, T0 + 6 * 86_400).unwrap();

        let db = env.connection().unwrap();
        let state: (String, u8) = db
            .query_row(
                "SELECT status, method_change_required FROM error_targets WHERE id='et-1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        let recommendation: (String, bool) = db
            .query_row(
                "SELECT status,resolved_at IS NOT NULL FROM recommendations WHERE id='rec-stale'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(state, ("resolved".to_owned(), 0));
        assert_eq!(
            recommendation,
            ("expired".to_owned(), true),
            "resolved error targets must not leave stale open planner recommendations"
        );
        drop(db);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn mismatched_recheck_target_is_rejected_without_mutation() {
        let (path, env) = create_env("mismatch");
        assert!(complete(&env, "et-2", "rc-1", true).is_err());

        let db = env.connection().unwrap();
        let recheck: (Option<u8>, Option<String>) = db
            .query_row(
                "SELECT passed, completed_at FROM error_rechecks WHERE id='rc-1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        let target_status: String = db
            .query_row(
                "SELECT status FROM error_targets WHERE id='et-2'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(recheck, (None, None));
        assert_eq!(target_status, "active");
        drop(db);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn recheck_from_another_profile_is_rejected() {
        let (path, env) = create_env("profile-mismatch");
        let db = env.connection().unwrap();
        assert!(
            apply_error_recheck(
                &db,
                Some("llp-other"),
                &tutor_contract::ErrorRecheckResult {
                    error_target_id: "et-1".to_owned(),
                    recheck_id: "rc-1".to_owned(),
                    passed: true,
                    assistance_level: 0,
                    context: "profile-mismatch".to_owned(),
                    repair_applied: false,
                    repair_action: None,
                },
                learning_core::Timestamp(1_800_000_000),
            )
            .is_err()
        );
        let completed: Option<String> = db
            .query_row(
                "SELECT completed_at FROM error_rechecks WHERE id='rc-1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(completed.is_none());
        drop(db);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn future_recheck_cannot_be_completed_early() {
        let (path, env) = create_env("not-due");
        env.connection()
            .unwrap()
            .execute(
                "UPDATE error_rechecks SET due_at='2999-01-01T00:00:00Z' WHERE id='rc-1'",
                [],
            )
            .unwrap();
        assert!(complete(&env, "et-1", "rc-1", true).is_err());
        let db = env.connection().unwrap();
        let completed: Option<String> = db
            .query_row(
                "SELECT completed_at FROM error_rechecks WHERE id='rc-1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(completed.is_none());
        drop(db);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn failed_recheck_reactivates_target_and_starts_fresh_series() {
        let (path, env) = create_env("failed-series");
        complete(&env, "et-1", "rc-1", false).unwrap();

        let db = env.connection().unwrap();
        let pending: u32 = db
            .query_row(
                "SELECT COUNT(*) FROM error_rechecks
                 WHERE error_target_id='et-1' AND completed_at IS NULL",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(pending, 3);
        drop(db);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn sticky_failed_recheck_requires_a_different_named_repair() {
        let (path, env) = create_env("sticky-recheck-method-change");
        env.connection()
            .unwrap()
            .execute(
                r#"INSERT INTO recommendations
                 (id,learner_language_profile_id,subject_type,subject_id,action,
                  detail_json,status,created_at,resolved_at)
                 VALUES('rec-last-method','llp-1','error_target','et-1',
                        'require_method_change',json_object('repair_action','test_repair'),
                        'accepted','2026-10-15T00:00:00Z','2026-10-15T01:00:00Z')"#,
                [],
            )
            .unwrap();

        let error = complete(&env, "et-1", "rc-1", false).unwrap_err();
        assert!(
            error.to_string().contains("repair_action") || error.to_string().contains("method"),
            "sticky failed recheck must not repeat the last named repair: {error}"
        );

        let db = env.connection().unwrap();
        let completed: Option<String> = db
            .query_row(
                "SELECT completed_at FROM error_rechecks WHERE id='rc-1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(completed.is_none());
        drop(db);

        complete_error_recheck(
            &env,
            "et-1",
            "rc-1",
            false,
            0,
            "test:scheduled-recheck",
            true,
            Some("semantic_reframe"),
        )
        .unwrap();

        let db = env.connection().unwrap();
        let action: Option<String> = db
            .query_row(
                "SELECT repair_action FROM error_rechecks WHERE id='rc-1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(action.as_deref(), Some("semantic_reframe"));
        drop(db);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn failed_recheck_persists_named_repair_action() {
        let (path, env) = create_env("persist-repair-action");
        complete(&env, "et-1", "rc-1", false).unwrap();

        let db = env.connection().unwrap();
        let repair_action: Option<String> = db
            .query_row(
                "SELECT repair_action FROM error_rechecks WHERE id='rc-1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(repair_action.as_deref(), Some("test_repair"));
        drop(db);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn direct_failed_recheck_requires_repair_action() {
        let (path, env) = create_env("missing-repair-action");
        let error = complete_error_recheck(
            &env,
            "et-1",
            "rc-1",
            false,
            0,
            "test:scheduled-recheck",
            true,
            None,
        )
        .unwrap_err();
        assert!(
            error.to_string().contains("repair_action"),
            "direct compatibility writes must enforce the same repair metadata as LessonResult"
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn direct_passed_recheck_rejects_corrective_repair_metadata() {
        let (path, env) = create_env("passed-with-repair");
        let error = complete_error_recheck(
            &env,
            "et-1",
            "rc-1",
            true,
            0,
            "test:scheduled-recheck",
            true,
            Some("contrast"),
        )
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("must not be recorded as corrective repair"),
            "a passing retention measurement must stay separate from corrective repair"
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn failed_recheck_repairs_then_returns_to_spaced_recheck_without_extra_practice_pressure() {
        let (path, env) = create_env("failed");
        const T0: i64 = 1_800_000_000;
        complete_at(&env, "et-1", "rc-1", true, T0).unwrap();
        complete_at(&env, "et-1", "rc-3", false, T0 + 2 * 86_400).unwrap();
        let (status, passed, completed, recommendations) = snapshot(&env);
        assert_eq!(
            status, "recheck",
            "same-lesson corrective repair completes active practice; the fresh cycle must wait for spaced retention measurement"
        );
        assert_eq!(passed, Some(0));
        assert!(completed);
        assert!(
            recommendations.is_empty(),
            "completed same-lesson repair must not leave duplicate practice_again pressure between rechecks"
        );
        let _ = std::fs::remove_file(path);
    }
}
