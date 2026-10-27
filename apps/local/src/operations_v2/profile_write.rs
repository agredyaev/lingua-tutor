#![allow(clippy::items_after_test_module)] // Test fixtures are intentionally grouped before the processing pass.
//! Profile mutation path without row/domain/row round-trips.

#[cfg(test)]
// Test fixtures intentionally fail fast and use compact tuple snapshots.
#[allow(clippy::unwrap_used, clippy::type_complexity)]
mod tests {
    use super::*;
    use crate::{Env, built_in_language_packs};
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
        Vec<(String, String, String, String, String, String)>,
        Vec<(String, u8, u16, u8, f64, u16)>,
        Vec<(
            String,
            String,
            String,
            Option<String>,
            u32,
            String,
            u8,
            String,
        )>,
    ) {
        let db = env.connection().unwrap();
        let packs = db
            .prepare(
                "SELECT id,package_name,version,target_language,target_locale,display_name
             FROM language_packs ORDER BY id",
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
                "SELECT language_pack_id,source_language,target_locale,estimated_level,
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
                    r.get(7)?,
                ))
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();

        (packs, learners, profiles)
    }

    #[test]
    fn unsupported_source_cannot_create_profile_or_mutate_database() {
        let (path, env) = create_env("unsupported-source");
        let registry = built_in_language_packs().unwrap();
        let error = create_profile(&registry, &env, "es-ES", "invalid-language").unwrap_err();
        assert!(matches!(
            error,
            crate::Error::Operation {
                status: 409,
                code: "pack_incompatible",
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
    fn create_persists_pack_learner_and_profile_contract() {
        let (path, env) = create_env("create");
        let registry = built_in_language_packs().unwrap();
        let manifest = language_pack_registry::manifest(&registry, "es-ES").unwrap();
        create_profile(&registry, &env, "es-ES", "ru").unwrap();

        let (packs, learners, profiles) = create_snapshot(&env);
        assert_eq!(
            packs,
            vec![(
                manifest.id.to_owned(),
                manifest.package.to_owned(),
                manifest.version.to_owned(),
                manifest.target_language.to_owned(),
                manifest.target_locale.to_owned(),
                manifest.display_name.to_owned(),
            )]
        );
        assert_eq!(
            learners,
            vec![("Europe/Madrid".to_owned(), 2, 20, 8, 0.90, 12)]
        );
        assert_eq!(
            profiles,
            vec![(
                "es-ES".to_owned(),
                "ru".to_owned(),
                manifest.target_locale.to_owned(),
                None,
                0,
                "active".to_owned(),
                1,
                "{}".to_owned(),
            )]
        );
        let _ = std::fs::remove_file(path);
    }

    fn seed_profile(env: &Env) {
        let db = env.connection().unwrap();
        db.execute_batch(
            r#"INSERT INTO language_packs
                (id,package_name,version,contract_version,target_language,target_locale,
                 display_name,installed_at,updated_at)
               VALUES ('es-ES','language-pack-es-es','0.2.0',1,'es','es-ES','Spanish (Spain)','a','a');
               INSERT INTO learner_profile(id,created_at,updated_at) VALUES('learner-1','a','a');
               INSERT INTO learner_language_profiles
                (id,learner_id,language_pack_id,source_language,target_locale,estimated_level,
                 current_lesson_number,status,is_default,preferences_json,created_at,updated_at)
               VALUES('llp-1','learner-1','es-ES','ru','es-ES','A1',7,'active',1,'{"x":1}','a','a');"#
        ).unwrap();
    }

    fn patch_snapshot(env: &Env) -> (Option<String>, String, u8, String, u32) {
        let db = env.connection().unwrap();
        db.query_row(
            "SELECT estimated_level,status,is_default,preferences_json,current_lesson_number
             FROM learner_language_profiles WHERE id='llp-1'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .unwrap()
    }

    #[test]
    fn patch_updates_requested_fields_and_preserves_lesson_counter() {
        let (path, env) = create_env("patch");
        seed_profile(&env);

        patch_profile(
            &env,
            "llp-1",
            PatchProfileInput {
                estimated_level: Some("A2"),
                status: Some("paused"),
                is_default: Some(false),
                preferences: Some(&serde_json::json!({"mode":"fast"})),
            },
        )
        .unwrap();

        assert_eq!(
            patch_snapshot(&env),
            (
                Some("A2".to_owned()),
                "paused".to_owned(),
                0,
                r#"{"mode":"fast"}"#.to_owned(),
                7,
            )
        );
        let _ = std::fs::remove_file(path);
    }
}

use crate::repositories::{packs, profiles, to_rfc3339};
use crate::{Env, Result};
use rusqlite::{TransactionBehavior, params};

pub(crate) struct PatchProfileInput<'a> {
    pub estimated_level: Option<&'a str>,
    pub status: Option<&'a str>,
    pub is_default: Option<bool>,
    pub preferences: Option<&'a serde_json::Value>,
}

pub(crate) fn create_profile(
    registry: &crate::BuiltInPackRegistry,
    env: &Env,
    language_pack_id: &str,
    source_language: &str,
) -> Result<serde_json::Value> {
    let Some(manifest) = language_pack_registry::manifest(registry, language_pack_id) else {
        return crate::api::operation_error(404, "not_found", "unknown language pack");
    };
    if manifest.contract_version != language_pack_api::CONTRACT_VERSION {
        return crate::api::operation_error(
            409,
            "pack_incompatible",
            &format!(
                "pack '{}' is contract-incompatible with this build",
                manifest.id
            ),
        );
    }
    if !manifest
        .supported_source_languages
        .iter()
        .any(|source| source == source_language)
    {
        return crate::api::operation_error(
            409,
            "pack_incompatible",
            &format!(
                "pack '{}' does not support source language '{}'",
                manifest.id, source_language
            ),
        );
    }

    let profile_id = crate::api::new_id("llp");
    let at = to_rfc3339(crate::api::now());
    let capabilities_json = serde_json::to_string(&manifest.capabilities)?;

    let mut database = env.connection()?;
    let transaction = database.transaction_with_behavior(TransactionBehavior::Immediate)?;

    let result = (|| -> rusqlite::Result<()> {
        transaction.execute(
            packs::UPSERT_PACK,
            params![
                manifest.id,
                manifest.package,
                manifest.version,
                manifest.contract_version,
                manifest.target_language,
                manifest.target_locale,
                manifest.display_name,
                capabilities_json,
                at,
            ],
        )?;
        transaction.execute(
            profiles::UPSERT_LEARNER,
            params!["learner-1", "Europe/Madrid", 2, 20, 8, 0.90, 12, at],
        )?;
        transaction.execute(
            profiles::INSERT_LANGUAGE_PROFILE,
            params![
                profile_id,
                "learner-1",
                manifest.id,
                source_language,
                manifest.target_locale,
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
                "active profile for this pack exists",
            );
        }
        return Err(error.into());
    }

    transaction.commit()?;
    crate::json(&serde_json::json!({"id":profile_id}))
}

pub(crate) fn patch_profile(
    env: &Env,
    profile_id: &str,
    input: PatchProfileInput<'_>,
) -> Result<serde_json::Value> {
    if let Some(status) = input.status
        && !matches!(status, "active" | "paused" | "archived")
    {
        return crate::api::operation_error(400, "invalid_field", "status outside closed set");
    }

    let preferences_json = input.preferences.map(serde_json::to_string).transpose()?;
    let is_default = input.is_default.map(u8::from);
    let at = to_rfc3339(crate::api::now());

    let mut database = env.connection()?;
    let transaction = database.transaction_with_behavior(TransactionBehavior::Immediate)?;

    let changed = transaction.execute(
        "UPDATE learner_language_profiles SET
            estimated_level = COALESCE(?2, estimated_level),
            status = COALESCE(?3, status),
            is_default = COALESCE(?4, is_default),
            preferences_json = COALESCE(?5, preferences_json),
            updated_at = ?6
         WHERE id = ?1",
        params![
            profile_id,
            input.estimated_level,
            input.status,
            is_default,
            preferences_json,
            at,
        ],
    )?;
    if changed == 0 {
        return crate::api::operation_error(404, "not_found", "unknown profile");
    }

    let (
        learner_id,
        language_pack_id,
        source_language,
        target_locale,
        estimated_level,
        current_lesson_number,
        status,
        is_default,
        preferences_json,
    ) = transaction.query_row(
        "SELECT learner_id,language_pack_id,source_language,target_locale,
                estimated_level,current_lesson_number,status,is_default,preferences_json
         FROM learner_language_profiles WHERE id=?1",
        params![profile_id],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, u32>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, u8>(7)?,
                row.get::<_, String>(8)?,
            ))
        },
    )?;

    transaction.commit()?;

    let preferences: serde_json::Value = serde_json::from_str(&preferences_json)?;
    crate::json(&serde_json::json!({
        "id": profile_id,
        "learner_id": learner_id,
        "language_pack_id": language_pack_id,
        "source_language": source_language,
        "target_locale": target_locale,
        "estimated_level": estimated_level,
        "current_lesson_number": current_lesson_number,
        "status": status,
        "is_default": is_default != 0,
        "preferences": preferences,
    }))
}
