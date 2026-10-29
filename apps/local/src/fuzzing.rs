//! Fuzz-only entry points that exercise private runtime components through stable invariants.
//!
//! Fail-fast assertions are intentional here: any unexpected storage/runtime error or broken
//! invariant must terminate the fuzz case and produce a reproducible artifact.
#![allow(clippy::expect_used, clippy::panic)]

use crate::{Env, Error};
use rusqlite::OptionalExtension as _;

fn no_storage_error<T>(result: crate::Result<T>) -> Option<T> {
    match result {
        Ok(value) => Some(value),
        Err(Error::Operation { .. }) => None,
        Err(Error::RustError(message)) => panic!("unexpected storage/runtime error: {message}"),
    }
}

fn minimal_lesson_result(
    profile_id: &str,
    pack_version: &str,
    operation_id: String,
    lesson_id: Option<String>,
    scenario: &str,
) -> tutor_contract::LessonResult {
    tutor_contract::LessonResult {
        schema_version: tutor_contract::SCHEMA_VERSION.to_owned(),
        client_operation_id: operation_id,
        language_profile_id: profile_id.to_owned(),
        language_pack_id: "es-ES".to_owned(),
        language_pack_version: pack_version.to_owned(),
        lesson: tutor_contract::LessonBlock {
            lesson_id,
            scenario: scenario.to_owned(),
            duration_seconds: Some(1),
            tutor_summary: None,
            next_goal: None,
        },
        metrics: None,
        new_learning_items: Vec::new(),
        evidence: Vec::new(),
        interventions: Vec::new(),
        error_occurrences: Vec::new(),
        listening_attempts: Vec::new(),
        mastery_changes: Vec::new(),
        card_mutations: Vec::new(),
    }
}

fn latest_open(env: &Env, profile_id: &str) -> Option<(String, u32)> {
    use rusqlite::{OptionalExtension as _, params};
    env.connection()
        .ok()?
        .query_row(
            crate::repositories::lessons::SELECT_LATEST_OPEN_LESSON,
            params![profile_id],
            |row| {
                Ok((
                    row.get::<_, String>("id")?,
                    row.get::<_, u32>("lesson_number")?,
                ))
            },
        )
        .optional()
        .ok()
        .flatten()
}

fn assert_lesson_invariants(env: &Env, registry: &crate::BuiltInPackRegistry, profile_id: &str) {
    let db = env.connection().expect("fuzz database must remain open");
    let integrity: String = db
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .expect("integrity check");
    assert_eq!(integrity, "ok");

    let mut fk = db
        .prepare("PRAGMA foreign_key_check")
        .expect("foreign key check");
    assert!(
        fk.query([])
            .expect("foreign key rows")
            .next()
            .expect("foreign key row")
            .is_none()
    );

    let open_count: u32 = db
        .query_row(
            "SELECT COUNT(*) FROM lessons
             WHERE learner_language_profile_id=?1
               AND status IN ('planned','active')",
            [profile_id],
            |row| row.get(0),
        )
        .expect("open lesson count");
    assert!(open_count <= 1, "more than one open lesson");

    let stored: u32 = db
        .query_row(
            "SELECT current_lesson_number
             FROM learner_language_profiles
             WHERE id=?1",
            [profile_id],
            |row| row.get(0),
        )
        .expect("stored counter");
    let max_number: u32 = db
        .query_row(
            "SELECT COALESCE(MAX(lesson_number),0)
             FROM lessons
             WHERE learner_language_profile_id=?1",
            [profile_id],
            |row| row.get(0),
        )
        .expect("max lesson number");
    let open_number: Option<u32> = db
        .query_row(
            "SELECT lesson_number
             FROM lessons
             WHERE learner_language_profile_id=?1
               AND status IN ('planned','active')
             ORDER BY started_at DESC, lesson_number DESC
             LIMIT 1",
            [profile_id],
            |row| row.get(0),
        )
        .optional()
        .expect("open lesson projection");
    let expected_next = open_number.unwrap_or_else(|| stored.max(max_number).saturating_add(1));
    drop(fk);
    drop(db);

    let next =
        crate::handlers::records::next_lesson_for(env, profile_id).expect("next lesson projection");
    assert_eq!(
        next["lesson_number"].as_u64(),
        Some(u64::from(expected_next))
    );

    let dashboard =
        crate::handlers::records::dashboard_for(env, profile_id).expect("dashboard projection");
    assert_eq!(
        dashboard["next_lesson_number"].as_u64(),
        Some(u64::from(expected_next))
    );

    let context = crate::handlers::records::tutor_context_for(registry, env, profile_id)
        .expect("tutor context projection");
    assert_eq!(
        context["next_lesson_number"].as_u64(),
        Some(u64::from(expected_next))
    );
}

