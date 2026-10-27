#![allow(clippy::items_after_test_module)] // Test fixtures are intentionally grouped before the processing pass.
//! Fixed-capacity SoA card/review persistence.

#[cfg(test)]
// Test fixtures intentionally fail fast and use compact tuple snapshots.
#[allow(clippy::unwrap_used, clippy::type_complexity)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn schema(connection: &Connection) {
        connection
            .execute_batch(include_str!("../../migrations/0001_init.sql"))
            .unwrap();
        connection
            .execute_batch(
                "INSERT INTO language_packs
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
                 INSERT INTO scheduler_parameter_sets
                    (id, learner_language_profile_id, algorithm, algorithm_version,
                     parameters_json, origin, created_at)
                 VALUES
                    ('default', 'llp-1', 'fsrs', '6.0', '{}', 'default',
                     '2026-10-16T00:00:00Z');",
            )
            .unwrap();

        for i in 0..5 {
            connection
                .execute(
                    "INSERT INTO lessons
                    (id, learner_language_profile_id, language_pack_id,
                     language_pack_version, lesson_number, status, scenario,
                     planned_goal, source, started_at, created_at, updated_at)
                 VALUES (?1, 'llp-1', 'es-ES', '0.2.0', ?2, 'completed',
                         'test', '', 'import', '2026-10-16T00:00:00Z',
                         '2026-10-16T00:00:00Z', '2026-10-16T00:00:00Z')",
                    rusqlite::params![format!("lesson-{i}"), i + 1],
                )
                .unwrap();
            connection
                .execute(
                    "INSERT INTO learning_items
                    (id, learner_language_profile_id, language_pack_id, item_type,
                     target_text, normalized_target_text, status,
                     language_metadata_json, created_at, updated_at)
                 VALUES (?1, 'llp-1', 'es-ES', 'word', ?2, ?2, 'introduced',
                         '{}', '2026-10-16T00:00:00Z', '2026-10-16T00:00:00Z')",
                    rusqlite::params![format!("li-{i}"), format!("seed-{i}")],
                )
                .unwrap();
            connection
                .execute(
                    "INSERT INTO error_targets
                    (id, learner_language_profile_id, language_pack_id,
                     language_pack_version, core_category, original_production,
                     corrected_production, occurrence_count,
                     method_change_required, status, created_at, updated_at)
                 VALUES (?1, 'llp-1', 'es-ES', '0.2.0', 'grammar', 'wrong',
                         'right', 1, 0, 'active',
                         '2026-10-16T00:00:00Z', '2026-10-16T00:00:00Z')",
                    [format!("et-{i}")],
                )
                .unwrap();
        }
    }

    #[test]
    fn card_insert_soa_preserves_nullable_and_scalar_columns() {
        let mut connection = Connection::open_in_memory().unwrap();
        schema(&connection);
        let transaction = connection.transaction().unwrap();

        for i in 0..5 {
            let id = format!("card-{i}");
            let explanation = (i % 2 == 0).then(|| format!("explanation-{i}"));
            let source_item = (i % 2 == 1).then(|| format!("li-{i}"));
            let source_error = (i % 3 == 0).then(|| format!("et-{i}"));
            let source_lesson = Some(format!("lesson-{i}"));
            let priority = (i % 2 == 0).then_some(i as f64 + 0.5);
            let hash = format!("hash-{i}");
            let at = format!("2026-10-16T00:00:0{i}Z");

            insert_card(
                &transaction,
                &id,
                "llp-1",
                "es-ES",
                "0.2.0",
                "production",
                if i % 2 == 0 { "new" } else { "candidate" },
                &format!("prompt-{i}"),
                "ru",
                &format!("answer-{i}"),
                "es-ES",
                &format!("[\"accepted-{i}\"]"),
                "exact",
                explanation.as_deref(),
                None,
                source_item.as_deref(),
                source_error.as_deref(),
                source_lesson.as_deref(),
                "lesson",
                priority,
                &hash,
                1,
                None,
                None,
                "tutor",
                &at,
            )
            .unwrap();
        }

        transaction.commit().unwrap();

        let rows: Vec<(
            String,
            String,
            String,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<f64>,
            String,
        )> = connection
            .prepare(
                "SELECT id, status, prompt, explanation,
                            source_learning_item_id, source_error_target_id,
                            priority_score, content_hash
                     FROM cards ORDER BY rowid",
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
                    row.get(7)?,
                ))
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();

        assert_eq!(rows.len(), 5);
        for (i, row) in rows.iter().enumerate() {
            assert_eq!(row.0, format!("card-{i}"));
            assert_eq!(row.1, if i % 2 == 0 { "new" } else { "candidate" });
            assert_eq!(row.2, format!("prompt-{i}"));
            assert_eq!(row.3, (i % 2 == 0).then(|| format!("explanation-{i}")));
            assert_eq!(row.4, (i % 2 == 1).then(|| format!("li-{i}")));
            assert_eq!(row.5, (i % 3 == 0).then(|| format!("et-{i}")));
            assert_eq!(row.6, (i % 2 == 0).then_some(i as f64 + 0.5));
            assert_eq!(row.7, format!("hash-{i}"));
        }
    }

    fn seed_card(connection: &Connection, id: &str) {
        connection
            .execute(
                "INSERT INTO cards
             (id, learner_language_profile_id, language_pack_id, language_pack_version,
              intent, status, prompt, prompt_language, answer, answer_language,
              accepted_answers_json, answer_evaluation_mode, creation_reason,
              content_hash, content_version, created_by, created_at, updated_at)
             VALUES (?1, 'llp-1', 'es-ES', '0.2.0', 'production', 'new',
                     'old prompt', 'ru', 'answer', 'es-ES', '[\"old\"]', 'exact',
                     'test', 'old-hash', 1, 'tutor', 'a', 'a')",
                [id],
            )
            .unwrap();
    }

    #[test]
    fn card_update_soas_apply_all_changes_across_flushes() {
        let mut connection = Connection::open_in_memory().unwrap();
        schema(&connection);
        for i in 0..5 {
            seed_card(&connection, &format!("card-{i}"));
        }

        let transaction = connection.transaction().unwrap();

        for i in 0..5 {
            let id = format!("card-{i}");
            let prompt = format!("new prompt {i}");
            let explanation = (i % 2 == 0).then(|| format!("why-{i}"));
            let accepted = format!("[\"a-{i}\",\"b-{i}\"]");
            let hash = format!("new-hash-{i}");
            let at = format!("2026-10-16T00:00:0{i}Z");

            update_card_content(
                &transaction,
                &id,
                &prompt,
                explanation.as_deref(),
                None,
                &accepted,
                &hash,
                &at,
            )
            .unwrap();
            update_card_status(
                &transaction,
                &id,
                if i % 2 == 0 { "review" } else { "suspended" },
                (i % 2 == 1).then_some("manual"),
                &at,
            )
            .unwrap();
            update_accepted_answers(
                &transaction,
                &id,
                &format!("[\"final-{i}\"]"),
                &format!("final-hash-{i}"),
                &at,
            )
            .unwrap();
        }

        transaction.commit().unwrap();

        for i in 0..5 {
            let row: (
                String,
                Option<String>,
                String,
                Option<String>,
                String,
                String,
            ) = connection
                .query_row(
                    "SELECT prompt, explanation, status, suspended_reason,
                            accepted_answers_json, content_hash
                     FROM cards WHERE id=?1",
                    [format!("card-{i}")],
                    |row| {
                        Ok((
                            row.get(0)?,
                            row.get(1)?,
                            row.get(2)?,
                            row.get(3)?,
                            row.get(4)?,
                            row.get(5)?,
                        ))
                    },
                )
                .unwrap();

            assert_eq!(row.0, format!("new prompt {i}"));
            assert_eq!(row.1, (i % 2 == 0).then(|| format!("why-{i}")));
            assert_eq!(row.2, if i % 2 == 0 { "review" } else { "suspended" });
            assert_eq!(row.3, (i % 2 == 1).then(|| "manual".to_owned()));
            assert_eq!(row.4, format!("[\"final-{i}\"]"));
            assert_eq!(row.5, format!("final-hash-{i}"));
        }
    }

    #[test]
    fn schedule_upsert_soa_preserves_state_and_updates_conflicts() {
        let mut connection = Connection::open_in_memory().unwrap();
        schema(&connection);
        for i in 0..5 {
            seed_card(&connection, &format!("card-{i}"));
        }

        let transaction = connection.transaction().unwrap();

        for i in 0..5 {
            let card = format!("card-{i}");
            upsert_schedule_state(
                &transaction,
                &card,
                "fsrs",
                "6.0",
                Some("default"),
                Some(1.0 + i as f64),
                Some(2.0 + i as f64),
                None,
                Some("2026-10-20T00:00:00Z"),
                Some(86_400 + i as i64),
                i as u32,
                (10 + i) as u32,
                1,
                &format!("state-{i}"),
                "2026-10-16T00:00:00Z",
            )
            .unwrap();
        }

        upsert_schedule_state(
            &transaction,
            "card-3",
            "fsrs",
            "6.1",
            None,
            Some(9.9),
            Some(8.8),
            Some("2026-10-16T00:00:00Z"),
            Some("2026-10-30T00:00:00Z"),
            Some(999),
            7,
            77,
            2,
            "state-updated",
            "2026-10-17T00:00:00Z",
        )
        .unwrap();

        transaction.commit().unwrap();

        let count: u32 = connection
            .query_row("SELECT COUNT(*) FROM card_schedule_state", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 5);

        let row: (
            String,
            Option<String>,
            Option<f64>,
            Option<i64>,
            u32,
            u32,
            u32,
            String,
        ) = connection
            .query_row(
                "SELECT algorithm_version, parameter_set_id, stability,
                        scheduled_interval_seconds, lapse_count, review_count,
                        state_version, state_hash
                 FROM card_schedule_state WHERE card_id='card-3'",
                [],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                        row.get(7)?,
                    ))
                },
            )
            .unwrap();

        assert_eq!(
            row,
            (
                "6.1".to_owned(),
                None,
                Some(8.8),
                Some(999),
                7,
                77,
                2,
                "state-updated".to_owned(),
            )
        );
    }

    #[test]
    fn tag_and_mutation_audit_soas_preserve_conflict_semantics() {
        let mut connection = Connection::open_in_memory().unwrap();
        schema(&connection);
        seed_card(&connection, "card-1");
        let transaction = connection.transaction().unwrap();

        insert_card_tag(&transaction, "card-1", "skill", "grammar").unwrap();
        insert_card_tag(&transaction, "card-1", "skill", "grammar").unwrap();
        insert_card_tag(&transaction, "card-1", "scenario", "shop").unwrap();

        for i in 0..3 {
            insert_card_mutation(
                &transaction,
                &format!("cm-{i}"),
                Some("card-1"),
                "update",
                &format!("op-{i}"),
                None,
                Some("{}"),
                Some("lesson-1"),
                "tutor",
                &format!("reason-{i}"),
                "es-ES",
                "0.2.0",
                "applied",
                "2026-10-16T00:00:00Z",
            )
            .unwrap();
        }
        // Same client operation must be ignored by ON CONFLICT.
        insert_card_mutation(
            &transaction,
            "cm-duplicate",
            Some("card-1"),
            "update",
            "op-1",
            None,
            Some("{\"duplicate\":true}"),
            Some("lesson-1"),
            "tutor",
            "duplicate",
            "es-ES",
            "0.2.0",
            "applied",
            "2026-10-16T00:00:00Z",
        )
        .unwrap();

        transaction.commit().unwrap();

        let tag_count: u32 = connection
            .query_row("SELECT COUNT(*) FROM card_tags", [], |row| row.get(0))
            .unwrap();
        let mutation_count: u32 = connection
            .query_row("SELECT COUNT(*) FROM card_mutations", [], |row| row.get(0))
            .unwrap();
        assert_eq!(tag_count, 2);
        assert_eq!(mutation_count, 3);
    }
}

