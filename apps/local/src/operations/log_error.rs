#![allow(clippy::items_after_test_module)] // Test fixtures are intentionally grouped before the processing pass.
//! Typed recurring-error operation built on fixed-capacity SoA buffers.

#[cfg(test)]
// Test fixtures intentionally fail fast and use compact tuple snapshots.
#[allow(clippy::unwrap_used, clippy::type_complexity)]
mod tests {
    use super::*;
    use crate::{Env, built_in_language_packs};
    use rusqlite::{Connection, params};
    use std::path::PathBuf;

    #[derive(Debug, PartialEq, Eq)]
    struct Snapshot {
        targets: Vec<(
            String,
            String,
            String,
            Option<String>,
            String,
            String,
            Option<String>,
            u32,
            u8,
            String,
        )>,
        occurrences: Vec<(u32, u32)>,
        recheck_offsets: Vec<u32>,
    }

    fn create_env(label: &str) -> (PathBuf, Env) {
        let path = std::env::temp_dir().join(format!(
            "lingua-v2-{label}-{}.sqlite3",
            uuid::Uuid::new_v4()
        ));
        let connection = crate::storage::open(&path).unwrap();
        seed_profile(&connection);
        drop(connection);
        (path.clone(), Env::new(path, "test-signing-key".to_owned()))
    }

    fn seed_profile(connection: &Connection) {
        connection
            .execute_batch(
                "INSERT INTO language_packs
                (id, package_name, version, contract_version, target_language,
                 target_locale, display_name, installed_at, updated_at)
             VALUES
                ('es-ES', 'language-pack-es-es', '0.2.0', 1, 'es', 'es-ES',
                 'Spanish (Spain)', '2026-10-16T00:00:00Z', '2026-10-16T00:00:00Z');
             INSERT INTO learner_profile (id, created_at, updated_at)
             VALUES ('learner-1', '2026-10-16T00:00:00Z', '2026-10-16T00:00:00Z');
             INSERT INTO learner_language_profiles
                (id, learner_id, language_pack_id, source_language, target_locale,
                 created_at, updated_at)
             VALUES
                ('llp-1', 'learner-1', 'es-ES', 'ru', 'es-ES',
                 '2026-10-16T00:00:00Z', '2026-10-16T00:00:00Z');",
            )
            .unwrap();
    }

    fn seed_existing_target(connection: &Connection, occurrence_count: u32) {
        connection
            .execute(
                crate::repositories::records::INSERT_ERROR_TARGET,
                params![
                    "et-existing",
                    "llp-1",
                    "es-ES",
                    "0.2.0",
                    "grammar",
                    Option::<String>::None,
                    "wrong",
                    "right",
                    Option::<String>::None,
                    occurrence_count,
                    0_u8,
                    "active",
                    "2026-10-16T00:00:00Z",
                ],
            )
            .unwrap();
        for (id, offset) in [("rc-1", 1_u32), ("rc-3", 3), ("rc-7", 7)] {
            connection
                .execute(
                    crate::repositories::records::INSERT_ERROR_RECHECK,
                    params![
                        id,
                        "et-existing",
                        offset,
                        format!("2026-10-{:02}T00:00:00Z", 16 + offset)
                    ],
                )
                .unwrap();
        }
    }

    fn new_call(env: &Env, existing: Option<&str>) {
        let registry = built_in_language_packs().unwrap();
        log_error_occurrence(
            &registry,
            env,
            LogErrorOccurrenceInput {
                language_profile_id: "llp-1".to_owned(),
                core_category: learning_core::CoreErrorCategory::Grammar,
                original_production: "wrong".to_owned(),
                corrected_production: "right".to_owned(),
                pack_error_code: Some("grammar.synthetic".to_owned()),
                explanation: Some("synthetic explanation".to_owned()),
                error_target_id: existing.map(str::to_owned),
                communicative_impact: learning_core::CommunicativeImpact::Medium,
            },
        )
        .unwrap();
    }

    fn snapshot(env: &Env) -> Snapshot {
        let connection = env.connection().unwrap();
        let targets = connection
            .prepare(
                "SELECT learner_language_profile_id, language_pack_id,
                        language_pack_version, pack_error_code,
                        original_production, corrected_production, explanation,
                        occurrence_count, method_change_required, status
                 FROM error_targets ORDER BY original_production, corrected_production",
            )
            .unwrap()
            .query_map([], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                    row.get(8)?,
                    row.get(9)?,
                ))
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();

        let occurrences = connection
            .prepare(
                "SELECT lesson_id IS NULL, context IS NULL
                 FROM error_occurrences ORDER BY rowid",
            )
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();

        let recheck_offsets = connection
            .prepare("SELECT offset_days FROM error_rechecks ORDER BY offset_days")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();

