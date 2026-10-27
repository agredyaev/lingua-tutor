//! Card operations and persistence-facing card state transitions.

use crate::api::operation_error;
use crate::repositories::cards;
use crate::{Env, Result};
use rusqlite::{OptionalExtension as _, params};

// Each SQLite row is consumed once and projected directly into the wire value.
fn project_card_wire(row: &rusqlite::Row<'_>) -> rusqlite::Result<Option<serde_json::Value>> {
    let version: String = row.get("language_pack_version")?;
    let intent: String = row.get("intent")?;
    let status: String = row.get("status")?;
    let actor: String = row.get("created_by")?;
    let accepted: String = row.get("accepted_answers_json")?;
    let (Ok(version), Ok(_), Ok(_), Ok(_), Ok(accepted)) = (
        semver::Version::parse(&version),
        serde_json::from_value::<learning_core::CardIntent>(serde_json::json!(intent)),
        serde_json::from_value::<learning_core::CardStatus>(serde_json::json!(status)),
        serde_json::from_value::<learning_core::CardActor>(serde_json::json!(actor)),
        serde_json::from_str::<Vec<String>>(&accepted),
    ) else {
        return Ok(None);
    };
    Ok(Some(serde_json::json!({
        "id": row.get::<_, String>("id")?,
        "language_profile_id": row.get::<_, String>("learner_language_profile_id")?,
        "language_pack_id": row.get::<_, String>("language_pack_id")?,
        "language_pack_version": version.to_string(),
        "intent": intent,
        "status": status,
        "prompt": row.get::<_, String>("prompt")?,
        "prompt_language": row.get::<_, String>("prompt_language")?,
        "answer": row.get::<_, String>("answer")?,
        "answer_language": row.get::<_, String>("answer_language")?,
        "accepted_answers": accepted,
        "answer_evaluation_mode": row.get::<_, String>("answer_evaluation_mode")?,
        "explanation": row.get::<_, Option<String>>("explanation")?,
        "pronunciation_note": row.get::<_, Option<String>>("pronunciation_note")?,
        "source_learning_item_id": row.get::<_, Option<String>>("source_learning_item_id")?,
        "source_error_target_id": row.get::<_, Option<String>>("source_error_target_id")?,
        "source_lesson_id": row.get::<_, Option<String>>("source_lesson_id")?,
        "creation_reason": row.get::<_, String>("creation_reason")?,
        "priority_score": row.get::<_, Option<f64>>("priority_score")?,
        "content_hash": row.get::<_, String>("content_hash")?,
        "content_version": row.get::<_, u32>("content_version")?,
        "parent_card_id": row.get::<_, Option<String>>("parent_card_id")?,
        "suspended_reason": row.get::<_, Option<String>>("suspended_reason")?,
        "created_by": actor,
    })))
}

pub(crate) fn list_for(
    env: &Env,
    profile_id: &str,
    status: Option<&str>,
    intent: Option<&str>,
) -> Result<serde_json::Value> {
    let connection = env.connection()?;
    let sql = cards::select_cards_sql();
    let mut statement = connection.prepare(&sql)?;
    let mut rows = statement.query(params![profile_id, status, intent, 100])?;
    let mut output = Vec::new();
    while let Some(row) = rows.next()? {
        if let Some(wire) = project_card_wire(row)? {
            output.push(wire);
        }
    }
    crate::json(&output)
}

pub(crate) fn get(env: &Env, card_id: &str) -> Result<serde_json::Value> {
    let connection = env.connection()?;
    let sql = cards::select_card_sql();
    let item = connection
        .query_row(&sql, params![card_id], project_card_wire)
        .optional()?
        .flatten();
    match item {
        Some(wire) => crate::json(&wire),
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

#[cfg(test)]
// Test fixtures intentionally fail fast and use compact tuple snapshots.
#[allow(clippy::unwrap_used, clippy::type_complexity)]
mod dod_card_projection_tests {
    use super::*;

    const CARD_ROW: &str = "SELECT
        'card-1' AS id, 'llp-1' AS learner_language_profile_id,
        'es-ES' AS language_pack_id, '0.2.0' AS language_pack_version,
        'production' AS intent, 'review' AS status,
        'hello' AS prompt, 'ru' AS prompt_language,
        'hola' AS answer, 'es-ES' AS answer_language,
        '[\"buenas\",\"buen día\"]' AS accepted_answers_json,
        'exact' AS answer_evaluation_mode, 'Explanation' AS explanation,
        NULL AS pronunciation_note, 'li-1' AS source_learning_item_id,
        NULL AS source_error_target_id, 'lesson-1' AS source_lesson_id,
        'learning' AS creation_reason, 0.75 AS priority_score,
        'hash-1' AS content_hash, 2 AS content_version,
        NULL AS parent_card_id, NULL AS suspended_reason, 'tutor' AS created_by";

    #[test]
    fn card_sql_projection_matches_wire_fixture() {
        let db = rusqlite::Connection::open_in_memory().unwrap();
        let wire = db
            .query_row(CARD_ROW, [], project_card_wire)
            .unwrap()
            .unwrap();
        assert_eq!(
            wire,
            serde_json::json!({
                "id":"card-1","language_profile_id":"llp-1","language_pack_id":"es-ES",
                "language_pack_version":"0.2.0","intent":"production","status":"review",
                "prompt":"hello","prompt_language":"ru","answer":"hola","answer_language":"es-ES",
                "accepted_answers":["buenas","buen día"],"answer_evaluation_mode":"exact",
                "explanation":"Explanation","pronunciation_note":null,
                "source_learning_item_id":"li-1","source_error_target_id":null,
                "source_lesson_id":"lesson-1","creation_reason":"learning","priority_score":0.75,
                "content_hash":"hash-1","content_version":2,"parent_card_id":null,
                "suspended_reason":null,"created_by":"tutor"
            })
        );
    }

    #[test]
    fn card_sql_projection_does_not_publish_corrupt_rows() {
        let db = rusqlite::Connection::open_in_memory().unwrap();
        for invalid in [
            CARD_ROW.replace("'review' AS status", "'corrupt' AS status"),
            CARD_ROW.replace("'tutor' AS created_by", "'corrupt' AS created_by"),
            CARD_ROW.replace(
                "'0.2.0' AS language_pack_version",
                "'broken' AS language_pack_version",
            ),
            CARD_ROW.replace(
                "'[\"buenas\",\"buen día\"]' AS accepted_answers_json",
                "'oops' AS accepted_answers_json",
            ),
        ] {
            assert!(
                db.query_row(&invalid, [], project_card_wire)
                    .unwrap()
                    .is_none()
            );
        }
    }
}
