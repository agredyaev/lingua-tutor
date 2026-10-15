//! Card operations and persistence-facing card state transitions.

use crate::api::{new_id, now, operation_error};
use crate::repositories::{cards, enum_str, review, to_rfc3339};
use crate::{Env, Result};
use learning_core::{Card, CardStatus};
use review_engine::ReviewScheduler as _;
use rusqlite::{OptionalExtension as _, params};

pub(crate) fn list_for(
    env: &Env,
    profile_id: &str,
    status: Option<&str>,
    intent: Option<&str>,
) -> Result<serde_json::Value> {
    let connection = env.connection()?;
    let sql = cards::select_cards_sql();
    let mut statement = connection.prepare(&sql)?;
    let rows = statement
        .query_map(
            params![profile_id, status, intent, 100],
            cards::CardRow::from_row,
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let domain: Vec<_> = rows
        .into_iter()
        .filter_map(|row| row.into_domain().ok())
        .collect();
    crate::json(&domain)
}

pub(crate) fn get(env: &Env, card_id: &str) -> Result<serde_json::Value> {
    match fetch_card(env, card_id)? {
        Some(card) => crate::json(&card),
        None => operation_error(404, "not_found", "unknown card"),
    }
}

#[derive(serde::Deserialize)]
pub(crate) struct CardPatchParams {
    pub edit_class: String,
    #[serde(default)]
    pub prompt: Option<String>,
    #[serde(default)]
    pub answer: Option<String>,
    #[serde(default)]
    pub explanation: Option<String>,
    #[serde(default)]
    pub pronunciation_note: Option<String>,
    #[serde(default)]
    pub accepted_answers: Option<Vec<String>>,
}

pub(crate) fn patch(env: &Env, card_id: &str, body: CardPatchParams) -> Result<serde_json::Value> {
    if !["cosmetic", "clarifying", "semantic"].contains(&body.edit_class.as_str()) {
        return operation_error(
            400,
            "invalid_field",
            "edit_class must be cosmetic|clarifying|semantic",
        );
    }
    let Some(mut card) = fetch_card(env, card_id)? else {
        return operation_error(404, "not_found", "unknown card");
    };
    let answer_changed = body
        .answer
        .as_ref()
        .is_some_and(|answer| answer.trim().to_lowercase() != card.answer.trim().to_lowercase());
    if answer_changed && body.edit_class != "semantic" {
        return operation_error(
            409,
            "semantic_edit_required",
            "answer change requires edit_class=semantic (schedule reset)",
        );
    }

    if let Some(prompt) = body.prompt {
        card.prompt = prompt;
    }
    if let Some(answer) = body.answer {
        card.answer = answer;
    }
    if let Some(explanation) = body.explanation {
        card.explanation = Some(explanation);
    }
    if let Some(note) = body.pronunciation_note {
        card.pronunciation_note = Some(note);
    }
    if let Some(accepted) = body.accepted_answers {
        card.accepted_answers = accepted;
    }
    card.content_hash = Card::content_hash_of(
        card.intent,
        &card.prompt,
        &card.answer,
        &card.accepted_answers,
    );

    let at = to_rfc3339(now());
    let accepted_answers =
        serde_json::to_string(&card.accepted_answers).unwrap_or_else(|_| "[]".to_owned());
    let mut connection = env.connection()?;
    let transaction =
        connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    transaction.execute(
        cards::UPDATE_CARD_CONTENT,
        params![
            card_id,
            card.prompt,
            card.explanation,
            card.pronunciation_note,
            accepted_answers,
            card.content_hash,
            at,
        ],
    )?;

    if body.edit_class == "semantic" {
        transaction.execute(
            "UPDATE cards SET answer = ?2, updated_at = ?3 WHERE id = ?1",
            params![card_id, card.answer, at],
        )?;
        let scheduler = review_engine::FsrsScheduler::new()
            .map_err(|error| crate::Error::RustError(error.to_string()))?;
        let state = scheduler
            .initial_state(review_engine::InitialReviewInput {
                now: review_engine::Timestamp(now().0),
            })
            .map_err(|error| crate::Error::RustError(error.to_string()))?;
        let state_row = review::ScheduleStateRow::from_state(card_id, &state);
        transaction.execute(
            review::UPSERT_SCHEDULE_STATE,
            params![
                state_row.card_id,
                state_row.algorithm,
                state_row.algorithm_version,
                state_row.parameter_set_id,
                state_row.difficulty,
                state_row.stability,
                state_row.last_review_at,
                state_row.due_at,
                state_row.scheduled_interval_seconds,
                state_row.lapse_count,
                state_row.review_count,
                state_row.state_version,
                state_row.state_hash,
                at,
            ],
        )?;
    }
    audit(
        &transaction,
        &card,
        "update",
        &format!("user edit ({})", body.edit_class),
    )?;
    transaction.commit()?;
    crate::json(&card)
}

pub(crate) fn set_status_with_reason(
    env: &Env,
    card_id: &str,
    status: CardStatus,
    reason: Option<&str>,
) -> Result<serde_json::Value> {
    let connection = env.connection()?;
    set_status_on(&connection, card_id, status, reason)
}

pub(crate) fn set_status_on(
    connection: &rusqlite::Connection,
    card_id: &str,
    status: CardStatus,
    reason: Option<&str>,
) -> Result<serde_json::Value> {
    let Some(card) = fetch_card_on(connection, card_id)? else {
        return operation_error(404, "not_found", "unknown card");
    };
    let valid = match status {
        CardStatus::Suspended => card.status != CardStatus::Retired,
        CardStatus::Review => card.status == CardStatus::Suspended,
        CardStatus::Retired => true,
        _ => false,
    };
    if !valid {
        return operation_error(
            409,
            "invalid_transition",
            &format!("cannot move card from {:?} to {status:?}", card.status),
        );
    }
    connection.execute(
        cards::UPDATE_CARD_STATUS,
        params![card_id, enum_str(&status), reason, to_rfc3339(now())],
    )?;
    let operation = match status {
        CardStatus::Suspended => "suspend",
        CardStatus::Retired => "retire",
        _ => "restore",
    };
    audit(
        connection,
        &card,
        operation,
        reason.unwrap_or("user action"),
    )?;
    crate::json(&serde_json::json!({ "card_id": card_id, "status": status }))
}

fn fetch_card(env: &Env, card_id: &str) -> Result<Option<Card>> {
    let connection = env.connection()?;
    fetch_card_on(&connection, card_id)
}

fn fetch_card_on(connection: &rusqlite::Connection, card_id: &str) -> Result<Option<Card>> {
    let sql = cards::select_card_sql();
    let row = connection
        .query_row(&sql, params![card_id], cards::CardRow::from_row)
        .optional()?;
    Ok(row.and_then(|row| row.into_domain().ok()))
}

fn audit(
    connection: &rusqlite::Connection,
    card: &Card,
    operation: &str,
    reason: &str,
) -> Result<()> {
    connection.execute(
        cards::INSERT_CARD_MUTATION,
        params![
            new_id("cm"),
            card.id.0,
            operation,
            new_id("op"),
            Option::<String>::None,
            Option::<String>::None,
            Option::<String>::None,
            "user",
            reason,
            card.language_pack_id.0,
            card.language_pack_version.to_string(),
            "applied",
            to_rfc3339(now()),
        ],
    )?;
    Ok(())
}
