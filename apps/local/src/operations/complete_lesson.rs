#![allow(clippy::items_after_test_module)] // Test fixtures are intentionally grouped before the processing pass.
//! End-to-end lesson completion on the new fixed-capacity data path.

#[cfg(test)]
// Test fixtures intentionally fail fast and use compact tuple snapshots.
#[allow(clippy::unwrap_used, clippy::type_complexity)]
mod tests {
    use super::*;
    use crate::Env;
    use rusqlite::Connection;
    use std::path::PathBuf;

    #[derive(Debug, PartialEq)]
    struct Snapshot {
        lessons: Vec<(u32, String, String, Option<u32>, Option<String>)>,
        items: Vec<(String, String, String)>,
        evidence: Vec<(String, String, u8, Option<String>)>,
        errors: Vec<(String, String, u32, u8, String)>,
        rechecks: Vec<(String, String, u32)>,
        recommendations: Vec<(String, String, String, String)>,
        profile_lesson_number: u32,
    }

    fn result() -> tutor_contract::LessonResult {
        tutor_contract::parse_lesson_result(
            &serde_json::json!({
                "schema_version": "2",
                "client_operation_id": "op-complete-1",
                "language_profile_id": "llp-1",
                "lesson": {
                    "scenario": "pharmacy",
                    "duration_seconds": 321,
                    "tutor_summary": "summary",
                    "next_goal": "next"
                },
                "metrics": {
                    "speaking_seconds": 120,
                    "unaided_recall_count": 4,
                    "aided_recall_count": 1,
                    "listening_score": 0.75,
                    "avg_answer_millis": 850,
                    "long_pause_count": 2,
                    "confidence": 0.8
                },
                "new_learning_items": [
                    {
                        "client_item_key": "new-cough",
                        "item_type": "phrase",
                        "target_text": "¿Tiene algo para la tos?"
                    },
                    {
                        "item_type": "phrase",
                        "target_text": "Buenos días"
                    }
                ],
                "evidence": [
                    {
                        "learning_item_id": "li-known",
                        "kind": "spontaneous_production",
                        "success": true,
                        "assistance_level": 0,
                        "response_millis": 900,
                        "probe_phase": "diagnostic",
                        "context": "pharmacy:request",
                        "pronunciation_intelligible": true,
                        "pronunciation_repeat_required": false
                    },
                    {
                        "client_item_key": "new-cough",
                        "kind": "recognition",
                        "success": true,
                        "assistance_level": 0,
                        "response_millis": 700,
                        "probe_phase": "diagnostic",
                        "context": "pharmacy:recognize",
                        "detail": "new item"
                    },
                    {
                        "learning_item_id": "li-known",
                        "kind": "spontaneous_production",
                        "success": true,
                        "assistance_level": 0,
                        "response_millis": 800,
                        "probe_phase": "practice",
                        "context": "pharmacy:post-fade",
                        "pronunciation_intelligible": true,
                        "pronunciation_repeat_required": false
                    }
                ],
                "interventions": [
                    {
                        "action": "fade_prompts",
                        "learning_item_id": "li-known",
                        "target_capability": "spontaneous_production",
                        "evidence_index": 2
                    }
                ],
                "listening_attempts": [
                    {
                        "client_item_key": "new-cough",
                        "success": true,
                        "assistance_level": 0,
                        "response_millis": 800,
                        "probe_phase": "practice",
                        "context_tag": "pharmacy:listen",
                        "source_reference": "audio-1",
                        "transcript": "¿Tiene algo para la tos?"
                    }
                ],
                "error_occurrences": [
                    {
                        "error_target_id": "et-known",
                        "core_category": "grammar",
                        "original_production": "estoy médico",
                        "corrected_production": "soy médico"
                    },
                    {
                        "core_category": "vocabulary",
                        "original_production": "x",
                        "corrected_production": "y",
                        "explanation": "new error"
                    }
                ],
                "mastery_changes": [
                    {
                        "learning_item_id": "li-known",
                        "new_status": "provisionally_mastered"
                    }
                ],
                "card_mutations": []
            })
            .to_string(),
        )
        .unwrap()
    }

    fn create_env(label: &str) -> (PathBuf, Env) {
        let path = std::env::temp_dir().join(format!(
            "lingua-complete-{label}-{}.sqlite3",
            uuid::Uuid::new_v4()
        ));
        let connection = crate::storage::open(&path).unwrap();
        seed(&connection);
        drop(connection);
        (path.clone(), Env::new(path, "test-signing-key".to_owned()))
    }

    fn seed(db: &Connection) {
        db.execute_batch(
            "
             INSERT INTO learner_profile (id, created_at, updated_at)
             VALUES ('learner-1', '2026-10-16T00:00:00Z', '2026-10-16T00:00:00Z');
             INSERT INTO learner_language_profiles
                (id, learner_id, source_language, target_locale,
                 current_lesson_number, created_at, updated_at)
             VALUES
                ('llp-1', 'learner-1', 'ru', 'es-ES', 7,
                 '2026-10-16T00:00:00Z', '2026-10-16T00:00:00Z');
             INSERT INTO learning_items
                (id, learner_language_profile_id, item_type,
                 target_text, normalized_target_text, status,
                 created_at, updated_at)
             VALUES
                ('li-known', 'llp-1', 'phrase', 'known phrase',
                 'known phrase', 'learning',
                 '2026-10-16T00:00:00Z', '2026-10-16T00:00:00Z'),
                ('li-greeting', 'llp-1', 'phrase', 'Buenos días',
                 'buenos días', 'introduced',
                 '2026-10-16T00:00:00Z', '2026-10-16T00:00:00Z');
             INSERT INTO error_targets
                (id, learner_language_profile_id,  core_category, original_production,
                 corrected_production, occurrence_count, method_change_required,
                 status, created_at, updated_at)
             VALUES
                ('et-known', 'llp-1', 'grammar',
                 'estoy médico', 'soy médico', 2, 0, 'active',
                 '2026-10-16T00:00:00Z', '2026-10-16T00:00:00Z');",
        )
        .unwrap();
    }