/// Stateful lesson/profile/MCP fuzzing over a fresh SQLite database.
pub fn exercise_lesson_lifecycle(data: &[u8]) {
    use rusqlite::params;

    let path = std::env::temp_dir().join(format!(
        "lingua-lifecycle-fuzz-{}.sqlite3",
        uuid::Uuid::new_v4()
    ));
    crate::storage::open(&path).expect("initialize fuzz database");
    let env = Env::new(
        path.clone(),
        "fuzz-signing-key-012345678901234567890123".to_owned(),
    );
    let registry = crate::built_in_language_packs().expect("built-in pack registry");

    let created = crate::operations::profile_write::create_profile(&registry, &env, "es-ES", "ru")
        .expect("create fuzz profile");
    let profile_id = created["id"].as_str().expect("profile id").to_owned();
    let pack_version = language_pack_registry::manifest(&registry, "es-ES")
        .expect("manifest")
        .version
        .to_owned();

    seed_fuzz_learning_item(&env, &profile_id);
    let _ = crate::mcp::dispatch_jsonrpc(&env, &registry, data);

    for (step, byte) in data.iter().copied().take(32).enumerate() {
        match byte % 8 {
            0 => {
                let _ = no_storage_error(crate::operations::start_lesson::start_lesson(
                    &registry,
                    &env,
                    &profile_id,
                    "fuzz",
                    "goal",
                    "mcp",
                ));
            }
            1 => {
                let lesson_id = latest_open(&env, &profile_id).map(|row| row.0);
                let result = rich_lesson_result(
                    &profile_id,
                    &pack_version,
                    format!("fuzz-complete-{step}-{byte}"),
                    lesson_id,
                    0,
                );
                let _ = no_storage_error(crate::operations::complete_lesson::complete_lesson(
                    &registry, &env, &result,
                ));
            }
            2 => {
                env.connection()
                    .expect("counter connection")
                    .execute(
                        "UPDATE learner_language_profiles
                         SET current_lesson_number=?2
                         WHERE id=?1",
                        params![profile_id, u32::from(byte >> 4)],
                    )
                    .expect("inject stale counter");
            }
            3 => {
                if let Some((id, _)) = latest_open(&env, &profile_id) {
                    env.connection()
                        .expect("abandon connection")
                        .execute("UPDATE lessons SET status='abandoned' WHERE id=?1", [id])
                        .expect("abandon open lesson");
                }
            }
            4 => {
                let _ = crate::handlers::profiles::list_profiles(&env)
                    .expect("list profiles after arbitrary history");
                let _ = crate::handlers::lessons::list_lessons_for(&env, &profile_id)
                    .expect("list lessons after arbitrary history");
            }
            5 => {
                let first = no_storage_error(crate::operations::start_lesson::start_lesson(
                    &registry,
                    &env,
                    &profile_id,
                    "fuzz",
                    "goal-a",
                    "mcp",
                ));
                let second = no_storage_error(crate::operations::start_lesson::start_lesson(
                    &registry,
                    &env,
                    &profile_id,
                    "fuzz-other",
                    "goal-b",
                    "mcp",
                ));
                if let (Some(first), Some(second)) = (first, second) {
                    assert_eq!(first["id"], second["id"]);
                    assert_eq!(first["lesson_number"], second["lesson_number"]);
                }
            }
            6 => {
                let result = rich_lesson_result(
                    &profile_id,
                    &pack_version,
                    format!("fuzz-direct-{step}-{byte}"),
                    None,
                    0,
                );
                let _ = no_storage_error(crate::operations::complete_lesson::complete_lesson(
                    &registry, &env, &result,
                ));
            }
            _ => {
                let level = if byte & 0x80 == 0 { "A1" } else { "A2" };
                let _ = no_storage_error(crate::operations::profile_write::patch_profile(
                    &env,
                    &profile_id,
                    crate::operations::profile_write::PatchProfileInput {
                        estimated_level: Some(level),
                        status: None,
                        is_default: None,
                        preferences: None,
                    },
                ));
            }
        }
        assert_lesson_invariants(&env, &registry, &profile_id);
    }

    drop(env);
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(format!("{}-wal", path.display()));
    let _ = std::fs::remove_file(format!("{}-shm", path.display()));
}

/// Exercise tunnel wire decoding and retry policy on arbitrary bytes.
pub fn exercise_tunnel_wire(data: &[u8]) {
    for status in [200, 401, 403, 404, 408, 429, 500, 502, 503, 504] {
        let _ = crate::tunnel_policy::classify_post(status);
    }
    for value in [
        "",
        "0",
        "1",
        "Mon, 28 Oct 2030 12:00:00 GMT",
        "Mon, 28 Oct 2024 12:00:00 GMT",
    ] {
        let _ = crate::tunnel_policy::retry_after_ms(value, 1_800_000_000);
    }
    for value in [
        "1ns",
        "2us",
        "3ms",
        "4s",
        "5m",
        "6h",
        "-1s",
        "1x",
        "",
        "ñ",
        "999999999999999999999999999999999999999h",
    ] {
        let _ = crate::tunnel_wire::parse_response_timeout(&serde_json::json!(value));
    }
    for value in [
        format!("tunnel_{}", "a".repeat(32)),
        format!("tunnel_{}_{}", "a".repeat(32), "b".repeat(32)),
    ] {
        let _ = crate::tunnel_wire::valid_tunnel_id(&value);
    }
    let envelope = serde_json::json!({"commands":[{"request_id":"r","shard_token":"s","command_type":"jsonrpc","response_timeout":"1s","jsonrpc":{"id":1}}]});
    let bytes = serde_json::to_vec(&envelope).expect("poll fixture");
    let parsed = crate::tunnel_wire::parse_poll(&bytes).expect("poll");
    for raw in parsed.commands {
        let command = crate::tunnel_wire::parse_command(&raw).expect("command");
        let _ = crate::tunnel_wire::extract_jsonrpc_id(command.jsonrpc.as_deref().expect("rpc"));
    }

    if let Ok(envelope) = crate::tunnel_wire::parse_poll(data) {
        for raw in envelope.commands.iter().take(64) {
            if let Ok(command) = crate::tunnel_wire::parse_command(raw.as_ref()) {
                if let Some(timeout) = command.response_timeout.as_ref() {
                    let _ = crate::tunnel_wire::parse_response_timeout(timeout);
                }
                if let Some(jsonrpc) = command.jsonrpc.as_ref() {
                    let _ = crate::tunnel_wire::extract_jsonrpc_id(jsonrpc.as_ref());
                }
                let _ = crate::tunnel_wire::valid_tunnel_id(&command.request_id);
            }
        }
    }
    if let Ok(text) = std::str::from_utf8(data) {
        let _ = crate::tunnel_wire::valid_tunnel_id(text);
        let retry_after = crate::tunnel_policy::retry_after_ms(text, 1_800_000_000);
        let entropy = data
            .iter()
            .take(8)
            .fold(0_u64, |value, byte| (value << 8) | u64::from(*byte));
        let attempt = data.first().copied().map_or(0, u32::from);
        let _ = crate::tunnel_policy::classify_post(u16::from(data.first().copied().unwrap_or(0)));
        let _ = crate::tunnel_policy::backoff_base_ms(attempt);
        let _ = crate::tunnel_policy::jittered_backoff_ms(attempt, entropy);
        let _ = crate::tunnel_policy::retry_delay_ms(attempt, entropy, retry_after);
    }
}

/// Existing direct SQLite error-write fuzz pass.
pub fn exercise_error_writes(data: &[u8]) {
    crate::operations::error::fuzz_error_occurrence_sequence(data);
}

fn cleanup_fuzz_database(path: &std::path::Path) {
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(format!("{}-wal", path.display()));
    let _ = std::fs::remove_file(format!("{}-shm", path.display()));
}

fn seed_fuzz_learning_item(env: &Env, profile_id: &str) {
    let at = crate::repositories::to_rfc3339(crate::api::now());
    env.connection()
        .expect("seed learning item connection")
        .execute(
            "INSERT OR IGNORE INTO learning_items
             (id,learner_language_profile_id,language_pack_id,item_type,target_text,
              normalized_target_text,status,language_metadata_json,created_at,updated_at)
             VALUES ('li-fuzz',?1,'es-ES','phrase','quiero agua','quiero agua',
                     'learning','{}',?2,?2)",
            rusqlite::params![profile_id, at],
        )
        .expect("seed fuzz learning item");
}

