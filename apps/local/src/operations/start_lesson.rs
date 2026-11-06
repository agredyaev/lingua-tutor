#![allow(clippy::items_after_test_module)] // Test fixtures are intentionally grouped before the processing pass.
//! Bounded start-lesson planner/data path.

#[cfg(test)]
// Test fixtures intentionally fail fast and use compact tuple snapshots.
#[allow(clippy::unwrap_used, clippy::type_complexity)]
mod tests {
    use super::*;
    use crate::Env;
    use std::path::PathBuf;

    fn create_env(label: &str) -> (PathBuf, Env) {
        let path = std::env::temp_dir().join(format!(
            "lingua-start-{label}-{}.sqlite3",
            uuid::Uuid::new_v4()
        ));
        let db = crate::storage::open(&path).unwrap();
        db.execute_batch(
            r#"
               INSERT INTO learner_profile(id,created_at,updated_at)
               VALUES('learner-1','2026-10-10T00:00:00Z','2026-10-10T00:00:00Z');
               INSERT INTO learner_language_profiles
                (id,learner_id,source_language,target_locale,
                 estimated_level,current_lesson_number,status,is_default,preferences_json,
                 created_at,updated_at)
               VALUES('llp-1','learner-1', 'ru','es-ES','A1',4,'active',1,
                 '{"max_new_items_per_lesson":2,"preferred_difficulty":"balanced"}',
                 '2026-10-10T00:00:00Z','2026-10-10T00:00:00Z');

               INSERT INTO lessons
                (id,learner_language_profile_id,
                 lesson_number,status,scenario,planned_goal,source,started_at,
                 completed_at,duration_seconds,created_at,updated_at)
               VALUES('done-1','llp-1',4,'completed','shop','old','pwa',
                 '2026-10-15T10:00:00Z','2026-10-15T10:20:00Z',1200,
                 '2026-10-15T10:00:00Z','2026-10-15T10:20:00Z');
               INSERT INTO lesson_metrics
                (lesson_id,speaking_seconds,unaided_recall_count,aided_recall_count,
                 listening_score,avg_answer_millis,long_pause_count,confidence,created_at)
               VALUES('done-1',480,2,3,0.65,6500,1,0.50,'2026-10-15T10:20:00Z');

               INSERT INTO learning_items
                (id,learner_language_profile_id,item_type,target_text,
                 normalized_target_text,prompt_text,prompt_language,status,
                 introduced_lesson_id,created_at,updated_at)
               VALUES('li-1','llp-1', 'phrase','¿Dónde está?','¿dónde está?',
                 'Где находится?','ru','learning','done-1',
                 '2026-10-15T10:00:00Z','2026-10-16T10:00:00Z');

               INSERT INTO learning_item_evidence
                (id,learning_item_id,lesson_id,kind,success,detail,metadata_json,recorded_at)
               VALUES
                ('ev-1','li-1','done-1','prompted_production',1,NULL,
                 '{"assistance_level":1,"response_millis":3200,"context_key":"shop:a",
                   "context_family":"shopping.location"}','2026-10-15T10:10:00Z'),
                ('ev-2','li-1','done-1','spontaneous_production',0,NULL,
                 '{"assistance_level":0,"response_millis":7000,"context_key":"street:a",
                   "context_family":"directions.location"}','2026-10-15T10:15:00Z');

               INSERT INTO listening_attempts
                (id,learner_language_profile_id,learning_item_id,lesson_id,
                 source_reference,transcript,success,recorded_at)
               VALUES('la-1','llp-1','li-1','done-1',NULL,NULL,0,'2026-10-15T10:12:00Z');

               INSERT INTO error_targets
                (id,learner_language_profile_id,
                 core_category,original_production,corrected_production,occurrence_count,
                 method_change_required,status,created_at,updated_at)
               VALUES('et-1','llp-1','grammar','x','y',5,1,'recheck',
                 '2026-10-15T00:00:00Z','2026-10-16T00:00:00Z');
               INSERT INTO error_rechecks(id,error_target_id,offset_days,due_at)
               VALUES('rc-1','et-1',1,'2020-01-01T00:00:00Z');

               INSERT INTO recommendations
                (id,learner_language_profile_id,subject_type,subject_id,action,detail_json,
                 status,created_at)
               VALUES('rec-1','llp-1','error_target','et-1','practice_again','{}','open',
                 '2026-10-16T00:00:00Z');

               INSERT INTO cards
                (id,learner_language_profile_id,
                 intent,status,prompt,prompt_language,answer,answer_language,
                 accepted_answers_json,creation_reason,content_hash,
                 content_version,created_by,created_at,updated_at)
               VALUES('card-1','llp-1','production','review',
                 'p','ru','a','es-ES','[]','seed','hash',1,'tutor',
                 '2026-10-15T00:00:00Z','2026-10-15T00:00:00Z');
               INSERT INTO card_schedule_state
                (card_id,algorithm,algorithm_version,due_at,lapse_count,review_count,
                 state_version,state_hash,updated_at)
               VALUES('card-1','fallback','fallback-1','2020-01-01T00:00:00Z',0,2,2,
                 'not-used-by-planner','2026-10-16T00:00:00Z');"#,
        )
        .unwrap();
        drop(db);
        (path.clone(), Env::new(path, "key".to_owned()))
    }

