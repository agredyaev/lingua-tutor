//! Lessons + lesson-result import.
//! Transport parsing is thin; lesson orchestration operates directly on one
//! request-local SQLite connection/transaction.

use crate::api::{now, operation_error, operation_error_with};
use crate::repositories::{lessons, to_rfc3339};
use crate::{Env, Result};
use learning_core::Timestamp;
use rusqlite::{OptionalExtension as _, params};
use tutor_contract::{LessonResult, parse_lesson_result, validate};

// SQL row -> wire fields in a single pass; no owned Row -> Domain conversion.
fn project_lesson_wire(row: &rusqlite::Row<'_>) -> rusqlite::Result<Option<serde_json::Value>> {
    let version: String = row.get("language_pack_version")?;
    let status: String = row.get("status")?;
    let source: String = row.get("source")?;
    let (Ok(version), Ok(_), Ok(_)) = (
        semver::Version::parse(&version),
        serde_json::from_value::<learning_core::LessonStatus>(serde_json::json!(status)),
        serde_json::from_value::<learning_core::LessonSource>(serde_json::json!(source)),
    ) else {
        return Ok(None);
    };
    let started: Option<String> = row.get("started_at")?;
    let completed: Option<String> = row.get("completed_at")?;
    let (Ok(started), Ok(completed)) = (
        started
            .as_deref()
            .map(crate::repositories::from_rfc3339)
            .transpose(),
        completed
            .as_deref()
            .map(crate::repositories::from_rfc3339)
            .transpose(),
    ) else {
        return Ok(None);
    };
    Ok(Some(serde_json::json!({
        "id": row.get::<_, String>("id")?,
        "language_profile_id": row.get::<_, String>("learner_language_profile_id")?,
        "language_pack_id": row.get::<_, String>("language_pack_id")?,
        "language_pack_version": version.to_string(),
        "lesson_number": row.get::<_, u32>("lesson_number")?,
        "status": status,
        "scenario": row.get::<_, String>("scenario")?,
        "planned_goal": row.get::<_, String>("planned_goal")?,
        "source": source,
        "started_at": started.map(|timestamp| timestamp.0),
        "completed_at": completed.map(|timestamp| timestamp.0),
        "duration_seconds": row.get::<_, Option<u32>>("duration_seconds")?,
        "tutor_summary": row.get::<_, Option<String>>("tutor_summary")?,
        "next_goal": row.get::<_, Option<String>>("next_goal")?,
        "idempotency_key": row.get::<_, Option<String>>("idempotency_key")?,
    })))
}

pub(crate) fn list_lessons_for(env: &Env, profile_id: &str) -> Result<serde_json::Value> {
    let database = env.connection()?;
    let mut statement = database.prepare(lessons::SELECT_LESSONS)?;
    let mut rows = statement.query(params![profile_id, 50_u32])?;
    let mut output = Vec::new();
    while let Some(row) = rows.next()? {
        if let Some(wire) = project_lesson_wire(row)? {
            output.push(wire);
        }
    }
    crate::json(&output)
}

pub(crate) fn get_lesson(env: &Env, lesson_id: &str) -> Result<serde_json::Value> {
    let database = env.connection()?;
    let item = database
        .query_row(
            lessons::SELECT_LESSON,
            params![lesson_id],
            project_lesson_wire,
        )
        .optional()?
        .flatten();
    match item {
        Some(wire) => crate::json(&wire),
        None => operation_error(404, "not_found", "unknown lesson"),
    }
}

pub(crate) fn validate_import_text(text: &str, env: &Env) -> Result<serde_json::Value> {
    let result = parse_lesson_result_text(text)?;
    let issues = validate(&result);
    if !issues.is_empty() {
        return operation_error_with(
            400,
            "validation_failed",
            "lesson result rejected",
            serde_json::json!({ "issues": issues }),
        );
    }
    let expires = now().0 + 15 * 60;
    let key = env.signing_key();
    crate::json(&serde_json::json!({
        "validation_token": validation_token(key, text, expires),
        "expires_at": to_rfc3339(Timestamp(expires)),
        "preview": {
            "lesson_scenario": result.lesson.scenario,
            "new_learning_items": result.new_learning_items.len(),
            "evidence": result.evidence.len(),
            "error_occurrences": result.error_occurrences.len(),
            "listening_attempts": result.listening_attempts.len(),
            "card_mutations": result.card_mutations.len(),
        },
        "warnings": [],
    }))
}

pub(crate) fn apply_import_text(
    registry: &crate::BuiltInPackRegistry,
    text: &str,
    token: &str,
    env: &Env,
) -> Result<serde_json::Value> {
    let Some(expires) = token_expiry_if_valid(env.signing_key(), token, text) else {
        return operation_error(409, "validation_required", "token mismatch — re-validate");
    };
    if expires < now().0 {
        return operation_error(409, "validation_expired", "token expired — re-validate");
    }
    let result = validate_lesson_result(parse_lesson_result_text(text)?)?;
    crate::operations::complete_lesson::complete_lesson(registry, env, &result)
}

