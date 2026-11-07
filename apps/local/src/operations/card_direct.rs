//! Direct user card status transitions.

use crate::sqlite::to_rfc3339;
use crate::{Env, Result};
use rusqlite::{OptionalExtension as _, TransactionBehavior, params};

use super::card_write::{insert_card_mutation, update_card_status};

pub(crate) fn set_card_status(
    env: &Env,
    card_id: &str,
    status: learning_core::CardStatus,
    reason: Option<&str>,
) -> Result<serde_json::Value> {
    let mut database = env.connection()?;
    let transaction = database.transaction_with_behavior(TransactionBehavior::Immediate)?;

    let current = transaction
        .query_row(
            "SELECT status FROM cards WHERE id=?1",
            params![card_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?;

    let Some(current_status) = current else {
        return crate::api::operation_error(404, "not_found", "unknown card");
    };

    let target = crate::sqlite::card_status_db(&status);
    let valid = match status {
        learning_core::CardStatus::Suspended => current_status != "retired",
        learning_core::CardStatus::Review => current_status == "suspended",
        learning_core::CardStatus::Retired => true,
        _ => false,
    };
    if !valid {
        return crate::api::operation_error(
            409,
            "invalid_transition",
            &format!("cannot move card from {current_status} to {target}"),
        );
    }

    let at = to_rfc3339(crate::api::now());
    update_card_status(&transaction, card_id, target, reason, &at)?;

    let operation = match status {
        learning_core::CardStatus::Suspended => "suspend",
        learning_core::CardStatus::Retired => "retire",
        _ => "restore",
    };
    insert_card_mutation(
        &transaction,
        &crate::api::new_id("cm"),
        Some(card_id),
        operation,
        &crate::api::new_id("op"),
        None,
        None,
        None,
        "user",
        reason.unwrap_or("user action"),
        "applied",
        &at,
    )?;

    transaction.commit()?;
    crate::json(&serde_json::json!({ "card_id":card_id, "status":status }))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn status_transition_suspends_and_audits_reason() {
        let path = std::env::temp_dir().join(format!(
            "lingua-card-status-{}.sqlite3",
            uuid::Uuid::new_v4()
        ));
        let db = crate::storage::open(&path).unwrap();
        db.execute_batch(
            r#"
            INSERT INTO learner_profile(id,created_at,updated_at)
            VALUES('learner-1','a','a');
            INSERT INTO learner_language_profiles
              (id,learner_id,source_language,target_locale,created_at,updated_at)
            VALUES('llp-1','learner-1','ru','es-ES','a','a');
            INSERT INTO cards
              (id,learner_language_profile_id,intent,status,prompt,prompt_language,
               answer,answer_language,accepted_answers_json,creation_reason,
               content_hash,content_version,created_by,created_at,updated_at)
            VALUES('card-1','llp-1','production','review','p','ru','a','es-ES',
                   '[]','seed','hash',1,'tutor','a','a');"#,
        )
        .unwrap();
        drop(db);
        let env = Env::new(path.clone(), "test-signing-key".to_owned());

        set_card_status(
            &env,
            "card-1",
            learning_core::CardStatus::Suspended,
            Some("pause"),
        )
        .unwrap();

        let db = env.connection().unwrap();
        let status: String = db
            .query_row("SELECT status FROM cards WHERE id='card-1'", [], |row| {
                row.get(0)
            })
            .unwrap();
        let reason: String = db
            .query_row(
                "SELECT reason FROM card_mutations WHERE card_id='card-1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(status, "suspended");
        assert_eq!(reason, "pause");
        drop(db);
        let _ = std::fs::remove_file(path);
    }
}