    fn snapshot(
        env: &Env,
    ) -> (
        (u32, String, String, String, String),
        [serde_json::Value; 5],
    ) {
        let db = env.connection().unwrap();
        let lesson: (u32, String, String, String, String) = db
            .query_row(
                "SELECT lesson_number,status,scenario,planned_goal,source
             FROM lessons WHERE status='active'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .unwrap();
        let row: (String, String, String, String, String) = db
            .query_row(
                "SELECT input_json,plan_json,task_candidates_json,capability_state_json,
                    focus_threads_json
             FROM planner_decisions ORDER BY rowid DESC LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .unwrap();
        (
            lesson,
            [
                serde_json::from_str(&row.0).unwrap(),
                serde_json::from_str(&row.1).unwrap(),
                serde_json::from_str(&row.2).unwrap(),
                serde_json::from_str(&row.3).unwrap(),
                serde_json::from_str(&row.4).unwrap(),
            ],
        )
    }

    #[test]
    fn bounded_start_persists_active_lesson_and_planner_decision() {
        let (path, env) = create_env("new");
        let result = start_lesson(&env, "llp-1", "pharmacy", "buy medicine", "pwa").unwrap();
        assert_eq!(result["lesson_number"], 5);
        assert_eq!(result["planner_version"], "planner-v3");
        assert!(result["adaptive_lesson_plan"].is_object());
        assert!(result["tutor_tasks"].is_array());
        let state = snapshot(&env);
        assert_eq!(result["adaptive_lesson_plan"], state.1[1]);
        assert_eq!(
            state.0,
            (
                5,
                "active".to_owned(),
                "pharmacy".to_owned(),
                "buy medicine".to_owned(),
                "pwa".to_owned()
            )
        );
        for value in &state.1 {
            assert!(!value.is_null());
        }

        let next = crate::handlers::records::next_lesson_for(&env, "llp-1").unwrap();
        assert_eq!(next["lesson_number"], 5);
        let context =
            crate::handlers::records::tutor_context_for(&registry, &env, "llp-1").unwrap();
        assert_eq!(context["next_lesson_number"], 5);
        let dashboard = crate::handlers::records::dashboard_for(&env, "llp-1").unwrap();
        assert_eq!(dashboard["next_lesson_number"], 5);
        let profiles = crate::handlers::profiles::list_profiles(&env).unwrap();
        assert_eq!(profiles[0]["current_lesson_number"], 4);

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn repeated_start_returns_existing_open_lesson() {
        let (path, env) = create_env("repeat-start");
        let registry = built_in_language_packs().unwrap();

        let first =
            start_lesson(&registry, &env, "llp-1", "pharmacy", "buy medicine", "mcp").unwrap();
        let second =
            start_lesson(&registry, &env, "llp-1", "transport", "buy a ticket", "mcp").unwrap();

        assert_eq!(second["id"], first["id"]);
        assert_eq!(second["lesson_number"], first["lesson_number"]);

        let db = env.connection().unwrap();
        let open_count: u32 = db
            .query_row(
                "SELECT COUNT(*) FROM lessons
                 WHERE learner_language_profile_id='llp-1'
                   AND status IN ('planned','active')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let planner_count: u32 = db
            .query_row(
                "SELECT COUNT(*) FROM planner_decisions
                 WHERE learner_language_profile_id='llp-1'
                   AND lesson_id=?1",
                [first["id"].as_str().unwrap()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(open_count, 1);
        assert_eq!(planner_count, 1);
        drop(db);

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn start_uses_persisted_max_when_profile_counter_is_stale() {
        let (path, env) = create_env("stale-counter");
        let registry = built_in_language_packs().unwrap();
        env.connection()
            .unwrap()
            .execute(
                "UPDATE learner_language_profiles
                 SET current_lesson_number = 0
                 WHERE id = 'llp-1'",
                [],
            )
            .unwrap();

        let profiles = crate::handlers::profiles::list_profiles(&env).unwrap();
        assert_eq!(profiles[0]["current_lesson_number"], 4);

        let result = start_lesson(
            &registry,
            &env,
            "llp-1",
            "pharmacy",
            "repair counter",
            "mcp",
        )
        .unwrap();

        assert_eq!(result["lesson_number"], 5);
        let db = env.connection().unwrap();
        let persisted: (u32, String) = db
            .query_row(
                "SELECT lesson_number, status
                 FROM lessons
                 WHERE learner_language_profile_id = 'llp-1'
                 ORDER BY lesson_number DESC
                 LIMIT 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(persisted, (5, "active".to_owned()));
        drop(db);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn abandoned_lesson_number_is_not_reused_by_reads_or_start() {
        let (path, env) = create_env("abandoned-number");
        let registry = built_in_language_packs().unwrap();
        env.connection()
            .unwrap()
            .execute(
                "INSERT INTO lessons
                 (id,learner_language_profile_id,language_pack_id,language_pack_version,
                  lesson_number,status,scenario,planned_goal,source,started_at,
                  created_at,updated_at)
                 VALUES
                 ('abandoned-5','llp-1','es-ES','0.2.0',5,'abandoned',
                  'transport','','mcp','2026-10-16T00:00:00Z',
                  '2026-10-16T00:00:00Z','2026-10-16T00:00:00Z')",
                [],
            )
            .unwrap();

        let next = crate::handlers::records::next_lesson_for(&env, "llp-1").unwrap();
        assert_eq!(next["lesson_number"], 6);
        let context =
            crate::handlers::records::tutor_context_for(&registry, &env, "llp-1").unwrap();
        assert_eq!(context["next_lesson_number"], 6);
        let dashboard = crate::handlers::records::dashboard_for(&env, "llp-1").unwrap();
        assert_eq!(dashboard["next_lesson_number"], 6);

        let started =
            start_lesson(&registry, &env, "llp-1", "pharmacy", "new attempt", "mcp").unwrap();
        assert_eq!(started["lesson_number"], 6);

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn record_views_preserve_seeded_profile_contract() {
        let (path, env) = create_env("profile-read");
        let next = crate::handlers::records::next_lesson_for(&env, "llp-1").unwrap();
        assert_eq!(next["lesson_number"], 5);
        assert_eq!(next["estimated_level"], "A1");
        assert_eq!(next["previous_scenario"], "shop");
        assert_eq!(next["tracked_item_count"], 1);
        assert_eq!(next["learner_preferences"]["max_new_items_per_lesson"], 2);

        let context = crate::handlers::records::tutor_context_for(&env, "llp-1").unwrap();
        assert_eq!(context["language_profile_id"], "llp-1");
        assert_eq!(context["next_lesson_number"], 5);
        assert_eq!(context["estimated_level"], "A1");
        for required in [
            "source_language",
            "target_locale",
            "learner_preferences",
            "tutor_instructions",
            "planner_input",
            "adaptive_lesson_plan",
            "tutor_tasks",
            "planner_history",
            "intervention_effectiveness",
            "active_error_targets",
            "learning_items",
            "due_error_rechecks",
            "existing_card_answers",
            "review_summary_7d",
            "redesign_candidates",
            "open_recommendations",
            "lesson_result_contract",
        ] {
            assert!(
                !context[required].is_null(),
                "authoritative LLM context lost decision-critical block: {required}"
            );
        }
        assert!(context["tutor_instructions"].is_object());
        assert!(context["adaptive_lesson_plan"].is_object());
        assert!(context["tutor_tasks"].is_array());
        assert!(context["lesson_result_contract"].is_object());
        let contract_instructions = context["lesson_result_contract"]["instructions"]
            .as_str()
            .unwrap();
        assert!(
            contract_instructions.contains(
                "минимум 1 diagnostic baseline и минимум 2 независимых measured probes overall"
            ),
            "LLM-facing lesson contract drifted from runtime validator"
        );
        assert!(
            !contract_instructions.contains("минимум 2 полностью измеренных diagnostic probes"),
            "stale duplicate-diagnostic rule leaked into LLM context"
        );

        let dashboard = crate::handlers::records::dashboard_for(&env, "llp-1").unwrap();
        assert_eq!(dashboard["next_lesson_number"], 5);
        assert_eq!(dashboard["items_by_status"]["learning"], 1);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn lesson_contract_uses_profile_source_language() {
        let (path, env) = create_env("source-language-contract");
        {
            let db = env.connection().unwrap();
            db.execute(
                "UPDATE learner_language_profiles SET source_language='en' WHERE id='llp-1'",
                [],
            )
            .unwrap();
        }

        let context = crate::handlers::records::tutor_context_for(&env, "llp-1").unwrap();
        let template = &context["lesson_result_contract"]["template"];
        assert_eq!(template["new_learning_items"][0]["prompt_language"], "en");
        assert_eq!(
            template["card_mutations"][0]["card"]["prompt_language"],
            "en"
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn llm_lesson_result_template_is_backend_valid() {
        let (path, env) = create_env("valid-llm-template");
        let context = crate::handlers::records::tutor_context_for(&env, "llp-1").unwrap();
        let template = context["lesson_result_contract"]["template"].clone();
        let parsed = tutor_contract::parse_lesson_result(&template.to_string()).unwrap();
        let issues = tutor_contract::validate(&parsed);
        assert!(
            issues.is_empty(),
            "LLM-facing template must be accepted by the same backend contract it documents: {issues:?}"
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn record_views_reject_corrupt_profile_before_planning() {
        let (path, env) = create_env("profile-invalid");
        for (column, bad) in [("preferences_json", "not-json"), ("status", "invalid")] {
            let db = env.connection().unwrap();
            let statement =
                format!("UPDATE learner_language_profiles SET {column}=?1 WHERE id='llp-1'");
            let original: String = db
                .query_row(
                    &format!("SELECT {column} FROM learner_language_profiles WHERE id='llp-1'"),
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            // Simulate a corrupt on-disk row without relaxing normal write constraints.
            db.execute_batch("PRAGMA ignore_check_constraints=ON")
                .unwrap();
            db.execute(&statement, [bad]).unwrap();
            assert!(crate::handlers::records::next_lesson_for(&env, "llp-1").is_err());
            assert!(crate::handlers::records::tutor_context_for(&env, "llp-1").is_err());
            assert!(crate::handlers::records::dashboard_for(&env, "llp-1").is_err());
            db.execute(&statement, [original]).unwrap();
        }
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn planner_history_preserves_latest_five_and_calibration_window() {
        let (path, env) = create_env("planner-history-window");
        {
            let db = env.connection().unwrap();
            for i in 0..8_u32 {
                let created_at = if i == 6 {
                    "invalid-time".to_owned()
                } else {
                    format!("2026-10-{:02}T12:00:00Z", i + 1)
                };
                let input = if i == 3 {
                    "{malformed"
                } else {
                    r#"{"due_reviews":4}"#
                };
                let outcome = if i == 4 {
                    Some("{invalid")
                } else {
                    Some(
                        r#"{"evidence":[{"pronunciation_intelligible":false,"pronunciation_repeat_required":true}],"error_occurrences":[]}"#,
                    )
                };
                db.execute(
                    "INSERT INTO planner_decisions
                     (id,learner_language_profile_id,planner_version,input_json,plan_json,
                      task_candidates_json,capability_state_json,focus_threads_json,
                      outcome_json,created_at,updated_at)
                     VALUES (?1,'llp-1','planner-v3',?2,'{\"primary_objective\":\"review\"}',
                             '[]','[]','[]',?3,?4,?4)",
                    rusqlite::params![format!("pd-window-{i}"), input, outcome, created_at],
                )
                .unwrap();
            }
        }
        let actual = crate::handlers::records::tutor_context_for(&env, "llp-1").unwrap();
        let history = actual["planner_history"].as_array().unwrap();
        assert_eq!(history.len(), 5);
        assert!(history.iter().any(|entry| {
            entry["outcome_summary"]["pronunciation_repeat_count"] == 1
                && entry["outcome_summary"]["pronunciation_unintelligible_count"] == 1
        }));
        assert!(
            actual["intervention_effectiveness"].is_object()
                || actual["intervention_effectiveness"].is_array()
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn tutor_context_read_only_sections_expose_seeded_state() {
        let (path, env) = create_env("tutor-context-rows");
        env.connection()
            .unwrap()
            .execute(
                "INSERT INTO recommendations
             (id,learner_language_profile_id,subject_type,subject_id,action,
              detail_json,status,created_at)
             VALUES ('rec-redesign','llp-1','card','card-1','redesign_card',
                     '{\"detail\":\"unicode ñ\"}','open','2026-10-16T01:00:00Z')",
                [],
            )
            .unwrap();
        let current = crate::handlers::records::tutor_context_for(&env, "llp-1").unwrap();
        assert_eq!(current["open_recommendations"].as_array().unwrap().len(), 2);
        assert_eq!(current["redesign_candidates"].as_array().unwrap().len(), 1);
        assert_eq!(current["due_error_rechecks"].as_array().unwrap().len(), 1);
        assert_eq!(current["learning_items"].as_array().unwrap().len(), 1);
        assert!(current["review_summary_7d"].is_object());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn batch_projection_includes_items_without_evidence() {
        let (path, env) = create_env("zero-evidence");
        let db = env.connection().unwrap();
        db.execute(
            "INSERT INTO learning_items
             (id,learner_language_profile_id,item_type,target_text,
              normalized_target_text,status,created_at,updated_at)
             VALUES('li-empty','llp-1', 'phrase','empty','empty','learning',
                    '2026-10-16T09:00:00Z','2026-10-16T09:00:00Z')",
            [],
        )
        .unwrap();
        let mut ids = Vec::new();
        let count = super::super::capability_read::visit_item_capabilities(
            &db,
            "llp-1",
            1_760_600_000,
            |state| {
                ids.push((
                    state.learning_item_id.to_owned(),
                    state.recognition.attempts,
                ));
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(count, 2);
        assert!(
            ids.iter()
                .any(|(id, count)| id == "li-empty" && *count == 0)
        );
        assert!(ids.iter().any(|(id, _)| id == "li-1"));
        drop(db);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn batch_projection_rejects_invalid_timestamps() {
        let (path, env) = create_env("invalid-timestamp");
        let db = env.connection().unwrap();
        db.execute(
            "UPDATE learning_item_evidence SET recorded_at='bad-date' WHERE id='ev-1'",
            [],
        )
        .unwrap();
        let outcome = super::super::capability_read::visit_item_capabilities(
            &db,
            "llp-1",
            1_760_600_000,
            |_| Ok(()),
        );
        assert!(outcome.is_err());
        drop(db);
        let _ = std::fs::remove_file(path);
    }
}

use crate::repositories::{lessons, to_rfc3339};
use crate::{Env, Result};
use rusqlite::{OptionalExtension as _, TransactionBehavior, params};
use serde::Serialize;

pub(crate) fn start_lesson(
    env: &Env,
    profile_id: &str,
    scenario: &str,
    planned_goal: &str,
    source: &str,
) -> Result<serde_json::Value> {
    let mut database = env.connection()?;
    let transaction = database.transaction_with_behavior(TransactionBehavior::Immediate)?;

    let profile = transaction
        .query_row(
            "SELECT learner_id,current_lesson_number,preferences_json
             FROM learner_language_profiles WHERE id=?1",
            params![profile_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, u32>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .optional()?
        .ok_or_else(|| {
            crate::Error::operation(404, "not_found", "unknown profile", serde_json::Value::Null)
        })?;
    let (_learner_id, current_lesson_number, preferences_json) = profile;

    if let Some((id, lesson_number)) = transaction
        .query_row(
            lessons::SELECT_LATEST_OPEN_LESSON,
            params![profile_id],
            |row| {
                Ok((
                    row.get::<_, String>("id")?,
                    row.get::<_, u32>("lesson_number")?,
                ))
            },
        )
        .optional()?
    {
        return crate::json(&serde_json::json!({
            "id": id,
            "lesson_number": lesson_number
        }));
    }

    let max_persisted_lesson_number: u32 = transaction.query_row(
        "SELECT COALESCE(MAX(lesson_number), 0)
         FROM lessons
         WHERE learner_language_profile_id = ?1",
        params![profile_id],
        |row| row.get(0),
    )?;
    let next_lesson_number = current_lesson_number
        .max(max_persisted_lesson_number)
        .saturating_add(1);

    let now = crate::api::now();
    let at = to_rfc3339(now);

    let preferences: serde_json::Value = serde_json::from_str(&preferences_json)?;
    let max_new_items_per_lesson = preferences
        .get("max_new_items_per_lesson")
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| u8::try_from(value).ok());
    let preferred_difficulty = preferences
        .get("preferred_difficulty")
        .and_then(serde_json::Value::as_str);
    let planner_input = super::planner_read::read_planner_input(
        &transaction,
        profile_id,
        now,
        adaptation::PlannerPreferencesView {
            max_new_items_per_lesson,
            preferred_difficulty,
        },
    )?;
    let plan = adaptation::plan_next_lesson_fixed(&planner_input);

    let mut selection = super::adaptive_selection::new_selection(
        super::adaptive_selection::DEFAULT_CANDIDATE_LIMIT,
        super::adaptive_selection::DEFAULT_FOCUS_LIMIT,
    );
    let mut capability_json = String::from("[");
    let mut capability_count = 0usize;

    super::capability_read::visit_item_capabilities(&transaction, profile_id, now.0, |state| {
        if capability_count > 0 {
            capability_json.push(',');
        }
        capability_json.push_str(&serde_json::to_string(state)?);
        capability_count += 1;
        let view = adaptation::item_capability_view(state);
        super::adaptive_selection::observe_selection(
            &mut selection,
            view,
            plan.primary_objective,
            now.0,
        );
        Ok(())
    })?;
    capability_json.push(']');

    let candidate_json = super::adaptive_selection::selection_serialize_candidates(&selection)?;
    let focus_json = super::adaptive_selection::selection_serialize_focus(&selection)?;

    #[derive(Serialize)]
    struct PlanJson<'a> {
        primary_objective: &'static str,
        difficulty: &'static str,
        max_new_items: u8,
        priorities: &'a [adaptation::PlanPriorityFixed],
        diagnostic_probes: &'a [&'static str],
        success_criteria: &'static [&'static str],
        recent_signals: adaptation::RecentLearningSignals,
    }
    let mut probes = [""; 3];
    let mut probe_len = 0usize;
    for index in 0..adaptation::plan_diagnostic_probe_count(&plan) {
        if let Some(probe) = adaptation::plan_diagnostic_probe(&plan, index) {
            probes[probe_len] = probe;
            probe_len += 1;
        }
    }
    let plan_value = serde_json::to_value(PlanJson {
        primary_objective: plan.primary_objective,
        difficulty: plan.difficulty,
        max_new_items: plan.max_new_items,
        priorities: adaptation::plan_priorities(&plan),
        diagnostic_probes: &probes[..probe_len],
        success_criteria: adaptation::plan_success_criteria(&plan),
        recent_signals: plan.recent_signals,
    })?;
    let plan_json = plan_value.to_string();
    let tutor_tasks = super::adaptive_selection::selection_tutor_tasks(&selection, 16);
    let input_json = serde_json::to_string(&planner_input)?;

    let lesson_id = crate::api::new_id("les");
    transaction.execute(
        lessons::INSERT_LESSON,
        params![
            lesson_id,
            profile_id,
            next_lesson_number,
            "active",
            scenario,
            planned_goal,
            source,
            at,
            Option::<String>::None,
            Option::<u32>::None,
            Option::<String>::None,
            Option::<String>::None,
            Option::<String>::None,
            at,
        ],
    )?;
    transaction.execute(
        "INSERT INTO planner_decisions
         (id,learner_language_profile_id,lesson_id,planner_version,
          input_json,plan_json,task_candidates_json,capability_state_json,
          focus_threads_json,outcome_json,created_at,updated_at)
         VALUES (?1,?2,?3,'planner-v3',?4,?5,?6,?7,?8,NULL,?9,?9)",
        params![
            crate::api::new_id("pd"),
            profile_id,
            lesson_id,
            input_json,
            plan_json,
            candidate_json,
            capability_json,
            focus_json,
            at,
        ],
    )?;

    transaction.commit()?;
    crate::json(&serde_json::json!({
        "id": lesson_id,
        "lesson_number": next_lesson_number,
        "planner_version": "planner-v3",
        "adaptive_lesson_plan": plan_value,
        "tutor_tasks": tutor_tasks,
    }))
}
