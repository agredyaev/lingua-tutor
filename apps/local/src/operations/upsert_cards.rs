#![allow(clippy::items_after_test_module)] // Test fixtures are intentionally grouped before the processing pass.
//! Standalone card mutation operation on fixed-capacity state/write buffers.

#[cfg(test)]
// Test fixtures intentionally fail fast and use compact tuple snapshots.
#[allow(clippy::unwrap_used, clippy::type_complexity)]
mod tests {
    use super::*;
    use crate::Env;
    use std::path::PathBuf;

    fn create_env(label: &str) -> (PathBuf, Env) {
        let path = std::env::temp_dir().join(format!(
            "lingua-upsert-{label}-{}.sqlite3",
            uuid::Uuid::new_v4()
        ));
        let connection = crate::storage::open(&path).unwrap();
        connection
            .execute_batch(
                r#"
             INSERT INTO learner_profile (id, created_at, updated_at)
             VALUES ('learner-1', '2026-10-16T00:00:00Z', '2026-10-16T00:00:00Z');
             INSERT INTO learner_language_profiles
                (id, learner_id, source_language, target_locale,
                 created_at, updated_at)
             VALUES
                ('llp-1', 'learner-1', 'ru', 'es-ES',
                 '2026-10-16T00:00:00Z', '2026-10-16T00:00:00Z');
             INSERT INTO cards
                (id, learner_language_profile_id,
                 intent, status, prompt, prompt_language, answer, answer_language,
                 accepted_answers_json, creation_reason,
                 content_hash, content_version, created_by, created_at, updated_at)
             VALUES
                ('card-1', 'llp-1', 'production', 'review',
                 'old prompt', 'ru', 'answer', 'es-ES', '["old"]',
                 'seed', 'seed-hash', 1, 'tutor',
                 '2026-10-15T00:00:00Z', '2026-10-15T00:00:00Z');"#,
            )
            .unwrap();
        drop(connection);
        (path.clone(), Env::new(path, "test-signing-key".to_owned()))
    }

    fn mutations() -> Vec<tutor_contract::CardMutation> {
        vec![
            tutor_contract::CardMutation {
                operation: tutor_contract::MutationOperation::Create,
                client_operation_id: "op-create".to_owned(),
                card_id: None,
                card: Some(tutor_contract::CardPayload {
                    intent: learning_core::CardIntent::Production,
                    prompt: "created".to_owned(),
                    answer: "respuesta".to_owned(),
                    accepted_answers: vec!["alt".to_owned()],
                    prompt_language: Some("ru".to_owned()),
                    explanation: Some("why".to_owned()),
                    scenario_tags: vec!["Shop".to_owned()],
                    skill_tags: vec!["Grammar".to_owned()],
                    source_learning_item_id: None,
                    source_error_target_id: None,
                    creation_reason: "standalone".to_owned(),
                    priority_score: Some(0.7),
                }),
                accepted_answer: None,
                reason: Some("create".to_owned()),
            },
            tutor_contract::CardMutation {
                operation: tutor_contract::MutationOperation::Update,
                client_operation_id: "op-update".to_owned(),
                card_id: Some("card-1".to_owned()),
                card: Some(tutor_contract::CardPayload {
                    intent: learning_core::CardIntent::Production,
                    prompt: "updated".to_owned(),
                    answer: "answer".to_owned(),
                    accepted_answers: vec!["old".to_owned()],
                    prompt_language: Some("ru".to_owned()),
                    explanation: None,
                    scenario_tags: vec![],
                    skill_tags: vec![],
                    source_learning_item_id: None,
                    source_error_target_id: None,
                    creation_reason: "update".to_owned(),
                    priority_score: None,
                }),
                accepted_answer: None,
                reason: Some("update".to_owned()),
            },
        ]
    }

    fn snapshot(env: &Env) -> Vec<(String, String, String, String)> {
        let db = env.connection().unwrap();
        db.prepare(
            "SELECT prompt, status, accepted_answers_json, content_hash
             FROM cards ORDER BY prompt",
        )
        .unwrap()
        .query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap()
    }

    #[test]
    fn standalone_upsert_creates_and_updates_expected_card_state() {
        let (path, env) = create_env("new");
        let input = mutations();
        upsert_cards(&env, "llp-1", &input).unwrap();

        let rows = snapshot(&env);
        assert_eq!(rows.len(), 2);
        let created = rows.iter().find(|row| row.0 == "created").unwrap();
        assert_eq!(created.1, "new");
        assert_eq!(created.2, r#"["alt"]"#);
        assert_eq!(
            created.3,
            learning_core::card_content_hash(
                learning_core::CardIntent::Production,
                "created",
                "respuesta",
                &["alt".to_owned()],
            )
        );
        let updated = rows.iter().find(|row| row.0 == "updated").unwrap();
        assert_eq!(updated.1, "review");
        assert_eq!(updated.2, r#"["old"]"#);
        assert_eq!(
            updated.3,
            learning_core::card_content_hash(
                learning_core::CardIntent::Production,
                "updated",
                "answer",
                &["old".to_owned()],
            )
        );
        let _ = std::fs::remove_file(path);
    }
}

use crate::{Env, Result};
use rusqlite::{OptionalExtension as _, TransactionBehavior, params};

use super::card_apply::{apply_card_mutations, prepare_card_mutations};

pub(crate) fn upsert_cards(
    env: &Env,
    profile_id: &str,
    mutations: &[tutor_contract::CardMutation],
) -> Result<serde_json::Value> {
    let mut database = env.connection()?;
    let transaction = database.transaction_with_behavior(TransactionBehavior::Immediate)?;

    let (source_language, target_locale): (String, String) = transaction
        .query_row(
            "SELECT source_language,target_locale
                 FROM learner_language_profiles WHERE id=?1",
            params![profile_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?
        .ok_or_else(|| {
            crate::Error::operation(404, "not_found", "unknown profile", serde_json::Value::Null)
        })?;

    let now = crate::api::now();
    let at = crate::repositories::to_rfc3339(now);
    let (state, plan) = prepare_card_mutations(&transaction, profile_id, mutations, now)?;
    let summary = apply_card_mutations(
        &transaction,
        &state,
        &plan,
        mutations,
        profile_id,
        &source_language,
        &target_locale,
        None,
        "standalone card mutation",
        now,
        &at,
    )?;

    transaction.commit()?;
    crate::json(&serde_json::json!({
        "card_mutations": summary.as_slice()
    }))
}