use crate::Result;
use crate::repositories::{cards, review};
use rusqlite::{Connection, params};

/// Bind admitted values directly to the SQLite statement (no staging allocation).
#[allow(clippy::too_many_arguments)]
pub(crate) fn insert_card(
    database: &Connection,
    id: &str,
    profile_id: &str,
    pack_id: &str,
    pack_version: &str,
    intent: &str,
    status: &str,
    prompt: &str,
    prompt_language: &str,
    answer: &str,
    answer_language: &str,
    accepted_answers_json: &str,
    answer_evaluation_mode: &str,
    explanation: Option<&str>,
    pronunciation_note: Option<&str>,
    source_learning_item_id: Option<&str>,
    source_error_target_id: Option<&str>,
    source_lesson_id: Option<&str>,
    creation_reason: &str,
    priority_score: Option<f64>,
    content_hash: &str,
    content_version: u32,
    parent_card_id: Option<&str>,
    suspended_reason: Option<&str>,
    created_by: &str,
    at: &str,
) -> Result<()> {
    let mut statement = database.prepare_cached(cards::INSERT_CARD)?;
    statement.execute(params![
        id,
        profile_id,
        pack_id,
        pack_version,
        intent,
        status,
        prompt,
        prompt_language,
        answer,
        answer_language,
        accepted_answers_json,
        answer_evaluation_mode,
        explanation,
        pronunciation_note,
        source_learning_item_id,
        source_error_target_id,
        source_lesson_id,
        creation_reason,
        priority_score,
        content_hash,
        content_version,
        parent_card_id,
        suspended_reason,
        created_by,
        at,
    ])?;
    Ok(())
}

