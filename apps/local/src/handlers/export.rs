//! Full data export and cards.csv download operations.
//! Versioned JSON, bounded queries; single-user data sizes.

use crate::{Env, Result};
use rusqlite::{params, types::ValueRef};

pub(crate) struct Download {
    pub body: Vec<u8>,
    pub content_type: &'static str,
    pub filename: &'static str,
}

const ROW_CAP: u32 = 10_000;

fn rows(
    connection: &rusqlite::Connection,
    sql: &str,
    profile_id: &str,
) -> Result<Vec<serde_json::Value>> {
    let mut statement = connection.prepare(sql)?;
    let columns: Vec<String> = statement
        .column_names()
        .iter()
        .map(|name| (*name).to_owned())
        .collect();
    let mapped = statement.query_map(params![profile_id, ROW_CAP], |row| {
        let mut object = serde_json::Map::with_capacity(columns.len());
        for (index, name) in columns.iter().enumerate() {
            let value = match row.get_ref(index)? {
                ValueRef::Null => serde_json::Value::Null,
                ValueRef::Integer(value) => serde_json::json!(value),
                ValueRef::Real(value) => serde_json::json!(value),
                ValueRef::Text(value) => {
                    serde_json::Value::String(String::from_utf8_lossy(value).into_owned())
                }
                ValueRef::Blob(_) => {
                    return Err(rusqlite::Error::InvalidColumnType(
                        index,
                        name.clone(),
                        rusqlite::types::Type::Blob,
                    ));
                }
            };
            object.insert(name.clone(), value);
        }
        Ok(serde_json::Value::Object(object))
    })?;
    Ok(mapped.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub(crate) fn full_for(env: &Env, profile: &str) -> Result<Download> {
    let connection = env.connection()?;

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
            serde_json::Value::Array(rows(&connection, sql, profile)?),
        );
    }
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
            serde_json::Value::Array(rows(&connection, sql, profile)?),
        );
    }
    Ok(Download {
        body: serde_json::to_vec(&serde_json::Value::Object(body))?,
        content_type: "application/json",
        filename: "lingua-tutor-export.json",
    })
}

pub(crate) fn cards_csv_for(env: &Env, profile: &str) -> Result<Download> {
    struct Row {
        id: String,
        intent: String,
        status: String,
        prompt: String,
        answer: String,
        accepted_answers_json: String,
        creation_reason: String,
    }
    let connection = env.connection()?;
    let mut statement = connection.prepare(
        "SELECT id, intent, status, prompt, answer, accepted_answers_json,
                creation_reason
         FROM cards WHERE learner_language_profile_id = ?1 LIMIT ?2",
    )?;
    let mapped = statement.query_map(params![profile, ROW_CAP], |row| {
        Ok(Row {
            id: row.get(0)?,
            intent: row.get(1)?,
            status: row.get(2)?,
            prompt: row.get(3)?,
            answer: row.get(4)?,
            accepted_answers_json: row.get(5)?,
            creation_reason: row.get(6)?,
        })
    })?;
    let list = mapped.collect::<rusqlite::Result<Vec<_>>>()?;

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
    Ok(Download {
        body: csv.into_bytes(),
        content_type: "text/csv; charset=utf-8",
        filename: "cards.csv",
    })
}
