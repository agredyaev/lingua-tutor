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
    fn import_without_lesson_id_creates_separate_lesson_and_preserves_live_lesson() {
        let (path, env) = create_env("import-with-live-lesson");
        env.connection()
            .unwrap()
            .execute_batch(
                "INSERT INTO lessons
                    (id, learner_language_profile_id, lesson_number, status, scenario,
                     planned_goal, source, started_at, created_at, updated_at)
                 VALUES
                    ('lesson-active','llp-1',8,'active','live','live','mcp',
                     '2026-11-06T10:00:00Z','2026-11-06T10:00:00Z',
                     '2026-11-06T10:00:00Z');",
            )
            .unwrap();

        let response = complete_lesson(&env, &result()).unwrap();
        assert_ne!(response["lesson_id"], "lesson-active");

        let db = env.connection().unwrap();
        let active_status: String = db
            .query_row(
                "SELECT status FROM lessons WHERE id='lesson-active'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(active_status, "active");
        let imported: (u32, String) = db
            .query_row(
                "SELECT lesson_number, status FROM lessons
                 WHERE id=?1",
                [response["lesson_id"].as_str().unwrap()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(imported, (9, "completed".to_owned()));
        drop(db);
        cleanup(path);
    }

    #[test]
    fn same_target_cannot_be_both_scheduled_recheck_and_ordinary_recurrence() {
        let (path, env) = create_env("recheck-occurrence-conflict");
        {
            let db = env.connection().unwrap();
            db.execute(
                "UPDATE error_targets
                 SET occurrence_count=5,method_change_required=1,status='recheck'
                 WHERE id='et-known'",
                [],
            )
            .unwrap();
            db.execute(
                "INSERT INTO error_rechecks
                 (id,error_target_id,cycle,offset_days,due_at)
                 VALUES('rc-conflict','et-known',0,1,'2020-01-01T00:00:00Z')",
                [],
            )
            .unwrap();
        }

        let mut input = result();
        input.client_operation_id = "op-recheck-occurrence-conflict".to_owned();
        input.error_occurrences.truncate(1);
        input.error_rechecks = vec![tutor_contract::ErrorRecheckResult {
            error_target_id: "et-known".to_owned(),
            recheck_id: "rc-conflict".to_owned(),
            passed: true,
            assistance_level: 0,
            context: "pharmacy:recheck".to_owned(),
            repair_applied: false,
            repair_action: None,
        }];

        let before = snapshot(&env);
        let error = complete_lesson(&env, &input).unwrap_err();
        assert!(
            error.to_string().contains("recheck") || error.to_string().contains("error target"),
            "ambiguous double-accounting must fail closed: {error}"
        );
        assert_eq!(snapshot(&env), before);
        cleanup(path);
    }

    #[test]
    fn live_completion_persists_observed_scenario_but_preserves_preplanned_goal() {
        let (path, env) = create_env("observed-scenario");
        let started = super::super::start_lesson::start_lesson(
            &env,
            "llp-1",
            "pharmacy",
            "buy medicine",
            "mcp",
        )
        .unwrap();

        let mut input = result();
        input.client_operation_id = "op-observed-scenario".to_owned();
        input.lesson.lesson_id = Some(started["id"].as_str().unwrap().to_owned());
        input.lesson.scenario = "cafe".to_owned();
        input.new_learning_items.clear();
        input
            .evidence
            .retain(|entry| entry.learning_item_id.is_some());
        input.interventions.clear();
        input.listening_attempts.clear();
        input.error_occurrences.clear();
        input.mastery_changes.clear();

        complete_lesson(&env, &input).unwrap();

        let db = env.connection().unwrap();
        let persisted: (String, String) = db
            .query_row(
                "SELECT scenario,planned_goal FROM lessons WHERE id=?1",
                [started["id"].as_str().unwrap()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(
            persisted,
            ("cafe".to_owned(), "buy medicine".to_owned()),
            "completion must retain the pre-teaching goal while recording the actually observed scenario"
        );
        drop(db);
        cleanup(path);
    }

    #[test]
    fn lesson_result_applies_due_error_recheck_on_canonical_write_path() {
        let (path, env) = create_env("lesson-recheck");
        {
            let db = env.connection().unwrap();
            db.execute(
                "UPDATE error_targets
                 SET occurrence_count=5,method_change_required=1,status='recheck'
                 WHERE id='et-known'",
                [],
            )
            .unwrap();
            db.execute_batch(
                "INSERT INTO error_rechecks
                    (id,error_target_id,cycle,offset_days,due_at)
                 VALUES
                    ('rc-1','et-known',0,1,'2020-01-01T00:00:00Z'),
                    ('rc-3','et-known',0,3,'2020-01-03T00:00:00Z'),
                    ('rc-7','et-known',0,7,'2020-01-07T00:00:00Z');",
            )
            .unwrap();
        }

        let context = crate::handlers::records::tutor_context_for(&env, "llp-1").unwrap();
        let due = context["due_error_rechecks"].as_array().unwrap();
        assert_eq!(
            due.len(),
            1,
            "Tutor context must expose only the next actionable stage, not all overdue stages"
        );
        assert_eq!(due[0]["recheck_id"], "rc-1");

        let mut input = result();
        input.error_occurrences.clear();
        input
            .error_rechecks
            .push(tutor_contract::ErrorRecheckResult {
                error_target_id: "et-known".to_owned(),
                recheck_id: "rc-1".to_owned(),
                passed: true,
                assistance_level: 0,
                context: "pharmacy:recheck".to_owned(),
                repair_applied: false,
                repair_action: None,
            });
        complete_lesson(&env, &input).unwrap();

        let db = env.connection().unwrap();
        let recheck: (Option<u8>, bool) = db
            .query_row(
                "SELECT passed,completed_at IS NOT NULL FROM error_rechecks WHERE id='rc-1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        let target: (String, u8) = db
            .query_row(
                "SELECT status,method_change_required FROM error_targets WHERE id='et-known'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(recheck, (Some(1), true));
        assert_eq!(target, ("recheck".to_owned(), 1));
        drop(db);
        cleanup(path);
    }

    #[test]
    fn suspending_learning_item_expires_open_practice_recommendation() {
        let (path, env) = create_env("suspend-item-recommendation");
        env.connection()
            .unwrap()
            .execute(
                "INSERT INTO recommendations
                 (id,learner_language_profile_id,subject_type,subject_id,action,
                  detail_json,status,created_at)
                 VALUES('rec-practice','llp-1','learning_item','li-known',
                        'practice_in_lesson','{}','open','2026-10-16T00:00:00Z')",
                [],
            )
            .unwrap();

        let mut input = result();
        input.client_operation_id = "op-suspend-item-recommendation".to_owned();
        input.mastery_changes[0].new_status = learning_core::LearningItemStatus::Suspended;
        complete_lesson(&env, &input).unwrap();

        let db = env.connection().unwrap();
        let recommendation: (String, bool) = db
            .query_row(
                "SELECT status,resolved_at IS NOT NULL
                 FROM recommendations WHERE id='rec-practice'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(
            recommendation,
            ("expired".to_owned(), true),
            "suspended learning item must not leave stale practice pressure in planner"
        );
        drop(db);
        cleanup(path);
    }

    #[test]
    fn lesson_result_resolves_only_explicit_open_recommendations_for_profile() {
        let (path, env) = create_env("recommendation-resolution");
        {
            let db = env.connection().unwrap();
            db.execute_batch(
                "INSERT INTO recommendations
                    (id,learner_language_profile_id,subject_type,subject_id,action,detail_json,status,created_at)
                 VALUES
                    ('rec-accept','llp-1','learning_item','li-known','practice_in_lesson','{}','open','2026-10-16T00:00:00Z'),
                    ('rec-dismiss','llp-1','card',NULL,'redesign_card','{}','open','2026-10-16T00:00:00Z'),
                    ('rec-open','llp-1','learning_item','li-known','create_listening_card','{}','open','2026-10-16T00:00:00Z');",
            )
            .unwrap();
        }

        let mut input = result();
        input.error_occurrences.clear();
        input.recommendation_resolutions = vec![
            tutor_contract::RecommendationResolution {
                recommendation_id: "rec-accept".to_owned(),
                status: tutor_contract::RecommendationResolutionStatus::Accepted,
                repair_action: None,
            },
            tutor_contract::RecommendationResolution {
                recommendation_id: "rec-dismiss".to_owned(),
                status: tutor_contract::RecommendationResolutionStatus::Dismissed,
                repair_action: None,
            },
        ];
        complete_lesson(&env, &input).unwrap();

        let db = env.connection().unwrap();
        let states = db
            .prepare(
                "SELECT id,status,resolved_at IS NOT NULL
                 FROM recommendations
                 WHERE id IN ('rec-accept','rec-dismiss','rec-open')
                 ORDER BY id",
            )
            .unwrap()
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, bool>(2)?,
                ))
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        assert_eq!(
            states,
            vec![
                ("rec-accept".to_owned(), "accepted".to_owned(), true),
                ("rec-dismiss".to_owned(), "dismissed".to_owned(), true),
                ("rec-open".to_owned(), "open".to_owned(), false),
            ]
        );
        drop(db);
        cleanup(path);
    }

    #[test]
    fn accepted_practice_recommendation_requires_same_item_activity() {
        let (path, env) = create_env("recommendation-practice-proof");
        env.connection()
            .unwrap()
            .execute(
                "INSERT INTO recommendations
                 (id,learner_language_profile_id,subject_type,subject_id,action,
                  detail_json,status,created_at)
                 VALUES('rec-practice-proof','llp-1','learning_item','li-known',
                        'practice_in_lesson','{}','open','2026-10-16T00:00:00Z')",
                [],
            )
            .unwrap();

        let mut input = result();
        input.client_operation_id = "op-recommendation-practice-proof".to_owned();
        input.evidence.clear();
        input.listening_attempts.clear();
        input.interventions.clear();
        input.mastery_changes.clear();
        input.error_occurrences.clear();
        input.recommendation_resolutions = vec![tutor_contract::RecommendationResolution {
            recommendation_id: "rec-practice-proof".to_owned(),
            status: tutor_contract::RecommendationResolutionStatus::Accepted,
            repair_action: None,
        }];

        let error = complete_lesson(&env, &input).unwrap_err();
        assert!(
            error.to_string().contains("practice_in_lesson"),
            "accepted practice recommendation must be backed by same-item lesson activity: {error}"
        );

        let status: String = env
            .connection()
            .unwrap()
            .query_row(
                "SELECT status FROM recommendations WHERE id='rec-practice-proof'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            status, "open",
            "rejected acceptance must roll back atomically"
        );
        cleanup(path);
    }

    #[test]
    fn accepted_redesign_recommendation_requires_applied_same_card_mutation() {
        let (path, env) = create_env("recommendation-redesign-proof");
        seed_cards(&env);
        env.connection()
            .unwrap()
            .execute(
                "INSERT INTO recommendations
                 (id,learner_language_profile_id,subject_type,subject_id,action,
                  detail_json,status,created_at)
                 VALUES('rec-redesign-proof','llp-1','card','card-update',
                        'redesign_card','{}','open','2026-10-16T00:00:00Z')",
                [],
            )
            .unwrap();

        let mut missing = result();
        missing.client_operation_id = "op-redesign-proof-missing".to_owned();
        missing.error_occurrences.clear();
        missing.recommendation_resolutions = vec![tutor_contract::RecommendationResolution {
            recommendation_id: "rec-redesign-proof".to_owned(),
            status: tutor_contract::RecommendationResolutionStatus::Accepted,
            repair_action: None,
        }];
        let error = complete_lesson(&env, &missing).unwrap_err();
        assert!(
            error.to_string().contains("redesign_card"),
            "accepted redesign recommendation must be backed by an applied same-card mutation: {error}"
        );

        let mut applied = card_result();
        applied.client_operation_id = "op-redesign-proof-applied".to_owned();
        applied.error_occurrences.clear();
        applied.recommendation_resolutions = vec![tutor_contract::RecommendationResolution {
            recommendation_id: "rec-redesign-proof".to_owned(),
            status: tutor_contract::RecommendationResolutionStatus::Accepted,
            repair_action: None,
        }];
        complete_lesson(&env, &applied).unwrap();

        let status: String = env
            .connection()
            .unwrap()
            .query_row(
                "SELECT status FROM recommendations WHERE id='rec-redesign-proof'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(status, "accepted");
        cleanup(path);
    }

    #[test]
    fn recommendation_resolution_rejects_foreign_or_already_resolved_rows_atomically() {
        let (path, env) = create_env("recommendation-resolution-reject");
        {
            let db = env.connection().unwrap();
            db.execute_batch(
                "INSERT INTO learner_language_profiles
                    (id,learner_id,source_language,target_locale,current_lesson_number,created_at,updated_at)
                 VALUES('llp-foreign','learner-1','ru','fr-FR',0,
                        '2026-10-16T00:00:00Z','2026-10-16T00:00:00Z');
                 INSERT INTO recommendations
                    (id,learner_language_profile_id,subject_type,subject_id,action,detail_json,status,created_at)
                 VALUES
                    ('rec-foreign','llp-foreign','language_profile',NULL,'practice_in_lesson','{}','open',
                     '2026-10-16T00:00:00Z');",
            )
            .unwrap();
        }

        let before = snapshot(&env);
        let mut input = result();
        input.error_occurrences.clear();
        input.recommendation_resolutions = vec![tutor_contract::RecommendationResolution {
            recommendation_id: "rec-foreign".to_owned(),
            status: tutor_contract::RecommendationResolutionStatus::Accepted,
            repair_action: None,
        }];
        assert!(complete_lesson(&env, &input).is_err());
        assert_eq!(snapshot(&env), before);

        let db = env.connection().unwrap();
        let status: String = db
            .query_row(
                "SELECT status FROM recommendations WHERE id='rec-foreign'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(status, "open");
        drop(db);
        cleanup(path);
    }

    #[test]
    fn omitted_error_target_id_reuses_exact_unresolved_signature() {
        let (path, env) = create_env("existing-error-signature-reuse");
        let mut input = result();
        let mut recurring = input.error_occurrences[0].clone();
        recurring.error_target_id = None;
        input.error_occurrences = vec![recurring];

        complete_lesson(&env, &input).unwrap();

        let db = env.connection().unwrap();
        let matching_targets: u32 = db
            .query_row(
                "SELECT COUNT(*) FROM error_targets
                 WHERE learner_language_profile_id='llp-1'
                   AND status != 'resolved'
                   AND core_category='grammar'
                   AND error_code IS NULL
                   AND TRIM(original_production)='estoy médico'
                   AND TRIM(corrected_production)='soy médico'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let count: u32 = db
            .query_row(
                "SELECT occurrence_count FROM error_targets WHERE id='et-known'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(
            matching_targets, 1,
            "an omitted id must not split one exact unresolved error signature into duplicate targets"
        );
        assert_eq!(count, 3);
        drop(db);
        cleanup(path);
    }

    #[test]
    fn repeated_new_error_in_one_lesson_reuses_one_target() {
        let (path, env) = create_env("new-error-dedup");
        let mut input = result();
        let repeated = input.error_occurrences[1].clone();
        input.error_occurrences = vec![repeated.clone(), repeated];
        complete_lesson(&env, &input).unwrap();

        let db = env.connection().unwrap();
        let target_count: u32 = db
            .query_row(
                "SELECT COUNT(*) FROM error_targets
                 WHERE learner_language_profile_id='llp-1'
                   AND core_category='vocabulary'
                   AND original_production='x'
                   AND corrected_production='y'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let occurrence_count: u32 = db
            .query_row(
                "SELECT COUNT(*)
                 FROM error_occurrences o
                 JOIN error_targets t ON t.id=o.error_target_id
                 WHERE t.learner_language_profile_id='llp-1'
                   AND t.core_category='vocabulary'
                   AND t.original_production='x'
                   AND t.corrected_production='y'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let tracked_count: u32 = db
            .query_row(
                "SELECT occurrence_count FROM error_targets
                 WHERE learner_language_profile_id='llp-1'
                   AND core_category='vocabulary'
                   AND original_production='x'
                   AND corrected_production='y'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let recheck_count: u32 = db
            .query_row(
                "SELECT COUNT(*)
                 FROM error_rechecks r
                 JOIN error_targets t ON t.id=r.error_target_id
                 WHERE t.learner_language_profile_id='llp-1'
                   AND t.core_category='vocabulary'
                   AND t.original_production='x'
                   AND t.corrected_production='y'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(
            target_count, 1,
            "one new error signature must create one target"
        );
        assert_eq!(
            occurrence_count, 2,
            "raw within-lesson observations may be retained"
        );
        assert_eq!(
            tracked_count, 1,
            "escalation count advances once per lesson episode"
        );
        assert_eq!(
            recheck_count, 0,
            "a first lesson-episode sighting must not create spaced recheck pressure"
        );
        drop(db);
        cleanup(path);
    }

    #[test]
    fn recurrence_threshold_requires_repair_before_first_recheck_series() {
        let (path, env) = create_env("existing-error-dedup");
        let mut input = result();
        let repeated = input.error_occurrences[0].clone();
        input.error_occurrences = vec![repeated.clone(), repeated];
        complete_lesson(&env, &input).unwrap();

        let db = env.connection().unwrap();
        let tracked_count: u32 = db
            .query_row(
                "SELECT occurrence_count FROM error_targets WHERE id='et-known'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let occurrence_count: u32 = db
            .query_row(
                "SELECT COUNT(*) FROM error_occurrences WHERE error_target_id='et-known'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let recheck_count: u32 = db
            .query_row(
                "SELECT COUNT(*) FROM error_rechecks WHERE error_target_id='et-known'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(
            tracked_count, 3,
            "recurrence advances once per lesson episode"
        );
        assert_eq!(
            occurrence_count, 2,
            "raw repeated sightings remain observable"
        );
        assert_eq!(
            recheck_count, 0,
            "recurrence threshold opens repair pressure; spaced rechecks start only after that repair is actually accepted"
        );
        let status: String = db
            .query_row(
                "SELECT status FROM error_targets WHERE id='et-known'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(status, "active");
        drop(db);
        cleanup(path);
    }

    #[test]
    fn accepted_error_repair_requires_named_repair_action_before_spacing_starts() {
        let (path, env) = create_env("accepted-error-repair-requires-action");
        {
            let db = env.connection().unwrap();
            db.execute(
                "UPDATE error_targets
                 SET occurrence_count=3,method_change_required=0,status='active'
                 WHERE id='et-known'",
                [],
            )
            .unwrap();
            db.execute(
                "INSERT INTO recommendations
                 (id,learner_language_profile_id,subject_type,subject_id,action,
                  detail_json,status,created_at)
                 VALUES('rec-repair','llp-1','error_target','et-known',
                        'activate_mini_target','{}','open','2026-10-16T00:00:00Z')",
                [],
            )
            .unwrap();
        }

        let mut input = result();
        input.client_operation_id = "op-repair-without-action".to_owned();
        input.error_occurrences.clear();
        input.error_rechecks.clear();
        input.new_learning_items.clear();
        input
            .evidence
            .retain(|entry| entry.learning_item_id.is_some());
        input.interventions.clear();
        input.listening_attempts.clear();
        input.mastery_changes.clear();
        input.card_mutations.clear();
        input.recommendation_resolutions = vec![tutor_contract::RecommendationResolution {
            recommendation_id: "rec-repair".to_owned(),
            status: tutor_contract::RecommendationResolutionStatus::Accepted,
            repair_action: None,
        }];

        let before = snapshot(&env);
        let error = complete_lesson(&env, &input).unwrap_err();
        assert!(
            error.to_string().contains("repair_action"),
            "accepted error repair must name the method actually applied: {error}"
        );
        assert_eq!(snapshot(&env), before);
        cleanup(path);
    }

    #[test]
    fn accepted_error_repair_recommendation_starts_first_spaced_recheck_series() {
        let (path, env) = create_env("accepted-error-repair-starts-series");

        let mut first = result();
        first.client_operation_id = "op-error-threshold".to_owned();
        first.error_occurrences.truncate(1);
        complete_lesson(&env, &first).unwrap();

        let db = env.connection().unwrap();
        let before_repair: (String, u32, String) = db
            .query_row(
                "SELECT t.status,
                        (SELECT COUNT(*) FROM error_rechecks r
                         WHERE r.error_target_id=t.id AND r.completed_at IS NULL),
                        rec.id
                 FROM error_targets t
                 JOIN recommendations rec
                   ON rec.subject_type='error_target'
                  AND rec.subject_id=t.id
                  AND rec.action='activate_mini_target'
                  AND rec.status='open'
                 WHERE t.id='et-known'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(
            (before_repair.0.as_str(), before_repair.1),
            ("active", 0),
            "threshold detection must wait for actual repair before scheduling retention"
        );
        let recommendation_id = before_repair.2;
        drop(db);

        let mut repair = result();
        repair.client_operation_id = "op-accepted-error-repair".to_owned();
        repair.error_occurrences.clear();
        repair.error_rechecks.clear();
        repair.new_learning_items.clear();
        repair
            .evidence
            .retain(|entry| entry.learning_item_id.is_some());
        repair.interventions.clear();
        repair.listening_attempts.clear();
        repair.mastery_changes.clear();
        repair.card_mutations.clear();
        repair.recommendation_resolutions = vec![tutor_contract::RecommendationResolution {
            recommendation_id,
            status: tutor_contract::RecommendationResolutionStatus::Accepted,
            repair_action: Some("contrast".to_owned()),
        }];
        complete_lesson(&env, &repair).unwrap();

        let db = env.connection().unwrap();
        let state: (String, u32) = db
            .query_row(
                "SELECT t.status,COUNT(r.id)
                 FROM error_targets t
                 LEFT JOIN error_rechecks r
                   ON r.error_target_id=t.id AND r.completed_at IS NULL
                 WHERE t.id='et-known'
                 GROUP BY t.id,t.status",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(
            state,
            ("recheck".to_owned(), 3),
            "spaced retention starts only after the repair recommendation was actually applied"
        );
        let persisted_repair_action: String = db
            .query_row(
                "SELECT json_extract(detail_json,'$.repair_action')
                 FROM recommendations
                 WHERE id=(
                     SELECT id FROM recommendations
                     WHERE subject_type='error_target'
                       AND subject_id='et-known'
                       AND action='activate_mini_target'
                     ORDER BY created_at DESC LIMIT 1
                 )",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(persisted_repair_action, "contrast");
        db.execute(
            "UPDATE error_rechecks
             SET due_at='2020-01-01T00:00:00Z'
             WHERE error_target_id='et-known' AND completed_at IS NULL AND offset_days=1",
            [],
        )
        .unwrap();
        drop(db);

        let context = crate::handlers::records::tutor_context_for(&env, "llp-1").unwrap();
        let due = context["due_error_rechecks"].as_array().unwrap();
        assert!(due.iter().any(|row| {
            row["error_target_id"] == "et-known" && row["last_repair_action"] == "contrast"
        }));

        cleanup(path);
    }

    #[test]
    fn method_change_escalation_supersedes_open_mini_target_recommendation() {
        let (path, env) = create_env("method-change-supersedes-mini-target");
        {
            let db = env.connection().unwrap();
            db.execute(
                "UPDATE error_targets
                 SET occurrence_count=4,method_change_required=0,status='active'
                 WHERE id='et-known'",
                [],
            )
            .unwrap();
            db.execute(
                "INSERT INTO recommendations
                 (id,learner_language_profile_id,subject_type,subject_id,action,
                  detail_json,status,created_at)
                 VALUES('rec-mini','llp-1','error_target','et-known',
                        'activate_mini_target','{}','open','2026-10-16T00:00:00Z')",
                [],
            )
            .unwrap();
        }

        let mut input = result();
        input.client_operation_id = "op-method-change-supersedes-mini".to_owned();
        input.error_occurrences.truncate(1);
        complete_lesson(&env, &input).unwrap();

        let db = env.connection().unwrap();
        let rows = db
            .prepare(
                "SELECT action,status FROM recommendations
                 WHERE subject_type='error_target' AND subject_id='et-known'
                 ORDER BY action",
            )
            .unwrap()
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        assert_eq!(
            rows,
            vec![
                ("activate_mini_target".to_owned(), "expired".to_owned()),
                ("require_method_change".to_owned(), "open".to_owned()),
            ],
            "method-change escalation must replace weaker repair pressure rather than leave two competing repairs"
        );
        drop(db);
        cleanup(path);
    }

    #[test]
    fn sticky_method_change_cannot_start_rechecks_via_weaker_legacy_repair() {
        let (path, env) = create_env("sticky-method-change-repair-gate");
        {
            let db = env.connection().unwrap();
            db.execute(
                "UPDATE error_targets
                 SET occurrence_count=5,method_change_required=1,status='active'
                 WHERE id='et-known'",
                [],
            )
            .unwrap();
            db.execute_batch(
                "INSERT INTO recommendations
                 (id,learner_language_profile_id,subject_type,subject_id,action,
                  detail_json,status,created_at)
                 VALUES
                 ('rec-mini-legacy','llp-1','error_target','et-known',
                  'activate_mini_target','{}','open','2026-10-16T00:00:00Z'),
                 ('rec-method','llp-1','error_target','et-known',
                  'require_method_change','{}','open','2026-10-16T00:00:01Z');",
            )
            .unwrap();
        }

        let mut weak = result();
        weak.client_operation_id = "op-weak-repair".to_owned();
        weak.error_occurrences.clear();
        weak.error_rechecks.clear();
        weak.new_learning_items.clear();
        weak.evidence
            .retain(|entry| entry.learning_item_id.is_some());
        weak.interventions.clear();
        weak.listening_attempts.clear();
        weak.mastery_changes.clear();
        weak.card_mutations.clear();
        weak.recommendation_resolutions = vec![tutor_contract::RecommendationResolution {
            recommendation_id: "rec-mini-legacy".to_owned(),
            status: tutor_contract::RecommendationResolutionStatus::Accepted,
            repair_action: Some("contrast".to_owned()),
        }];
        complete_lesson(&env, &weak).unwrap();

        let db = env.connection().unwrap();
        let after_weak: (String, u32) = db
            .query_row(
                "SELECT t.status,
                        (SELECT COUNT(*) FROM error_rechecks r
                         WHERE r.error_target_id=t.id AND r.completed_at IS NULL)
                 FROM error_targets t WHERE t.id='et-known'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(
            after_weak,
            ("active".to_owned(), 0),
            "sticky method change must not be bypassed by accepting an older weaker repair"
        );
        drop(db);

        let mut repeated = weak.clone();
        repeated.client_operation_id = "op-repeated-method-change-repair".to_owned();
        repeated.recommendation_resolutions[0].recommendation_id = "rec-method".to_owned();
        repeated.recommendation_resolutions[0].repair_action = Some("contrast".to_owned());
        let error = complete_lesson(&env, &repeated).unwrap_err();
        assert!(
            error.to_string().contains("repair_action") || error.to_string().contains("method"),
            "require_method_change must reject the same named method that already failed: {error}"
        );

        let db = env.connection().unwrap();
        let unchanged: (String, u32, String) = db
            .query_row(
                "SELECT t.status,
                        (SELECT COUNT(*) FROM error_rechecks r
                         WHERE r.error_target_id=t.id AND r.completed_at IS NULL),
                        (SELECT status FROM recommendations WHERE id='rec-method')
                 FROM error_targets t WHERE t.id='et-known'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(unchanged, ("active".to_owned(), 0, "open".to_owned()));
        drop(db);

        let mut strong = weak;
        strong.client_operation_id = "op-method-change-repair".to_owned();
        strong.recommendation_resolutions[0].recommendation_id = "rec-method".to_owned();
        strong.recommendation_resolutions[0].repair_action = Some("semantic_reframe".to_owned());
        complete_lesson(&env, &strong).unwrap();

        let db = env.connection().unwrap();
        let after_strong: (String, u32) = db
            .query_row(
                "SELECT t.status,
                        (SELECT COUNT(*) FROM error_rechecks r
                         WHERE r.error_target_id=t.id AND r.completed_at IS NULL)
                 FROM error_targets t WHERE t.id='et-known'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(after_strong, ("recheck".to_owned(), 3));
        drop(db);
        cleanup(path);
    }

    #[test]
    fn method_change_requirement_is_sticky_until_target_resolution() {
        let (path, env) = create_env("sticky-method-change");
        {
            let db = env.connection().unwrap();
            db.execute(
                "UPDATE error_targets
                 SET occurrence_count=4,method_change_required=1,status='active'
                 WHERE id='et-known'",
                [],
            )
            .unwrap();
        }

        let mut input = result();
        input.error_occurrences.truncate(1);
        input.error_occurrences[0].communicative_impact = learning_core::CommunicativeImpact::Low;
        complete_lesson(&env, &input).unwrap();

        let db = env.connection().unwrap();
        let state: (u32, u8) = db
            .query_row(
                "SELECT occurrence_count,method_change_required
                 FROM error_targets WHERE id='et-known'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(state.0, 5);
        assert_eq!(
            state.1, 1,
            "once method change is required it must remain sticky until target resolution"
        );
        drop(db);
        cleanup(path);
    }

    #[test]
    fn recurring_error_escalation_preserves_strongest_historical_impact() {
        let (path, env) = create_env("historical-high-impact");
        {
            let db = env.connection().unwrap();
            db.execute(
                "UPDATE error_targets
                 SET occurrence_count=1,status='active',method_change_required=0
                 WHERE id='et-known'",
                [],
            )
            .unwrap();
            db.execute(
                "INSERT INTO error_occurrences
                    (id,error_target_id,lesson_id,context,communicative_impact,recorded_at)
                 VALUES
                    ('eo-high-old','et-known',NULL,'historical','high',
                     '2026-10-15T10:00:00Z')",
                [],
            )
            .unwrap();
        }

        let mut input = result();
        let mut recurrence = input.error_occurrences[0].clone();
        recurrence.communicative_impact = learning_core::CommunicativeImpact::Low;
        input.error_occurrences = vec![recurrence];
        complete_lesson(&env, &input).unwrap();

        let db = env.connection().unwrap();
        let status: String = db
            .query_row(
                "SELECT status FROM error_targets WHERE id='et-known'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let recheck_count: u32 = db
            .query_row(
                "SELECT COUNT(*) FROM error_rechecks WHERE error_target_id='et-known'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            status, "active",
            "historical high-impact recurrence must escalate into repair before any spaced retention series"
        );
        assert_eq!(recheck_count, 0);
        drop(db);
        cleanup(path);
    }

    #[test]
    fn recurring_error_escalation_uses_strongest_impact_in_episode() {
        for (label, first, second) in [
            (
                "low-high",
                learning_core::CommunicativeImpact::Low,
                learning_core::CommunicativeImpact::High,
            ),
            (
                "high-low",
                learning_core::CommunicativeImpact::High,
                learning_core::CommunicativeImpact::Low,
            ),
        ] {
            let (path, env) = create_env(label);
            let mut input = result();
            let mut left = input.error_occurrences[0].clone();
            let mut right = left.clone();
            left.communicative_impact = first;
            right.communicative_impact = second;
            input.error_occurrences = vec![left, right];
            complete_lesson(&env, &input).unwrap();

            let db = env.connection().unwrap();
            let actions = db
                .prepare(
                    "SELECT action FROM recommendations
                     WHERE subject_type='error_target' AND subject_id='et-known'
                     ORDER BY action",
                )
                .unwrap()
                .query_map([], |row| row.get::<_, String>(0))
                .unwrap()
                .collect::<rusqlite::Result<Vec<_>>>()
                .unwrap();
            assert_eq!(
                actions,
                vec!["activate_mini_target".to_owned()],
                "episode escalation must use the strongest observed impact regardless of row order"
            );
            drop(db);
            cleanup(path);
        }
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
    fn temporally_massed_historical_lessons_cannot_fake_distributed_mastery() {
        let (path, env) = create_env("massed-mastery-history");
        {
            let db = env.connection().unwrap();
            db.execute_batch(
                "INSERT INTO lessons
                    (id, learner_language_profile_id, lesson_number, status, scenario,
                     planned_goal, source, started_at, completed_at, created_at, updated_at)
                 VALUES
                    ('les-massed-1','llp-1',1,'completed','old','old','import',
                     '2026-11-05T10:00:00Z','2026-11-05T10:20:00Z',
                     '2026-11-05T10:00:00Z','2026-11-05T10:20:00Z'),
                    ('les-massed-2','llp-1',2,'completed','old','old','import',
                     '2026-11-05T11:00:00Z','2026-11-05T11:20:00Z',
                     '2026-11-05T11:00:00Z','2026-11-05T11:20:00Z');
                 INSERT INTO learning_item_evidence
                    (id, learning_item_id, lesson_id, kind, success, metadata_json, recorded_at)
                 VALUES
                    ('ev-massed-1','li-known','les-massed-1','transfer',1,
                     json_object('assistance_level',0,'probe_phase','transfer',
                                 'context_family','transport.ask_stop'),
                     '2026-11-05T10:10:00Z'),
                    ('ev-massed-2','li-known','les-massed-2','transfer',1,
                     json_object('assistance_level',0,'probe_phase','transfer',
                                 'context_family','commerce.request_item'),
                     '2026-11-05T11:10:00Z');",
            )
            .unwrap();
        }

        let mut input = result();
        input.client_operation_id = "op-massed-mastery-history".to_owned();
        input.mastery_changes[0].new_status = learning_core::LearningItemStatus::Mastered;
        input.evidence.push(tutor_contract::EvidenceEntry {
            learning_item_id: Some("li-known".to_owned()),
            client_item_key: None,
            context: Some("fresh".to_owned()),
            context_family: Some("service.ask_help".to_owned()),
            kind: learning_core::EvidenceKind::Transfer,
            success: true,
            assistance_level: Some(0),
            response_millis: None,
            probe_phase: Some(tutor_contract::ProbePhase::Transfer),
            pronunciation_intelligible: None,
            pronunciation_repeat_required: None,
            detail: None,
        });

        let response = complete_lesson(&env, &input).unwrap();
        assert_eq!(
            response["warnings"].as_array().unwrap().len(),
            1,
            "distinct lesson IDs inside the anti-massing gap must not manufacture distributed transfer mastery"
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
        assert_eq!(
            state.rechecks.len(),
            0,
            "recurrence threshold must open repair pressure, not schedule retention before repair"
        );
        assert!(
            state.recommendations.iter().any(|row| {
                row.0 == "error_target" && row.1 == "et-known" && row.2 == "activate_mini_target"
            }),
            "the recurring existing target should open the medium-impact repair recommendation"
        );
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
    fn started_lesson_cannot_silently_skip_exposed_due_recheck() {
        let (path, env) = create_env("started-recheck-obligation");
        {
            let db = env.connection().unwrap();
            db.execute(
                "UPDATE error_targets
                 SET occurrence_count=5,method_change_required=1,status='recheck'
                 WHERE id='et-known'",
                [],
            )
            .unwrap();
            db.execute_batch(
                "INSERT INTO error_rechecks
                    (id,error_target_id,cycle,offset_days,due_at)
                 VALUES
                    ('rc-required-1','et-known',0,1,'2020-01-01T00:00:00Z'),
                    ('rc-required-3','et-known',0,3,'2020-01-03T00:00:00Z'),
                    ('rc-required-7','et-known',0,7,'2020-01-07T00:00:00Z');",
            )
            .unwrap();
        }

        let started = super::super::start_lesson::start_lesson(
            &env,
            "llp-1",
            "pharmacy",
            "complete the planned pharmacy task",
            "mcp",
        )
        .unwrap();
        let context = crate::handlers::records::tutor_context_for(&env, "llp-1").unwrap();
        assert_eq!(context["due_error_rechecks"].as_array().unwrap().len(), 1);
        assert_eq!(
            context["due_error_rechecks"][0]["recheck_id"],
            "rc-required-1"
        );

        let mut input = result();
        input.client_operation_id = "op-skip-required-recheck".to_owned();
        input.lesson.lesson_id = Some(started["id"].as_str().unwrap().to_owned());
        input.error_occurrences.clear();
        input.error_rechecks.clear();

        let error = complete_lesson(&env, &input).unwrap_err();
        assert!(
            matches!(
                error,
                crate::Error::Operation {
                    code: "required_recheck_missing",
                    ..
                }
            ),
            "live lesson completion must fail closed when an exposed maintenance recheck is omitted"
        );
        cleanup(path);
    }

    #[test]
    fn started_lessons_complete_once_and_advance_numbers() {
        let (path, env) = create_env("started-sequence");
        for number in 8..=9 {
            let started = super::super::start_lesson::start_lesson(
                &env,
                "llp-1",
                "pharmacy",
                "complete the planned pharmacy task",
                "mcp",
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
        {
            let db = env.connection().unwrap();
            db.execute(
                "INSERT INTO learner_profile(id,created_at,updated_at)
                 VALUES('learner-foreign','2026-10-16T00:00:00Z','2026-10-16T00:00:00Z')",
                [],
            )
            .unwrap();
            db.execute(
                "INSERT INTO learner_language_profiles
                 (id,learner_id,source_language,target_locale,created_at,updated_at)
                 VALUES('llp-foreign','learner-foreign','ru','es-ES',
                        '2026-10-16T00:00:00Z','2026-10-16T00:00:00Z')",
                [],
            )
            .unwrap();
            db.execute(
                "INSERT INTO error_targets
                 (id,learner_language_profile_id,core_category,original_production,
                  corrected_production,occurrence_count,method_change_required,status,
                  created_at,updated_at)
                 VALUES('et-foreign','llp-foreign','grammar','wrong','right',1,0,'active',
                        '2026-10-16T00:00:00Z','2026-10-16T00:00:00Z')",
                [],
            )
            .unwrap();
        }
        let mut input = result();
        input.error_occurrences[0].error_target_id = Some("et-foreign".to_owned());
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
    fn started_lesson_rejects_persisted_new_items_above_planner_cap() {
        let (path, env) = create_env("planner-new-item-cap");
        seed_started_lesson(&env);
        env.connection()
            .unwrap()
            .execute(
                r#"UPDATE planner_decisions
                 SET plan_json='{"max_new_items":0,"primary_objective":"change_method"}'
                 WHERE id='pd-active'"#,
                [],
            )
            .unwrap();

        let before_created: u32 = env
            .connection()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM learning_items
                 WHERE learner_language_profile_id='llp-1'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        let mut input = result();
        input.client_operation_id = "op-planner-new-item-cap".to_owned();
        input.lesson.lesson_id = Some("lesson-active".to_owned());

        let error = complete_lesson(&env, &input).unwrap_err();
        assert!(
            error.to_string().contains("max_new_items"),
            "planner cap rejection should identify the violated new-item constraint: {error}"
        );

        let db = env.connection().unwrap();
        let status: String = db
            .query_row(
                "SELECT status FROM lessons WHERE id='lesson-active'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let created: u32 = db
            .query_row(
                "SELECT COUNT(*) FROM learning_items
                 WHERE learner_language_profile_id='llp-1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(status, "active");
        assert_eq!(
            created, before_created,
            "cap failure must roll back before persisting new items"
        );
        drop(db);
        cleanup(path);
    }

    #[test]
    fn established_bottleneck_repair_slot_rejects_unlinked_new_item() {
        let (path, env) = create_env("planner-repair-slot-unlinked");
        seed_started_lesson(&env);
        env.connection()
            .unwrap()
            .execute(
                r#"UPDATE planner_decisions
                 SET plan_json='{"max_new_items":1,"primary_objective":"improve_communicative_task_success","priorities":[{"action":"improve_communicative_task_success","score":98,"rationale":"established task failure"}]}'
                 WHERE id='pd-active'"#,
                [],
            )
            .unwrap();

        let mut input = result();
        input.client_operation_id = "op-repair-slot-unlinked".to_owned();
        input.lesson.lesson_id = Some("lesson-active".to_owned());
        input.new_learning_items.truncate(1);

        let error = complete_lesson(&env, &input).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("established-bottleneck repair slot"),
            "established-bottleneck new material must be tied to an applied intervention: {error}"
        );
        cleanup(path);
    }

    #[test]
    fn established_bottleneck_repair_slot_accepts_intervention_target() {
        let (path, env) = create_env("planner-repair-slot-linked");
        seed_started_lesson(&env);
        env.connection()
            .unwrap()
            .execute(
                r#"UPDATE planner_decisions
                 SET plan_json='{"max_new_items":1,"primary_objective":"improve_communicative_task_success","priorities":[{"action":"improve_communicative_task_success","score":98,"rationale":"established task failure"}]}'
                 WHERE id='pd-active'"#,
                [],
            )
            .unwrap();

        let mut input = result();
        input.client_operation_id = "op-repair-slot-linked".to_owned();
        input.lesson.lesson_id = Some("lesson-active".to_owned());
        input.new_learning_items.truncate(1);
        input.evidence[1].probe_phase = Some(tutor_contract::ProbePhase::Practice);
        input.interventions[0] = tutor_contract::InterventionEvent {
            action: "teach_missing_material".to_owned(),
            learning_item_id: None,
            client_item_key: Some("new-cough".to_owned()),
            target_capability: learning_core::EvidenceKind::Recognition,
            evidence_index: Some(1),
            listening_attempt_index: None,
        };

        complete_lesson(&env, &input).unwrap();
        let created: u32 = env
            .connection()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM learning_items
                 WHERE learner_language_profile_id='llp-1'
                   AND target_text='¿Tiene algo para la tos?'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(created, 1);
        cleanup(path);
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
    fn one_diagnostic_plus_delayed_probe_keeps_calibration_valid() {
        let (path, env) = create_env("planner-delayed-calibration");
        seed_started_lesson(&env);
        let mut input = result();
        input.client_operation_id = "op-delayed-calibration".to_owned();
        input.lesson.lesson_id = Some("lesson-active".to_owned());
        input.evidence[1].probe_phase = Some(tutor_contract::ProbePhase::DelayedRetrieval);

        assert!(tutor_contract::validate(&input).is_empty());
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
        assert_eq!(outcome["diagnostic_probe_count"], 1);
        assert_eq!(outcome["measurement_eligible_probe_count"], 2);
        assert_eq!(outcome["calibration_valid"], true);
        assert_eq!(outcome["interventions"][0]["calibration_eligible"], true);
        drop(db);
        cleanup(path);
    }

    #[test]
    fn low_coverage_lesson_persists_but_is_not_calibration_valid() {
        let (path, env) = create_env("planner-low-coverage");
        seed_started_lesson(&env);
        let mut input = result();
        input.client_operation_id = "op-low-coverage".to_owned();
        input.lesson.lesson_id = Some("lesson-active".to_owned());
        input.evidence.truncate(1);
        input.listening_attempts.clear();
        input.interventions.clear();
        input.mastery_changes.clear();

        assert!(
            tutor_contract::validate(&input).is_empty(),
            "lesson persistence must not require calibration-grade measurement coverage"
        );
        let response = complete_lesson(&env, &input).unwrap();
        assert_eq!(response["diagnostic_probe_count"], 1);
        assert_eq!(response["measurement_eligible_probe_count"], 1);
        assert_eq!(response["calibration_valid"], false);

        let db = env.connection().unwrap();
        let outcome: String = db
            .query_row(
                "SELECT outcome_json FROM planner_decisions WHERE id='pd-active'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let outcome: serde_json::Value = serde_json::from_str(&outcome).unwrap();
        assert_eq!(outcome["diagnostic_probe_count"], 1);
        assert_eq!(outcome["measurement_eligible_probe_count"], 1);
        assert_eq!(outcome["calibration_valid"], false);
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
    fn repeated_same_transfer_family_is_not_calibration_eligible() {
        let mut input = result();
        input.interventions[0] = tutor_contract::InterventionEvent {
            action: "transfer_practice".to_owned(),
            learning_item_id: Some("li-known".to_owned()),
            client_item_key: None,
            target_capability: learning_core::EvidenceKind::Transfer,
            evidence_index: Some(1),
            listening_attempt_index: None,
        };
        input.evidence = vec![
            tutor_contract::EvidenceEntry {
                learning_item_id: Some("li-known".to_owned()),
                client_item_key: None,
                context: Some("service:baseline".to_owned()),
                context_family: Some("service.ask_help".to_owned()),
                kind: learning_core::EvidenceKind::Transfer,
                success: false,
                assistance_level: Some(0),
                response_millis: None,
                probe_phase: Some(tutor_contract::ProbePhase::Diagnostic),
                pronunciation_intelligible: None,
                pronunciation_repeat_required: None,
                detail: None,
            },
            tutor_contract::EvidenceEntry {
                learning_item_id: Some("li-known".to_owned()),
                client_item_key: None,
                context: Some("service:repeat".to_owned()),
                context_family: Some("service.ask_help".to_owned()),
                kind: learning_core::EvidenceKind::Transfer,
                success: true,
                assistance_level: Some(0),
                response_millis: None,
                probe_phase: Some(tutor_contract::ProbePhase::Practice),
                pronunciation_intelligible: None,
                pronunciation_repeat_required: None,
                detail: None,
            },
        ];

        assert!(
            !intervention_calibration_eligible(&input, &input.interventions[0]),
            "repeating the same semantic family is practice, not evidence that a transfer intervention generalized"
        );
        input.evidence[1].context_family = Some("transport.ask_stop".to_owned());
        assert!(
            intervention_calibration_eligible(&input, &input.interventions[0]),
            "an explicitly different semantic family may support transfer-intervention calibration"
        );
    }

    #[test]
    fn repeated_same_listening_variant_is_not_calibration_eligible() {
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
                assistance_level: Some(0),
                response_millis: None,
                probe_phase: Some(tutor_contract::ProbePhase::Diagnostic),
                source_reference: Some("audio-a".to_owned()),
                source_variant_id: Some("variant-a".to_owned()),
                talker_key: Some("voice-a".to_owned()),
                context_tag: Some("pharmacy".to_owned()),
                transcript: Some("baseline".to_owned()),
            },
            tutor_contract::ListeningAttemptEntry {
                learning_item_id: Some("li-known".to_owned()),
                client_item_key: None,
                success: true,
                assistance_level: Some(0),
                response_millis: None,
                probe_phase: Some(tutor_contract::ProbePhase::Practice),
                source_reference: Some("audio-a".to_owned()),
                source_variant_id: Some("variant-a".to_owned()),
                talker_key: Some("voice-a".to_owned()),
                context_tag: Some("pharmacy".to_owned()),
                transcript: Some("repeated".to_owned()),
            },
        ];

        assert!(
            !intervention_calibration_eligible(&input, &input.interventions[0]),
            "repeating the same listening variant can show practice performance but must not receive intervention-effectiveness credit"
        );
        input.listening_attempts[1].source_variant_id = Some("variant-b".to_owned());
        assert!(
            intervention_calibration_eligible(&input, &input.interventions[0]),
            "an explicitly fresh listening variant may support intervention calibration"
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
    fn max_new_error_batch_persists_without_premature_rechecks() {
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
            rechecks, 0,
            "first sightings are tracked but do not create spaced recheck cycles"
        );

        cleanup(path);
    }
}

use crate::sqlite::to_rfc3339;
// Two transfer half-lives: old context diversity can support mastery only while
// it remains recent enough to be meaningfully revalidated by a fresh transfer.
const MASTERY_HISTORY_WINDOW_SECONDS: i64 = 84 * 86_400;
use crate::{Env, Result};
use rusqlite::{OptionalExtension as _, TransactionBehavior, params};

use super::card_apply::{apply_card_mutations, prepare_card_mutations};
use super::error::{
    apply_error_recheck, insert_error_occurrence, insert_error_target, schedule_error_rechecks,
    update_error_target,
};
use super::lesson_plan::{
    LessonPlan, LessonPlanKind, LessonPlanState, ResolvedItem, add_known_lesson_item,
    add_lesson_error_target, add_lesson_existing_normalized,
    add_lesson_mastery_signal_from_episode, bind_lesson_error_source, lesson_error_id,
    lesson_existing_id, lesson_normalized_new, lesson_profile_id, new_lesson_plan_state,
    plan_lesson_completion, plan_resolved_item_id, plan_reused_item_id, set_lesson_existing,
    set_lesson_normalized_item,
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

fn listening_variant_differs(
    baseline: &tutor_contract::ListeningAttemptEntry,
    outcome: &tutor_contract::ListeningAttemptEntry,
) -> bool {
    let differs = |left: Option<&str>, right: Option<&str>| match (
        left.filter(|value| !value.trim().is_empty()),
        right.filter(|value| !value.trim().is_empty()),
    ) {
        (Some(left), Some(right)) => left != right,
        _ => false,
    };
    differs(
        baseline.source_variant_id.as_deref(),
        outcome.source_variant_id.as_deref(),
    ) || differs(
        baseline.talker_key.as_deref(),
        outcome.talker_key.as_deref(),
    ) || differs(
        baseline.context_tag.as_deref(),
        outcome.context_tag.as_deref(),
    )
}

fn transfer_family_differs(
    baseline: &tutor_contract::EvidenceEntry,
    outcome: &tutor_contract::EvidenceEntry,
) -> bool {
    match (
        baseline
            .context_family
            .as_deref()
            .filter(|value| learning_core::stable_context_family_id(value)),
        outcome
            .context_family
            .as_deref()
            .filter(|value| learning_core::stable_context_family_id(value)),
    ) {
        (Some(baseline), Some(outcome)) => baseline != outcome,
        _ => false,
    }
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
                    && listening_variant_differs(attempt, outcome)
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
            && (intervention.target_capability != learning_core::EvidenceKind::Transfer
                || transfer_family_differs(entry, outcome))
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

fn same_lesson_error_target(
    left: &tutor_contract::ErrorOccurrenceEntry,
    right: &tutor_contract::ErrorOccurrenceEntry,
) -> bool {
    match (
        left.error_target_id.as_deref(),
        right.error_target_id.as_deref(),
    ) {
        (Some(left_id), Some(right_id)) => left_id == right_id,
        (None, None) => {
            left.core_category == right.core_category
                && left.error_code.as_deref().map(str::trim)
                    == right.error_code.as_deref().map(str::trim)
                && left.original_production.trim() == right.original_production.trim()
                && left.corrected_production.trim() == right.corrected_production.trim()
        }
        _ => false,
    }
}

const fn error_impact_rank(impact: learning_core::CommunicativeImpact) -> u8 {
    match impact {
        learning_core::CommunicativeImpact::Low => 0,
        learning_core::CommunicativeImpact::Medium => 1,
        learning_core::CommunicativeImpact::High => 2,
    }
}

fn strongest_lesson_error_impact(
    occurrences: &[tutor_contract::ErrorOccurrenceEntry],
    target: &tutor_contract::ErrorOccurrenceEntry,
) -> learning_core::CommunicativeImpact {
    let mut strongest = target.communicative_impact;
    for occurrence in occurrences {
        if same_lesson_error_target(occurrence, target)
            && error_impact_rank(occurrence.communicative_impact) > error_impact_rank(strongest)
        {
            strongest = occurrence.communicative_impact;
        }
    }
    strongest
}

use super::lesson_write::{
    insert_evidence, insert_learning_item, insert_lesson, insert_listening_attempt,
    update_item_status,
};
use super::recommendation_write::insert_recommendation;

fn require_started_lesson_constraints(
    database: &rusqlite::Connection,
    result: &tutor_contract::LessonResult,
    plan: &LessonPlan,
) -> Result<()> {
    let Some(lesson_id) = result.lesson.lesson_id.as_deref() else {
        return Ok(());
    };
    let started_plan: Option<(String, String, Option<u32>, u8)> = database
        .query_row(
            "SELECT COALESCE(l.started_at,l.created_at),
                    COALESCE(json_extract(pd.plan_json,'$.primary_objective'),''),
                    json_extract(pd.plan_json,'$.max_new_items'),
                    CASE WHEN EXISTS (
                        SELECT 1
                        FROM json_each(pd.plan_json,'$.priorities') p
                        WHERE (
                            json_extract(p.value,'$.action')='improve_communicative_task_success'
                            AND CAST(json_extract(p.value,'$.score') AS INTEGER)>=?3
                        ) OR (
                            json_extract(p.value,'$.action')='probe_prompt_independence'
                            AND CAST(json_extract(p.value,'$.score') AS INTEGER)>=?4
                        )
                    ) THEN 1 ELSE 0 END
             FROM lessons l
             JOIN planner_decisions pd ON pd.lesson_id=l.id
             WHERE l.id=?1
               AND l.learner_language_profile_id=?2
               AND l.status IN ('planned','active')
             LIMIT 1",
            params![
                lesson_id,
                result.language_profile_id,
                adaptation::ESTABLISHED_TASK_FAILURE_PRIORITY,
                adaptation::ESTABLISHED_SPONTANEOUS_WEAKNESS_PRIORITY
            ],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()?;
    let Some((started_at, primary_objective, max_new_items, repair_slot_only)) = started_plan
    else {
        return Ok(());
    };

    let actual_new_items = plan.new_items_len;
    if max_new_items.is_some_and(|limit| actual_new_items > limit as usize) {
        return Err(crate::Error::operation(
            400,
            "max_new_items_exceeded",
            "lesson result exceeds persisted planner max_new_items",
            serde_json::json!({
                "max_new_items": max_new_items,
                "actual_new_items": actual_new_items
            }),
        ));
    }

    if repair_slot_only != 0 && actual_new_items > 0 {
        for plan_index in 0..plan.new_items_len {
            let source_index = dense_slot(plan.new_item_source[plan_index])?;
            let item = &result.new_learning_items[source_index];
            let key = item
                .client_item_key
                .as_deref()
                .filter(|value| !value.trim().is_empty());
            let linked_to_applied_intervention = key.is_some_and(|key| {
                result
                    .interventions
                    .iter()
                    .any(|intervention| intervention.client_item_key.as_deref() == Some(key))
            });
            if !linked_to_applied_intervention {
                return Err(crate::Error::operation(
                    400,
                    "repair_slot_unlinked_new_item",
                    "new material in an established-bottleneck repair slot must be targeted by an applied intervention",
                    serde_json::json!({
                        "client_item_key": key,
                        "primary_objective": primary_objective
                    }),
                ));
            }
        }
    }

    let limit = if primary_objective == "run_error_rechecks" {
        adaptation::DUE_RECHECK_PRIMARY_LIMIT
    } else {
        adaptation::DUE_RECHECK_MAINTENANCE_LIMIT
    };
    let mut statement = database.prepare(
        "SELECT r.id
         FROM error_rechecks r
         JOIN error_targets t ON t.id=r.error_target_id
         WHERE t.learner_language_profile_id=?1
           AND r.completed_at IS NULL
           AND r.due_at<=?2
           AND r.offset_days=(
               SELECT MIN(r2.offset_days)
               FROM error_rechecks r2
               WHERE r2.error_target_id=r.error_target_id
                 AND r2.cycle=r.cycle
                 AND r2.completed_at IS NULL
           )
         ORDER BY r.due_at,r.id
         LIMIT ?3",
    )?;
    let mut rows = statement.query(params![result.language_profile_id, started_at, limit])?;
    while let Some(row) = rows.next()? {
        let required_id: String = row.get(0)?;
        if result
            .error_rechecks
            .iter()
            .all(|outcome| outcome.recheck_id != required_id)
        {
            return Err(crate::Error::operation(
                400,
                "required_recheck_missing",
                "lesson result omitted a due recheck exposed for this started lesson",
                serde_json::json!({ "recheck_id": required_id }),
            ));
        }
    }
    Ok(())
}

fn apply_recommendation_resolutions(
    transaction: &rusqlite::Transaction<'_>,
    result: &tutor_contract::LessonResult,
    card_plan: &super::card_plan::CardPlan,
    now: learning_core::Timestamp,
    at: &str,
) -> Result<String> {
    let mut error_repairs = String::new();
    for resolution in &result.recommendation_resolutions {
        let row: Option<(String, Option<String>, String)> = transaction
            .query_row(
                "SELECT subject_type,subject_id,action
                 FROM recommendations
                 WHERE id=?1 AND learner_language_profile_id=?2 AND status='open'",
                params![resolution.recommendation_id, result.language_profile_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        let Some((subject_type, subject_id, action)) = row else {
            return Err(crate::Error::operation(
                409,
                "recommendation_not_open",
                "recommendation is unknown, belongs to another profile, or is already resolved",
                serde_json::json!({
                    "recommendation_id": resolution.recommendation_id
                }),
            ));
        };

        let accepted =
            resolution.status == tutor_contract::RecommendationResolutionStatus::Accepted;
        let repair_resolution = accepted
            && subject_type == "error_target"
            && matches!(
                action.as_str(),
                "activate_mini_target" | "require_method_change" | "practice_again"
            );
        if accepted && subject_type == "card" && action == "redesign_card" {
            let target_id = subject_id.as_deref().ok_or_else(|| {
                crate::Error::operation(
                    409,
                    "recommendation_target_missing",
                    "redesign_card recommendation has no card target",
                    serde_json::json!({ "recommendation_id": resolution.recommendation_id }),
                )
            })?;
            let applied = (0..card_plan.len).any(|index| {
                card_plan.verdict[index] == super::card_plan::CardVerdict::Applied
                    && card_plan.effect[index] != super::card_plan::CardEffectKind::None
                    && result.card_mutations
                        [super::card_plan::card_plan_source_index(card_plan, index)]
                    .card_id
                    .as_deref()
                        == Some(target_id)
            });
            if !applied {
                return Err(crate::Error::operation(
                    409,
                    "redesign_recommendation_not_applied",
                    "accepted redesign_card recommendation requires an applied same-card mutation in this lesson",
                    serde_json::json!({
                        "recommendation_id": resolution.recommendation_id,
                        "card_id": target_id
                    }),
                ));
            }
        }
        if accepted && subject_type == "learning_item" && action == "practice_in_lesson" {
            let target_id = subject_id.as_deref().ok_or_else(|| {
                crate::Error::operation(
                    409,
                    "recommendation_target_missing",
                    "practice_in_lesson recommendation has no learning-item target",
                    serde_json::json!({ "recommendation_id": resolution.recommendation_id }),
                )
            })?;
            let observed = result
                .evidence
                .iter()
                .any(|entry| entry.learning_item_id.as_deref() == Some(target_id))
                || result
                    .listening_attempts
                    .iter()
                    .any(|entry| entry.learning_item_id.as_deref() == Some(target_id))
                || result
                    .interventions
                    .iter()
                    .any(|entry| entry.learning_item_id.as_deref() == Some(target_id));
            if !observed {
                return Err(crate::Error::operation(
                    409,
                    "practice_recommendation_not_applied",
                    "accepted practice_in_lesson recommendation requires same-item evidence, listening, or intervention activity in this lesson",
                    serde_json::json!({
                        "recommendation_id": resolution.recommendation_id,
                        "learning_item_id": target_id
                    }),
                ));
            }
        }
        if repair_resolution
            && resolution
                .repair_action
                .as_deref()
                .is_none_or(|value| value.trim().is_empty())
        {
            return Err(crate::Error::operation(
                400,
                "repair_action_missing",
                "accepted error repair must name the repair_action actually applied",
                serde_json::json!({
                    "recommendation_id": resolution.recommendation_id
                }),
            ));
        }
        if !repair_resolution && resolution.repair_action.is_some() {
            return Err(crate::Error::operation(
                400,
                "unexpected_repair_action",
                "repair_action is valid only for an accepted error-target repair recommendation",
                serde_json::json!({
                    "recommendation_id": resolution.recommendation_id
                }),
            ));
        }

        if repair_resolution {
            transaction.execute(
                "UPDATE recommendations
                 SET status=?3,resolved_at=?4,
                     detail_json=json_set(
                         CASE WHEN json_valid(detail_json) THEN detail_json ELSE '{}' END,
                         '$.repair_action',?5
                     )
                 WHERE id=?1 AND learner_language_profile_id=?2 AND status='open'",
                params![
                    resolution.recommendation_id,
                    result.language_profile_id,
                    tutor_contract::recommendation_resolution_status_id(resolution.status),
                    at,
                    resolution.repair_action.as_deref().unwrap_or_default()
                ],
            )?;
        } else {
            transaction.execute(
                "UPDATE recommendations SET status=?3,resolved_at=?4
                 WHERE id=?1 AND learner_language_profile_id=?2 AND status='open'",
                params![
                    resolution.recommendation_id,
                    result.language_profile_id,
                    tutor_contract::recommendation_resolution_status_id(resolution.status),
                    at
                ],
            )?;
        }

        if repair_resolution {
            let target_id = subject_id.as_deref().ok_or_else(|| {
                crate::Error::operation(
                    409,
                    "repair_target_missing",
                    "error repair recommendation has no target",
                    serde_json::json!({
                        "recommendation_id": resolution.recommendation_id
                    }),
                )
            })?;
            let target_state: Option<(String, bool)> = transaction
                .query_row(
                    "SELECT status,method_change_required
                     FROM error_targets
                     WHERE id=?1 AND learner_language_profile_id=?2",
                    params![target_id, result.language_profile_id],
                    |row| Ok((row.get(0)?, row.get::<_, u8>(1)? != 0)),
                )
                .optional()?;
            let Some((status, method_change_required)) = target_state else {
                return Err(crate::Error::operation(
                    409,
                    "repair_target_missing",
                    "error repair recommendation target no longer exists",
                    serde_json::json!({ "error_target_id": target_id }),
                ));
            };
            if !matches!(status.as_str(), "active" | "escalated") {
                return Err(crate::Error::operation(
                    409,
                    "repair_target_not_active",
                    "error repair can be accepted only while the target is in active repair state",
                    serde_json::json!({
                        "error_target_id": target_id,
                        "status": status
                    }),
                ));
            }

            let repair_action = resolution.repair_action.as_deref().unwrap_or_default();
            if method_change_required && action == "require_method_change" {
                let previous_action: Option<String> = transaction
                    .query_row(
                        "SELECT repair_action
                         FROM (
                             SELECT r.repair_action AS repair_action,
                                    r.completed_at AS repaired_at
                             FROM error_rechecks r
                             WHERE r.error_target_id=?1
                               AND r.completed_at IS NOT NULL
                               AND r.repair_action IS NOT NULL
                             UNION ALL
                             SELECT json_extract(rec.detail_json,'$.repair_action') AS repair_action,
                                    rec.resolved_at AS repaired_at
                             FROM recommendations rec
                             WHERE rec.subject_type='error_target'
                               AND rec.subject_id=?1
                               AND rec.id!=?2
                               AND rec.status='accepted'
                               AND json_valid(rec.detail_json)
                               AND json_type(rec.detail_json,'$.repair_action')='text'
                         )
                         WHERE repair_action IS NOT NULL AND TRIM(repair_action)!=''
                         ORDER BY repaired_at DESC
                         LIMIT 1",
                        params![target_id, resolution.recommendation_id],
                        |row| row.get(0),
                    )
                    .optional()?;
                if previous_action.as_deref() == Some(repair_action) {
                    return Err(crate::Error::operation(
                        409,
                        "repair_method_not_changed",
                        "require_method_change must use a different named repair_action than the last applied repair",
                        serde_json::json!({
                            "error_target_id": target_id,
                            "repair_action": repair_action
                        }),
                    ));
                }
            }

            if !error_repairs.is_empty() {
                error_repairs.push(';');
            }
            error_repairs.push_str(target_id);
            error_repairs.push('=');
            error_repairs.push_str(repair_action);

            let repair_satisfies_constraint =
                !method_change_required || action == "require_method_change";
            if repair_satisfies_constraint {
                if action == "require_method_change" {
                    transaction.execute(
                        "UPDATE recommendations
                         SET status='expired',resolved_at=?3
                         WHERE learner_language_profile_id=?1
                           AND subject_type='error_target' AND subject_id=?2
                           AND action IN ('activate_mini_target','practice_again')
                           AND status='open'",
                        params![result.language_profile_id, target_id, at],
                    )?;
                }
                schedule_error_rechecks(transaction, target_id, now)?;
            }
        }
    }
    Ok(error_repairs)
}

fn reject_recheck_occurrence_overlap(
    result: &tutor_contract::LessonResult,
    state: &LessonPlanState,
    plan: &LessonPlan,
) -> Result<()> {
    for recheck in &result.error_rechecks {
        for i in 0..plan.error_len {
            if !plan.error_target_present[i] {
                continue;
            }
            let target_id = lesson_error_id(state, dense_slot(plan.error_target_index[i])?);
            if target_id == recheck.error_target_id {
                return Err(crate::Error::operation(
                    422,
                    "ambiguous_error_observation",
                    "one error target cannot be recorded as both a scheduled recheck and an ordinary recurrence in the same lesson result",
                    serde_json::json!({ "error_target_id": target_id }),
                ));
            }
        }
    }
    Ok(())
}

fn persist_lesson_header_and_metadata(
    transaction: &rusqlite::Transaction<'_>,
    result: &tutor_contract::LessonResult,
    plan: &LessonPlan,
    state: &LessonPlanState,
    at: &str,
) -> Result<String> {
    let lesson_id = match plan.lesson_kind {
        LessonPlanKind::CompleteExisting => {
            transaction.execute(
                "UPDATE lessons SET
                    status='completed',completed_at=?2,duration_seconds=?3,
                    tutor_summary=?4,next_goal=?5,idempotency_key=?6,
                    scenario=?7,updated_at=?2
                 WHERE id=?1 AND status IN ('planned','active')",
                params![
                    lesson_existing_id(state),
                    at,
                    result.lesson.duration_seconds,
                    result.lesson.tutor_summary,
                    result.lesson.next_goal,
                    result.client_operation_id,
                    result.lesson.scenario,
                ],
            )?;
            lesson_existing_id(state).to_owned()
        }
        LessonPlanKind::CreateCompleted => {
            let id = crate::api::new_id("les");
            insert_lesson(
                transaction,
                &id,
                &result.language_profile_id,
                plan.lesson_number,
                "completed",
                &result.lesson.scenario,
                "",
                "import",
                Some(at),
                Some(at),
                result.lesson.duration_seconds,
                result.lesson.tutor_summary.as_deref(),
                result.lesson.next_goal.as_deref(),
                Some(&result.client_operation_id),
                at,
            )?;
            id
        }
        LessonPlanKind::DuplicateNoop => unreachable!("handled before persistence"),
    };

    if let Some(metrics) = &result.metrics {
        transaction.execute(
            "INSERT INTO lesson_metrics
             (lesson_id,speaking_seconds,unaided_recall_count,aided_recall_count,
              listening_score,avg_answer_millis,long_pause_count,
              task_attempt_count,task_success_count,task_success_rubric,confidence,created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
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

    Ok(lesson_id)
}

fn apply_mastery_status_updates(
    transaction: &rusqlite::Transaction<'_>,
    result: &tutor_contract::LessonResult,
    plan: &LessonPlan,
    at: &str,
) -> Result<()> {
    for index in 0..plan.status_len {
        let source = dense_slot(plan.status_source[index])?;
        let change = &result.mastery_changes[source];
        update_item_status(
            transaction,
            &change.learning_item_id,
            crate::sqlite::learning_item_status_db(&change.new_status),
            at,
        )?;
        if change.new_status == learning_core::LearningItemStatus::Suspended {
            transaction.execute(
                "UPDATE recommendations
                 SET status='expired',resolved_at=?3
                 WHERE learner_language_profile_id=?1
                   AND subject_type='learning_item' AND subject_id=?2
                   AND action='practice_in_lesson' AND status='open'",
                params![result.language_profile_id, change.learning_item_id, at],
            )?;
        }
    }
    Ok(())
}

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
    reject_recheck_occurrence_overlap(result, &state, &plan)?;
    require_started_lesson_constraints(&transaction, result, &plan)?;

    let at = to_rfc3339(now);

    let (card_state, card_plan) = prepare_card_mutations(
        &transaction,
        &result.language_profile_id,
        &result.card_mutations,
        now,
    )?;

    let lesson_id = persist_lesson_header_and_metadata(&transaction, result, &plan, &state, &at)?;

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
            crate::sqlite::learning_item_type_db(&source.item_type),
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
            crate::sqlite::evidence_kind_db(&source.kind),
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

    for recheck in &result.error_rechecks {
        apply_error_recheck(
            &transaction,
            Some(&result.language_profile_id),
            recheck,
            now,
        )?;
    }

    const MAX_ERRORS: usize = tutor_contract::bounds::MAX_ERROR_OCCURRENCES;
    let mut new_error_id_text = String::new();
    let mut new_error_ids = [IdRef::default(); MAX_ERRORS];

    for i in 0..plan.error_len {
        let source = &result.error_occurrences[i];
        let prior_target_index = result.error_occurrences[..i]
            .iter()
            .position(|previous| same_lesson_error_target(previous, source));
        let first_for_target = prior_target_index.is_none();
        let episode_impact =
            strongest_lesson_error_impact(&result.error_occurrences[..plan.error_len], source);
        let episode_method_change = result.error_occurrences[..plan.error_len]
            .iter()
            .enumerate()
            .any(|(index, occurrence)| {
                same_lesson_error_target(occurrence, source) && plan.error_method_change[index]
            });
        let target_id = if plan.error_target_present[i] {
            let id = lesson_error_id(&state, dense_slot(plan.error_target_index[i])?);
            if first_for_target {
                transaction.execute(
                    "UPDATE error_rechecks
                     SET completed_at=?2,passed=NULL
                     WHERE error_target_id=?1 AND completed_at IS NULL",
                    params![id, at],
                )?;
                update_error_target(
                    &transaction,
                    id,
                    plan.error_new_count[i],
                    episode_method_change,
                    "active",
                    &at,
                )?;
            }
            id.to_owned()
        } else if let Some(prior_index) = prior_target_index {
            id_at(&new_error_id_text, new_error_ids[prior_index]).to_owned()
        } else {
            let id = crate::api::new_id("et");
            new_error_ids[i] = push_id(&mut new_error_id_text, &id);
            insert_error_target(
                &transaction,
                &id,
                &result.language_profile_id,
                crate::sqlite::core_error_category_db(&source.core_category),
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
            crate::sqlite::communicative_impact_db(&source.communicative_impact),
            &at,
        )?;

        if first_for_target && plan.error_target_present[i] {
            let count = plan.error_new_count[i];
            let method_threshold = learning_core::error_method_change_threshold(episode_impact);
            let escalation_threshold = learning_core::error_escalation_threshold(episode_impact);
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
                if action == "require_method_change" {
                    transaction.execute(
                        "UPDATE recommendations
                         SET status='expired',resolved_at=?3
                         WHERE learner_language_profile_id=?1
                           AND subject_type='error_target' AND subject_id=?2
                           AND action IN ('activate_mini_target','practice_again')
                           AND status='open'",
                        params![result.language_profile_id, target_id, at],
                    )?;
                }
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
    }

    let error_recommendation_repairs =
        apply_recommendation_resolutions(&transaction, result, &card_plan, now, &at)?;
    apply_mastery_status_updates(&transaction, result, &plan, &at)?;

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
    let (diagnostic_probe_count, measurement_eligible_probe_count) =
        tutor_contract::measurement_coverage_counts(result);
    let calibration_valid = diagnostic_probe_count >= 1 && measurement_eligible_probe_count >= 2;

    let outcome = serde_json::json!({
        "completed_at_unix": now.0,
        "scenario": result.lesson.scenario,
        "metrics": result.metrics,
        "evidence": result.evidence,
        "listening_attempts": result.listening_attempts,
        "interventions": intervention_outcome,
        "diagnostic_probe_count": diagnostic_probe_count,
        "measurement_eligible_probe_count": measurement_eligible_probe_count,
        "calibration_valid": calibration_valid,
        "error_occurrences": result.error_occurrences,
        "error_rechecks": result.error_rechecks,
        "recommendation_resolutions": result.recommendation_resolutions,
        "error_recommendation_repairs": error_recommendation_repairs,
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
        "diagnostic_probe_count": diagnostic_probe_count,
        "measurement_eligible_probe_count": measurement_eligible_probe_count,
        "calibration_valid": calibration_valid,
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
            None => None,
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

    for (source_index, occurrence) in result.error_occurrences.iter().enumerate() {
        let target = if let Some(id) = occurrence.error_target_id.as_deref() {
            database
                .query_row(
                    "SELECT id,occurrence_count,status,method_change_required FROM error_targets
                     WHERE id=?1 AND learner_language_profile_id=?2",
                    params![id, result.language_profile_id],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, u32>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, u8>(3)? != 0,
                        ))
                    },
                )
                .optional()?
        } else {
            database
                .query_row(
                    "SELECT id,occurrence_count,status,method_change_required
                     FROM error_targets
                     WHERE learner_language_profile_id=?1
                       AND status!='resolved'
                       AND core_category=?2
                       AND (
                            (error_code IS NULL AND ?3 IS NULL)
                            OR (error_code IS NOT NULL AND ?3 IS NOT NULL
                                AND TRIM(error_code)=TRIM(?3))
                       )
                       AND TRIM(original_production)=TRIM(?4)
                       AND TRIM(corrected_production)=TRIM(?5)
                     ORDER BY occurrence_count DESC,updated_at DESC,id
                     LIMIT 1",
                    params![
                        result.language_profile_id,
                        crate::sqlite::core_error_category_db(&occurrence.core_category),
                        occurrence.error_code,
                        occurrence.original_production,
                        occurrence.corrected_production,
                    ],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, u32>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, u8>(3)? != 0,
                        ))
                    },
                )
                .optional()?
        };

        if let Some((id, count, status, method_change_required)) = target {
            let target_index = add_lesson_error_target(
                &mut state,
                &id,
                count,
                status == "resolved",
                method_change_required,
            )
            .map_err(plan_error)?;
            bind_lesson_error_source(&mut state, source_index, target_index).map_err(plan_error)?;
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
             SELECT lesson_id,success,metadata_json,recorded_at,
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
         SELECT lesson_id,success,metadata_json,recorded_at
         FROM ranked
         WHERE episode_rn=1
         ORDER BY recorded_at DESC",
    )?;
    let mut rows = statement.query(params![item_id, cutoff, now_at])?;
    let mut newest_counted_at: Option<i64> = None;
    while let Some(row) = rows.next()? {
        let lesson_id: Option<String> = row.get(0)?;
        let success: u8 = row.get(1)?;
        let metadata: Option<String> = row.get(2)?;
        let recorded_at: String = row.get(3)?;

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
            let observed_at =
                crate::sqlite::from_rfc3339(&recorded_at).map_err(crate::Error::RustError)?;
            if newest_counted_at.is_some_and(|newer_at| {
                newer_at.saturating_sub(observed_at.0)
                    < adaptation::MIN_DISTRIBUTED_SUCCESS_GAP_SECONDS
            }) {
                continue;
            }
            add_lesson_mastery_signal_from_episode(state, item_id, lesson_id.as_deref(), &[family])
                .map_err(plan_error)?;
            newest_counted_at = Some(observed_at.0);
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

// FUZZ SUPPORT BELOW: excluded from production line coverage.

/// Exercise private completion passes through real bounded state and SQLite rows.
#[cfg(fuzzing)]
#[allow(clippy::expect_used, missing_docs)]
pub(crate) fn fuzz_private_paths(
    env: &crate::Env,
    profile_id: &str,
    base: &tutor_contract::LessonResult,
) {
    let mut database = env.connection().expect("completion fuzz connection");
    let transaction = database.transaction().expect("completion fuzz transaction");
    let now = learning_core::Timestamp(crate::api::now().0.saturating_sub(86_400));
    let at = crate::sqlite::to_rfc3339(now);

    // Pure helper branches used by completion admission/calibration.
    let mut left = base
        .listening_attempts
        .first()
        .cloned()
        .expect("rich fuzz listening attempt");
    left.source_variant_id = Some("variant-a".to_owned());
    left.talker_key = Some("talker-a".to_owned());
    left.context_tag = Some("context-a".to_owned());
    let mut right = left.clone();
    right.source_variant_id = Some("variant-b".to_owned());
    assert!(listening_variant_differs(&left, &right));
    right.source_variant_id = left.source_variant_id.clone();
    right.talker_key = Some("talker-b".to_owned());
    assert!(listening_variant_differs(&left, &right));
    right.talker_key = left.talker_key.clone();
    right.context_tag = Some("context-b".to_owned());
    assert!(listening_variant_differs(&left, &right));

    let mut transfer_a = base.evidence[0].clone();
    transfer_a.kind = learning_core::EvidenceKind::Transfer;
    transfer_a.context_family = Some("family-a".to_owned());
    let mut transfer_b = transfer_a.clone();
    transfer_b.context_family = Some("family-b".to_owned());
    assert!(transfer_family_differs(&transfer_a, &transfer_b));

    let mut calibration_result = base.clone();
    calibration_result.evidence = vec![base.evidence[0].clone(), base.evidence[1].clone()];
    calibration_result.evidence[0].probe_phase = Some(tutor_contract::ProbePhase::Diagnostic);
    calibration_result.evidence[0].assistance_level = Some(0);
    calibration_result.evidence[1].probe_phase = Some(tutor_contract::ProbePhase::Practice);
    let evidence_intervention = tutor_contract::InterventionEvent {
        action: "fuzz".to_owned(),
        learning_item_id: Some("li-fuzz".to_owned()),
        client_item_key: None,
        target_capability: learning_core::EvidenceKind::SpontaneousProduction,
        evidence_index: Some(1),
        listening_attempt_index: None,
    };
    let _ = intervention_calibration_eligible(&calibration_result, &evidence_intervention);

    let mut listening_result = base.clone();
    let mut diagnostic = left.clone();
    diagnostic.learning_item_id = Some("li-fuzz".to_owned());
    diagnostic.client_item_key = None;
    diagnostic.probe_phase = Some(tutor_contract::ProbePhase::Diagnostic);
    diagnostic.assistance_level = Some(0);
    let mut post = diagnostic.clone();
    post.probe_phase = Some(tutor_contract::ProbePhase::Practice);
    post.source_variant_id = Some("variant-post".to_owned());
    listening_result.listening_attempts = vec![diagnostic, post];
    let listening_intervention = tutor_contract::InterventionEvent {
        action: "fuzz-listening".to_owned(),
        learning_item_id: Some("li-fuzz".to_owned()),
        client_item_key: None,
        target_capability: learning_core::EvidenceKind::Listening,
        evidence_index: None,
        listening_attempt_index: Some(1),
    };
    let _ = intervention_calibration_eligible(&listening_result, &listening_intervention);

    assert_eq!(canonical_target_text("  uno\t dos  "), "uno dos");
    let mut error_a = base.error_occurrences[0].clone();
    let mut error_b = error_a.clone();
    error_a.communicative_impact = learning_core::CommunicativeImpact::Low;
    error_b.communicative_impact = learning_core::CommunicativeImpact::High;
    assert!(same_lesson_error_target(&error_a, &error_b));
    assert_eq!(
        strongest_lesson_error_impact(&[error_a.clone(), error_b.clone()], &error_a),
        learning_core::CommunicativeImpact::High
    );
    error_b.error_target_id = Some("other-target".to_owned());
    assert!(!same_lesson_error_target(&error_a, &error_b));

    // Real recommendation rows exercise the private resolution pass without
    // mutating the caller because this transaction is intentionally rolled back.
    for (id, method_change_required, status) in [
        ("fuzz-rec-error", false, "active"),
        ("fuzz-rec-change", true, "active"),
        ("fuzz-rec-inactive", false, "recheck"),
    ] {
        crate::operations::error::insert_error_target(
            &transaction,
            id,
            profile_id,
            "grammar",
            None,
            "wrong",
            "right",
            Some("completion fuzz target"),
            1,
            method_change_required,
            status,
            &at,
        )
        .expect("seed completion fuzz error target");
    }

    transaction
        .execute(
            "INSERT INTO recommendations
             (id,learner_language_profile_id,subject_type,subject_id,action,detail_json,status,created_at,resolved_at)
             VALUES
             ('rec-prev',?1,'error_target','fuzz-rec-change','require_method_change',
              json_object('repair_action','repeat_drill'),'accepted',?2,?2),
             ('rec-repair',?1,'error_target','fuzz-rec-error','practice_again','{}','open',?2,NULL),
             ('rec-repair-missing',?1,'error_target','fuzz-rec-error','practice_again','{}','open',?2,NULL),
             ('rec-change-same',?1,'error_target','fuzz-rec-change','require_method_change','{}','open',?2,NULL),
             ('rec-change-diff',?1,'error_target','fuzz-rec-change','require_method_change','{}','open',?2,NULL),
             ('rec-inactive',?1,'error_target','fuzz-rec-inactive','practice_again','{}','open',?2,NULL),
             ('rec-no-target',?1,'error_target',NULL,'practice_again','{}','open',?2,NULL),
             ('rec-missing-target',?1,'error_target','missing-error','practice_again','{}','open',?2,NULL),
             ('rec-card-null',?1,'card',NULL,'redesign_card','{}','open',?2,NULL),
             ('rec-card',?1,'card','missing-card','redesign_card','{}','open',?2,NULL),
             ('rec-learning',?1,'learning_item','never-observed','practice_in_lesson','{}','open',?2,NULL),
             ('rec-dismiss',?1,'learning_item','li-fuzz','practice_in_lesson','{}','open',?2,NULL),
             ('rec-unexpected-repair',?1,'learning_item','li-fuzz','practice_in_lesson','{}','open',?2,NULL)",
            rusqlite::params![profile_id, at],
        )
        .expect("seed completion fuzz recommendations");

    let card_state = super::card_plan::new_card_plan_state(0);
    let card_plan =
        super::card_plan::plan_card_mutations(&[], &card_state).expect("empty card plan");

    let run = |recommendation_id: &str,
               status: tutor_contract::RecommendationResolutionStatus,
               repair_action: Option<&str>| {
        let mut result = base.clone();
        result.language_profile_id = profile_id.to_owned();
        result.recommendation_resolutions = vec![tutor_contract::RecommendationResolution {
            recommendation_id: recommendation_id.to_owned(),
            status,
            repair_action: repair_action.map(str::to_owned),
        }];
        apply_recommendation_resolutions(&transaction, &result, &card_plan, now, &at)
    };

    let _ = run(
        "not-open",
        tutor_contract::RecommendationResolutionStatus::Accepted,
        None,
    );
    let _ = run(
        "rec-card-null",
        tutor_contract::RecommendationResolutionStatus::Accepted,
        None,
    );
    let _ = run(
        "rec-card",
        tutor_contract::RecommendationResolutionStatus::Accepted,
        None,
    );
    let _ = run(
        "rec-learning",
        tutor_contract::RecommendationResolutionStatus::Accepted,
        None,
    );
    let _ = run(
        "rec-repair-missing",
        tutor_contract::RecommendationResolutionStatus::Accepted,
        None,
    );
    let _ = run(
        "rec-unexpected-repair",
        tutor_contract::RecommendationResolutionStatus::Dismissed,
        Some("unexpected"),
    );
    let _ = run(
        "rec-no-target",
        tutor_contract::RecommendationResolutionStatus::Accepted,
        Some("drill"),
    );
    let _ = run(
        "rec-missing-target",
        tutor_contract::RecommendationResolutionStatus::Accepted,
        Some("drill"),
    );
    let _ = run(
        "rec-inactive",
        tutor_contract::RecommendationResolutionStatus::Accepted,
        Some("drill"),
    );
    let _ = run(
        "rec-change-same",
        tutor_contract::RecommendationResolutionStatus::Accepted,
        Some("repeat_drill"),
    );
    let _ = run(
        "rec-change-diff",
        tutor_contract::RecommendationResolutionStatus::Accepted,
        Some("semantic_reframe"),
    )
    .expect("changed repair method must be accepted");
    let _ = run(
        "rec-repair",
        tutor_contract::RecommendationResolutionStatus::Accepted,
        Some("practice_drill"),
    )
    .expect("ordinary repair must be accepted");
    let _ = run(
        "rec-dismiss",
        tutor_contract::RecommendationResolutionStatus::Dismissed,
        None,
    )
    .expect("dismissal must be accepted");

    drop(transaction);
}
