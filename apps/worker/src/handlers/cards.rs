//! Card endpoints (08-http-api.md §5; WBS 5.6). Split/merge/redesign land
//! with the redesign loop (WBS 8).

use crate::api::{db, error_response, new_id, now, query_param};
use crate::repositories::{cards, enum_str, to_rfc3339};
use learning_core::{Card, CardStatus};
use worker::{Env, Request, Response, Result};

/// `GET /api/v1/cards?language_profile_id={id}&status=…&intent=…`.
pub async fn list(req: &Request, env: &Env) -> Result<Response> {
    let url = req.url()?;
    let Some(profile_id) = query_param(&url, "language_profile_id") else {
        return error_response(400, "missing_param", "language_profile_id required");
    };
    let rows: Vec<cards::CardRow> = db(env)?
        .prepare(cards::select_cards_sql())
        .bind(&[
            profile_id.into(),
            query_param(&url, "status").map_or(worker::wasm_bindgen::JsValue::NULL, Into::into),
            query_param(&url, "intent").map_or(worker::wasm_bindgen::JsValue::NULL, Into::into),
            100.into(),
        ])?
        .all()
        .await?
        .results()?;
    let domain: Vec<_> = rows
        .into_iter()
        .filter_map(|r| r.into_domain().ok())
        .collect();
    Response::from_json(&domain)
}

/// `GET /api/v1/cards/{card_id}`.
pub async fn get(env: &Env, card_id: &str) -> Result<Response> {
    match fetch_card(env, card_id).await? {
        Some(card) => Response::from_json(&card),
        None => error_response(404, "not_found", "unknown card"),
    }
}

/// `PATCH /api/v1/cards/{card_id}` — requires `edit_class`; semantic edits
/// bump content_version and reset schedule via new initial state at next
/// review (07-tutor-contract.md §4 table).
pub async fn patch(mut req: Request, env: &Env, card_id: &str) -> Result<Response> {
    #[derive(serde::Deserialize)]
    struct Body {
        edit_class: String,
        #[serde(default)]
        prompt: Option<String>,
        #[serde(default)]
        answer: Option<String>,
        #[serde(default)]
        explanation: Option<String>,
        #[serde(default)]
        pronunciation_note: Option<String>,
        #[serde(default)]
        accepted_answers: Option<Vec<String>>,
    }
    let Ok(body) = req.json::<Body>().await else {
        return error_response(400, "invalid_body", "malformed patch");
    };
    if !["cosmetic", "clarifying", "semantic"].contains(&body.edit_class.as_str()) {
        return error_response(
            400,
            "invalid_field",
            "edit_class must be cosmetic|clarifying|semantic",
        );
    }
    let Some(mut card) = fetch_card(env, card_id).await? else {
        return error_response(404, "not_found", "unknown card");
    };
    let answer_changed = body
        .answer
        .as_ref()
        .is_some_and(|a| a.trim().to_lowercase() != card.answer.trim().to_lowercase());
    if answer_changed && body.edit_class != "semantic" {
        return error_response(
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

    let database = db(env)?;
    let at = to_rfc3339(now());
    let mut statements = vec![
        database.prepare(cards::UPDATE_CARD_CONTENT).bind(&[
            card_id.into(),
            card.prompt.clone().into(),
            card.explanation
                .clone()
                .map_or(worker::wasm_bindgen::JsValue::NULL, Into::into),
            card.pronunciation_note
                .clone()
                .map_or(worker::wasm_bindgen::JsValue::NULL, Into::into),
            serde_json::to_string(&card.accepted_answers)
                .unwrap_or_else(|_| "[]".to_owned())
                .into(),
            card.content_hash.clone().into(),
            at.clone().into(),
        ])?,
    ];
    if body.edit_class == "semantic" {
        // Semantic edit: answer column update + schedule reset (due now).
        statements.push(
            database
                .prepare("UPDATE cards SET answer = ?2, updated_at = ?3 WHERE id = ?1")
                .bind(&[
                    card_id.into(),
                    card.answer.clone().into(),
                    at.clone().into(),
                ])?,
        );
        statements.push(
            database
                .prepare("DELETE FROM card_schedule_state WHERE card_id = ?1")
                .bind(&[card_id.into()])?,
        );
    }
    database.batch(statements).await?;
    audit(
        &database,
        &card,
        "update",
        &format!("user edit ({})", body.edit_class),
    )
    .await?;
    Response::from_json(&card)
}

/// `POST /api/v1/cards/{card_id}/suspend|restore|retire`.
pub async fn set_status(
    mut req: Request,
    env: &Env,
    card_id: &str,
    status: CardStatus,
) -> Result<Response> {
    #[derive(serde::Deserialize, Default)]
    struct Body {
        #[serde(default)]
        reason: Option<String>,
    }
    let body = req.json::<Body>().await.unwrap_or_default();
    let Some(card) = fetch_card(env, card_id).await? else {
        return error_response(404, "not_found", "unknown card");
    };
    let valid = match status {
        CardStatus::Suspended => card.status != CardStatus::Retired,
        CardStatus::Review => card.status == CardStatus::Suspended,
        CardStatus::Retired => true,
        _ => false,
    };
    if !valid {
        return error_response(
            409,
            "invalid_transition",
            &format!("cannot move card from {:?} to {status:?}", card.status),
        );
    }
    let database = db(env)?;
    database
        .prepare(cards::UPDATE_CARD_STATUS)
        .bind(&[
            card_id.into(),
            enum_str(&status).into(),
            body.reason
                .clone()
                .map_or(worker::wasm_bindgen::JsValue::NULL, Into::into),
            to_rfc3339(now()).into(),
        ])?
        .run()
        .await?;
    let operation = match status {
        CardStatus::Suspended => "suspend",
        CardStatus::Retired => "retire",
        _ => "restore",
    };
    audit(
        &database,
        &card,
        operation,
        body.reason.as_deref().unwrap_or("user action"),
    )
    .await?;
    Response::from_json(&serde_json::json!({ "card_id": card_id, "status": status }))
}

async fn fetch_card(env: &Env, card_id: &str) -> Result<Option<Card>> {
    let row = db(env)?
        .prepare(cards::select_card_sql())
        .bind(&[card_id.into()])?
        .first::<cards::CardRow>(None)
        .await?;
    Ok(row.and_then(|r| r.into_domain().ok()))
}

async fn audit(
    database: &worker::D1Database,
    card: &Card,
    operation: &str,
    reason: &str,
) -> Result<()> {
    database
        .prepare(cards::INSERT_CARD_MUTATION)
        .bind(&[
            new_id("cm").into(),
            card.id.0.clone().into(),
            operation.into(),
            new_id("op").into(),
            worker::wasm_bindgen::JsValue::NULL,
            worker::wasm_bindgen::JsValue::NULL,
            worker::wasm_bindgen::JsValue::NULL,
            "user".into(),
            reason.into(),
            card.language_pack_id.0.clone().into(),
            card.language_pack_version.to_string().into(),
            "applied".into(),
            to_rfc3339(now()).into(),
        ])?
        .run()
        .await?;
    Ok(())
}