fn rich_lesson_result(
    profile_id: &str,
    pack_version: &str,
    operation_id: String,
    lesson_id: Option<String>,
    seed: u8,
) -> tutor_contract::LessonResult {
    let mut result = minimal_lesson_result(
        profile_id,
        pack_version,
        operation_id,
        lesson_id,
        "fuzz-rich",
    );
    result.lesson.duration_seconds = Some(30 + u32::from(seed));
    result.lesson.tutor_summary = Some(format!("summary-{seed}"));
    result.lesson.next_goal = Some(format!("goal-{seed}"));
    result.metrics = Some(tutor_contract::MetricsBlock {
        speaking_seconds: 10 + u32::from(seed % 20),
        unaided_recall_count: u32::from(seed % 5),
        aided_recall_count: u32::from((seed / 2) % 4),
        listening_score: Some(f64::from(seed) / 255.0),
        avg_answer_millis: Some(300 + u32::from(seed) * 17),
        long_pause_count: u32::from(seed % 4),
        task_attempt_count: 2,
        task_success_count: u32::from(seed & 1),
        task_success_rubric: Some(tutor_contract::CommunicativeTaskRubric::GoalAchievementV1),
        confidence: Some(f64::from(255_u8.saturating_sub(seed)) / 255.0),
    });

    let item_key = format!("fuzz-new-{seed}");
    result
        .new_learning_items
        .push(tutor_contract::NewLearningItem {
            client_item_key: Some(item_key.clone()),
            item_type: learning_core::LearningItemType::Phrase,
            target_text: format!("frase nueva {seed}"),
            prompt_text: Some(format!("prompt {seed}")),
            prompt_language: Some("ru".to_owned()),
        });
    result.evidence.push(tutor_contract::EvidenceEntry {
        learning_item_id: Some("li-fuzz".to_owned()),
        client_item_key: None,
        context: Some("fuzz:conversation".to_owned()),
        context_family: None,
        kind: learning_core::EvidenceKind::SpontaneousProduction,
        success: seed & 1 == 0,
        assistance_level: Some(0),
        response_millis: Some(500 + u32::from(seed) * 11),
        probe_phase: Some(tutor_contract::ProbePhase::Diagnostic),
        pronunciation_intelligible: Some(seed & 2 == 0),
        pronunciation_repeat_required: Some(seed & 2 != 0),
        detail: Some("stateful fuzz evidence".to_owned()),
    });
    result.evidence.push(tutor_contract::EvidenceEntry {
        learning_item_id: None,
        client_item_key: Some(item_key.clone()),
        context: Some("fuzz:recognition".to_owned()),
        context_family: None,
        kind: learning_core::EvidenceKind::Recognition,
        success: true,
        assistance_level: Some(0),
        response_millis: Some(250 + u32::from(seed)),
        probe_phase: Some(tutor_contract::ProbePhase::Diagnostic),
        pronunciation_intelligible: None,
        pronunciation_repeat_required: None,
        detail: None,
    });
    result
        .interventions
        .push(tutor_contract::InterventionEvent {
            action: if seed & 4 == 0 {
                "fade_prompts".to_owned()
            } else {
                "spontaneous_retrieval".to_owned()
            },
            learning_item_id: Some("li-fuzz".to_owned()),
            client_item_key: None,
            target_capability: learning_core::EvidenceKind::SpontaneousProduction,
            evidence_index: Some(0),
            listening_attempt_index: None,
        });
    result
        .listening_attempts
        .push(tutor_contract::ListeningAttemptEntry {
            learning_item_id: None,
            client_item_key: Some(item_key),
            success: seed & 8 == 0,
            assistance_level: Some(seed % 4),
            response_millis: Some(400 + u32::from(seed) * 7),
            probe_phase: Some(tutor_contract::ProbePhase::DelayedRetrieval),
            source_reference: Some(format!("audio-{seed}")),
            source_variant_id: Some(format!("variant-{}", seed % 3)),
            talker_key: Some(format!("talker-{}", seed % 2)),
            context_tag: Some("fuzz:listening".to_owned()),
            transcript: Some(format!("frase nueva {seed}")),
        });
    result
        .error_occurrences
        .push(tutor_contract::ErrorOccurrenceEntry {
            error_target_id: None,
            core_category: learning_core::CoreErrorCategory::Grammar,
            communicative_impact: match seed % 3 {
                0 => learning_core::CommunicativeImpact::Low,
                1 => learning_core::CommunicativeImpact::Medium,
                _ => learning_core::CommunicativeImpact::High,
            },
            pack_error_code: None,
            original_production: format!("error {seed}"),
            corrected_production: format!("correcto {seed}"),
            explanation: Some("fuzz grammar repair".to_owned()),
        });
    result.mastery_changes.push(tutor_contract::MasteryChange {
        learning_item_id: "li-fuzz".to_owned(),
        new_status: if seed & 16 == 0 {
            learning_core::LearningItemStatus::Learning
        } else {
            learning_core::LearningItemStatus::TransferPending
        },
    });

    let payload = tutor_contract::CardPayload {
        intent: if seed & 32 == 0 {
            language_pack_api::CardIntent::Production
        } else {
            language_pack_api::CardIntent::Listening
        },
        prompt: format!("card prompt {seed}"),
        answer: format!("card answer {seed}"),
        accepted_answers: vec![format!("card alt {seed}")],
        prompt_language: Some("ru".to_owned()),
        explanation: Some("stateful fuzz card".to_owned()),
        scenario_tags: vec!["fuzz".to_owned()],
        skill_tags: vec!["production".to_owned()],
        source_learning_item_id: Some("li-fuzz".to_owned()),
        source_error_target_id: None,
        creation_reason: "stateful fuzz coverage".to_owned(),
        priority_score: Some(f64::from(seed) / 255.0),
    };
    result.card_mutations.push(tutor_contract::CardMutation {
        operation: tutor_contract::MutationOperation::Create,
        client_operation_id: format!("card-create-{seed}"),
        card_id: None,
        card: Some(payload.clone()),
        accepted_answer: None,
        reason: Some("exercise create path".to_owned()),
    });
    if seed & 64 != 0 {
        result.card_mutations.push(tutor_contract::CardMutation {
            operation: tutor_contract::MutationOperation::Create,
            client_operation_id: format!("card-duplicate-{seed}"),
            card_id: None,
            card: Some(payload),
            accepted_answer: None,
            reason: Some("exercise duplicate path".to_owned()),
        });
    }
    result
}

fn first_card_id(env: &Env, profile_id: &str) -> Option<String> {
    env.connection()
        .ok()?
        .query_row(
            "SELECT id FROM cards WHERE learner_language_profile_id=?1 ORDER BY created_at,id LIMIT 1",
            [profile_id],
            |row| row.get(0),
        )
        .optional()
        .ok()
        .flatten()
}

fn schedule_hash(env: &Env, card_id: &str) -> Option<String> {
    env.connection()
        .ok()?
        .query_row(
            "SELECT state_hash FROM card_schedule_state WHERE card_id=?1",
            [card_id],
            |row| row.get(0),
        )
        .optional()
        .ok()
        .flatten()
}

