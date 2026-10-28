#![allow(clippy::items_after_test_module)] // Test fixtures are intentionally grouped before the processing pass.
//! End-to-end lesson completion on the new fixed-capacity data path.

#[cfg(test)]
// Test fixtures intentionally fail fast and use compact tuple snapshots.
#[allow(clippy::unwrap_used, clippy::type_complexity)]
mod tests {
    use super::*;
    use crate::{Env, built_in_language_packs};
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
                "language_pack_id": "es-ES",
                "language_pack_version": "0.2.0",
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
                    }
                ],
                "interventions": [
                    {
                        "action": "fade_prompts",
                        "learning_item_id": "li-known",
                        "target_capability": "spontaneous_production",
                        "evidence_index": 0
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
                 current_lesson_number, created_at, updated_at)
             VALUES
                ('llp-1', 'learner-1', 'es-ES', 'ru', 'es-ES', 7,
                 '2026-10-16T00:00:00Z', '2026-10-16T00:00:00Z');
             INSERT INTO learning_items
                (id, learner_language_profile_id, language_pack_id, item_type,
                 target_text, normalized_target_text, status, language_metadata_json,
                 created_at, updated_at)
             VALUES
                ('li-known', 'llp-1', 'es-ES', 'phrase', 'known phrase',
                 'known phrase', 'learning', '{}',
                 '2026-10-16T00:00:00Z', '2026-10-16T00:00:00Z'),
                ('li-greeting', 'llp-1', 'es-ES', 'phrase', 'Buenos días',
                 'buenos días', 'introduced', '{}',
                 '2026-10-16T00:00:00Z', '2026-10-16T00:00:00Z');
             INSERT INTO error_targets
                (id, learner_language_profile_id, language_pack_id,
                 language_pack_version, core_category, original_production,
                 corrected_production, occurrence_count, method_change_required,
                 status, created_at, updated_at)
             VALUES
                ('et-known', 'llp-1', 'es-ES', '0.2.0', 'grammar',
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
    fn complete_lesson_persists_core_business_effects() {
        let (path, env) = create_env("new");
        let registry = built_in_language_packs().unwrap();
        complete_lesson(&registry, &env, &result()).unwrap();
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
        assert_eq!(state.evidence.len(), 3);
        assert_eq!(state.errors.len(), 2);
        assert_eq!(state.rechecks.len(), 6);
        cleanup(path);
    }

    #[test]
    fn started_lessons_complete_once_and_advance_numbers() {
        let (path, env) = create_env("started-sequence");
        let registry = built_in_language_packs().unwrap();
        for number in 8..=9 {
            let started = super::super::start_lesson::start_lesson(
                &registry, &env, "llp-1", "pharmacy", "", "mcp",
            )
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
            complete_lesson(&registry, &env, &input).unwrap();
            assert_eq!(
                complete_lesson(&registry, &env, &input).unwrap()["status"],
                "duplicate"
            );
        }
        assert_eq!(snapshot(&env).profile_lesson_number, 9);
        cleanup(path);
    }

    #[test]
    fn foreign_profile_references_are_rejected() {
        let (path, env) = create_env("foreign-profile");
        let registry = built_in_language_packs().unwrap();
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
                (id, learner_id, language_pack_id, source_language, target_locale,
                 created_at, updated_at)
                 VALUES ('llp-2', 'learner-2', 'es-ES', 'ru', 'es-ES',
                 '2026-10-16T00:00:00Z', '2026-10-16T00:00:00Z')",
                [],
            )
            .unwrap();
            db.execute(
                "INSERT INTO learning_items
                (id, learner_language_profile_id, language_pack_id, item_type,
                 target_text, normalized_target_text, status, language_metadata_json,
                 created_at, updated_at)
                 VALUES ('li-foreign', 'llp-2','es-ES','phrase','foreign','foreign',
                 'learning','{}','2026-10-16T00:00:00Z','2026-10-16T00:00:00Z')",
                [],
            )
            .unwrap();
        }
        let mut input = result();
        input.new_learning_items.clear();
        input.evidence[0].learning_item_id = Some("li-foreign".to_owned());
        assert!(complete_lesson(&registry, &env, &input).is_err());
        cleanup(path);
    }

    #[test]
    fn foreign_error_target_is_rejected_without_side_effects() {
        let (path, env) = create_env("foreign-error");
        let registry = built_in_language_packs().unwrap();
        let mut input = result();
        input.error_occurrences[0].error_target_id = Some("et-nonexistent".to_owned());
        let before = snapshot(&env);
        assert!(complete_lesson(&registry, &env, &input).is_err());
        assert_eq!(snapshot(&env), before);
        cleanup(path);
    }

    #[test]
    fn latest_historical_transfer_requires_explicit_unaided_success() {
        let db = rusqlite::Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE learning_item_evidence(
                id TEXT PRIMARY KEY,
                learning_item_id TEXT NOT NULL,
                kind TEXT NOT NULL,
                success INTEGER NOT NULL,
                pack_metadata_json TEXT NOT NULL,
                recorded_at TEXT NOT NULL,
                observation_order INTEGER NOT NULL DEFAULT 0
             );
             INSERT INTO learning_item_evidence
                (id,learning_item_id,kind,success,pack_metadata_json,recorded_at,observation_order)
             VALUES
                ('old-fail','li-1','transfer',0,'{\"assistance_level\":0}','2026-10-22T10:00:00Z',0),
                ('new-unqualified','li-1','transfer',1,'{}','2026-10-23T10:00:00Z',0);",
        )
        .unwrap();

        assert_eq!(latest_transfer_qualified(&db, "li-1").unwrap(), Some(false));

        db.execute(
            "INSERT INTO learning_item_evidence
             (id,learning_item_id,kind,success,pack_metadata_json,recorded_at,observation_order)
             VALUES('new-qualified','li-1','transfer',1,'{\"assistance_level\":0}',
                    '2026-10-23T10:00:00Z',1)",
            [],
        )
        .unwrap();
        assert_eq!(latest_transfer_qualified(&db, "li-1").unwrap(), Some(true));
    }

    #[test]
    fn duplicate_retry_is_a_noop() {
        let (path, env) = create_env("duplicate");
        let registry = built_in_language_packs().unwrap();
        let input = result();

        complete_lesson(&registry, &env, &input).unwrap();
        let once = snapshot(&env);
        complete_lesson(&registry, &env, &input).unwrap();
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
                 (id, learner_language_profile_id, language_pack_id, language_pack_version,
                  intent, status, prompt, prompt_language, answer, answer_language,
                  accepted_answers_json, answer_evaluation_mode, creation_reason,
                  content_hash, content_version, created_by, created_at, updated_at)
                 VALUES (?1, 'llp-1', 'es-ES', '0.2.0', 'production', ?2, ?3, 'ru',
                         'answer', 'es-ES', ?4, 'exact', 'seed', ?5, 1, 'tutor',
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
                    intent: language_pack_api::CardIntent::Production,
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
                    intent: language_pack_api::CardIntent::Production,
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
                    intent: language_pack_api::CardIntent::Production,
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
        let registry = built_in_language_packs().unwrap();
        complete_lesson(&registry, &env, &card_result()).unwrap();
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
                (id, learner_language_profile_id, language_pack_id,
                 language_pack_version, lesson_number, status, scenario,
                 planned_goal, source, started_at, created_at, updated_at)
             VALUES
                ('lesson-active', 'llp-1', 'es-ES', '0.2.0', 8, 'active',
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
    fn rejects_noncanonical_transfer_context_family() {
        let (path, env) = create_env("noncanonical-family");
        let registry = built_in_language_packs().unwrap();
        let mut input = result();
        input.client_operation_id = "op-noncanonical-family".to_owned();
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

        let error = complete_lesson(&registry, &env, &input).unwrap_err();
        assert!(matches!(
            error,
            crate::Error::Operation {
                status: 400,
                code: "validation_failed",
                ..
            }
        ));
        if let crate::Error::Operation { details, .. } = error {
            assert_eq!(
                details["issues"][0]["code"],
                serde_json::json!("unknown_context_family")
            );
        }
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
        let registry = built_in_language_packs().unwrap();
        let mut input = result();
        input.client_operation_id = "op-existing-lesson".to_owned();
        input.lesson.lesson_id = Some("lesson-active".to_owned());
        complete_lesson(&registry, &env, &input).unwrap();

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
    fn late_foreign_key_failure_rolls_back_every_earlier_flush() {
        let (path, env) = create_env("rollback");
        seed_cards(&env);
        let registry = built_in_language_packs().unwrap();

        let before = snapshot(&env);
        let cards_before = card_snapshot(&env);

        let mut input = card_result();
        input.client_operation_id = "op-rollback".to_owned();
        input.card_mutations[0]
            .card
            .as_mut()
            .unwrap()
            .source_error_target_id = Some("missing-error-target".to_owned());

        assert!(complete_lesson(&registry, &env, &input).is_err());

        assert_eq!(snapshot(&env), before);
        assert_eq!(card_snapshot(&env), cards_before);
        cleanup(path);
    }

    #[test]
    fn max_error_batch_flushes_all_recheck_chunks_without_loss() {
        let (path, env) = create_env("max-errors");
        let registry = built_in_language_packs().unwrap();

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

        complete_lesson(&registry, &env, &input).unwrap();

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
use crate::{Env, Result};
use language_pack_api::NormalizationMode;
use rusqlite::{OptionalExtension as _, TransactionBehavior, params};

use super::card_apply::{apply_card_mutations, prepare_card_mutations};
use super::error::{
    insert_error_occurrence, insert_error_recheck, insert_error_target, update_error_target,
};
use super::lesson_plan::{
    LessonPlanKind, LessonPlanState, ResolvedItem, add_known_lesson_item, add_lesson_error_target,
    add_lesson_existing_normalized, add_lesson_mastery_signal, lesson_existing_id,
    lesson_normalized_new, lesson_profile_id, new_lesson_plan_state, plan_lesson_completion,
    plan_resolved_item_id, plan_reused_item_id, set_lesson_existing,
    set_lesson_mastery_latest_transfer_failed, set_lesson_normalized_item,
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

use super::lesson_write::{
    insert_evidence, insert_learning_item, insert_listening_attempt, update_item_status,
};
use super::recommendation_write::insert_recommendation;

pub(crate) fn complete_lesson(
    registry: &crate::BuiltInPackRegistry,
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

    let (pack_id, current_lesson_number, source_language, target_locale): (
        String,
        u32,
        String,
        String,
    ) = transaction
        .query_row(
            "SELECT language_pack_id, current_lesson_number, source_language, target_locale
             FROM learner_language_profiles
             WHERE id = ?1",
            params![result.language_profile_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()?
        .ok_or_else(|| {
            crate::Error::operation(404, "not_found", "unknown profile", serde_json::Value::Null)
        })?;

    let Some(pack) = language_pack_registry::get(registry, &pack_id) else {
        return Err(crate::Error::operation(
            409,
            "pack_incompatible",
            "profile pack not registered",
            serde_json::Value::Null,
        ));
    };
    for (index, evidence) in result.evidence.iter().enumerate() {
        if let Some(family) = evidence.context_family.as_deref()
            && !language_pack_es_es::is_context_family(pack, family)
        {
            return Err(crate::Error::operation(
                400,
                "validation_failed",
                "lesson result rejected",
                serde_json::json!({
                    "issues": [{
                        "code": "unknown_context_family",
                        "path": format!("evidence/{index}/context_family"),
                        "message": format!("'{family}' is not a canonical context_family for {pack_id}")
                    }]
                }),
            ));
        }
    }

    let state =
        load_lesson_plan_state(&transaction, result, &pack_id, current_lesson_number, pack)?;

    let plan = plan_lesson_completion(result, &state).map_err(plan_error)?;
    if plan.lesson_kind == LessonPlanKind::DuplicateNoop {
        return crate::json(&serde_json::json!({
            "status": "duplicate",
            "client_operation_id": result.client_operation_id
        }));
    }

    let now = crate::api::now();
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
                    pack_id,
                    result.language_pack_version,
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
            &pack_id,
            crate::repositories::learning_item_type_db(&source.item_type),
            &source.target_text,
            lesson_normalized_new(&state, source_index),
            source.prompt_text.as_deref(),
            source.prompt_language.as_deref(),
            "introduced",
            "{}",
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
                &pack_id,
                &result.language_pack_version,
                crate::repositories::core_error_category_db(&source.core_category),
                source.pack_error_code.as_deref(),
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
        &pack_id,
        &result.language_pack_version,
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
    pack_id: &str,
    current_lesson_number: u32,
    pack: &language_pack_es_es::EsEsPack,
) -> Result<LessonPlanState> {
    let mut state =
        new_lesson_plan_state(&result.language_profile_id, pack_id, current_lesson_number);

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
        let normalized = language_pack_es_es::normalize_target_text(
            &item.target_text,
            NormalizationMode::Canonical,
        )
        .0;
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
        load_mastery_signal(database, &mut state, &change.learning_item_id, pack)?;
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
    pack: &language_pack_es_es::EsEsPack,
) -> Result<()> {
    let mut statement = database.prepare(
        "SELECT kind, success, pack_metadata_json
         FROM learning_item_evidence
         WHERE learning_item_id = ?1",
    )?;
    let mut rows = statement.query(params![item_id])?;
    while let Some(row) = rows.next()? {
        let kind: String = row.get(0)?;
        let success: u8 = row.get(1)?;
        let metadata: Option<String> = row.get(2)?;

        if success == 0 || !matches!(kind.as_str(), "spontaneous_production" | "transfer") {
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

        let family = if kind == "transfer" {
            parsed
                .as_ref()
                .and_then(|value| value.get("context_family"))
                .and_then(serde_json::Value::as_str)
                .filter(|value| language_pack_es_es::is_context_family(pack, value))
        } else {
            None
        };

        match family {
            Some(family) => {
                add_lesson_mastery_signal(state, item_id, true, &[family]).map_err(plan_error)?
            }
            None => add_lesson_mastery_signal(state, item_id, true, &[]).map_err(plan_error)?,
        }
    }

    if let Some(qualified) = latest_transfer_qualified(database, item_id)? {
        set_lesson_mastery_latest_transfer_failed(state, item_id, !qualified)
            .map_err(plan_error)?;
    }
    Ok(())
}

fn latest_transfer_qualified(
    database: &rusqlite::Connection,
    item_id: &str,
) -> Result<Option<bool>> {
    let latest: Option<(u8, String)> = database
        .query_row(
            "SELECT success, pack_metadata_json
             FROM learning_item_evidence
             WHERE learning_item_id=?1 AND kind='transfer'
             ORDER BY recorded_at DESC, observation_order DESC, rowid DESC
             LIMIT 1",
            params![item_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    Ok(latest.map(|(success, metadata)| {
        let assistance = serde_json::from_str::<serde_json::Value>(&metadata)
            .ok()
            .and_then(|value| {
                value
                    .get("assistance_level")
                    .and_then(serde_json::Value::as_u64)
            });
        success != 0 && assistance == Some(0)
    }))
}

fn plan_error(error: super::lesson_plan::LessonPlanError) -> crate::Error {
    crate::Error::operation(
        422,
        "apply_rejected",
        format!("{error:?}"),
        serde_json::Value::Null,
    )
}