pub(crate) fn complete_lesson_result(
    registry: &crate::BuiltInPackRegistry,
    env: &Env,
    lesson_id: &str,
    mut result: LessonResult,
) -> Result<serde_json::Value> {
    result = validate_lesson_result(result)?;
    result.lesson.lesson_id = Some(lesson_id.to_owned());
    crate::operations::complete_lesson::complete_lesson(registry, env, &result)
}

pub(crate) fn parse_lesson_result_text(text: &str) -> Result<LessonResult> {
    if text.len() > 256 * 1024 {
        return operation_error(413, "too_large", "body over 256 KB");
    }
    parse_lesson_result(text).map_err(|error| {
        crate::Error::operation(
            400,
            "parse_error",
            "not a valid lesson-result document",
            serde_json::json!({ "detail": error.to_string() }),
        )
    })
}

pub(crate) fn validate_lesson_result(result: LessonResult) -> Result<LessonResult> {
    let issues = validate(&result);
    if !issues.is_empty() {
        return operation_error_with(
            400,
            "validation_failed",
            "lesson result rejected",
            serde_json::json!({ "issues": issues }),
        );
    }
    Ok(result)
}

fn validation_token(key: &str, body: &str, expires: i64) -> String {
    use hmac::Mac as _;
    #[allow(clippy::expect_used)]
    let mut mac = hmac::Hmac::<sha2::Sha256>::new_from_slice(key.as_bytes())
        .expect("HMAC accepts any key length");
    mac.update(body.as_bytes());
    mac.update(&expires.to_be_bytes());
    format!("vt_{}_{expires}", hex::encode(mac.finalize().into_bytes()))
}

fn token_expiry_if_valid(key: &str, token: &str, body: &str) -> Option<i64> {
    let expires: i64 = token.rsplit('_').next()?.parse().ok()?;
    let expected = validation_token(key, body, expires);
    let mismatch = expected
        .as_bytes()
        .iter()
        .zip(token.as_bytes())
        .fold(expected.len() != token.len(), |acc, (a, b)| acc | (a != b));
    (!mismatch).then_some(expires)
}

#[cfg(test)]
// Test fixtures intentionally fail fast and use compact tuple snapshots.
#[allow(clippy::unwrap_used, clippy::type_complexity)]
mod dod_projection_tests {
    use super::*;

    const LESSON_ROW: &str = "SELECT 'les-1' AS id, 'llp-1' AS learner_language_profile_id,
        'es-ES' AS language_pack_id, '0.2.0' AS language_pack_version,
        7 AS lesson_number, 'completed' AS status, 'pharmacy' AS scenario,
        'request medicine' AS planned_goal, 'mcp' AS source,
        '2026-10-18T08:30:00Z' AS started_at,
        '2026-10-18T08:50:00Z' AS completed_at,
        1200 AS duration_seconds, 'great' AS tutor_summary,
        'next goal' AS next_goal, 'op-7' AS idempotency_key";

    #[test]
    fn lesson_sql_projection_matches_wire_fixture() {
        let db = rusqlite::Connection::open_in_memory().unwrap();
        let projected = db
            .query_row(LESSON_ROW, [], project_lesson_wire)
            .unwrap()
            .unwrap();
        assert_eq!(projected["id"], "les-1");
        assert_eq!(projected["language_profile_id"], "llp-1");
        assert_eq!(projected["language_pack_id"], "es-ES");
        assert_eq!(projected["language_pack_version"], "0.2.0");
        assert_eq!(projected["lesson_number"], 7);
        assert_eq!(projected["status"], "completed");
        assert_eq!(projected["scenario"], "pharmacy");
        assert_eq!(projected["planned_goal"], "request medicine");
        assert_eq!(projected["source"], "mcp");
        assert_eq!(projected["duration_seconds"], 1200);
        assert_eq!(projected["tutor_summary"], "great");
        assert_eq!(projected["next_goal"], "next goal");
        assert_eq!(projected["idempotency_key"], "op-7");
        assert!(projected["started_at"].is_number());
        assert!(projected["completed_at"].is_number());
    }

    #[test]
    fn lesson_sql_projection_preserves_invalid_row_exclusion() {
        let db = rusqlite::Connection::open_in_memory().unwrap();
        for invalid in [
            LESSON_ROW.replace("'completed' AS status", "'invalid' AS status"),
            LESSON_ROW.replace(
                "'0.2.0' AS language_pack_version",
                "'bad' AS language_pack_version",
            ),
            LESSON_ROW.replace(
                "'2026-10-18T08:30:00Z' AS started_at",
                "'not-a-date' AS started_at",
            ),
        ] {
            assert!(
                db.query_row(&invalid, [], project_lesson_wire)
                    .unwrap()
                    .is_none()
            );
        }
    }
}