/// Bind admitted values directly to the SQLite statement (no staging allocation).
#[allow(clippy::too_many_arguments)]
pub(crate) fn update_card_content(
    database: &Connection,
    id: &str,
    prompt: &str,
    explanation: Option<&str>,
    pronunciation_note: Option<&str>,
    accepted_answers_json: &str,
    content_hash: &str,
    at: &str,
) -> Result<()> {
    let mut statement = database.prepare_cached(cards::UPDATE_CARD_CONTENT)?;
    statement.execute(params![
        id,
        prompt,
        explanation,
        pronunciation_note,
        accepted_answers_json,
        content_hash,
        at,
    ])?;
    Ok(())
}

/// Bind admitted values directly to the SQLite statement (no staging allocation).
#[allow(clippy::too_many_arguments)]
pub(crate) fn update_card_status(
    database: &Connection,
    id: &str,
    status: &str,
    suspended_reason: Option<&str>,
    at: &str,
) -> Result<()> {
    let mut statement = database.prepare_cached(cards::UPDATE_CARD_STATUS)?;
    statement.execute(params![id, status, suspended_reason, at,])?;
    Ok(())
}

/// Bind admitted values directly to the SQLite statement (no staging allocation).
#[allow(clippy::too_many_arguments)]
pub(crate) fn update_accepted_answers(
    database: &Connection,
    id: &str,
    accepted_answers_json: &str,
    content_hash: &str,
    at: &str,
) -> Result<()> {
    let mut statement = database.prepare_cached(cards::UPDATE_ACCEPTED_ANSWERS)?;
    statement.execute(params![id, accepted_answers_json, content_hash, at,])?;
    Ok(())
}

