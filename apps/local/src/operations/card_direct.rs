#![allow(clippy::items_after_test_module)] // Test fixtures are intentionally grouped before the processing pass.
//! Direct card patch/status operations on fixed buffers.

#[cfg(test)]
// Test fixtures intentionally fail fast and use compact tuple snapshots.
#[allow(clippy::unwrap_used, clippy::type_complexity)]
mod tests {
    use super::*;
    use crate::Env;
    use std::path::PathBuf;

    fn env(label: &str) -> (PathBuf, Env) {
        let path = std::env::temp_dir().join(format!(
            "lingua-card-direct-{label}-{}.sqlite3",
            uuid::Uuid::new_v4()
        ));
        let db = crate::storage::open(&path).unwrap();
        db.execute_batch(
            r#"INSERT INTO language_packs
                (id,package_name,version,contract_version,target_language,target_locale,display_name,installed_at,updated_at)
               VALUES('es-ES','language-pack-es-es','0.2.0',1,'es','es-ES','Spanish (Spain)','a','a');
               INSERT INTO learner_profile(id,created_at,updated_at) VALUES('learner-1','a','a');
               INSERT INTO learner_language_profiles
                (id,learner_id,language_pack_id,source_language,target_locale,created_at,updated_at)
               VALUES('llp-1','learner-1','es-ES','ru','es-ES','a','a');
               INSERT INTO cards
                (id,learner_language_profile_id,language_pack_id,language_pack_version,intent,status,prompt,prompt_language,
                 answer,answer_language,accepted_answers_json,answer_evaluation_mode,explanation,pronunciation_note,
                 creation_reason,content_hash,content_version,created_by,created_at,updated_at)
               VALUES
                ('card-1','llp-1','es-ES','0.2.0','production','review','old prompt','ru','answer','es-ES',
                 '["old"]','exact','old exp',NULL,'seed','old-hash',1,'tutor','a','a'),
                ('card-2','llp-1','es-ES','0.2.0','production','review','two','ru','answer','es-ES',
                 '[]','exact',NULL,NULL,'seed','h2',1,'tutor','a','a');"#
        ).unwrap();
        drop(db);
        (path.clone(), Env::new(path, "test-signing-key".to_owned()))
    }