fn assert_database_integrity(env: &Env) {
    let db = env.connection().expect("fuzz database must remain open");
    let integrity: String = db
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .expect("integrity check");
    assert_eq!(integrity, "ok");
    let mut fk = db
        .prepare("PRAGMA foreign_key_check")
        .expect("foreign key check");
    assert!(
        fk.query([])
            .expect("foreign key rows")
            .next()
            .expect("foreign key row")
            .is_none()
    );
}

/// Rich LessonResult state machine covering evidence, errors, mastery, cards and completion writes.
pub fn exercise_lesson_result_stateful(data: &[u8]) {
    let path = std::env::temp_dir().join(format!(
        "lingua-result-stateful-fuzz-{}.sqlite3",
        uuid::Uuid::new_v4()
    ));
    crate::storage::open(&path).expect("initialize result fuzz database");
    let env = Env::new(
        path.clone(),
        "fuzz-signing-key-012345678901234567890123".to_owned(),
    );
    let registry = crate::built_in_language_packs().expect("built-in pack registry");
    let created = crate::operations::profile_write::create_profile(&registry, &env, "es-ES", "ru")
        .expect("create fuzz profile");
    let profile_id = created["id"].as_str().expect("profile id").to_owned();
    let pack_version = language_pack_registry::manifest(&registry, "es-ES")
        .expect("manifest")
        .version
        .to_owned();
    seed_fuzz_learning_item(&env, &profile_id);

    let source = if data.is_empty() { &[0_u8][..] } else { data };
    for (step, seed) in source.iter().copied().take(8).enumerate() {
        if seed & 1 != 0 {
            let _ = no_storage_error(crate::operations::start_lesson::start_lesson(
                &registry,
                &env,
                &profile_id,
                "fuzz-rich",
                "exercise-completion",
                "mcp",
            ));
        }
        let lesson_id = latest_open(&env, &profile_id).map(|row| row.0);
        let mut result = rich_lesson_result(
            &profile_id,
            &pack_version,
            format!("rich-complete-{step}-{seed}"),
            lesson_id,
            seed,
        );
        deepen_lesson_result(&env, &profile_id, &mut result, seed);
        let completed = no_storage_error(crate::operations::complete_lesson::complete_lesson(
            &registry, &env, &result,
        ));
        if completed.is_some() {
            let repeat =
                crate::operations::complete_lesson::complete_lesson(&registry, &env, &result)
                    .expect("idempotent lesson completion");
            assert_eq!(repeat["status"], "duplicate");
        }
        let _ = crate::handlers::lessons::list_lessons_for(&env, &profile_id)
            .expect("lesson list after rich completion");
        let _ = crate::handlers::records::dashboard_for(&env, &profile_id)
            .expect("dashboard after rich completion");
        assert_lesson_invariants(&env, &registry, &profile_id);
        assert_database_integrity(&env);
    }

    drop(env);
    cleanup_fuzz_database(&path);
}

fn card_mutation_result(
    profile_id: &str,
    pack_version: &str,
    operation_id: String,
    card_id: &str,
    seed: u8,
) -> tutor_contract::LessonResult {
    let mut result = rich_lesson_result(profile_id, pack_version, operation_id, None, 0);
    result.card_mutations.clear();
    let (operation, accepted_answer, reason) = match seed % 6 {
        0 => (
            tutor_contract::MutationOperation::Suspend,
            None,
            Some("pause".to_owned()),
        ),
        1 => (
            tutor_contract::MutationOperation::Restore,
            None,
            Some("restore".to_owned()),
        ),
        2 => (
            tutor_contract::MutationOperation::AddAcceptedAnswer,
            Some(format!("accepted-{seed}")),
            Some("variant".to_owned()),
        ),
        3 => (
            tutor_contract::MutationOperation::RemoveAcceptedAnswer,
            Some(format!("accepted-{}", seed.saturating_sub(1))),
            Some("remove variant".to_owned()),
        ),
        4 => (
            tutor_contract::MutationOperation::Retire,
            None,
            Some("retire".to_owned()),
        ),
        _ => (
            tutor_contract::MutationOperation::Link,
            None,
            Some("unsupported".to_owned()),
        ),
    };
    result.card_mutations.push(tutor_contract::CardMutation {
        operation,
        client_operation_id: format!("card-state-{seed}"),
        card_id: Some(card_id.to_owned()),
        card: None,
        accepted_answer,
        reason,
    });
    result
}

/// Card mutation/direct-edit lifecycle over persisted cards.
pub fn exercise_card_lifecycle(data: &[u8]) {
    let path = std::env::temp_dir().join(format!(
        "lingua-card-lifecycle-fuzz-{}.sqlite3",
        uuid::Uuid::new_v4()
    ));
    crate::storage::open(&path).expect("initialize card fuzz database");
    let env = Env::new(
        path.clone(),
        "fuzz-signing-key-012345678901234567890123".to_owned(),
    );
    let registry = crate::built_in_language_packs().expect("built-in pack registry");
    let created = crate::operations::profile_write::create_profile(&registry, &env, "es-ES", "ru")
        .expect("create fuzz profile");
    let profile_id = created["id"].as_str().expect("profile id").to_owned();
    let pack_version = language_pack_registry::manifest(&registry, "es-ES")
        .expect("manifest")
        .version
        .to_owned();
    seed_fuzz_learning_item(&env, &profile_id);

    let bootstrap = rich_lesson_result(
        &profile_id,
        &pack_version,
        "card-bootstrap".to_owned(),
        None,
        0,
    );
    crate::operations::complete_lesson::complete_lesson(&registry, &env, &bootstrap)
        .expect("card fuzz bootstrap must complete");
    let Some(card_id) = first_card_id(&env, &profile_id) else {
        drop(env);
        cleanup_fuzz_database(&path);
        return;
    };

    let source = if data.is_empty() { &[0_u8][..] } else { data };
    for (step, seed) in source.iter().copied().take(16).enumerate() {
        match seed % 7 {
            0 => {
                let accepted = vec![format!("alt-{step}"), format!("unicode-ñ-{}", seed % 5)];
                let _ = no_storage_error(crate::operations::card_direct::patch_card(
                    &env,
                    &card_id,
                    crate::operations::card_direct::CardPatchInput {
                        edit_class: "cosmetic",
                        prompt: Some("fuzz cosmetic prompt"),
                        answer: None,
                        explanation: Some("fuzz explanation"),
                        pronunciation_note: Some("fuzz pronunciation"),
                        accepted_answers: Some(&accepted),
                    },
                ));
            }
            1 => {
                let answer = format!("semantic-answer-{step}");
                let _ = no_storage_error(crate::operations::card_direct::patch_card(
                    &env,
                    &card_id,
                    crate::operations::card_direct::CardPatchInput {
                        edit_class: "semantic",
                        prompt: None,
                        answer: Some(&answer),
                        explanation: None,
                        pronunciation_note: None,
                        accepted_answers: None,
                    },
                ));
            }
            2 => {
                let _ = no_storage_error(crate::operations::card_direct::set_card_status(
                    &env,
                    &card_id,
                    learning_core::CardStatus::Suspended,
                    Some("fuzz pause"),
                ));
            }
            3 => {
                let _ = no_storage_error(crate::operations::card_direct::set_card_status(
                    &env,
                    &card_id,
                    learning_core::CardStatus::Review,
                    Some("fuzz resume"),
                ));
            }
            _ => {
                let result = card_mutation_result(
                    &profile_id,
                    &pack_version,
                    format!("card-complete-{step}-{seed}"),
                    &card_id,
                    seed,
                );
                let _ = no_storage_error(crate::operations::complete_lesson::complete_lesson(
                    &registry, &env, &result,
                ));
            }
        }
        let _ = crate::handlers::cards::list_for(&env, &profile_id, None, None)
            .expect("card list after arbitrary history");
        assert_database_integrity(&env);
    }

    drop(env);
    cleanup_fuzz_database(&path);
}