    fn snapshot(env: &Env) -> Snapshot {
        let db = env.connection().unwrap();

        let lessons = db
            .prepare(
                "SELECT lesson_number, status, scenario, duration_seconds, next_goal
             FROM lessons ORDER BY lesson_number",
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

        let items = db
            .prepare(
                "SELECT target_text, normalized_target_text, status
             FROM learning_items ORDER BY normalized_target_text",
            )
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();

        let evidence = db
            .prepare(
                "SELECT i.target_text, e.kind, e.success, e.detail
             FROM learning_item_evidence e
             JOIN learning_items i ON i.id=e.learning_item_id
             ORDER BY i.target_text, e.kind",
            )
            .unwrap()
            .query_map([], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();

        let errors = db
            .prepare(
                "SELECT original_production, corrected_production,
                    occurrence_count, method_change_required, status
             FROM error_targets ORDER BY original_production",
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

        let rechecks = db
            .prepare(
                "SELECT t.original_production, t.corrected_production, r.offset_days
             FROM error_rechecks r
             JOIN error_targets t ON t.id=r.error_target_id
             ORDER BY t.original_production, r.offset_days",
            )
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();

        let recommendations = db
            .prepare(
                "SELECT subject_type, COALESCE(subject_id,''), action, detail_json
             FROM recommendations ORDER BY subject_type, subject_id, action",
            )
            .unwrap()
            .query_map([], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();

        let profile_lesson_number = db
            .query_row(
                "SELECT current_lesson_number FROM learner_language_profiles WHERE id='llp-1'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        Snapshot {
            lessons,
            items,
            evidence,
            errors,
            rechecks,
            recommendations,
            profile_lesson_number,
        }
    }

    fn cleanup(path: PathBuf) {
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("sqlite3-wal"));
        let _ = std::fs::remove_file(path.with_extension("sqlite3-shm"));
    }

    #[test]
    fn stale_transfer_history_does_not_grant_full_mastery() {
        let (path, env) = create_env("stale-mastery");
        {
            let db = env.connection().unwrap();
            db.execute_batch(
                "INSERT INTO lessons
                    (id, learner_language_profile_id, lesson_number, status, scenario,
                     planned_goal, source, started_at, completed_at, created_at, updated_at)
                 VALUES
                    ('les-old-1','llp-1',1,'completed','old','old','import',
                     '2026-01-01T10:00:00Z','2026-01-01T10:20:00Z',
                     '2026-01-01T10:00:00Z','2026-01-01T10:20:00Z'),
                    ('les-old-2','llp-1',2,'completed','old','old','import',
                     '2026-01-10T10:00:00Z','2026-01-10T10:20:00Z',
                     '2026-01-10T10:00:00Z','2026-01-10T10:20:00Z');
                 INSERT INTO learning_item_evidence
                    (id, learning_item_id, lesson_id, kind, success, metadata_json, recorded_at)
                 VALUES
                    ('ev-old-1','li-known','les-old-1','transfer',1,
                     json_object('assistance_level',0,'context_family','transport.ask_stop'),
                     '2026-01-01T10:10:00Z'),
                    ('ev-old-2','li-known','les-old-2','transfer',1,
                     json_object('assistance_level',0,'context_family','commerce.request_item'),
                     '2026-01-10T10:10:00Z');",
            )
            .unwrap();
        }

        let mut input = result();
        input.client_operation_id = "op-stale-mastery".to_owned();
        input.mastery_changes[0].new_status = learning_core::LearningItemStatus::Mastered;
        input.evidence.push(tutor_contract::EvidenceEntry {
            learning_item_id: Some("li-known".to_owned()),
            client_item_key: None,
            context: Some("fresh".to_owned()),
            context_family: Some("service.ask_help".to_owned()),
            kind: learning_core::EvidenceKind::Transfer,
            success: true,
            assistance_level: Some(0),
            response_millis: Some(1_200),
            probe_phase: Some(tutor_contract::ProbePhase::Transfer),
            pronunciation_intelligible: Some(true),
            pronunciation_repeat_required: Some(false),
            detail: None,
        });

        let response = complete_lesson(&env, &input).unwrap();
        assert_eq!(response["warnings"].as_array().unwrap().len(), 1);
        let db = env.connection().unwrap();
        let status: String = db
            .query_row(
                "SELECT status FROM learning_items WHERE id='li-known'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_ne!(status, "mastered");
        drop(db);
        cleanup(path);
    }

    #[test]
    fn legacy_transfer_rows_without_episode_identity_cannot_fake_distributed_mastery() {
        let (path, env) = create_env("legacy-mastery-episodes");
        {
            let db = env.connection().unwrap();
            db.execute_batch(
                "INSERT INTO learning_item_evidence
                    (id, learning_item_id, lesson_id, kind, success, metadata_json, recorded_at)
                 VALUES
                    ('ev-legacy-1','li-known',NULL,'transfer',1,
                     json_object('assistance_level',0,'probe_phase','transfer',
                                 'context_family','transport.ask_stop'),
                     '2026-11-05T10:00:00Z'),
                    ('ev-legacy-2','li-known',NULL,'transfer',1,
                     json_object('assistance_level',0,'probe_phase','transfer',
                                 'context_family','commerce.request_item'),
                     '2026-11-05T10:05:00Z');",
            )
            .unwrap();
        }

        let mut input = result();
        input.client_operation_id = "op-legacy-mastery-episodes".to_owned();
        input.mastery_changes[0].new_status = learning_core::LearningItemStatus::Mastered;
        input.evidence.push(tutor_contract::EvidenceEntry {
            learning_item_id: Some("li-known".to_owned()),
            client_item_key: None,
            context: Some("fresh".to_owned()),
            context_family: Some("service.ask_help".to_owned()),
            kind: learning_core::EvidenceKind::Transfer,
            success: true,
            assistance_level: Some(0),
            response_millis: Some(1_200),
            probe_phase: Some(tutor_contract::ProbePhase::Transfer),
            pronunciation_intelligible: Some(true),
            pronunciation_repeat_required: Some(false),
            detail: None,
        });

        let response = complete_lesson(&env, &input).unwrap();
        assert_eq!(
            response["warnings"].as_array().unwrap().len(),
            1,
            "legacy rows without lesson/episode identity must not manufacture two prior episodes"
        );
        cleanup(path);
    }

    #[test]
    fn historical_practice_transfer_cannot_supply_mastery_history() {
        let (path, env) = create_env("practice-mastery-history");
        {
            let db = env.connection().unwrap();
            db.execute_batch(
                "INSERT INTO lessons
                    (id, learner_language_profile_id, lesson_number, status, scenario,
                     planned_goal, source, started_at, completed_at, created_at, updated_at)
                 VALUES
                    ('les-hist-1','llp-1',1,'completed','old','old','import',
                     '2026-10-20T10:00:00Z','2026-10-20T10:20:00Z',
                     '2026-10-20T10:00:00Z','2026-10-20T10:20:00Z'),
                    ('les-hist-2','llp-1',2,'completed','old','old','import',
                     '2026-10-21T10:00:00Z','2026-10-21T10:20:00Z',
                     '2026-10-21T10:00:00Z','2026-10-21T10:20:00Z');
                 INSERT INTO learning_item_evidence
                    (id, learning_item_id, lesson_id, kind, success, metadata_json, recorded_at)
                 VALUES
                    ('ev-hist-1','li-known','les-hist-1','transfer',1,
                     json_object('assistance_level',0,'probe_phase','transfer',
                                 'context_family','transport.ask_stop'),
                     '2026-10-20T10:10:00Z'),
                    ('ev-hist-2','li-known','les-hist-2','transfer',1,
                     json_object('assistance_level',0,'probe_phase','practice',
                                 'context_family','commerce.request_item'),
                     '2026-10-21T10:10:00Z');",
            )
            .unwrap();
        }

        let mut input = result();
        input.client_operation_id = "op-practice-mastery-history".to_owned();
        input.mastery_changes[0].new_status = learning_core::LearningItemStatus::Mastered;
        input.evidence.push(tutor_contract::EvidenceEntry {
            learning_item_id: Some("li-known".to_owned()),
            client_item_key: None,
            context: Some("fresh".to_owned()),
            context_family: Some("service.ask_help".to_owned()),
            kind: learning_core::EvidenceKind::Transfer,
            success: true,
            assistance_level: Some(0),
            response_millis: Some(1_200),
            probe_phase: Some(tutor_contract::ProbePhase::Transfer),
            pronunciation_intelligible: Some(true),
            pronunciation_repeat_required: Some(false),
            detail: None,
        });

        let response = complete_lesson(&env, &input).unwrap();
        assert_eq!(
            response["warnings"].as_array().unwrap().len(),
            1,
            "practice transfer must not count as an independent prior mastery episode"
        );
        cleanup(path);
    }

    #[test]
    fn later_historical_transfer_failure_invalidates_that_mastery_episode() {
        let (path, env) = create_env("failed-mastery-history");
        {
            let db = env.connection().unwrap();
            db.execute_batch(
                "INSERT INTO lessons
                    (id, learner_language_profile_id, lesson_number, status, scenario,
                     planned_goal, source, started_at, completed_at, created_at, updated_at)
                 VALUES
                    ('les-hist-1','llp-1',1,'completed','old','old','import',
                     '2026-10-20T10:00:00Z','2026-10-20T10:20:00Z',
                     '2026-10-20T10:00:00Z','2026-10-20T10:20:00Z'),
                    ('les-hist-2','llp-1',2,'completed','old','old','import',
                     '2026-10-21T10:00:00Z','2026-10-21T10:20:00Z',
                     '2026-10-21T10:00:00Z','2026-10-21T10:20:00Z');
                 INSERT INTO learning_item_evidence
                    (id, learning_item_id, lesson_id, kind, success, metadata_json, recorded_at, observation_order)
                 VALUES
                    ('ev-hist-1-ok','li-known','les-hist-1','transfer',1,
                     json_object('assistance_level',0,'probe_phase','transfer',
                                 'context_family','transport.ask_stop'),
                     '2026-10-20T10:10:00Z',0),
                    ('ev-hist-1-fail','li-known','les-hist-1','transfer',0,
                     json_object('assistance_level',0,'probe_phase','transfer',
                                 'context_family','transport.ask_stop'),
                     '2026-10-20T10:15:00Z',1),
                    ('ev-hist-2','li-known','les-hist-2','transfer',1,
                     json_object('assistance_level',0,'probe_phase','transfer',
                                 'context_family','commerce.request_item'),
                     '2026-10-21T10:10:00Z',0);",
            )
            .unwrap();
        }

        let mut input = result();
        input.client_operation_id = "op-failed-mastery-history".to_owned();
        input.mastery_changes[0].new_status = learning_core::LearningItemStatus::Mastered;
        input.evidence.push(tutor_contract::EvidenceEntry {
            learning_item_id: Some("li-known".to_owned()),
            client_item_key: None,
            context: Some("fresh".to_owned()),
            context_family: Some("service.ask_help".to_owned()),
            kind: learning_core::EvidenceKind::Transfer,
            success: true,
            assistance_level: Some(0),
            response_millis: Some(1_200),
            probe_phase: Some(tutor_contract::ProbePhase::Transfer),
            pronunciation_intelligible: Some(true),
            pronunciation_repeat_required: Some(false),
            detail: None,
        });

        let response = complete_lesson(&env, &input).unwrap();
        assert_eq!(
            response["warnings"].as_array().unwrap().len(),
            1,
            "an episode whose latest transfer failed must not count toward mastery history"
        );
        cleanup(path);
    }

    #[test]
    fn complete_lesson_persists_core_business_effects() {
        let (path, env) = create_env("new");
        complete_lesson(&env, &result()).unwrap();
        let state = snapshot(&env);
        assert_eq!(state.profile_lesson_number, 8);
        assert_eq!(state.lessons.len(), 1);
        assert_eq!(
            state.lessons[0],
            (
                8,
                "completed".to_owned(),
                "pharmacy".to_owned(),
                Some(321),
                Some("next".to_owned())
            )
        );
        assert!(
            state
                .items
                .iter()
                .any(|row| row.0 == "¿Tiene algo para la tos?")
        );
        assert!(state.items.iter().any(|row| row.0 == "Buenos días"));
        assert_eq!(state.evidence.len(), 4);
        assert_eq!(state.errors.len(), 2);
        assert_eq!(state.rechecks.len(), 6);
        cleanup(path);
    }

    #[test]
    fn direct_completion_uses_persisted_max_when_profile_counter_is_stale() {
        let (path, env) = create_env("direct-stale-counter");
        env.connection()
            .unwrap()
            .execute(
                "INSERT INTO lessons
                 (id, learner_language_profile_id, lesson_number, status, scenario,
                  planned_goal, source, started_at, completed_at, created_at, updated_at)
                 VALUES
                 ('lesson-stale-8', 'llp-1', 8, 'completed',
                  'seed', '', 'import', '2026-10-15T00:00:00Z',
                  '2026-10-15T00:20:00Z', '2026-10-15T00:00:00Z',
                  '2026-10-15T00:20:00Z')",
                [],
            )
            .unwrap();

        let mut input = result();
        input.client_operation_id = "op-direct-stale".to_owned();
        complete_lesson(&env, &input).unwrap();

        let state = snapshot(&env);
        assert_eq!(state.profile_lesson_number, 9);
        assert_eq!(state.lessons.len(), 2);
        assert_eq!(state.lessons[0].0, 8);
        assert_eq!(state.lessons[1].0, 9);
        cleanup(path);
    }

    #[test]
    fn started_lessons_complete_once_and_advance_numbers() {
        let (path, env) = create_env("started-sequence");
        for number in 8..=9 {
            let started =
                super::super::start_lesson::start_lesson(&env, "llp-1", "pharmacy", "", "mcp")
                    .unwrap();
            assert_eq!(started["lesson_number"], number);
            let mut input = result();
            input.client_operation_id = format!("op-started-{number}");
            input.lesson.lesson_id = Some(started["id"].as_str().unwrap().to_owned());
            input.new_learning_items.clear();
            input.evidence.truncate(1);
            let mut second_probe = input.evidence[0].clone();
            second_probe.kind = learning_core::EvidenceKind::Recognition;
            second_probe.context = Some("pharmacy:diagnostic-2".to_owned());
            second_probe.response_millis = Some(750);
            second_probe.probe_phase = Some(tutor_contract::ProbePhase::Diagnostic);
            input.evidence.push(second_probe);
            input.interventions.clear();
            input.listening_attempts.clear();
            input.error_occurrences.clear();
            input.mastery_changes.clear();
            complete_lesson(&env, &input).unwrap();
            assert_eq!(
                complete_lesson(&env, &input).unwrap()["status"],
                "duplicate"
            );
        }
        assert_eq!(snapshot(&env).profile_lesson_number, 9);
        cleanup(path);
    }

    #[test]
    fn foreign_profile_references_are_rejected() {
        let (path, env) = create_env("foreign-profile");
        {
            let db = env.connection().unwrap();
            db.execute(
                "INSERT INTO learner_profile (id, created_at, updated_at)
                 VALUES ('learner-2', '2026-10-16T00:00:00Z', '2026-10-16T00:00:00Z')",
                [],
            )
            .unwrap();
            db.execute(
                "INSERT INTO learner_language_profiles
                (id, learner_id, source_language, target_locale,
                 created_at, updated_at)
                 VALUES ('llp-2', 'learner-2', 'ru', 'es-ES',
                 '2026-10-16T00:00:00Z', '2026-10-16T00:00:00Z')",
                [],
            )
            .unwrap();
            db.execute(
                "INSERT INTO learning_items
                (id, learner_language_profile_id, item_type,
                 target_text, normalized_target_text, status,
                 created_at, updated_at)
                 VALUES ('li-foreign', 'llp-2', 'phrase','foreign','foreign',
                 'learning','2026-10-16T00:00:00Z','2026-10-16T00:00:00Z')",
                [],
            )
            .unwrap();
        }
        let mut input = result();
        input.new_learning_items.clear();
        input.evidence[0].learning_item_id = Some("li-foreign".to_owned());
        assert!(complete_lesson(&env, &input).is_err());
        cleanup(path);
    }

    #[test]
    fn foreign_error_target_is_rejected_without_side_effects() {
        let (path, env) = create_env("foreign-error");
        let mut input = result();
        input.error_occurrences[0].error_target_id = Some("et-nonexistent".to_owned());
        let before = snapshot(&env);
        assert!(complete_lesson(&env, &input).is_err());
        assert_eq!(snapshot(&env), before);
        cleanup(path);
    }

    #[test]
    fn duplicate_retry_is_a_noop() {
        let (path, env) = create_env("duplicate");
        let input = result();

        complete_lesson(&env, &input).unwrap();
        let once = snapshot(&env);
        complete_lesson(&env, &input).unwrap();
        let twice = snapshot(&env);

        assert_eq!(twice, once);
        cleanup(path);
    }

    #[derive(Debug, PartialEq)]
    struct CardSnapshot {
        cards: Vec<(String, String, String, String, String)>,
        tags: Vec<(String, String, String)>,
        schedules: Vec<(String, String, String, u32, u32, u32)>,
        audits: Vec<(String, String, String, String, String, Option<String>)>,
    }

    fn seed_cards(env: &Env) {
        let db = env.connection().unwrap();
        for (id, prompt, status, accepted, hash) in [
            (
                "card-update",
                "old prompt",
                "review",
                "[\"old\"]",
                "hash-update",
            ),
            (
                "card-suspend",
                "suspend prompt",
                "review",
                "[]",
                "hash-suspend",
            ),
            (
                "card-accept",
                "accept prompt",
                "review",
                "[\"one\",\"two\"]",
                "hash-accept",
            ),
        ] {
            db.execute(
                "INSERT INTO cards
                 (id, learner_language_profile_id,
                  intent, status, prompt, prompt_language, answer, answer_language,
                  accepted_answers_json, creation_reason,
                  content_hash, content_version, created_by, created_at, updated_at)
                 VALUES (?1, 'llp-1', 'production', ?2, ?3, 'ru',
                         'answer', 'es-ES', ?4, 'seed', ?5, 1, 'tutor',
                         '2026-10-15T00:00:00Z', '2026-10-15T00:00:00Z')",
                rusqlite::params![id, status, prompt, accepted, hash],
            )
            .unwrap();
        }
    }

    fn card_result() -> tutor_contract::LessonResult {
        let mut input = result();
        input.client_operation_id = "op-cards-1".to_owned();
        input.card_mutations = vec![
            tutor_contract::CardMutation {
                operation: tutor_contract::MutationOperation::Create,
                client_operation_id: "card-op-create".to_owned(),
                card_id: None,
                card: Some(tutor_contract::CardPayload {
                    intent: learning_core::CardIntent::Production,
                    prompt: "created prompt".to_owned(),
                    answer: "created answer".to_owned(),
                    accepted_answers: vec!["created alt".to_owned()],
                    prompt_language: Some("ru".to_owned()),
                    explanation: Some("created explanation".to_owned()),
                    scenario_tags: vec!["Pharmacy".to_owned()],
                    skill_tags: vec!["Grammar".to_owned()],
                    source_learning_item_id: None,
                    source_error_target_id: None,
                    creation_reason: "lesson create".to_owned(),
                    priority_score: Some(0.8),
                }),
                accepted_answer: None,
                reason: Some("create reason".to_owned()),
            },
            tutor_contract::CardMutation {
                operation: tutor_contract::MutationOperation::Create,
                client_operation_id: "card-op-duplicate".to_owned(),
                card_id: None,
                card: Some(tutor_contract::CardPayload {
                    intent: learning_core::CardIntent::Production,
                    prompt: "created prompt".to_owned(),
                    answer: "created answer".to_owned(),
                    accepted_answers: vec!["created alt".to_owned()],
                    prompt_language: Some("ru".to_owned()),
                    explanation: Some("created explanation".to_owned()),
                    scenario_tags: vec![],
                    skill_tags: vec![],
                    source_learning_item_id: None,
                    source_error_target_id: None,
                    creation_reason: "duplicate create".to_owned(),
                    priority_score: Some(0.8),
                }),
                accepted_answer: None,
                reason: None,
            },
            tutor_contract::CardMutation {
                operation: tutor_contract::MutationOperation::Update,
                client_operation_id: "card-op-update".to_owned(),
                card_id: Some("card-update".to_owned()),
                card: Some(tutor_contract::CardPayload {
                    intent: learning_core::CardIntent::Production,
                    prompt: "updated prompt".to_owned(),
                    answer: "answer".to_owned(),
                    accepted_answers: vec!["old".to_owned()],
                    prompt_language: Some("ru".to_owned()),
                    explanation: Some("updated explanation".to_owned()),
                    scenario_tags: vec![],
                    skill_tags: vec![],
                    source_learning_item_id: None,
                    source_error_target_id: None,
                    creation_reason: "update".to_owned(),
                    priority_score: None,
                }),
                accepted_answer: None,
                reason: Some("update reason".to_owned()),
            },
            tutor_contract::CardMutation {
                operation: tutor_contract::MutationOperation::Suspend,
                client_operation_id: "card-op-suspend".to_owned(),
                card_id: Some("card-suspend".to_owned()),
                card: None,
                accepted_answer: None,
                reason: Some("pause".to_owned()),
            },
            tutor_contract::CardMutation {
                operation: tutor_contract::MutationOperation::AddAcceptedAnswer,
                client_operation_id: "card-op-accepted".to_owned(),
                card_id: Some("card-accept".to_owned()),
                card: None,
                accepted_answer: Some("three".to_owned()),
                reason: Some("accept variant".to_owned()),
            },
            tutor_contract::CardMutation {
                operation: tutor_contract::MutationOperation::Link,
                client_operation_id: "card-op-link".to_owned(),
                card_id: Some("card-update".to_owned()),
                card: None,
                accepted_answer: None,
                reason: Some("unsupported link".to_owned()),
            },
        ];
        input
    }

    fn card_snapshot(env: &Env) -> CardSnapshot {
        let db = env.connection().unwrap();

        let cards = db
            .prepare(
                "SELECT prompt, status, answer, accepted_answers_json, content_hash
             FROM cards ORDER BY prompt",
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

        let tags = db
            .prepare(
                "SELECT c.prompt, t.tag_type, t.normalized_tag
             FROM card_tags t JOIN cards c ON c.id=t.card_id
             ORDER BY c.prompt, t.tag_type, t.normalized_tag",
            )
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();

        let schedules = db
            .prepare(
                "SELECT c.prompt, s.algorithm, s.algorithm_version,
                    s.lapse_count, s.review_count, s.state_version
             FROM card_schedule_state s JOIN cards c ON c.id=s.card_id
             ORDER BY c.prompt",
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
                ))
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();

        let audits = db
            .prepare(
                "SELECT m.client_operation_id, m.operation, m.validation_result,
                    m.reason, COALESCE(c.prompt, m.card_id, ''), m.after_json
             FROM card_mutations m
             LEFT JOIN cards c ON c.id=m.card_id
             ORDER BY m.client_operation_id",
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
                ))
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();

        CardSnapshot {
            cards,
            tags,
            schedules,
            audits,
        }
    }

    #[test]
    fn card_mutations_apply_expected_create_update_status_and_answer_changes() {
        let (path, env) = create_env("cards-new");
        seed_cards(&env);
        complete_lesson(&env, &card_result()).unwrap();
        let state = card_snapshot(&env);
        assert!(state.cards.iter().any(|row| row.0 == "created prompt"));
        assert!(state.cards.iter().any(|row| row.0 == "updated prompt"));
        assert!(
            state
                .cards
                .iter()
                .any(|row| row.0 == "suspend prompt" && row.1 == "suspended")
        );
        assert!(!state.audits.is_empty());
        cleanup(path);
    }

    fn seed_started_lesson(env: &Env) {
        let db = env.connection().unwrap();
        db.execute_batch(
            "INSERT INTO lessons
                (id, learner_language_profile_id,  lesson_number, status, scenario,
                 planned_goal, source, started_at, created_at, updated_at)
             VALUES
                ('lesson-active', 'llp-1', 8, 'active',
                 'pharmacy', 'goal', 'pwa', '2026-10-16T00:00:00Z',
                 '2026-10-16T00:00:00Z', '2026-10-16T00:00:00Z');
             INSERT INTO planner_decisions
                (id, learner_language_profile_id, lesson_id, planner_version,
                 input_json, plan_json, task_candidates_json, capability_state_json,
                 focus_threads_json, outcome_json, created_at, updated_at)
             VALUES
                ('pd-active', 'llp-1', 'lesson-active', 'planner-v3',
                 '{}', '{}', '[]', '[]', '[]', NULL,
                 '2026-10-16T00:00:00Z', '2026-10-16T00:00:00Z');",
        )
        .unwrap();
    }

    #[test]
    fn accepts_tutor_defined_transfer_context_family_without_language_whitelist() {
        let (path, env) = create_env("tutor-defined-family");
        let mut input = result();
        input.client_operation_id = "op-tutor-defined-family".to_owned();
        input.evidence.push(tutor_contract::EvidenceEntry {
            learning_item_id: Some("li-known".to_owned()),
            client_item_key: None,
            context: Some("cafe:bill".to_owned()),
            context_family: Some("cafe.request_bill".to_owned()),
            kind: learning_core::EvidenceKind::Transfer,
            success: true,
            assistance_level: Some(0),
            response_millis: Some(1_200),
            probe_phase: Some(tutor_contract::ProbePhase::Transfer),
            pronunciation_intelligible: Some(true),
            pronunciation_repeat_required: Some(false),
            detail: None,
        });

        complete_lesson(&env, &input).unwrap();
        let database = env.connection().unwrap();
        let metadata: String = database
            .query_row(
                "SELECT metadata_json FROM learning_item_evidence
                 WHERE kind='transfer' AND metadata_json LIKE '%cafe.request_bill%'
                 LIMIT 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let metadata: serde_json::Value = serde_json::from_str(&metadata).unwrap();
        assert_eq!(
            metadata["context_family"],
            serde_json::json!("cafe.request_bill")
        );
        cleanup(path);
    }

    #[test]
    fn completing_started_lesson_preserves_single_planner_decision_and_records_outcome() {
        let (path, env) = create_env("planner-new");
        seed_started_lesson(&env);
        env.connection()
            .unwrap()
            .execute(
                "UPDATE learner_language_profiles
                 SET current_lesson_number = 0
                 WHERE id = 'llp-1'",
                [],
            )
            .unwrap();
        let mut input = result();
        input.client_operation_id = "op-existing-lesson".to_owned();
        input.lesson.lesson_id = Some("lesson-active".to_owned());
        complete_lesson(&env, &input).unwrap();

        let state = snapshot(&env);
        assert_eq!(state.profile_lesson_number, 8);
        let db = env.connection().unwrap();
        let outcome: Option<String> = db
            .query_row(
                "SELECT outcome_json FROM planner_decisions WHERE id='pd-active'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let outcome: serde_json::Value =
            serde_json::from_str(outcome.as_deref().unwrap_or("{}")).unwrap();
        assert_eq!(outcome["calibration_valid"], true);
        assert_eq!(outcome["diagnostic_probe_count"], 2);
        assert_eq!(outcome["interventions"][0]["action"], "fade_prompts");
        assert_eq!(outcome["interventions"][0]["learning_item_id"], "li-known");
        assert_eq!(
            outcome["interventions"][0]["target_capability"],
            "spontaneous_production"
        );
        assert_eq!(outcome["interventions"][0]["calibration_eligible"], true);
        let count: u32 = db
            .query_row("SELECT COUNT(*) FROM planner_decisions", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 1);
        drop(db);
        cleanup(path);
    }

    #[test]
    fn assisted_listening_diagnostic_is_not_a_causal_baseline() {
        let mut input = result();
        input.interventions[0] = tutor_contract::InterventionEvent {
            action: "listening_support".to_owned(),
            learning_item_id: Some("li-known".to_owned()),
            client_item_key: None,
            target_capability: learning_core::EvidenceKind::Listening,
            evidence_index: None,
            listening_attempt_index: Some(1),
        };
        input.listening_attempts = vec![
            tutor_contract::ListeningAttemptEntry {
                learning_item_id: Some("li-known".to_owned()),
                client_item_key: None,
                success: false,
                assistance_level: Some(2),
                response_millis: Some(1_500),
                probe_phase: Some(tutor_contract::ProbePhase::Diagnostic),
                source_reference: Some("audio-baseline".to_owned()),
                source_variant_id: None,
                talker_key: None,
                context_tag: Some("pharmacy:baseline".to_owned()),
                transcript: Some("baseline".to_owned()),
            },
            tutor_contract::ListeningAttemptEntry {
                learning_item_id: Some("li-known".to_owned()),
                client_item_key: None,
                success: true,
                assistance_level: Some(0),
                response_millis: Some(1_000),
                probe_phase: Some(tutor_contract::ProbePhase::Practice),
                source_reference: Some("audio-post".to_owned()),
                source_variant_id: None,
                talker_key: None,
                context_tag: Some("pharmacy:post".to_owned()),
                transcript: Some("post".to_owned()),
            },
        ];

        assert!(
            !intervention_calibration_eligible(&input, &input.interventions[0]),
            "an assisted listening diagnostic is not an independent pre-intervention baseline"
        );
    }

    #[test]
    fn diagnostic_after_intervention_outcome_is_not_a_causal_baseline() {
        let mut input = result();
        input.evidence[0].probe_phase = Some(tutor_contract::ProbePhase::Practice);
        input.evidence.push(tutor_contract::EvidenceEntry {
            learning_item_id: Some("li-known".to_owned()),
            client_item_key: None,
            context: Some("pharmacy:late-diagnostic".to_owned()),
            context_family: None,
            kind: learning_core::EvidenceKind::SpontaneousProduction,
            success: true,
            assistance_level: Some(0),
            response_millis: Some(850),
            probe_phase: Some(tutor_contract::ProbePhase::Diagnostic),
            pronunciation_intelligible: Some(true),
            pronunciation_repeat_required: Some(false),
            detail: None,
        });

        assert!(
            !intervention_calibration_eligible(&input, &input.interventions[0]),
            "a diagnostic observed after the referenced intervention outcome cannot serve as its baseline"
        );
    }

    #[test]
    fn unrelated_diagnostics_do_not_make_intervention_calibration_eligible() {
        let (path, env) = create_env("planner-unrelated-calibration");
        seed_started_lesson(&env);
        let mut input = result();
        input.client_operation_id = "op-unrelated-calibration".to_owned();
        input.lesson.lesson_id = Some("lesson-active".to_owned());
        input.interventions[0].target_capability = learning_core::EvidenceKind::Recognition;
        input.interventions[0].evidence_index = None;

        complete_lesson(&env, &input).unwrap();

        let db = env.connection().unwrap();
        let outcome: String = db
            .query_row(
                "SELECT outcome_json FROM planner_decisions WHERE id='pd-active'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let outcome: serde_json::Value = serde_json::from_str(&outcome).unwrap();
        assert_eq!(outcome["calibration_valid"], true);
        assert_eq!(outcome["interventions"][0]["calibration_eligible"], false);
        drop(db);
        cleanup(path);
    }

    #[test]
    fn late_foreign_key_failure_rolls_back_every_earlier_flush() {
        let (path, env) = create_env("rollback");
        seed_cards(&env);

        let before = snapshot(&env);
        let cards_before = card_snapshot(&env);

        let mut input = card_result();
        input.client_operation_id = "op-rollback".to_owned();
        input.card_mutations[0]
            .card
            .as_mut()
            .unwrap()
            .source_error_target_id = Some("missing-error-target".to_owned());

        assert!(complete_lesson(&env, &input).is_err());

        assert_eq!(snapshot(&env), before);
        assert_eq!(card_snapshot(&env), cards_before);
        cleanup(path);
    }

    #[test]
    fn max_error_batch_flushes_all_recheck_chunks_without_loss() {
        let (path, env) = create_env("max-errors");

        let mut value = serde_json::to_value(result()).unwrap();
        value["client_operation_id"] = serde_json::json!("op-max-errors");
        value["error_occurrences"] = serde_json::Value::Array(
            (0..tutor_contract::bounds::MAX_ERROR_OCCURRENCES)
                .map(|i| {
                    serde_json::json!({
                        "core_category": "grammar",
                        "original_production": format!("wrong-{i}"),
                        "corrected_production": format!("right-{i}")
                    })
                })
                .collect(),
        );
        value["card_mutations"] = serde_json::json!([]);
        let input = tutor_contract::parse_lesson_result(&value.to_string()).unwrap();

        complete_lesson(&env, &input).unwrap();

        let db = env.connection().unwrap();
        let targets: u32 = db
            .query_row(
                "SELECT COUNT(*) FROM error_targets WHERE id != 'et-known'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let occurrences: u32 = db
            .query_row("SELECT COUNT(*) FROM error_occurrences", [], |row| {
                row.get(0)
            })
            .unwrap();
        let rechecks: u32 = db
            .query_row("SELECT COUNT(*) FROM error_rechecks", [], |row| row.get(0))
            .unwrap();

        assert_eq!(
            targets as usize,
            tutor_contract::bounds::MAX_ERROR_OCCURRENCES
        );
        assert_eq!(
            occurrences as usize,
            tutor_contract::bounds::MAX_ERROR_OCCURRENCES
        );
        assert_eq!(
            rechecks as usize,
            tutor_contract::bounds::MAX_ERROR_OCCURRENCES * 3
        );

        cleanup(path);
    }
}

use crate::repositories::{lessons, to_rfc3339};
// Two transfer half-lives: old context diversity can support mastery only while
// it remains recent enough to be meaningfully revalidated by a fresh transfer.
const MASTERY_HISTORY_WINDOW_SECONDS: i64 = 84 * 86_400;
use crate::{Env, Result};
use rusqlite::{OptionalExtension as _, TransactionBehavior, params};

use super::card_apply::{apply_card_mutations, prepare_card_mutations};
use super::error::{
    insert_error_occurrence, insert_error_recheck, insert_error_target, update_error_target,
};
use super::lesson_plan::{
    LessonPlanKind, LessonPlanState, ResolvedItem, add_known_lesson_item, add_lesson_error_target,
    add_lesson_existing_normalized, add_lesson_mastery_signal_from_episode, lesson_existing_id,
    lesson_normalized_new, lesson_profile_id, new_lesson_plan_state, plan_lesson_completion,
    plan_resolved_item_id, plan_reused_item_id, set_lesson_existing, set_lesson_normalized_item,
};
#[derive(Clone, Copy, Default)]
struct IdRef {
    start: usize,
    len: usize,
}

fn push_id(text: &mut String, value: &str) -> IdRef {
    let start = text.len();
    text.push_str(value);
    IdRef {
        start,
        len: value.len(),
    }
}

fn id_at(text: &str, reference: IdRef) -> &str {
    &text[reference.start..reference.start + reference.len]
}

fn dense_slot(index: u32) -> Result<usize> {
    usize::try_from(index).map_err(|_| {
        crate::Error::operation(
            500,
            "internal_invariant",
            "dense u32 index does not fit usize",
            serde_json::Value::Null,
        )
    })
}

fn intervention_calibration_eligible(
    result: &tutor_contract::LessonResult,
    intervention: &tutor_contract::InterventionEvent,
) -> bool {
    let same_item = |learning_item_id: Option<&str>, client_item_key: Option<&str>| {
        learning_item_id == intervention.learning_item_id.as_deref()
            && client_item_key == intervention.client_item_key.as_deref()
    };
    let baseline_qualified =
        |kind: learning_core::EvidenceKind, assistance_level: Option<u8>| match kind {
            learning_core::EvidenceKind::PromptedProduction => {
                assistance_level.is_some_and(|level| level > 0)
            }
            learning_core::EvidenceKind::Recognition
            | learning_core::EvidenceKind::Listening
            | learning_core::EvidenceKind::SpontaneousProduction
            | learning_core::EvidenceKind::Transfer => assistance_level == Some(0),
            learning_core::EvidenceKind::Correction => false,
        };

    if intervention.target_capability == learning_core::EvidenceKind::Listening {
        let Some(outcome_index) = intervention.listening_attempt_index.map(usize::from) else {
            return false;
        };
        let Some(outcome) = result.listening_attempts.get(outcome_index) else {
            return false;
        };
        if outcome.probe_phase == Some(tutor_contract::ProbePhase::Diagnostic) {
            return false;
        }
        return result.listening_attempts[..outcome_index]
            .iter()
            .any(|attempt| {
                attempt.probe_phase == Some(tutor_contract::ProbePhase::Diagnostic)
                    && baseline_qualified(
                        learning_core::EvidenceKind::Listening,
                        attempt.assistance_level,
                    )
                    && same_item(
                        attempt.learning_item_id.as_deref(),
                        attempt.client_item_key.as_deref(),
                    )
            });
    }

    let Some(outcome_index) = intervention.evidence_index.map(usize::from) else {
        return false;
    };
    let Some(outcome) = result.evidence.get(outcome_index) else {
        return false;
    };
    if outcome.probe_phase == Some(tutor_contract::ProbePhase::Diagnostic) {
        return false;
    }

    result.evidence[..outcome_index].iter().any(|entry| {
        entry.probe_phase == Some(tutor_contract::ProbePhase::Diagnostic)
            && entry.kind == intervention.target_capability
            && baseline_qualified(entry.kind, entry.assistance_level)
            && same_item(
                entry.learning_item_id.as_deref(),
                entry.client_item_key.as_deref(),
            )
    })
}

fn canonical_target_text(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut pending_space = false;
    for ch in input.trim().chars() {
        if ch.is_whitespace() {
            pending_space = true;
        } else {
            if pending_space && !out.is_empty() {
                out.push(' ');
            }
            pending_space = false;
            out.push(ch);
        }
    }
    out
}

use super::lesson_write::{
    insert_evidence, insert_learning_item, insert_listening_attempt, update_item_status,
};
use super::recommendation_write::insert_recommendation;

pub(crate) fn complete_lesson(
    env: &Env,
    result: &tutor_contract::LessonResult,
) -> Result<serde_json::Value> {
    let issues = tutor_contract::validate(result);
    if !issues.is_empty() {
        return Err(crate::Error::operation(
            400,
            "validation_failed",
            "lesson result rejected",
            serde_json::json!({ "issues": issues }),
        ));
    }

    let mut database = env.connection()?;
    let transaction = database.transaction_with_behavior(TransactionBehavior::Immediate)?;

    let (current_lesson_number, source_language, target_locale): (u32, String, String) =
        transaction
            .query_row(
                "SELECT current_lesson_number, source_language, target_locale
                 FROM learner_language_profiles WHERE id = ?1",
                params![result.language_profile_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?
            .ok_or_else(|| {
                crate::Error::operation(
                    404,
                    "not_found",
                    "unknown profile",
                    serde_json::Value::Null,
                )
            })?;

    let max_persisted_lesson_number: u32 = transaction.query_row(
        "SELECT COALESCE(MAX(lesson_number), 0)
         FROM lessons
         WHERE learner_language_profile_id = ?1",
        params![result.language_profile_id],
        |row| row.get(0),
    )?;
    let current_lesson_number = current_lesson_number.max(max_persisted_lesson_number);

    let now = crate::api::now();
    let state = load_lesson_plan_state(&transaction, result, current_lesson_number, now)?;

    let plan = plan_lesson_completion(result, &state).map_err(plan_error)?;
    if plan.lesson_kind == LessonPlanKind::DuplicateNoop {
        return crate::json(&serde_json::json!({
            "status": "duplicate",
            "client_operation_id": result.client_operation_id
        }));
    }

    let at = to_rfc3339(now);

    let (card_state, card_plan) = prepare_card_mutations(
        &transaction,
        &result.language_profile_id,
        &result.card_mutations,
        now,
    )?;

    let lesson_id = match plan.lesson_kind {
        LessonPlanKind::CompleteExisting => {
            transaction.execute(
                lessons::COMPLETE_LESSON,
                params![
                    lesson_existing_id(&state),
                    at,
                    result.lesson.duration_seconds,
                    result.lesson.tutor_summary,
                    result.lesson.next_goal,
                    result.client_operation_id,
                ],
            )?;
            lesson_existing_id(&state).to_owned()
        }
        LessonPlanKind::CreateCompleted => {
            let id = crate::api::new_id("les");
            transaction.execute(
                lessons::INSERT_LESSON,
                params![
                    id,
                    result.language_profile_id,
                    plan.lesson_number,
                    "completed",
                    result.lesson.scenario,
                    "",
                    "import",
                    at,
                    at,
                    result.lesson.duration_seconds,
                    result.lesson.tutor_summary,
                    result.lesson.next_goal,
                    result.client_operation_id,
                    at,
                ],
            )?;
            id
        }
        LessonPlanKind::DuplicateNoop => unreachable!("handled above"),
    };

    if let Some(metrics) = &result.metrics {
        transaction.execute(
            lessons::INSERT_METRICS,
            params![
                lesson_id,
                metrics.speaking_seconds,
                metrics.unaided_recall_count,
                metrics.aided_recall_count,
                metrics.listening_score,
                metrics.avg_answer_millis,
                metrics.long_pause_count,
                metrics.task_attempt_count,
                metrics.task_success_count,
                metrics
                    .task_success_rubric
                    .map(tutor_contract::communicative_task_rubric_id),
                metrics.confidence,
                at,
            ],
        )?;
    }

    const MAX_NEW: usize = tutor_contract::bounds::MAX_NEW_ITEMS;

    let mut new_item_id_text = String::new();
    let mut new_item_ids = [IdRef::default(); MAX_NEW];

    for (plan_index, id_slot) in new_item_ids.iter_mut().enumerate().take(plan.new_items_len) {
        let source_index = dense_slot(plan.new_item_source[plan_index])?;
        let source = &result.new_learning_items[source_index];
        let id = crate::api::new_id("li");
        *id_slot = push_id(&mut new_item_id_text, &id);

        insert_learning_item(
            &transaction,
            &id,
            &result.language_profile_id,
            crate::repositories::learning_item_type_db(&source.item_type),
            &source.target_text,
            lesson_normalized_new(&state, source_index),
            source.prompt_text.as_deref(),
            source.prompt_language.as_deref(),
            "introduced",
            Some(&lesson_id),
            &at,
        )?;
    }

    for i in 0..plan.evidence_len {
        let source = &result.evidence[i];
        let item_id = match plan.evidence_item[i] {
            ResolvedItem::New(index) => id_at(&new_item_id_text, new_item_ids[dense_slot(index)?]),
            resolved => plan_resolved_item_id(&plan, &state, resolved),
        };
        let metadata = serde_json::json!({
            "context_key": source
                .context
                .as_deref()
                .unwrap_or(result.lesson.scenario.as_str()),
            "context_family": source.context_family,
            "assistance_level": source.assistance_level,
            "response_millis": source.response_millis,
            "probe_phase": source.probe_phase,
            "pronunciation_intelligible": source.pronunciation_intelligible,
            "pronunciation_repeat_required": source.pronunciation_repeat_required
        })
        .to_string();
        insert_evidence(
            &transaction,
            &crate::api::new_id("ev"),
            item_id,
            &lesson_id,
            crate::repositories::evidence_kind_db(&source.kind),
            source.success,
            source.detail.as_deref(),
            &metadata,
            &at,
            u32::try_from(i).map_err(|_| {
                crate::Error::RustError("evidence observation order overflow".to_owned())
            })?,
        )?;
    }

    for i in 0..plan.listening_len {
        let source = &result.listening_attempts[i];
        let item_id = match plan.listening_item[i] {
            Some(ResolvedItem::New(index)) => {
                Some(id_at(&new_item_id_text, new_item_ids[dense_slot(index)?]))
            }
            Some(known) => Some(plan_resolved_item_id(&plan, &state, known)),
            None => None,
        };

        insert_listening_attempt(
            &transaction,
            &crate::api::new_id("la"),
            &result.language_profile_id,
            item_id,
            Some(&lesson_id),
            source.source_reference.as_deref(),
            source.source_variant_id.as_deref(),
            source.talker_key.as_deref(),
            source.context_tag.as_deref(),
            source.transcript.as_deref(),
            source.success,
            &at,
        )?;

        if let Some(item_id) = item_id {
            let context_key = format!(
                "variant={}|talker={}|context={}",
                source.source_variant_id.as_deref().unwrap_or(""),
                source.talker_key.as_deref().unwrap_or(""),
                source.context_tag.as_deref().unwrap_or("")
            );
            let metadata = serde_json::json!({
                "context_key": context_key,
                "assistance_level": source.assistance_level,
                "response_millis": source.response_millis,
                "probe_phase": source.probe_phase
            })
            .to_string();
            let order = plan
                .evidence_len
                .checked_add(i)
                .and_then(|value| u32::try_from(value).ok())
                .ok_or_else(|| {
                    crate::Error::RustError("listening observation order overflow".to_owned())
                })?;
            insert_evidence(
                &transaction,
                &crate::api::new_id("ev"),
                item_id,
                &lesson_id,
                "listening",
                source.success,
                None,
                &metadata,
                &at,
                order,
            )?;
        }
    }

    for i in 0..plan.error_len {
        let source = &result.error_occurrences[i];
        let target_id = if plan.error_target_present[i] {
            let id = source.error_target_id.as_deref().ok_or_else(|| {
                crate::Error::RustError("existing error target missing after validation".to_owned())
            })?;
            update_error_target(
                &transaction,
                id,
                plan.error_new_count[i],
                plan.error_method_change[i],
                "recheck",
                &at,
            )?;
            id.to_owned()
        } else {
            let id = crate::api::new_id("et");
            insert_error_target(
                &transaction,
                &id,
                &result.language_profile_id,
                crate::repositories::core_error_category_db(&source.core_category),
                source.error_code.as_deref(),
                &source.original_production,
                &source.corrected_production,
                source.explanation.as_deref(),
                1,
                false,
                "active",
                &at,
            )?;
            id
        };

        insert_error_occurrence(
            &transaction,
            &crate::api::new_id("eo"),
            &target_id,
            Some(&lesson_id),
            None,
            crate::repositories::communicative_impact_db(&source.communicative_impact),
            &at,
        )?;

        if plan.error_target_present[i] {
            let count = plan.error_new_count[i];
            let method_threshold =
                learning_core::error_method_change_threshold(source.communicative_impact);
            let escalation_threshold =
                learning_core::error_escalation_threshold(source.communicative_impact);
            let recommendation = if count >= method_threshold {
                Some((
                    "require_method_change",
                    format!("{count} unresolved occurrences — change the method"),
                ))
            } else if count >= escalation_threshold {
                Some((
                    "activate_mini_target",
                    format!("{count} occurrences — activate a mini-target and a contrast card"),
                ))
            } else {
                None
            };

            if let Some((action, detail)) = recommendation {
                insert_recommendation(
                    &transaction,
                    &result.language_profile_id,
                    "error_target",
                    Some(&target_id),
                    action,
                    &serde_json::json!({ "detail": detail }).to_string(),
                    &at,
                )?;
            }
        }

        for offset in learning_core::ERROR_RECHECK_OFFSETS_DAYS {
            let due = learning_core::Timestamp(now.0 + i64::from(offset) * 86_400);
            insert_error_recheck(
                &transaction,
                &crate::api::new_id("rc"),
                &target_id,
                offset,
                &to_rfc3339(due),
            )?;
        }
    }

    for i in 0..plan.status_len {
        update_item_status(
            &transaction,
            &result.mastery_changes[dense_slot(plan.status_source[i])?].learning_item_id,
            crate::repositories::learning_item_status_db(
                &result.mastery_changes[dense_slot(plan.status_source[i])?].new_status,
            ),
            &at,
        )?;
    }

    transaction.execute(
        "UPDATE learner_language_profiles
         SET current_lesson_number = MAX(
                 current_lesson_number,
                 COALESCE(
                     (SELECT MAX(lesson_number)
                      FROM lessons
                      WHERE learner_language_profile_id = ?1
                        AND status = 'completed'),
                     current_lesson_number
                 )
             ),
             updated_at = ?2
         WHERE id = ?1",
        params![result.language_profile_id, at],
    )?;

    let card_summary = apply_card_mutations(
        &transaction,
        &card_state,
        &card_plan,
        &result.card_mutations,
        &result.language_profile_id,
        &source_language,
        &target_locale,
        Some(&lesson_id),
        "lesson result",
        now,
        &at,
    )?;

    let mut intervention_outcome = Vec::with_capacity(result.interventions.len());
    for intervention in &result.interventions {
        let item_id = if let Some(id) = intervention.learning_item_id.as_deref() {
            id.to_owned()
        } else {
            let key = intervention.client_item_key.as_deref().ok_or_else(|| {
                crate::Error::RustError("intervention item missing after validation".to_owned())
            })?;
            let source_index = result
                .new_learning_items
                .iter()
                .position(|item| item.client_item_key.as_deref() == Some(key))
                .ok_or_else(|| {
                    crate::Error::RustError(
                        "intervention client item key missing after validation".to_owned(),
                    )
                })?;
            let source_key = u32::try_from(source_index).map_err(|_| {
                crate::Error::RustError("intervention source index overflow".to_owned())
            })?;
            if let Some(plan_index) = plan.new_item_source[..plan.new_items_len]
                .iter()
                .position(|candidate| *candidate == source_key)
            {
                id_at(&new_item_id_text, new_item_ids[plan_index]).to_owned()
            } else if let Some(reused_index) = plan.reused_item_source[..plan.reused_items_len]
                .iter()
                .position(|candidate| *candidate == source_key)
            {
                plan_reused_item_id(&plan, &state, reused_index).to_owned()
            } else {
                return Err(crate::Error::RustError(
                    "intervention item did not resolve to persisted item".to_owned(),
                ));
            }
        };
        intervention_outcome.push(serde_json::json!({
            "action": intervention.action,
            "learning_item_id": item_id,
            "target_capability": intervention.target_capability,
            "evidence_index": intervention.evidence_index,
            "listening_attempt_index": intervention.listening_attempt_index,
            "calibration_eligible": intervention_calibration_eligible(result, intervention),
        }));
    }
    let diagnostic_probe_count = result
        .evidence
        .iter()
        .filter(|entry| entry.probe_phase == Some(tutor_contract::ProbePhase::Diagnostic))
        .count()
        + result
            .listening_attempts
            .iter()
            .filter(|entry| entry.probe_phase == Some(tutor_contract::ProbePhase::Diagnostic))
            .count();
    let calibration_valid = diagnostic_probe_count >= 2;

    let outcome = serde_json::json!({
        "scenario": result.lesson.scenario,
        "metrics": result.metrics,
        "evidence": result.evidence,
        "listening_attempts": result.listening_attempts,
        "interventions": intervention_outcome,
        "diagnostic_probe_count": diagnostic_probe_count,
        "calibration_valid": calibration_valid,
        "error_occurrences": result.error_occurrences,
        "mastery_changes": result.mastery_changes,
        "next_goal": result.lesson.next_goal,
    });
    transaction.execute(
        "UPDATE planner_decisions
         SET outcome_json = ?2, updated_at = ?3
         WHERE lesson_id = ?1",
        params![lesson_id, outcome.to_string(), at],
    )?;

    transaction.commit()?;

    let mut new_learning_items = [""; MAX_NEW];
    for i in 0..plan.new_items_len {
        new_learning_items[i] = id_at(&new_item_id_text, new_item_ids[i]);
    }
    let mut reused_learning_items = [""; MAX_NEW];
    for (i, slot) in reused_learning_items
        .iter_mut()
        .enumerate()
        .take(plan.reused_items_len)
    {
        *slot = plan_reused_item_id(&plan, &state, i);
    }
    let mut warnings = Vec::with_capacity(plan.warning_len);
    for i in 0..plan.warning_len {
        warnings.push(format!(
            "mastery proposal for '{}' downgraded",
            result.mastery_changes[dense_slot(plan.warning_source[i])?].learning_item_id,
        ));
    }

    crate::json(&serde_json::json!({
        "lesson_id": lesson_id,
        "new_learning_items": &new_learning_items[..plan.new_items_len],
        "reused_learning_items": &reused_learning_items[..plan.reused_items_len],
        "card_mutations": card_summary.as_slice(),
        "warnings": warnings,
    }))
}

fn load_lesson_plan_state(
    database: &rusqlite::Connection,
    result: &tutor_contract::LessonResult,
    current_lesson_number: u32,
    now: learning_core::Timestamp,
) -> Result<LessonPlanState> {
    let mut state = new_lesson_plan_state(&result.language_profile_id, current_lesson_number);

    state.duplicate_import = database
        .query_row(
            "SELECT 1 FROM lessons WHERE idempotency_key = ?1",
            params![result.client_operation_id],
            |_| Ok(()),
        )
        .optional()?
        .is_some();

    if !state.duplicate_import {
        let existing_lesson = match result.lesson.lesson_id.as_deref() {
            Some(id) => database
                .query_row(
                    "SELECT id FROM lessons WHERE id = ?1 AND learner_language_profile_id = ?2
                     AND status IN ('planned','active')",
                    params![id, result.language_profile_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()?,
            None => database
                .query_row(
                    "SELECT id FROM lessons
                     WHERE learner_language_profile_id = ?1
                       AND status IN ('planned','active')
                     ORDER BY started_at DESC LIMIT 1",
                    params![result.language_profile_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()?,
        };
        if let Some(id) = existing_lesson {
            set_lesson_existing(&mut state, &id);
        }
    }

    for (index, item) in result.new_learning_items.iter().enumerate() {
        let normalized = canonical_target_text(&item.target_text);
        set_lesson_normalized_item(&mut state, index, &normalized).map_err(plan_error)?;

        if let Some(existing_id) = database
            .query_row(
                "SELECT id FROM learning_items
                 WHERE learner_language_profile_id = ?1
                   AND normalized_target_text = ?2
                 LIMIT 1",
                params![result.language_profile_id, normalized],
                |row| row.get::<_, String>(0),
            )
            .optional()?
        {
            add_lesson_existing_normalized(&mut state, &normalized, &existing_id)
                .map_err(plan_error)?;
        }
    }

    for evidence in &result.evidence {
        if let Some(id) = evidence.learning_item_id.as_deref() {
            load_known_item(database, &mut state, id)?;
        }
    }
    for attempt in &result.listening_attempts {
        if let Some(id) = attempt.learning_item_id.as_deref() {
            load_known_item(database, &mut state, id)?;
        }
    }
    for change in &result.mastery_changes {
        load_known_item(database, &mut state, &change.learning_item_id)?;
        load_mastery_signal(database, &mut state, &change.learning_item_id, now)?;
    }

    for occurrence in &result.error_occurrences {
        let Some(id) = occurrence.error_target_id.as_deref() else {
            continue;
        };
        if let Some((count, status)) = database
            .query_row(
                "SELECT occurrence_count, status FROM error_targets
                 WHERE id = ?1 AND learner_language_profile_id = ?2",
                params![id, result.language_profile_id],
                |row| Ok((row.get::<_, u32>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?
        {
            add_lesson_error_target(&mut state, id, count, status == "resolved")
                .map_err(plan_error)?;
        }
    }

    Ok(state)
}

fn load_known_item(
    database: &rusqlite::Connection,
    state: &mut LessonPlanState,
    id: &str,
) -> Result<()> {
    let exists = database
        .query_row(
            "SELECT 1 FROM learning_items
             WHERE id = ?1 AND learner_language_profile_id = ?2",
            params![id, lesson_profile_id(state)],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if exists {
        add_known_lesson_item(state, id).map_err(plan_error)?;
    }
    Ok(())
}

fn load_mastery_signal(
    database: &rusqlite::Connection,
    state: &mut LessonPlanState,
    item_id: &str,
    now: learning_core::Timestamp,
) -> Result<()> {
    let cutoff = to_rfc3339(learning_core::Timestamp(
        now.0.saturating_sub(MASTERY_HISTORY_WINDOW_SECONDS),
    ));
    let now_at = to_rfc3339(now);
    let mut statement = database.prepare(
        "WITH ranked AS (
             SELECT lesson_id,success,metadata_json,
                    ROW_NUMBER() OVER (
                      PARTITION BY CASE WHEN lesson_id IS NULL THEN id ELSE lesson_id END
                      ORDER BY recorded_at DESC,observation_order DESC,rowid DESC
                    ) AS episode_rn
             FROM learning_item_evidence
             WHERE learning_item_id = ?1
               AND kind = 'transfer'
               AND recorded_at >= ?2
               AND recorded_at <= ?3
               AND CASE
                     WHEN json_valid(metadata_json)
                     THEN COALESCE(json_extract(metadata_json,'$.probe_phase'),'legacy') != 'practice'
                     ELSE 1
                   END
         )
         SELECT lesson_id,success,metadata_json
         FROM ranked
         WHERE episode_rn=1",
    )?;
    let mut rows = statement.query(params![item_id, cutoff, now_at])?;
    while let Some(row) = rows.next()? {
        let lesson_id: Option<String> = row.get(0)?;
        let success: u8 = row.get(1)?;
        let metadata: Option<String> = row.get(2)?;

        if success == 0 {
            continue;
        }

        let parsed = metadata
            .as_deref()
            .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok());
        let assistance = parsed
            .as_ref()
            .and_then(|value| value.get("assistance_level"))
            .and_then(serde_json::Value::as_u64);

        if assistance != Some(0) {
            continue;
        }

        if let Some(family) = parsed
            .as_ref()
            .and_then(|value| value.get("context_family"))
            .and_then(serde_json::Value::as_str)
            .filter(|family| !family.trim().is_empty())
        {
            add_lesson_mastery_signal_from_episode(state, item_id, lesson_id.as_deref(), &[family])
                .map_err(plan_error)?;
        }
    }

    Ok(())
}

fn plan_error(error: super::lesson_plan::LessonPlanError) -> crate::Error {
    crate::Error::operation(
        422,
        "apply_rejected",
        format!("{error:?}"),
        serde_json::Value::Null,
    )
}
