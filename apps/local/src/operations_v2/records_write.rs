#![allow(clippy::items_after_test_module)] // Test fixtures are intentionally grouped before the processing pass.
//! Fixed-capacity records mutation path.

#[cfg(test)]
// Test fixtures intentionally fail fast and use compact tuple snapshots.
#[allow(clippy::unwrap_used, clippy::type_complexity)]
mod tests {
    use super::*;
    use crate::Env;
    use std::path::PathBuf;

    fn create_env(label: &str) -> (PathBuf, Env) {
        let path = std::env::temp_dir().join(format!(
            "lingua-records-{label}-{}.sqlite3",
            uuid::Uuid::new_v4()
        ));
        let db = crate::storage::open(&path).unwrap();
        db.execute_batch(
            r#"INSERT INTO language_packs
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
                 '2026-10-16T00:00:00Z', '2026-10-16T00:00:00Z');
             INSERT INTO error_targets
                (id, learner_language_profile_id, language_pack_id,
                 language_pack_version, core_category, original_production,
                 corrected_production, occurrence_count, method_change_required,
                 status, created_at, updated_at)
             VALUES
                ('et-1', 'llp-1', 'es-ES', '0.2.0', 'grammar',
                 'wrong', 'right', 3, 0, 'recheck',
                 '2026-10-16T00:00:00Z', '2026-10-16T00:00:00Z');
             INSERT INTO error_rechecks
                (id, error_target_id, offset_days, due_at)
             VALUES
                ('rc-3', 'et-1', 3, '2026-10-19T00:00:00Z');"#,
        )
        .unwrap();
        drop(db);
        (path.clone(), Env::new(path, "test-signing-key".to_owned()))
    }

    fn recheck_snapshot(env: &Env) -> (String, Option<u8>, String, Vec<(String, String, String)>) {
        let db = env.connection().unwrap();
        let target_status: String = db
            .query_row(
                "SELECT status FROM error_targets WHERE id='et-1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let passed: Option<u8> = db
            .query_row(
                "SELECT passed FROM error_rechecks WHERE id='rc-3'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let completed_at: Option<String> = db
            .query_row(
                "SELECT completed_at FROM error_rechecks WHERE id='rc-3'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let recommendations = db
            .prepare(
                "SELECT action, detail_json, status
             FROM recommendations ORDER BY action",
            )
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();

        (
            target_status,
            passed,
            completed_at.map(|_| "set".to_owned()).unwrap_or_default(),
            recommendations,
        )
    }

    #[test]
    fn passed_recheck_completes_without_reactivation() {
        let (path, env) = create_env("passed");
        complete_error_recheck(&env, "et-1", "rc-3", true).unwrap();
        assert_eq!(
            recheck_snapshot(&env),
            ("recheck".to_owned(), Some(1), "set".to_owned(), Vec::new())
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn failed_recheck_reactivates_target_and_recommends_practice() {
        let (path, env) = create_env("failed");
        complete_error_recheck(&env, "et-1", "rc-3", false).unwrap();
        let (status, passed, completed, recommendations) = recheck_snapshot(&env);
        assert_eq!(status, "active");
        assert_eq!(passed, Some(0));
        assert_eq!(completed, "set");
        assert_eq!(recommendations.len(), 1);
        assert_eq!(recommendations[0].0, "practice_again");
        assert_eq!(recommendations[0].2, "open");
        assert!(recommendations[0].1.contains("failed the 3-day recheck"));
        let _ = std::fs::remove_file(path);
    }

    fn listening_snapshot(
        env: &Env,
    ) -> Vec<(
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        u8,
    )> {
        let db = env.connection().unwrap();
        db.prepare(
            "SELECT learner_language_profile_id, learning_item_id, lesson_id,
                    source_reference, transcript, success
             FROM listening_attempts ORDER BY rowid",
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
            ))
        })
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap()
    }

    #[test]
    fn listening_attempt_preserves_optional_source_metadata_and_requires_measurement() {
        let (path, env) = create_env("listen");
        create_listening_attempt(
            &env,
            ListeningAttemptInput {
                language_profile_id: "llp-1",
                learning_item_id: None,
                lesson_id: None,
                source_reference: Some(""),
                source_variant_id: None,
                talker_key: None,
                context_tag: "standalone:listen",
                transcript: None,
                success: true,
                assistance_level: 0,
                response_millis: 1_200,
                probe_phase: tutor_contract::ProbePhase::Diagnostic,
            },
        )
        .unwrap();

        assert_eq!(
            listening_snapshot(&env),
            vec![("llp-1".to_owned(), None, None, Some(String::new()), None, 1,)]
        );

        let error = create_listening_attempt(
            &env,
            ListeningAttemptInput {
                language_profile_id: "llp-1",
                learning_item_id: None,
                lesson_id: None,
                source_reference: None,
                source_variant_id: None,
                talker_key: None,
                context_tag: "",
                transcript: None,
                success: false,
                assistance_level: 4,
                response_millis: 600_001,
                probe_phase: tutor_contract::ProbePhase::Practice,
            },
        )
        .unwrap_err();
        assert!(matches!(
            error,
            crate::Error::Operation {
                status: 400,
                code: "invalid_field",
                ..
            }
        ));
        let _ = std::fs::remove_file(path);
    }
}

use crate::repositories::{records, to_rfc3339};
use crate::{Env, Result};
use rusqlite::{OptionalExtension as _, TransactionBehavior, params};

use super::lesson_write::insert_listening_attempt;
use super::recommendation_write::insert_recommendation;

pub(crate) struct ListeningAttemptInput<'a> {
    pub language_profile_id: &'a str,
    pub learning_item_id: Option<&'a str>,
    pub lesson_id: Option<&'a str>,
    pub source_reference: Option<&'a str>,
    pub source_variant_id: Option<&'a str>,
    pub talker_key: Option<&'a str>,
    pub context_tag: &'a str,
    pub transcript: Option<&'a str>,
    pub success: bool,
    pub assistance_level: u8,
    pub response_millis: u32,
    pub probe_phase: tutor_contract::ProbePhase,
}