/// Review queue/grade/undo/complete lifecycle with scheduler-state invariants.
pub fn exercise_review_lifecycle(data: &[u8]) {
    let path = std::env::temp_dir().join(format!(
        "lingua-review-lifecycle-fuzz-{}.sqlite3",
        uuid::Uuid::new_v4()
    ));
    crate::storage::open(&path).expect("initialize review fuzz database");
    let env = Env::new(
        path.clone(),
        "fuzz-signing-key-012345678901234567890123".to_owned(),
    );
    let registry = crate::built_in_language_packs().expect("built-in pack registry");
    let created = crate::operations::profile_write::create_profile(&registry, &env, "es-ES", "ru")
        .expect("create fuzz profile");
    let profile_id = created["id"].as_str().expect("profile id").to_owned();
    let pack_version = language_pack_registry::manifest(&registry, "es-ES")
        .expect("manifest")
        .version
        .to_owned();
    seed_fuzz_learning_item(&env, &profile_id);

    let bootstrap = rich_lesson_result(
        &profile_id,
        &pack_version,
        "review-bootstrap".to_owned(),
        None,
        0,
    );
    crate::operations::complete_lesson::complete_lesson(&registry, &env, &bootstrap)
        .expect("review fuzz bootstrap must complete");
    let Some(card_id) = first_card_id(&env, &profile_id) else {
        drop(env);
        cleanup_fuzz_database(&path);
        return;
    };

    let session = crate::operations::review_ops::create_session(&env, &profile_id, "online")
        .expect("create review session");
    let mut session_id = session["id"]
        .as_str()
        .expect("review session id")
        .to_owned();
    let _ = no_storage_error(crate::operations::review_queue::queue(&env, &session_id));

    let source = if data.is_empty() { &[0_u8][..] } else { data };
    for (step, seed) in source.iter().copied().take(20).enumerate() {
        match seed % 8 {
            0 => {
                let _ = no_storage_error(crate::operations::review_queue::queue(&env, &session_id));
            }
            1 | 2 => {
                if let Some(prior) = schedule_hash(&env, &card_id) {
                    let at = crate::repositories::to_rfc3339(learning_core::Timestamp(
                        crate::api::now().0.saturating_add(step as i64 + 1),
                    ));
                    let operation = format!("review-{step}-{seed}");
                    let input = crate::operations::review_ops::ReviewEventInputData {
                        client_operation_id: &operation,
                        card_id: &card_id,
                        grade: 1 + seed % 4,
                        presented_at: &at,
                        answered_at: &at,
                        response_millis: 100 + u32::from(seed) * 37,
                        answer_revealed: seed & 1 != 0,
                        learner_note: (seed & 2 != 0).then_some("fuzz note"),
                        prior_state_hash: &prior,
                        source: "online",
                    };
                    let _ = no_storage_error(crate::operations::review_ops::submit_event(
                        &env,
                        &session_id,
                        input,
                    ));
                }
            }
            3 => {
                if let Some(prior) = schedule_hash(&env, &card_id) {
                    let at = crate::repositories::to_rfc3339(learning_core::Timestamp(
                        crate::api::now().0.saturating_add(step as i64 + 1),
                    ));
                    let operation = format!("review-duplicate-{step}");
                    let make = || crate::operations::review_ops::ReviewEventInputData {
                        client_operation_id: &operation,
                        card_id: &card_id,
                        grade: 3,
                        presented_at: &at,
                        answered_at: &at,
                        response_millis: 500,
                        answer_revealed: false,
                        learner_note: None,
                        prior_state_hash: &prior,
                        source: "online",
                    };
                    let _ = no_storage_error(crate::operations::review_ops::submit_event(
                        &env,
                        &session_id,
                        make(),
                    ));
                    let _ = no_storage_error(crate::operations::review_ops::submit_event(
                        &env,
                        &session_id,
                        make(),
                    ));
                }
            }
            4 => {
                if schedule_hash(&env, &card_id).is_some() {
                    let at = crate::repositories::to_rfc3339(learning_core::Timestamp(
                        crate::api::now().0.saturating_add(step as i64 + 1),
                    ));
                    let operation = format!("review-stale-{step}");
                    let _ = no_storage_error(crate::operations::review_ops::submit_event(
                        &env,
                        &session_id,
                        crate::operations::review_ops::ReviewEventInputData {
                            client_operation_id: &operation,
                            card_id: &card_id,
                            grade: 2,
                            presented_at: &at,
                            answered_at: &at,
                            response_millis: 900,
                            answer_revealed: true,
                            learner_note: None,
                            prior_state_hash: "intentionally-stale",
                            source: "offline",
                        },
                    ));
                }
            }
            5 => {
                let _ =
                    no_storage_error(crate::operations::review_ops::undo_last(&env, &session_id));
            }
            6 => {
                let _ = crate::handlers::review::summary_for(&env, &profile_id)
                    .expect("review summary");
                let _ = crate::handlers::review::forecast_for(&env, &profile_id, 7)
                    .expect("review forecast");
                let _ = crate::handlers::review::retention_for(&env, &profile_id, 14)
                    .expect("review retention");
            }
            _ => {
                let _ = no_storage_error(crate::operations::review_ops::complete_session(
                    &env,
                    &session_id,
                    u32::from(seed % 10),
                    u32::from(seed) * 3,
                ));
                if let Some(value) = no_storage_error(
                    crate::operations::review_ops::create_session(&env, &profile_id, "online"),
                ) && let Some(id) = value["id"].as_str()
                {
                    session_id = id.to_owned();
                }
            }
        }
        assert_database_integrity(&env);
    }

    drop(env);
    cleanup_fuzz_database(&path);
}