        Snapshot {
            targets,
            occurrences,
            recheck_offsets,
        }
    }

    fn cleanup(path: PathBuf) {
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("sqlite3-wal"));
        let _ = std::fs::remove_file(path.with_extension("sqlite3-shm"));
    }

    #[test]
    fn new_target_creates_occurrence_and_three_rechecks() {
        let (path, env) = create_env("new-target");
        new_call(&env, None);
        let state = snapshot(&env);
        assert_eq!(state.targets.len(), 1);
        assert_eq!(state.targets[0].7, 1);
        assert_eq!(state.targets[0].8, 0);
        assert_eq!(state.targets[0].9, "active");
        assert_eq!(state.occurrences, vec![(1, 1)]);
        assert_eq!(state.recheck_offsets, vec![1, 3, 7]);
        cleanup(path);
    }

    #[test]
    fn existing_target_below_method_change_threshold_stays_unflagged() {
        let (path, env) = create_env("existing");
        seed_existing_target(&env.connection().unwrap(), 2);
        new_call(&env, Some("et-existing"));
        let state = snapshot(&env);
        assert_eq!(state.targets[0].7, 3);
        assert_eq!(state.targets[0].8, 0);
        assert_eq!(state.targets[0].9, "recheck");
        assert_eq!(state.occurrences, vec![(1, 1)]);
        cleanup(path);
    }

    #[test]
    fn existing_target_at_canonical_threshold_requires_method_change() {
        let (path, env) = create_env("threshold");
        seed_existing_target(&env.connection().unwrap(), 4);
        new_call(&env, Some("et-existing"));
        let state = snapshot(&env);
        assert_eq!(state.targets[0].7, 5);
        assert_eq!(state.targets[0].8, 1);
        assert_eq!(state.targets[0].9, "recheck");
        cleanup(path);
    }
}

use super::error::{
    insert_error_occurrence, insert_error_recheck, insert_error_target, update_error_target,
};
use crate::repositories::to_rfc3339;
use crate::{Env, Result};
use rusqlite::{OptionalExtension as _, TransactionBehavior, params};

pub(crate) struct LogErrorOccurrenceInput {
    pub language_profile_id: String,
    pub core_category: learning_core::CoreErrorCategory,
    pub original_production: String,
    pub corrected_production: String,
    pub pack_error_code: Option<String>,
    pub explanation: Option<String>,
    pub error_target_id: Option<String>,
    pub communicative_impact: learning_core::CommunicativeImpact,
}

pub(crate) fn log_error_occurrence(
    registry: &crate::BuiltInPackRegistry,
    env: &Env,
    input: LogErrorOccurrenceInput,
) -> Result<String> {
    let mut database = env.connection()?;
    let transaction = database.transaction_with_behavior(TransactionBehavior::Immediate)?;

    let pack_id: Option<String> = transaction
        .query_row(
            "SELECT language_pack_id
             FROM learner_language_profiles
             WHERE id = ?1",
            params![input.language_profile_id],
            |row| row.get(0),
        )
        .optional()?;
    let Some(pack_id) = pack_id else {
        return Err(crate::Error::operation(
            404,
            "not_found",
            "unknown profile",
            serde_json::Value::Null,
        ));
    };

    let Some(manifest) = language_pack_registry::manifest(registry, &pack_id) else {
        return Err(crate::Error::operation(
            409,
            "pack_unavailable",
            "active language pack is not available",
            serde_json::Value::Null,
        ));
    };

    let now = crate::api::now();
    let at = to_rfc3339(now);

    let target_id = if let Some(existing_id) = input.error_target_id.as_deref() {
        let existing: Option<(String, u32, u8)> = transaction
            .query_row(
                "SELECT learner_language_profile_id, occurrence_count, method_change_required
                 FROM error_targets
                 WHERE id = ?1",
                params![existing_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        let Some((target_profile_id, occurrence_count, method_change_required)) = existing else {
            return Err(crate::Error::operation(
                404,
                "not_found",
                "unknown error target",
                serde_json::Value::Null,
            ));
        };
        if target_profile_id != input.language_profile_id {
            return Err(crate::Error::operation(
                409,
                "profile_mismatch",
                "error target belongs to another profile",
                serde_json::Value::Null,
            ));
        }

        let next_count = occurrence_count.saturating_add(1);
        let next_method_change = method_change_required != 0
            || learning_core::error_method_change_due_for(
                next_count,
                false,
                input.communicative_impact,
            );

        update_error_target(
            &transaction,
            existing_id,
            next_count,
            next_method_change,
            "recheck",
            &at,
        )?;

        existing_id.to_owned()
    } else {
        let target_id = crate::api::new_id("et");

        insert_error_target(
            &transaction,
            &target_id,
            &input.language_profile_id,
            &pack_id,
            manifest.version,
            crate::repositories::core_error_category_db(&input.core_category),
            input.pack_error_code.as_deref(),
            &input.original_production,
            &input.corrected_production,
            input.explanation.as_deref(),
            1,
            false,
            "active",
            &at,
        )?;

        for offset in learning_core::ERROR_RECHECK_OFFSETS_DAYS {
            let due = learning_core::Timestamp(now.0 + i64::from(offset) * 86_400);
            let recheck_id = crate::api::new_id("rc");
            let due_at = to_rfc3339(due);
            insert_error_recheck(&transaction, &recheck_id, &target_id, offset, &due_at)?;
        }

        target_id
    };

    let occurrence_id = crate::api::new_id("eo");
    insert_error_occurrence(
        &transaction,
        &occurrence_id,
        &target_id,
        None,
        None,
        crate::repositories::communicative_impact_db(&input.communicative_impact),
        &at,
    )?;

    transaction.commit()?;
    Ok(target_id)
}
