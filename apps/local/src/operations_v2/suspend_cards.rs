#![allow(clippy::items_after_test_module)] // Test fixtures are intentionally grouped before the processing pass.
//! Fixed-capacity standalone card suspension.

#[cfg(test)]
// Test fixtures intentionally fail fast and use compact tuple snapshots.
#[allow(clippy::unwrap_used, clippy::type_complexity)]
mod tests {
    use super::*;
    use crate::Env;
    use std::path::PathBuf;

    fn create_env(label: &str) -> (PathBuf, Env) {
        let path = std::env::temp_dir().join(format!(
            "lingua-suspend-{label}-{}.sqlite3",
            uuid::Uuid::new_v4()
        ));
        let connection = crate::storage::open(&path).unwrap();
        connection
            .execute_batch(
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
             INSERT INTO cards
                (id, learner_language_profile_id, language_pack_id, language_pack_version,
                 intent, status, prompt, prompt_language, answer, answer_language,
                 accepted_answers_json, answer_evaluation_mode, creation_reason,
                 content_hash, content_version, created_by, created_at, updated_at)
             VALUES
                ('card-a', 'llp-1', 'es-ES', '0.2.0', 'production', 'review',
                 'a', 'ru', 'a', 'es-ES', '[]', 'exact', 'seed', 'h-a', 1,
                 'tutor', '2026-10-15T00:00:00Z', '2026-10-15T00:00:00Z'),
                ('card-b', 'llp-1', 'es-ES', '0.2.0', 'production', 'candidate',
                 'b', 'ru', 'b', 'es-ES', '[]', 'exact', 'seed', 'h-b', 1,
                 'tutor', '2026-10-15T00:00:00Z', '2026-10-15T00:00:00Z');"#,
            )
            .unwrap();
        drop(connection);
        (path.clone(), Env::new(path, "test-signing-key".to_owned()))
    }

    fn snapshot(env: &Env) -> (Vec<(String, String)>, Vec<(String, String, String)>) {
        let db = env.connection().unwrap();
        let cards = db
            .prepare("SELECT id,status FROM cards ORDER BY id")
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        let audits = db
            .prepare("SELECT card_id,operation,reason FROM card_mutations ORDER BY card_id")
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        (cards, audits)
    }

    #[test]
    fn fixed_suspend_updates_cards_and_audits_atomically() {
        let (path, env) = create_env("new");
        let ids = serde_json::json!(["card-a", "card-b"]);
        suspend_cards_json(&env, &ids, Some("batch pause")).unwrap();

        assert_eq!(
            snapshot(&env),
            (
                vec![
                    ("card-a".to_owned(), "suspended".to_owned()),
                    ("card-b".to_owned(), "suspended".to_owned()),
                ],
                vec![
                    (
                        "card-a".to_owned(),
                        "suspend".to_owned(),
                        "batch pause".to_owned()
                    ),
                    (
                        "card-b".to_owned(),
                        "suspend".to_owned(),
                        "batch pause".to_owned()
                    ),
                ],
            )
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn parser_rejects_zero_and_over_capacity() {
        let (_path, env) = create_env("bounds");
        assert!(suspend_cards_json(&env, &serde_json::json!([]), None).is_err());
        let too_many: Vec<String> = (0..21).map(|i| format!("card-{i}")).collect();
        assert!(suspend_cards_json(&env, &serde_json::json!(too_many), None).is_err());
    }
}

use crate::{Env, Result};
use rusqlite::{OptionalExtension as _, TransactionBehavior, params};

use super::card_write::{insert_card_mutation, update_card_status};

const MAX_SUSPEND: usize = 20;

pub(crate) fn suspend_cards_json(
    env: &Env,
    ids_value: &serde_json::Value,
    reason: Option<&str>,
) -> Result<serde_json::Value> {
    let Some(ids) = ids_value.as_array() else {
        return Err(crate::Error::operation(
            400,
            "invalid_arguments",
            "'card_ids' (1-20 strings) is required",
            serde_json::Value::Null,
        ));
    };
    if ids.is_empty() || ids.len() > MAX_SUSPEND {
        return Err(crate::Error::operation(
            400,
            "invalid_arguments",
            "'card_ids' (1-20 strings) is required",
            serde_json::Value::Null,
        ));
    }

    for value in ids {
        if value.as_str().is_none_or(str::is_empty) {
            return Err(crate::Error::operation(
                400,
                "invalid_arguments",
                "'card_ids' (1-20 strings) is required",
                serde_json::Value::Null,
            ));
        }
    }

    let mut database = env.connection()?;
    let transaction = database.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let at = crate::repositories::to_rfc3339(crate::api::now());

    let mut results: [serde_json::Value; MAX_SUSPEND] =
        std::array::from_fn(|_| serde_json::Value::Null);

    for (index, value) in ids.iter().enumerate() {
        let Some(id) = value.as_str() else {
            return Err(crate::Error::operation(
                400,
                "invalid_arguments",
                "'card_ids' (1-20 strings) is required",
                serde_json::Value::Null,
            ));
        };
        let row: Option<(String, String, String)> = transaction
            .query_row(
                "SELECT status, language_pack_id, language_pack_version
                 FROM cards WHERE id=?1",
                params![id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;

        let Some((status, pack_id, pack_version)) = row else {
            return Err(crate::Error::operation(
                404,
                "not_found",
                "unknown card",
                serde_json::Value::Null,
            ));
        };

        if status == "retired" {
            return Err(crate::Error::operation(
                409,
                "invalid_transition",
                format!("cannot move card from {status} to suspended"),
                serde_json::Value::Null,
            ));
        }

        update_card_status(&transaction, id, "suspended", reason, &at)?;

        insert_card_mutation(
            &transaction,
            &crate::api::new_id("cm"),
            Some(id),
            "suspend",
            &crate::api::new_id("op"),
            None,
            None,
            None,
            "user",
            reason.unwrap_or("user action"),
            &pack_id,
            &pack_version,
            "applied",
            &at,
        )?;

        results[index] = serde_json::json!({
            "card_id": id,
            "status": "suspended"
        });
    }

    transaction.commit()?;

    crate::json(&serde_json::json!({
        "results": &results[..ids.len()]
    }))
}
