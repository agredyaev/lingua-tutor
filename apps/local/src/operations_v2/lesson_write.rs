#![allow(clippy::items_after_test_module)] // Test fixtures are intentionally grouped before the processing pass.
//! Fixed-capacity SoA write buffers for lesson completion.

#[cfg(test)]
// Test fixtures intentionally fail fast and use compact tuple snapshots.
#[allow(clippy::unwrap_used, clippy::type_complexity)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn schema(connection: &Connection) {
        connection
            .execute_batch(
                "CREATE TABLE learning_items (
                id TEXT PRIMARY KEY,
                learner_language_profile_id TEXT NOT NULL,
                language_pack_id TEXT NOT NULL,
                item_type TEXT NOT NULL,
                target_text TEXT NOT NULL,
                normalized_target_text TEXT NOT NULL,
                prompt_text TEXT,
                prompt_language TEXT,
                status TEXT NOT NULL,
                language_metadata_json TEXT NOT NULL,
                introduced_lesson_id TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE TABLE learning_item_evidence (
                id TEXT PRIMARY KEY,
                learning_item_id TEXT NOT NULL,
                lesson_id TEXT NOT NULL,
                kind TEXT NOT NULL,
                success INTEGER NOT NULL,
                detail TEXT,
                pack_metadata_json TEXT NOT NULL,
                recorded_at TEXT NOT NULL,
                observation_order INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE listening_attempts (
                id TEXT PRIMARY KEY,
                learner_language_profile_id TEXT NOT NULL,
                learning_item_id TEXT,
                lesson_id TEXT,
                source_reference TEXT,
                source_variant_id TEXT,
                talker_key TEXT,
                context_tag TEXT,
                transcript TEXT,
                success INTEGER NOT NULL,
                assistance_level INTEGER,
                response_millis INTEGER,
                probe_phase TEXT,
                recorded_at TEXT NOT NULL
            );",
            )
            .unwrap();
    }

    #[test]
    fn item_insert_soa_preserves_columns_across_multiple_flushes() {
        let mut connection = Connection::open_in_memory().unwrap();
        schema(&connection);
        let transaction = connection.transaction().unwrap();

        for i in 0..5 {
            let id = format!("li-{i}");
            let profile = format!("p-{i}");
            let pack = format!("pack-{i}");
            let item_type = if i % 2 == 0 { "phrase" } else { "word" };
            let target = format!("target-{i}");
            let normalized = format!("norm-{i}");
            let prompt = (i % 2 == 0).then(|| format!("prompt-{i}"));
            let prompt_lang = (i % 3 == 0).then(|| "ru".to_owned());
            let lesson = format!("lesson-{i}");
            let at = format!("2026-10-16T00:00:0{i}Z");

            insert_learning_item(
                &transaction,
                &id,
                &profile,
                &pack,
                item_type,
                &target,
                &normalized,
                prompt.as_deref(),
                prompt_lang.as_deref(),
                "introduced",
                "{}",
                Some(&lesson),
                &at,
            )
            .unwrap();
        }

        transaction.commit().unwrap();

        let rows: Vec<(
            String,
            String,
            String,
            String,
            Option<String>,
            Option<String>,
            String,
        )> = connection
            .prepare(
                "SELECT id, learner_language_profile_id, target_text,
                            normalized_target_text, prompt_text, prompt_language, status
                     FROM learning_items ORDER BY rowid",
            )
            .unwrap()
            .query_map([], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                ))
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();

        assert_eq!(rows.len(), 5);
        for (i, row) in rows.iter().enumerate() {
            assert_eq!(row.0, format!("li-{i}"));
            assert_eq!(row.1, format!("p-{i}"));
            assert_eq!(row.2, format!("target-{i}"));
            assert_eq!(row.3, format!("norm-{i}"));
            assert_eq!(row.4, (i % 2 == 0).then(|| format!("prompt-{i}")));
            assert_eq!(row.5, (i % 3 == 0).then(|| "ru".to_owned()));
            assert_eq!(row.6, "introduced");
        }
    }

    #[test]
    fn evidence_soa_preserves_nullable_detail_and_metadata() {
        let mut connection = Connection::open_in_memory().unwrap();
        schema(&connection);
        let transaction = connection.transaction().unwrap();

        for i in 0..8 {
            let id = format!("ev-{i}");
            let item = format!("li-{i}");
            let lesson = format!("lesson-{i}");
            let detail = (i % 2 == 1).then(|| format!("detail-{i}"));
            let metadata = format!("{{\"response_millis\":{}}}", 100 + i);
            let at = format!("2026-10-16T00:00:{i:02}Z");
            insert_evidence(
                &transaction,
                &id,
                &item,
                &lesson,
                "production",
                i % 2 == 0,
                detail.as_deref(),
                &metadata,
                &at,
                u32::try_from(i).unwrap(),
            )
            .unwrap();
        }

        transaction.commit().unwrap();

        let rows: Vec<(String, u8, Option<String>, String)> = connection
            .prepare(
                "SELECT id, success, detail, pack_metadata_json
                 FROM learning_item_evidence ORDER BY rowid",
            )
            .unwrap()
            .query_map([], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();

        assert_eq!(rows.len(), 8);
        for (i, row) in rows.iter().enumerate() {
            assert_eq!(row.0, format!("ev-{i}"));
            assert_eq!(row.1, u8::from(i % 2 == 0));
            assert_eq!(row.2, (i % 2 == 1).then(|| format!("detail-{i}")));
            assert_eq!(row.3, format!("{{\"response_millis\":{}}}", 100 + i));
        }
    }

    #[test]
    fn listening_soa_preserves_optional_item_reference() {
        let mut connection = Connection::open_in_memory().unwrap();
        schema(&connection);
        let transaction = connection.transaction().unwrap();

        for i in 0..5 {
            let id = format!("la-{i}");
            let item = (i % 2 == 0).then(|| format!("li-{i}"));
            let source = format!("source-{i}");
            let transcript = format!("transcript-{i}");
            let at = format!("2026-10-16T00:00:0{i}Z");
            insert_listening_attempt(
                &transaction,
                &id,
                "llp-1",
                item.as_deref(),
                Some("lesson-1"),
                Some(&source),
                None,
                None,
                None,
                Some(&transcript),
                i % 2 == 1,
                None,
                None,
                None,
                &at,
            )
            .unwrap();
        }

        transaction.commit().unwrap();

        let rows: Vec<(String, Option<String>, String, String, u8)> = connection
            .prepare(
                "SELECT id, learning_item_id, source_reference, transcript, success
                 FROM listening_attempts ORDER BY rowid",
            )
            .unwrap()
            .query_map([], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();

        assert_eq!(rows.len(), 5);
        for (i, row) in rows.iter().enumerate() {
            assert_eq!(row.0, format!("la-{i}"));
            assert_eq!(row.1, (i % 2 == 0).then(|| format!("li-{i}")));
            assert_eq!(row.2, format!("source-{i}"));
            assert_eq!(row.3, format!("transcript-{i}"));
            assert_eq!(row.4, u8::from(i % 2 == 1));
        }
    }

    #[test]
    fn listening_soa_preserves_null_and_empty_text_as_distinct_states() {
        let mut connection = Connection::open_in_memory().unwrap();
        schema(&connection);
        let transaction = connection.transaction().unwrap();

        insert_listening_attempt(
            &transaction,
            "la-null",
            "llp-1",
            None,
            Some("lesson-1"),
            None,
            None,
            None,
            None,
            None,
            true,
            None,
            None,
            None,
            "2026-10-16T00:00:00Z",
        )
        .unwrap();
        insert_listening_attempt(
            &transaction,
            "la-empty",
            "llp-1",
            None,
            Some("lesson-1"),
            Some(""),
            None,
            None,
            None,
            Some(""),
            false,
            None,
            None,
            None,
            "2026-10-16T00:00:01Z",
        )
        .unwrap();

        transaction.commit().unwrap();

        let null_row: (Option<String>, Option<String>) = connection
            .query_row(
                "SELECT source_reference, transcript FROM listening_attempts WHERE id='la-null'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        let empty_row: (Option<String>, Option<String>) = connection
            .query_row(
                "SELECT source_reference, transcript FROM listening_attempts WHERE id='la-empty'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();

        assert_eq!(null_row, (None, None));
        assert_eq!(empty_row, (Some(String::new()), Some(String::new())));
    }

    #[test]
    fn item_status_update_soa_updates_all_rows_without_loss() {
        let mut connection = Connection::open_in_memory().unwrap();
        schema(&connection);
        for i in 0..5 {
            connection
                .execute(
                    "INSERT INTO learning_items
                 (id, learner_language_profile_id, language_pack_id, item_type,
                  target_text, normalized_target_text, status, language_metadata_json,
                  created_at, updated_at)
                 VALUES (?1, 'p', 'pack', 'word', 'x', 'x', 'introduced', '{}', 'a', 'a')",
                    [format!("li-{i}")],
                )
                .unwrap();
        }

        let transaction = connection.transaction().unwrap();

        for i in 0..5 {
            let id = format!("li-{i}");
            let status = if i % 2 == 0 { "mastered" } else { "learning" };
            let at = format!("2026-10-16T00:00:0{i}Z");
            update_item_status(&transaction, &id, status, &at).unwrap();
        }

        transaction.commit().unwrap();

        for i in 0..5 {
            let row: (String, String) = connection
                .query_row(
                    "SELECT status, updated_at FROM learning_items WHERE id=?1",
                    [format!("li-{i}")],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .unwrap();
            assert_eq!(row.0, if i % 2 == 0 { "mastered" } else { "learning" });
            assert_eq!(row.1, format!("2026-10-16T00:00:0{i}Z"));
        }
    }
}

use crate::Result;
use crate::repositories::records;
use rusqlite::{Connection, params};

/// Bind admitted values directly to the SQLite statement (no staging allocation).
#[allow(clippy::too_many_arguments)]
pub(crate) fn insert_learning_item(
    database: &Connection,
    id: &str,
    profile_id: &str,
    pack_id: &str,
    item_type: &str,
    target_text: &str,
    normalized_target_text: &str,
    prompt_text: Option<&str>,
    prompt_language: Option<&str>,
    status: &str,
    language_metadata_json: &str,
    introduced_lesson_id: Option<&str>,
    at: &str,
) -> Result<()> {
    let mut statement = database.prepare_cached(records::INSERT_ITEM)?;
    statement.execute(params![
        id,
        profile_id,
        pack_id,
        item_type,
        target_text,
        normalized_target_text,
        prompt_text,
        prompt_language,
        status,
        language_metadata_json,
        introduced_lesson_id,
        at,
    ])?;
    Ok(())
}

/// Bind admitted values directly to the SQLite statement (no staging allocation).
#[allow(clippy::too_many_arguments)]
pub(crate) fn insert_evidence(
    database: &Connection,
    id: &str,
    item_id: &str,
    lesson_id: &str,
    kind: &str,
    success: bool,
    detail: Option<&str>,
    pack_metadata_json: &str,
    recorded_at: &str,
    observation_order: u32,
) -> Result<()> {
    let mut statement = database.prepare_cached(records::INSERT_EVIDENCE)?;
    statement.execute(params![
        id,
        item_id,
        lesson_id,
        kind,
        u8::from(success),
        detail,
        pack_metadata_json,
        recorded_at,
        observation_order,
    ])?;
    Ok(())
}

/// Bind admitted values directly to the SQLite statement (no staging allocation).
#[allow(clippy::too_many_arguments)]
pub(crate) fn insert_listening_attempt(
    database: &Connection,
    id: &str,
    profile_id: &str,
    item_id: Option<&str>,
    lesson_id: Option<&str>,
    source_reference: Option<&str>,
    source_variant_id: Option<&str>,
    talker_key: Option<&str>,
    context_tag: Option<&str>,
    transcript: Option<&str>,
    success: bool,
    assistance_level: Option<u8>,
    response_millis: Option<u32>,
    probe_phase: Option<&str>,
    recorded_at: &str,
) -> Result<()> {
    let mut statement = database.prepare_cached(records::INSERT_LISTENING_ATTEMPT)?;
    statement.execute(params![
        id,
        profile_id,
        item_id,
        lesson_id,
        source_reference,
        source_variant_id,
        talker_key,
        context_tag,
        transcript,
        u8::from(success),
        assistance_level,
        response_millis,
        probe_phase,
        recorded_at,
    ])?;
    Ok(())
}

/// Bind admitted values directly to the SQLite statement (no staging allocation).
#[allow(clippy::too_many_arguments)]
pub(crate) fn update_item_status(
    database: &Connection,
    item_id: &str,
    status: &str,
    updated_at: &str,
) -> Result<()> {
    let mut statement = database.prepare_cached(records::UPDATE_ITEM_STATUS)?;
    statement.execute(params![item_id, status, updated_at,])?;
    Ok(())
}