/// Pure adaptive-model fuzzing for capability projection, focus, tasks, rules and planner.
pub fn exercise_adaptation_model(data: &[u8]) {
    calibration::exercise(data.first().copied().unwrap_or(0));
    use adaptation::CapabilityKind;

    let source = if data.is_empty() { &[0_u8][..] } else { data };
    let len = source
        .len()
        .min(adaptation::MAX_CAPABILITY_OBSERVATIONS.saturating_mul(2))
        .max(1);
    let mut capability = Vec::with_capacity(len);
    let mut success = Vec::with_capacity(len);
    let mut assistance_level = Vec::with_capacity(len);
    let mut assistance_present = Vec::with_capacity(len);
    let mut response_millis = Vec::with_capacity(len);
    let mut response_present = Vec::with_capacity(len);
    let mut context_key = Vec::with_capacity(len);
    let mut context_family = Vec::with_capacity(len);
    let mut recorded_at = Vec::with_capacity(len);
    const CONTEXTS: [&str; 4] = ["home", "school", "cafe", "transport"];
    const FAMILIES: [&str; 4] = [
        "home.request",
        "school.answer",
        "cafe.order",
        "transport.ask",
    ];

    for index in 0..len {
        let byte = source[index % source.len()];
        capability.push(match byte % 5 {
            0 => CapabilityKind::Recognition,
            1 => CapabilityKind::Listening,
            2 => CapabilityKind::PromptedProduction,
            3 => CapabilityKind::SpontaneousProduction,
            _ => CapabilityKind::Transfer,
        });
        success.push(byte & 1 == 0);
        assistance_level.push((byte >> 1) % 4);
        assistance_present.push(byte & 0x20 == 0);
        response_millis.push(200 + u32::from(byte) * 50);
        response_present.push(byte & 0x40 == 0);
        context_key.push((byte & 2 == 0).then_some(CONTEXTS[index % CONTEXTS.len()]));
        context_family.push((byte & 4 == 0).then_some(FAMILIES[index % FAMILIES.len()]));
        recorded_at.push(1_800_000_000_i64.saturating_sub((index as i64) * 43_200));
    }

    let status = match source[0] % 7 {
        0 => "candidate",
        1 => "introduced",
        2 => "learning",
        3 => "transfer_pending",
        4 => "provisionally_mastered",
        5 => "mastered",
        _ => "suspended",
    };
    let state = adaptation::project_item_capabilities_columns(
        "li-fuzz",
        "quiero pedir agua",
        status,
        len,
        &capability,
        &success,
        &assistance_level,
        &assistance_present,
        &response_millis,
        &response_present,
        &context_key,
        &context_family,
        &recorded_at,
        1_800_000_000,
    );
    let item = adaptation::item_capability_view(&state);
    let objective = match source[0] % 4 {
        0 => "reduce_prompt_dependence",
        1 => "strengthen_listening",
        2 => "build_automaticity",
        _ => "diagnose_and_expand",
    };
    adaptation::visit_task_candidates(item, objective, |candidate| {
        assert!(candidate.score <= 100);
        assert!(!candidate.task_type.is_empty());
        assert!(!candidate.expected_evidence.is_empty());
    });
    adaptation::visit_tutor_focus_decisions(item, 1_800_000_000, |decision| {
        assert!(decision.stage <= 4);
        let _ = adaptation::format_focus_policy(
            decision.capability,
            decision.estimate.successful_contexts,
            decision.estimate.successful_context_families,
        );
        let _ = adaptation::format_focus_rationale(
            decision.capability,
            decision.estimate.weighted_success_rate,
            decision.estimate.evidence_strength,
            decision.stage,
            decision.estimate.distinct_success_contexts,
            decision.estimate.distinct_success_families,
        );
    });
    adaptation::visit_tutor_focus_threads(item, 1_800_000_000, |thread| {
        assert!(thread.stage <= 4);
        assert!(!thread.next_context_policy.is_empty());
        assert!(!thread.rationale.is_empty());
    });

    let ratio = |offset: usize| -> Option<f64> {
        source
            .get(offset % source.len())
            .map(|value| f64::from(*value) / 255.0)
    };
    let scenarios = ["pharmacy", "transport"];
    let planner_input = adaptation::LessonPlannerInputView {
        due_reviews: u32::from(source[0]),
        overdue_reviews: u32::from(*source.get(1 % source.len()).unwrap_or(&0)),
        due_error_rechecks: u32::from(*source.get(2 % source.len()).unwrap_or(&0) % 8),
        active_errors: u32::from(*source.get(3 % source.len()).unwrap_or(&0) % 8),
        method_change_errors: u32::from(*source.get(4 % source.len()).unwrap_or(&0) % 3),
        open_recommendations: u32::from(*source.get(5 % source.len()).unwrap_or(&0) % 10),
        new_card_budget: *source.get(6 % source.len()).unwrap_or(&0) % 10,
        recent: adaptation::RecentLearningSignals {
            lessons: *source.get(7 % source.len()).unwrap_or(&0) % 12,
            speaking_ratio: ratio(8),
            unaided_recall_rate: ratio(9),
            listening_score: ratio(10),
            avg_answer_millis: Some(
                250 + u32::from(*source.get(11 % source.len()).unwrap_or(&0)) * 60,
            ),
            communicative_task_success_rate: ratio(12),
            confidence: ratio(13),
        },
        preferences: adaptation::PlannerPreferencesView {
            max_new_items_per_lesson: Some(*source.get(14 % source.len()).unwrap_or(&0) % 8),
            preferred_difficulty: Some(match source[0] % 3 {
                0 => "easy",
                1 => "balanced",
                _ => "stretch",
            }),
            priority_scenarios: &scenarios,
        },
    };
    let plan = adaptation::plan_next_lesson_fixed(&planner_input);
    assert!(plan.max_new_items <= 5);
    let priorities = adaptation::plan_priorities(&plan);
    assert!(!priorities.is_empty());
    assert!(
        priorities
            .windows(2)
            .all(|pair| pair[0].score >= pair[1].score)
    );
    for index in 0..=adaptation::plan_diagnostic_probe_count(&plan) {
        let _ = adaptation::plan_diagnostic_probe(&plan, index);
    }
    assert!(!adaptation::plan_success_criteria(&plan).is_empty());

    let grades: Vec<u8> = source
        .iter()
        .take(adaptation::GRADE_WINDOW)
        .map(|b| 1 + b % 4)
        .collect();
    adaptation::visit_card_rules(
        adaptation::CardSignalView {
            card_id: "card-fuzz",
            learning_item_id: Some("li-fuzz"),
            recent_grades: &grades,
            avg_response_millis: Some(500 + u32::from(source[0]) * 100),
            spontaneous_failures: u32::from(source[0] % 5),
            spontaneous_successes: u32::from(source[0].rotate_left(1) % 5),
            prompted_successes: u32::from(source[0].rotate_left(2) % 5),
            listening_failures: u32::from(source[0].rotate_left(3) % 5),
            review_count: u32::from(source[0]),
        },
        |recommendation| {
            assert!(!recommendation.action.is_empty());
            assert!(!recommendation.subject_id.is_empty());
        },
    );
    let _ = adaptation::error_escalation_action(u32::from(source[0] % 8));
    let _ = adaptation::recheck_failed_fields(u32::from(source[0] % 30));
    let _ = adaptation::new_card_budget(
        source[0] % 12,
        source[0].rotate_left(1) % 12,
        u32::from(source[0]),
        12,
        Some((
            adaptation::DailySignal {
                review_minutes: u16::from(source[0] % 30),
                recall_rate: f64::from(source[0]) / 255.0,
            },
            adaptation::DailySignal {
                review_minutes: u16::from(source[0].rotate_left(1) % 30),
                recall_rate: f64::from(source[0].rotate_left(1)) / 255.0,
            },
        )),
    );

    for evidence in [
        "recognition",
        "listening",
        "prompted_production",
        "spontaneous_production",
        "transfer",
        "unknown",
    ] {
        if let Some(kind) = adaptation::evidence_capability(evidence) {
            assert!(!adaptation::default_action(kind).is_empty());
            assert!(adaptation::default_focus_score(kind) <= 100);
            assert!(!adaptation::criterion(kind).is_empty());
        }
    }

    const BEFORE: &str = r#"[{"learning_item_id":"li-1","recognition":{},"listening":{},"prompted_production":{},"spontaneous_production":{"unaided_success_rate":0.4,"last_observed_at_unix":10},"transfer":{}}]"#;
    const AFTER: &str = r#"[{"learning_item_id":"li-1","recognition":{},"listening":{},"prompted_production":{},"spontaneous_production":{"unaided_success_rate":0.8,"last_observed_at_unix":30},"transfer":{}}]"#;
    const OUTCOME: &str = r#"{"calibration_valid":true,"interventions":[{"action":"fade_prompts","learning_item_id":"li-1","target_capability":"spontaneous_production"}]}"#;
    let second = if source[0] & 0x80 == 0 { AFTER } else { "{" };
    let effects = adaptation::summarize_intervention_history(
        [(0_i64, BEFORE, OUTCOME), (30_000_i64, second, "{}")],
        i64::from(source[0] % 30) * 1_000,
    );
    for effect in effects {
        assert!(effect.samples >= effect.improved.saturating_add(effect.declined));
    }
}