/// Bind admitted values directly to the SQLite statement (no staging allocation).
#[allow(clippy::too_many_arguments)]
pub(crate) fn upsert_schedule_state(
    database: &Connection,
    card_id: &str,
    algorithm: &str,
    algorithm_version: &str,
    parameter_set_id: Option<&str>,
    difficulty: Option<f64>,
    stability: Option<f64>,
    last_review_at: Option<&str>,
    due_at: Option<&str>,
    scheduled_interval_seconds: Option<i64>,
    lapse_count: u32,
    review_count: u32,
    state_version: u32,
    state_hash: &str,
    updated_at: &str,
) -> Result<()> {
    let mut statement = database.prepare_cached(review::UPSERT_SCHEDULE_STATE)?;
    statement.execute(params![
        card_id,
        algorithm,
        algorithm_version,
        parameter_set_id,
        difficulty,
        stability,
        last_review_at,
        due_at,
        scheduled_interval_seconds,
        lapse_count,
        review_count,
        state_version,
        state_hash,
        updated_at,
    ])?;
    Ok(())
}

/// Bind admitted values directly to the SQLite statement (no staging allocation).
#[allow(clippy::too_many_arguments)]
pub(crate) fn insert_card_tag(
    database: &Connection,
    card_id: &str,
    tag_type: &str,
    normalized_tag: &str,
) -> Result<()> {
    let mut statement = database.prepare_cached(cards::INSERT_CARD_TAG)?;
    statement.execute(params![card_id, tag_type, normalized_tag,])?;
    Ok(())
}

/// Bind admitted values directly to the SQLite statement (no staging allocation).
#[allow(clippy::too_many_arguments)]
pub(crate) fn insert_card_mutation(
    database: &Connection,
    id: &str,
    card_id: Option<&str>,
    operation: &str,
    client_operation_id: &str,
    before_json: Option<&str>,
    after_json: Option<&str>,
    source_lesson_id: Option<&str>,
    actor: &str,
    reason: &str,
    pack_id: &str,
    pack_version: &str,
    validation_result: &str,
    created_at: &str,
) -> Result<()> {
    let mut statement = database.prepare_cached(cards::INSERT_CARD_MUTATION)?;
    statement.execute(params![
        id,
        card_id,
        operation,
        client_operation_id,
        before_json,
        after_json,
        source_lesson_id,
        actor,
        reason,
        pack_id,
        pack_version,
        validation_result,
        created_at,
    ])?;
    Ok(())
}