    fn snapshot(
        env: &Env,
        id: &str,
    ) -> (
        String,
        String,
        String,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        u32,
    ) {
        let db = env.connection().unwrap();
        let (prompt,answer,status,accepted,exp,note,hash):(String,String,String,String,Option<String>,Option<String>,String)=db.query_row(
            "SELECT prompt,answer,status,accepted_answers_json,explanation,pronunciation_note,content_hash FROM cards WHERE id=?1",
            [id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?))
        ).unwrap();
        let audits: u32 = db
            .query_row(
                "SELECT COUNT(*) FROM card_mutations WHERE card_id=?1",
                [id],
                |r| r.get(0),
            )
            .unwrap();
        (
            prompt,
            answer,
            status,
            accepted,
            exp,
            note,
            Some(hash),
            audits,
        )
    }

    #[test]
    fn cosmetic_patch_updates_content_without_changing_answer_or_status() {
        let (path, env) = env("patch");
        patch_card(
            &env,
            "card-1",
            CardPatchInput {
                edit_class: "cosmetic",
                prompt: Some("new prompt"),
                answer: None,
                explanation: Some("new exp"),
                pronunciation_note: Some("note"),
                accepted_answers: Some(&["old".to_owned(), "alt".to_owned()]),
            },
        )
        .unwrap();
        let state = snapshot(&env, "card-1");
        assert_eq!(state.0, "new prompt");
        assert_eq!(state.1, "answer");
        assert_eq!(state.2, "review");
        assert_eq!(state.3, r#"["old","alt"]"#);
        assert_eq!(state.4, Some("new exp".to_owned()));
        assert_eq!(state.5, Some("note".to_owned()));
        assert_eq!(state.7, 1);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn escaped_accepted_answers_survive_unrelated_patch() {
        let (path, env) = env("json-esc");
        let source = r#"["ni\u00f1o","line\nbreak","quote\"here","🚀"]"#;
        env.connection()
            .unwrap()
            .execute(
                "UPDATE cards SET accepted_answers_json=?2 WHERE id=?1",
                params!["card-1", source],
            )
            .unwrap();
        patch_card(
            &env,
            "card-1",
            CardPatchInput {
                edit_class: "cosmetic",
                prompt: Some("nuevo"),
                answer: None,
                explanation: None,
                pronunciation_note: None,
                accepted_answers: None,
            },
        )
        .unwrap();
        let actual = snapshot(&env, "card-1");
        let decoded: Vec<String> = serde_json::from_str(&actual.3).unwrap();
        assert_eq!(decoded, ["niño", "line\nbreak", "quote\"here", "🚀"]);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn invalid_accepted_json_is_atomic_even_with_replacement() {
        let (path, env) = env("invalid-accepted");
        let db = env.connection().unwrap();
        for invalid in [
            "null".to_owned(),
            "[42]".to_owned(),
            "[\"broken\", \"\\q\"]".to_owned(),
            serde_json::to_string(
                &(0..=tutor_contract::bounds::MAX_ACCEPTED_ANSWERS)
                    .map(|i| format!("answer-{i}"))
                    .collect::<Vec<_>>(),
            )
            .unwrap(),
        ] {
            db.execute(
                "UPDATE cards SET accepted_answers_json=?2 WHERE id=?1",
                params!["card-1", invalid],
            )
            .unwrap();
            let before = snapshot(&env, "card-1");
            for accepted_answers in [None, Some(vec!["replacement".to_owned()])] {
                let patch = CardPatchInput {
                    edit_class: "cosmetic",
                    prompt: Some("new prompt"),
                    answer: None,
                    explanation: None,
                    pronunciation_note: None,
                    accepted_answers: accepted_answers.as_deref(),
                };
                assert!(patch_card(&env, "card-1", patch).is_err());
                assert_eq!(snapshot(&env, "card-1"), before);
            }
        }
        drop(db);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn semantic_patch_changes_answer_and_resets_schedule() {
        let (path, env) = env("semantic");
        patch_card(
            &env,
            "card-1",
            CardPatchInput {
                edit_class: "semantic",
                prompt: None,
                answer: Some("new answer"),
                explanation: None,
                pronunciation_note: None,
                accepted_answers: None,
            },
        )
        .unwrap();
        let state = snapshot(&env, "card-1");
        assert_eq!(state.1, "new answer");
        assert_eq!(state.7, 1);
        let (count, reviews): (u32,u32) = env.connection().unwrap().query_row(
            "SELECT COUNT(*), COALESCE(MAX(review_count),0) FROM card_schedule_state WHERE card_id='card-1'",
            [], |r| Ok((r.get(0)?,r.get(1)?))
        ).unwrap();
        assert_eq!((count, reviews), (1, 0));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn status_transition_suspends_and_audits_reason() {
        let (path, env) = env("status");
        set_card_status(
            &env,
            "card-2",
            learning_core::CardStatus::Suspended,
            Some("pause"),
        )
        .unwrap();
        let state = snapshot(&env, "card-2");
        assert_eq!(state.2, "suspended");
        assert_eq!(state.7, 1);
        let reason: String = env
            .connection()
            .unwrap()
            .query_row(
                "SELECT reason FROM card_mutations WHERE card_id='card-2'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(reason, "pause");
        let _ = std::fs::remove_file(path);
    }
}

use crate::repositories::to_rfc3339;
use crate::{Env, Result};
use rusqlite::{OptionalExtension as _, TransactionBehavior, params};

use super::card_plan::{TextRef, decode_accepted_answers, text_at};
use super::card_write::{
    insert_card_mutation, update_card_content, update_card_status, upsert_schedule_state,
};

pub(crate) struct CardPatchInput<'a> {
    pub edit_class: &'a str,
    pub prompt: Option<&'a str>,
    pub answer: Option<&'a str>,
    pub explanation: Option<&'a str>,
    pub pronunciation_note: Option<&'a str>,
    pub accepted_answers: Option<&'a [String]>,
}

pub(crate) fn patch_card(
    env: &Env,
    card_id: &str,
    input: CardPatchInput<'_>,
) -> Result<serde_json::Value> {
    if !matches!(input.edit_class, "cosmetic" | "clarifying" | "semantic") {
        return crate::api::operation_error(
            400,
            "invalid_field",
            "edit_class must be cosmetic|clarifying|semantic",
        );
    }

    let mut database = env.connection()?;
    let transaction = database.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let current = transaction
        .query_row(
            "SELECT language_pack_id,language_pack_version,intent,prompt,answer,
                    accepted_answers_json,explanation,pronunciation_note
             FROM cards WHERE id=?1",
            params![card_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, Option<String>>(7)?,
                ))
            },
        )
        .optional()?;

    let Some((
        pack_id,
        pack_version,
        intent_text,
        current_prompt,
        current_answer,
        current_accepted_json,
        current_explanation,
        current_pronunciation_note,
    )) = current
    else {
        return crate::api::operation_error(404, "not_found", "unknown card");
    };

    let answer_changed = input
        .answer
        .is_some_and(|answer| !answer.trim().eq_ignore_ascii_case(current_answer.trim()));
    if answer_changed && input.edit_class != "semantic" {
        return crate::api::operation_error(
            409,
            "semantic_edit_required",
            "answer change requires edit_class=semantic (schedule reset)",
        );
    }

    let intent = crate::repositories::card_intent_from_db(&intent_text)
        .ok_or_else(|| crate::Error::RustError("stored card intent is invalid".to_owned()))?;

    const MAX_ACCEPTED: usize = tutor_contract::bounds::MAX_ACCEPTED_ANSWERS;
    let mut accepted_text = String::new();
    let mut accepted_refs = [TextRef::default(); MAX_ACCEPTED];
    let accepted_count = decode_accepted_answers(
        &current_accepted_json,
        &mut accepted_text,
        &mut accepted_refs,
    )?;

    let prompt = input.prompt.unwrap_or(&current_prompt);
    let answer = input.answer.unwrap_or(&current_answer);
    let explanation = input.explanation.or(current_explanation.as_deref());
    let pronunciation_note = input
        .pronunciation_note
        .or(current_pronunciation_note.as_deref());

    let (accepted_json, content_hash) = if let Some(accepted) = input.accepted_answers {
        (
            serde_json::to_string(accepted)?,
            learning_core::card_content_hash(intent, prompt, answer, accepted),
        )
    } else {
        let mut accepted = [""; MAX_ACCEPTED];
        for (slot, reference) in accepted
            .iter_mut()
            .zip(accepted_refs.iter())
            .take(accepted_count)
        {
            *slot = text_at(&accepted_text, *reference);
        }
        (
            serde_json::to_string(&accepted[..accepted_count])?,
            learning_core::card_content_hash_refs(
                intent,
                prompt,
                answer,
                accepted[..accepted_count].iter().copied(),
            ),
        )
    };

    let at = to_rfc3339(crate::api::now());

    update_card_content(
        &transaction,
        card_id,
        prompt,
        explanation,
        pronunciation_note,
        &accepted_json,
        &content_hash,
        &at,
    )?;

    if input.edit_class == "semantic" {
        transaction.execute(
            "UPDATE cards SET answer=?2, updated_at=?3 WHERE id=?1",
            params![card_id, answer, at],
        )?;

        let state = review_engine::initial_state(
            review_engine::FSRS_ID,
            review_engine::Timestamp(crate::api::now().0),
        )
        .map_err(|error| crate::Error::RustError(error.to_string()))?;
        let last_review_at = state
            .last_review_at
            .map(|value| to_rfc3339(learning_core::Timestamp(value.0)));
        let due_at = state
            .due_at
            .map(|value| to_rfc3339(learning_core::Timestamp(value.0)));
        let state_hash = review_engine::state_hash(&state);

        upsert_schedule_state(
            &transaction,
            card_id,
            &state.algorithm,
            &state.algorithm_version,
            state.parameter_set_id.as_deref(),
            state.difficulty,
            state.stability,
            last_review_at.as_deref(),
            due_at.as_deref(),
            state.scheduled_interval_seconds,
            state.lapse_count,
            state.review_count,
            state.state_version,
            &state_hash,
            &at,
        )?;
    }

    let reason = format!("user edit ({})", input.edit_class);
    insert_card_mutation(
        &transaction,
        &crate::api::new_id("cm"),
        Some(card_id),
        "update",
        &crate::api::new_id("op"),
        None,
        None,
        None,
        "user",
        &reason,
        &pack_id,
        &pack_version,
        "applied",
        &at,
    )?;

    transaction.commit()?;
    crate::handlers::cards::get(env, card_id)
}

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
            "SELECT status,language_pack_id,language_pack_version
             FROM cards WHERE id=?1",
            params![card_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .optional()?;

    let Some((current_status, pack_id, pack_version)) = current else {
        return crate::api::operation_error(404, "not_found", "unknown card");
    };

    let target = crate::repositories::card_status_db(&status);
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
        &pack_id,
        &pack_version,
        "applied",
        &at,
    )?;

    transaction.commit()?;
    crate::json(&serde_json::json!({ "card_id":card_id, "status":status }))
}