pub(crate) fn complete_error_recheck(
    env: &Env,
    target_id: &str,
    recheck_id: &str,
    passed: bool,
) -> Result<serde_json::Value> {
    let mut database = env.connection()?;
    let transaction = database.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let at = to_rfc3339(crate::api::now());

    transaction.execute(
        records::COMPLETE_ERROR_RECHECK,
        params![recheck_id, at, u8::from(passed)],
    )?;

    let new_status = if passed { "recheck" } else { "active" };
    transaction.execute(
        "UPDATE error_targets SET status=?2, updated_at=?3 WHERE id=?1",
        params![target_id, new_status, at],
    )?;

    if !passed {
        let profile_id: Option<String> = transaction
            .query_row(
                "SELECT learner_language_profile_id
                 FROM error_targets WHERE id=?1",
                params![target_id],
                |row| row.get(0),
            )
            .optional()?;
        let offset_days = transaction
            .query_row(
                "SELECT offset_days FROM error_rechecks WHERE id=?1",
                params![recheck_id],
                |row| row.get::<_, u32>(0),
            )
            .optional()?
            .unwrap_or(1);

        if let Some(profile_id) = profile_id {
            let (subject_type, action, detail) = adaptation::recheck_failed_fields(offset_days);

            insert_recommendation(
                &transaction,
                &profile_id,
                subject_type,
                Some(target_id),
                action,
                &serde_json::json!({ "detail": detail }).to_string(),
                &at,
            )?;
        }
    }

    transaction.commit()?;
    crate::json(&serde_json::json!({ "recheck_id": recheck_id, "passed": passed }))
}

pub(crate) fn create_listening_attempt(
    env: &Env,
    input: ListeningAttemptInput<'_>,
) -> Result<serde_json::Value> {
    if input.assistance_level > 3 {
        return crate::api::operation_error(400, "invalid_field", "assistance_level must be 0..=3");
    }
    if input.response_millis > 600_000 {
        return crate::api::operation_error(
            400,
            "invalid_field",
            "response_millis must be <= 600000",
        );
    }
    if input.context_tag.trim().is_empty() {
        return crate::api::operation_error(400, "invalid_field", "context_tag must be non-empty");
    }

    let id = crate::api::new_id("la");
    let database = env.connection()?;
    let at = to_rfc3339(crate::api::now());

    insert_listening_attempt(
        &database,
        &id,
        input.language_profile_id,
        input.learning_item_id,
        input.lesson_id,
        input.source_reference,
        input.source_variant_id,
        input.talker_key,
        Some(input.context_tag),
        input.transcript,
        input.success,
        Some(input.assistance_level),
        Some(input.response_millis),
        Some(tutor_contract::probe_phase_id(input.probe_phase)),
        &at,
    )?;

    crate::json(&serde_json::json!({ "id": id }))
}
