//! Full data export + cards.csv (08-http-api.md §7; DoD "export works").
//! Versioned JSON, bounded queries; single-user data sizes.

use crate::api::{db, error_response, query_param};
use worker::{D1Database, Env, Request, Response, Result};

const ROW_CAP: u32 = 10_000; // sanity bound per table; personal use « this

async fn rows(
    database: &D1Database,
    sql: &str,
    profile_id: &str,
) -> Result<Vec<serde_json::Value>> {
    database
        .prepare(sql)
        .bind(&[profile_id.into(), ROW_CAP.into()])?
        .all()
        .await?
        .results()
}

/// `GET /api/v1/export/full?language_profile_id={id}` — complete versioned
/// learning data for the profile.
pub async fn full(req: &Request, env: &Env) -> Result<Response> {
    let url = req.url()?;
    let Some(profile) = query_param(&url, "language_profile_id") else {
        return error_response(400, "missing_param", "language_profile_id required");
    };
    let database = db(env)?;

    let sections: [(&str, &str); 11] = [
        (
            "lessons",
            "SELECT * FROM lessons WHERE learner_language_profile_id = ?1 LIMIT ?2",
        ),
        (
            "lesson_metrics",
            "SELECT m.* FROM lesson_metrics m JOIN lessons l ON l.id = m.lesson_id WHERE l.learner_language_profile_id = ?1 LIMIT ?2",
        ),
        (
            "learning_items",
            "SELECT * FROM learning_items WHERE learner_language_profile_id = ?1 LIMIT ?2",
        ),
        (
            "learning_item_evidence",
            "SELECT e.* FROM learning_item_evidence e JOIN learning_items i ON i.id = e.learning_item_id WHERE i.learner_language_profile_id = ?1 LIMIT ?2",
        ),
        (
            "error_targets",
            "SELECT * FROM error_targets WHERE learner_language_profile_id = ?1 LIMIT ?2",
        ),
        (
            "error_occurrences",
            "SELECT o.* FROM error_occurrences o JOIN error_targets t ON t.id = o.error_target_id WHERE t.learner_language_profile_id = ?1 LIMIT ?2",
        ),
        (
            "error_rechecks",
            "SELECT r.* FROM error_rechecks r JOIN error_targets t ON t.id = r.error_target_id WHERE t.learner_language_profile_id = ?1 LIMIT ?2",
        ),
        (
            "listening_attempts",
            "SELECT * FROM listening_attempts WHERE learner_language_profile_id = ?1 LIMIT ?2",
        ),
        (
            "cards",
            "SELECT * FROM cards WHERE learner_language_profile_id = ?1 LIMIT ?2",
        ),
        (
            "card_schedule_state",
            "SELECT s.* FROM card_schedule_state s JOIN cards c ON c.id = s.card_id WHERE c.learner_language_profile_id = ?1 LIMIT ?2",
        ),
        (
            "review_sessions",
            "SELECT * FROM review_sessions WHERE learner_language_profile_id = ?1 LIMIT ?2",
        ),
    ];
    let mut body = serde_json::Map::new();
    body.insert("schema_version".to_owned(), serde_json::json!("1"));
    body.insert("language_profile_id".to_owned(), serde_json::json!(profile));
    for (name, sql) in sections {
        body.insert(
            name.to_owned(),
            serde_json::Value::Array(rows(&database, sql, &profile).await?),
        );
    }
    // Events + mutations + tags + recommendations join through their parents.
    for (name, sql) in [
        (
            "review_events",
            "SELECT e.* FROM review_events e JOIN cards c ON c.id = e.card_id WHERE c.learner_language_profile_id = ?1 LIMIT ?2",
        ),
        (
            "card_mutations",
            "SELECT m.* FROM card_mutations m LEFT JOIN cards c ON c.id = m.card_id WHERE c.learner_language_profile_id = ?1 OR m.card_id IS NULL LIMIT ?2",
        ),
        (
            "card_tags",
            "SELECT t.* FROM card_tags t JOIN cards c ON c.id = t.card_id WHERE c.learner_language_profile_id = ?1 LIMIT ?2",
        ),
        (
            "recommendations",
            "SELECT * FROM recommendations WHERE learner_language_profile_id = ?1 LIMIT ?2",
        ),
    ] {
        body.insert(
            name.to_owned(),
            serde_json::Value::Array(rows(&database, sql, &profile).await?),
        );
    }
    Response::from_json(&serde_json::Value::Object(body))
}

/// `GET /api/v1/export/cards.csv?language_profile_id={id}`.
pub async fn cards_csv(req: &Request, env: &Env) -> Result<Response> {
    let url = req.url()?;
    let Some(profile) = query_param(&url, "language_profile_id") else {
        return error_response(400, "missing_param", "language_profile_id required");
    };
    #[derive(serde::Deserialize)]
    struct Row {
        id: String,
        intent: String,
        status: String,
        prompt: String,
        answer: String,
        accepted_answers_json: String,
        creation_reason: String,
    }
    let list: Vec<Row> = db(env)?
        .prepare(
            "SELECT id, intent, status, prompt, answer, accepted_answers_json,
                    creation_reason
             FROM cards WHERE learner_language_profile_id = ?1 LIMIT ?2",
        )
        .bind(&[profile.into(), ROW_CAP.into()])?
        .all()
        .await?
        .results()?;

    let mut csv = String::with_capacity(list.len() * 96 + 64);
    csv.push_str("id,intent,status,prompt,answer,accepted_answers,creation_reason\n");
    for row in &list {
        let accepted: Vec<String> =
            serde_json::from_str(&row.accepted_answers_json).unwrap_or_default();
        for field in [
            &row.id,
            &row.intent,
            &row.status,
            &row.prompt,
            &row.answer,
            &accepted.join(" | "),
            &row.creation_reason,
        ] {
            csv.push('"');
            csv.push_str(&field.replace('"', "\"\""));
            csv.push_str("\",");
        }
        csv.pop();
        csv.push('\n');
    }
    let headers = worker::Headers::new();
    headers.set("content-type", "text/csv; charset=utf-8")?;
    headers.set("content-disposition", "attachment; filename=cards.csv")?;
    Ok(Response::ok(csv)?.with_headers(headers))
}