/// Standalone maintenance/write APIs that are intentionally outside lesson completion.
pub fn exercise_maintenance_writes(data: &[u8]) {
    let path = std::env::temp_dir().join(format!(
        "lingua-maintenance-fuzz-{}.sqlite3",
        uuid::Uuid::new_v4()
    ));
    crate::storage::open(&path).expect("initialize maintenance fuzz database");
    let env = Env::new(
        path.clone(),
        "fuzz-signing-key-012345678901234567890123".to_owned(),
    );
    let registry = crate::built_in_language_packs().expect("built-in pack registry");
    let created = crate::operations::profile_write::create_profile(&registry, &env, "es-ES", "ru")
        .expect("create fuzz profile");
    let profile_id = created["id"].as_str().expect("profile id").to_owned();
    let pack_version = language_pack_registry::manifest(&registry, "es-ES")
        .expect("manifest")
        .version
        .to_owned();
    seed_fuzz_learning_item(&env, &profile_id);

    let bootstrap = rich_lesson_result(
        &profile_id,
        &pack_version,
        "maintenance-bootstrap".to_owned(),
        None,
        0,
    );
    crate::operations::complete_lesson::complete_lesson(&registry, &env, &bootstrap)
        .expect("maintenance fuzz bootstrap must complete");

    let db = env.connection().expect("maintenance lookup connection");
    let lesson_id: String = db
        .query_row(
            "SELECT id FROM lessons WHERE learner_language_profile_id=?1 AND status='completed'
             ORDER BY lesson_number DESC LIMIT 1",
            [&profile_id],
            |row| row.get(0),
        )
        .expect("completed lesson id");
    drop(db);
    let card_id = first_card_id(&env, &profile_id).expect("maintenance bootstrap card");

    let source = if data.is_empty() { &[0_u8][..] } else { data };
    for (step, seed) in source.iter().copied().take(12).enumerate() {
        let duration = if seed & 1 == 0 {
            Some(i64::from(seed) * 120)
        } else {
            Some(100_000)
        };
        let _ = no_storage_error(crate::operations::lesson_correction::correct_lesson_record(
            &env,
            if seed & 0x80 == 0 {
                &lesson_id
            } else {
                "missing-lesson"
            },
            Some("fuzz corrected summary"),
            Some("fuzz corrected next goal"),
            duration,
        ));

        let error_input = crate::operations::log_error::LogErrorOccurrenceInput {
            language_profile_id: profile_id.clone(),
            core_category: match seed % 4 {
                0 => learning_core::CoreErrorCategory::Grammar,
                1 => learning_core::CoreErrorCategory::Vocabulary,
                2 => learning_core::CoreErrorCategory::Pronunciation,
                _ => learning_core::CoreErrorCategory::Other,
            },
            original_production: format!("fuzz original {step}"),
            corrected_production: format!("fuzz corrected {step}"),
            pack_error_code: None,
            explanation: Some("standalone fuzz error".to_owned()),
            error_target_id: None,
            communicative_impact: match seed % 3 {
                0 => learning_core::CommunicativeImpact::Low,
                1 => learning_core::CommunicativeImpact::Medium,
                _ => learning_core::CommunicativeImpact::High,
            },
        };
        let target_id =
            crate::operations::log_error::log_error_occurrence(&registry, &env, error_input)
                .expect("new standalone error target");

        if seed & 2 != 0 {
            for repeat in 0..3 {
                let _ = no_storage_error(crate::operations::log_error::log_error_occurrence(
                    &registry,
                    &env,
                    crate::operations::log_error::LogErrorOccurrenceInput {
                        language_profile_id: profile_id.clone(),
                        core_category: learning_core::CoreErrorCategory::Grammar,
                        original_production: format!("repeat {step}-{repeat}"),
                        corrected_production: "corrected repeat".to_owned(),
                        pack_error_code: None,
                        explanation: None,
                        error_target_id: Some(target_id.clone()),
                        communicative_impact: learning_core::CommunicativeImpact::High,
                    },
                ));
            }
        }

        let rechecks: Vec<String> = {
            let db = env.connection().expect("recheck lookup");
            let mut statement = db
                .prepare(
                    "SELECT id FROM error_rechecks WHERE error_target_id=?1
                     ORDER BY offset_days,id LIMIT 2",
                )
                .expect("prepare recheck lookup");
            statement
                .query_map([&target_id], |row| row.get(0))
                .expect("query rechecks")
                .collect::<rusqlite::Result<Vec<_>>>()
                .expect("collect rechecks")
        };
        for (index, recheck_id) in rechecks.iter().enumerate() {
            let _ = no_storage_error(crate::operations::records_write::complete_error_recheck(
                &env,
                &target_id,
                recheck_id,
                index == 1 && seed & 4 != 0,
            ));
        }

        let context = format!("maintenance-context-{}", seed % 5);
        let _ = no_storage_error(crate::operations::records_write::create_listening_attempt(
            &env,
            crate::operations::records_write::ListeningAttemptInput {
                language_profile_id: &profile_id,
                learning_item_id: Some("li-fuzz"),
                lesson_id: Some(&lesson_id),
                source_reference: Some("maintenance-audio"),
                source_variant_id: Some("variant-a"),
                talker_key: Some("talker-a"),
                context_tag: &context,
                transcript: Some("quiero agua"),
                success: seed & 8 == 0,
                assistance_level: seed % 4,
                response_millis: 100 + u32::from(seed) * 100,
                probe_phase: if seed & 16 == 0 {
                    tutor_contract::ProbePhase::Practice
                } else {
                    tutor_contract::ProbePhase::Transfer
                },
            },
        ));
        if seed & 32 != 0 {
            let _ = no_storage_error(crate::operations::records_write::create_listening_attempt(
                &env,
                crate::operations::records_write::ListeningAttemptInput {
                    language_profile_id: &profile_id,
                    learning_item_id: None,
                    lesson_id: None,
                    source_reference: None,
                    source_variant_id: None,
                    talker_key: None,
                    context_tag: "",
                    transcript: None,
                    success: false,
                    assistance_level: 4,
                    response_millis: 600_001,
                    probe_phase: tutor_contract::ProbePhase::Practice,
                },
            ));
        }

        let at = crate::repositories::to_rfc3339(crate::api::now());
        {
            let db = env.connection().expect("recommendation insert");
            crate::operations::recommendation_write::insert_recommendation(
                &db,
                &profile_id,
                "card",
                Some(&card_id),
                "redesign_card",
                &serde_json::json!({"reason":"maintenance fuzz"}).to_string(),
                &at,
            )
            .expect("insert redesign recommendation");
            crate::operations::recommendation_write::insert_recommendation(
                &db,
                &profile_id,
                "card",
                Some(&card_id),
                "redesign_card",
                &serde_json::json!({"reason":"duplicate"}).to_string(),
                &at,
            )
            .expect("dedupe redesign recommendation");
        }
        let _ = crate::handlers::recommendations::list_for(&env, &profile_id)
            .expect("list recommendations");
        let _ = crate::handlers::recommendations::redesign_candidates_for(&env, &profile_id)
            .expect("redesign candidates");

        let recommendation_id: Option<String> = env
            .connection()
            .expect("recommendation lookup")
            .query_row(
                "SELECT id FROM recommendations
                 WHERE learner_language_profile_id=?1 AND status='open'
                 ORDER BY created_at,id LIMIT 1",
                [&profile_id],
                |row| row.get(0),
            )
            .optional()
            .expect("recommendation query");
        if let Some(recommendation_id) = recommendation_id {
            let status = if seed & 64 == 0 {
                "accepted"
            } else {
                "dismissed"
            };
            let _ = no_storage_error(crate::operations::recommendation_ops::resolve(
                &env,
                &recommendation_id,
                status,
            ));
        }

        let standalone_payload = tutor_contract::CardPayload {
            intent: language_pack_api::CardIntent::Production,
            prompt: format!("standalone prompt {step}-{seed}"),
            answer: format!("standalone answer {step}-{seed}"),
            accepted_answers: vec![format!("standalone alt {step}")],
            prompt_language: Some("ru".to_owned()),
            explanation: Some("standalone mutation fuzz".to_owned()),
            scenario_tags: vec!["maintenance".to_owned()],
            skill_tags: vec!["production".to_owned()],
            source_learning_item_id: Some("li-fuzz".to_owned()),
            source_error_target_id: None,
            creation_reason: "standalone maintenance fuzz".to_owned(),
            priority_score: Some(f64::from(seed) / 255.0),
        };
        let mutations = [
            tutor_contract::CardMutation {
                operation: tutor_contract::MutationOperation::Create,
                client_operation_id: format!("standalone-create-{step}-{seed}"),
                card_id: None,
                card: Some(standalone_payload.clone()),
                accepted_answer: None,
                reason: Some("standalone create".to_owned()),
            },
            tutor_contract::CardMutation {
                operation: tutor_contract::MutationOperation::Update,
                client_operation_id: format!("standalone-update-{step}-{seed}"),
                card_id: Some(card_id.clone()),
                card: Some(tutor_contract::CardPayload {
                    prompt: format!("updated bootstrap {step}"),
                    answer: format!("updated answer {step}"),
                    ..standalone_payload
                }),
                accepted_answer: None,
                reason: Some("standalone update".to_owned()),
            },
        ];
        let _ = no_storage_error(crate::operations::upsert_cards::upsert_cards(
            &registry,
            &env,
            &profile_id,
            &mutations,
        ));

        let ids = if seed & 1 == 0 {
            serde_json::json!([card_id.clone()])
        } else {
            serde_json::json!([])
        };
        let _ = no_storage_error(crate::operations::suspend_cards::suspend_cards_json(
            &env,
            &ids,
            Some("maintenance fuzz pause"),
        ));

        assert_database_integrity(&env);
    }

    drop(env);
    cleanup_fuzz_database(&path);
}

