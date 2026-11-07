#![allow(clippy::items_after_test_module)] // Test fixtures are intentionally grouped before the processing pass.
//! Profile mutation path without row/domain/row round-trips.

#[cfg(test)]
// Test fixtures intentionally fail fast and use compact tuple snapshots.
#[allow(clippy::unwrap_used, clippy::type_complexity)]
mod tests {
    use super::*;
    use crate::Env;
    use std::path::PathBuf;

    fn create_env(label: &str) -> (PathBuf, Env) {
        let path = std::env::temp_dir().join(format!(
            "lingua-profile-{label}-{}.sqlite3",
            uuid::Uuid::new_v4()
        ));
        let db = crate::storage::open(&path).unwrap();
        drop(db);
        (path.clone(), Env::new(path, "test-signing-key".to_owned()))
    }

    fn create_snapshot(
        env: &Env,
    ) -> (
        Vec<(String, u8, u16, u8, f64, u16)>,
        Vec<(String, String, Option<String>, u32, String, u8, String)>,
    ) {
        let db = env.connection().unwrap();
        let learners = db
            .prepare(
                "SELECT timezone,daily_session_target,session_minutes_target,max_new_cards,
                    desired_retention,max_review_minutes
                 FROM learner_profile",
            )
            .unwrap()
            .query_map([], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                ))
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();

        let profiles = db
            .prepare(
                "SELECT source_language,target_locale,estimated_level,
                    current_lesson_number,status,is_default,preferences_json
                 FROM learner_language_profiles",
            )
            .unwrap()
            .query_map([], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                ))
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();

        (learners, profiles)
    }

    #[test]
    fn empty_locale_cannot_create_profile_or_mutate_database() {
        let (path, env) = create_env("empty-locale");
        let error = create_profile(&env, " ", "ru").unwrap_err();
        assert!(matches!(
            error,
            crate::Error::Operation {
                status: 400,
                code: "invalid_field",
                ..
            }
        ));
        let count: u32 = env
            .connection()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM learner_language_profiles",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn create_persists_locale_metadata_learner_and_profile_contract() {
        let (path, env) = create_env("create");
        create_profile(&env, "de-DE", "en").unwrap();

        let (learners, profiles) = create_snapshot(&env);
        assert_eq!(
            learners,
            vec![("Europe/Madrid".to_owned(), 2, 20, 8, 0.90, 12)]
        );
        assert_eq!(
            profiles,
            vec![(
                "en".to_owned(),
                "de-DE".to_owned(),
                None,
                0,
                "active".to_owned(),
                1,
                "{}".to_owned(),
            )]
        );
        let _ = std::fs::remove_file(path);
    }
}

use crate::sqlite::to_rfc3339;
use crate::{Env, Result};
use rusqlite::{TransactionBehavior, params};

const UPSERT_LEARNER: &str = "INSERT INTO learner_profile
    (id, timezone, daily_session_target, session_minutes_target, max_new_cards,
     desired_retention, max_review_minutes, created_at, updated_at)
 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)
 ON CONFLICT(id) DO UPDATE SET
    timezone = excluded.timezone,
    daily_session_target = excluded.daily_session_target,
    session_minutes_target = excluded.session_minutes_target,
    max_new_cards = excluded.max_new_cards,
    desired_retention = excluded.desired_retention,
    max_review_minutes = excluded.max_review_minutes,
    updated_at = excluded.updated_at";

const INSERT_LANGUAGE_PROFILE: &str = "INSERT INTO learner_language_profiles
    (id, learner_id, source_language, target_locale,
     estimated_level, current_lesson_number, status, is_default,
     preferences_json, created_at, updated_at)
 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10)";

pub(crate) fn create_profile(
    env: &Env,
    target_locale: &str,
    source_language: &str,
) -> Result<serde_json::Value> {
    if target_locale.trim().is_empty() || source_language.trim().is_empty() {
        return crate::api::operation_error(
            400,
            "invalid_field",
            "target_locale and source_language must be non-empty",
        );
    }

    let profile_id = crate::api::new_id("llp");
    let at = to_rfc3339(crate::api::now());

    let mut database = env.connection()?;
    let transaction = database.transaction_with_behavior(TransactionBehavior::Immediate)?;

    let result = (|| -> rusqlite::Result<()> {
        transaction.execute(
            UPSERT_LEARNER,
            params!["learner-1", "Europe/Madrid", 2, 20, 8, 0.90, 12, at],
        )?;
        transaction.execute(
            INSERT_LANGUAGE_PROFILE,
            params![
                profile_id,
                "learner-1",
                source_language,
                target_locale,
                Option::<String>::None,
                0_u32,
                "active",
                1_u8,
                "{}",
                at,
            ],
        )?;
        Ok(())
    })();

    if let Err(error) = result {
        if error.to_string().contains("UNIQUE") {
            return crate::api::operation_error(
                409,
                "profile_exists",
                "active profile for this target locale exists",
            );
        }
        return Err(error.into());
    }

    transaction.commit()?;
    crate::json(&serde_json::json!({"id":profile_id}))
}