mod contract;
pub use contract::exercise_contract;
mod card_model;
pub use card_model::exercise_card_model;
mod boundaries;
pub use boundaries::exercise_boundaries;
mod lesson_variants;
use lesson_variants::deepen_lesson_result;
/// Exercise temporary historical databases and verified backups.
pub fn exercise_storage(data: &[u8]) {
    crate::storage::fuzz_storage(data);
}

/// Exercise the tunnel against an isolated local peer.
pub fn exercise_tunnel_transport(data: &[u8]) {
    let dir = std::env::temp_dir().join(format!("tunnel-peer-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).expect("directory");
    let path = dir.join("db.sqlite3");
    crate::storage::open(&path).expect("database");
    let env = Env::new(path, "fuzz-key".into());
    crate::tunnel::fuzz_transport(&env, data);
    drop(env);
    std::fs::remove_dir_all(dir).expect("cleanup");
}
mod calibration;
mod review_states;
pub use review_states::exercise_review_states;

#[cfg(test)]
mod harness_tests {
    #[test]
    fn structured_fuzz_fixtures_reach_real_runtime() {
        for seed in 0..16 {
            super::exercise_contract(&[seed]);
            super::exercise_card_model(&[seed]);
            super::exercise_boundaries(&[seed]);
            super::exercise_review_states(&[seed]);
        }
    }
}
