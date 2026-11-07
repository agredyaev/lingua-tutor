//! Fixture tests: the 07-tutor-contract.md §2 lesson-result JSON parses and
//! validates; malformed and oversized inputs are rejected without panic.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use tutor_contract::{MutationOperation, bounds, parse_lesson_result, validate};

fn spec_fixture() -> String {
    // 07-tutor-contract.md §2 example, concretized (core_category uses the
    // core closed set; language-specific specificity belongs in error_code
    // lives in error_code).
    serde_json::json!({
      "schema_version": "2",
      "client_operation_id": "op_01J5",
      "language_profile_id": "llp_01J5",
      "lesson": {
        "lesson_id": "les_01J5",
        "scenario": "pharmacy",
        "duration_seconds": 1260,
        "tutor_summary": "Practiced pharmacy dialogue.",
        "next_goal": "Ask about dosage."
      },
      "metrics": { "speaking_seconds": 540, "unaided_recall_count": 6,
                   "aided_recall_count": 3, "listening_score": 0.7,
                   "avg_answer_millis": 2100, "long_pause_count": 2, "confidence": 0.6 },
      "new_learning_items": [ { "item_type": "phrase", "target_text": "¿Tiene algo para la tos?",
                                "prompt_text": "Ask for something for a cough", "prompt_language": "en" } ],
      "evidence": [
        { "learning_item_id": "phr_01J5", "kind": "spontaneous_production",
          "success": true, "context": "pharmacy:ask-remedy",
          "assistance_level": 0, "response_millis": 1800,
          "probe_phase": "diagnostic",
          "pronunciation_intelligible": true,
          "pronunciation_repeat_required": false, "detail": null },
        { "learning_item_id": "phr_01J5", "kind": "spontaneous_production",
          "success": true, "context": "pharmacy:post-fade",
          "assistance_level": 0, "response_millis": 1500,
          "probe_phase": "practice",
          "pronunciation_intelligible": true,
          "pronunciation_repeat_required": false, "detail": null }
      ],
      "interventions": [ { "action": "fade_prompts", "learning_item_id": "phr_01J5",
                           "target_capability": "spontaneous_production",
                           "evidence_index": 1 } ],
      "error_occurrences": [ { "error_target_id": null,
                               "core_category": "grammar", "error_code": "es.verb.ser_estar",
                               "original_production": "estoy médico",
                               "corrected_production": "soy médico",
                               "explanation": "Profession takes ser." } ],
      "listening_attempts": [ { "learning_item_id": "phr_01J5", "success": false,
                                "assistance_level": 0, "response_millis": 1600,
                                "probe_phase": "diagnostic", "context_tag": "pharmacy:question",
                                "source_reference": "lesson-audio-3", "transcript": "¿tiene algo…?" } ],
      "mastery_changes": [ { "learning_item_id": "phr_01J5", "new_status": "provisionally_mastered" } ],
      "card_mutations": [ { "operation": "create", "client_operation_id": "op_01J5x",
        "card": {
          "intent": "production",
          "prompt": "You need to ask whether there is an appointment available this week.",
          "answer": "¿Hay alguna cita disponible para esta semana?",
          "accepted_answers": ["¿Hay alguna cita libre para esta semana?"],
          "prompt_language": "en",
          "explanation": "Use hay alguna + noun + disponible/libre.",
          "scenario_tags": ["medical", "appointment"],
          "skill_tags": ["question", "availability"],
          "source_learning_item_id": "phr_01J5",
          "source_error_target_id": null,
          "creation_reason": "Failed unaided production twice",
          "priority_score": 0.91
        } } ]
    })
    .to_string()
}

#[test]
fn duplicate_mastery_change_for_same_item_is_rejected() {
    let mut result = parse_lesson_result(&spec_fixture()).unwrap();
    let mut duplicate = result.mastery_changes[0].clone();
    duplicate.new_status = learning_core::LearningItemStatus::Suspended;
    result.mastery_changes.push(duplicate);
    let issues = validate(&result);
    assert!(issues.iter().any(|issue| {
        issue.code == "duplicate_reference" && issue.path == "mastery_changes/1/learning_item_id"
    }));
}

#[test]
fn mastery_change_cannot_demote_existing_item_to_candidate() {
    let mut result = parse_lesson_result(&spec_fixture()).unwrap();
    result.mastery_changes[0].new_status = learning_core::LearningItemStatus::Candidate;
    let issues = validate(&result);
    assert!(issues.iter().any(|issue| {
        issue.code == "invalid_status_transition" && issue.path == "mastery_changes/0/new_status"
    }));
}

#[test]
fn spec_example_parses_and_validates_clean() {
    let result = parse_lesson_result(&spec_fixture()).unwrap();
    assert_eq!(
        result.card_mutations[0].operation,
        MutationOperation::Create
    );
    assert_eq!(result.evidence.len(), 2);
    let issues = validate(&result);
    assert!(issues.is_empty(), "unexpected issues: {issues:?}");
}

#[test]
fn lesson_result_accepts_bounded_recommendation_resolutions() {
    let mut json: serde_json::Value = serde_json::from_str(&spec_fixture()).unwrap();
    json["recommendation_resolutions"] = serde_json::json!([
        { "recommendation_id": "rec-1", "status": "accepted" },
        { "recommendation_id": "rec-2", "status": "dismissed" }
    ]);
    let parsed = parse_lesson_result(&json.to_string());
    assert!(
        parsed.is_ok(),
        "recommendation lifecycle must be part of canonical lesson completion: {parsed:?}"
    );
    assert!(validate(&parsed.unwrap()).is_empty());
}

#[test]
fn duplicate_recommendation_resolution_is_rejected() {
    let mut json: serde_json::Value = serde_json::from_str(&spec_fixture()).unwrap();
    json["recommendation_resolutions"] = serde_json::json!([
        { "recommendation_id": "rec-1", "status": "accepted" },
        { "recommendation_id": "rec-1", "status": "dismissed" }
    ]);
    let result = parse_lesson_result(&json.to_string()).unwrap();
    let issues = validate(&result);
    assert!(issues.iter().any(|issue| {
        issue.path == "recommendation_resolutions/1/recommendation_id"
            && issue.code == "duplicate_reference"
    }));
}

#[test]
fn lesson_result_accepts_measured_error_recheck_results() {
    let mut json: serde_json::Value = serde_json::from_str(&spec_fixture()).unwrap();
    json["error_rechecks"] = serde_json::json!([
        {
            "error_target_id": "err-1",
            "recheck_id": "rc-1",
            "passed": true,
            "assistance_level": 0,
            "context": "pharmacy:recheck-ser-estar",
            "repair_applied": false
        }
    ]);
    let result = parse_lesson_result(&json.to_string()).unwrap();
    assert_eq!(result.error_rechecks.len(), 1);
    assert!(validate(&result).is_empty());
}

#[test]
fn assisted_or_contextless_error_recheck_is_rejected() {
    let mut json: serde_json::Value = serde_json::from_str(&spec_fixture()).unwrap();
    json["error_rechecks"] = serde_json::json!([
        {
            "error_target_id": "err-1",
            "recheck_id": "rc-1",
            "passed": true,
            "assistance_level": 1,
            "context": "",
            "repair_applied": false
        }
    ]);
    let result = parse_lesson_result(&json.to_string()).unwrap();
    let issues = validate(&result);
    assert!(issues.iter().any(|issue| {
        issue.path == "error_rechecks/0/assistance_level" && issue.code == "inconsistent_evidence"
    }));
    assert!(issues.iter().any(|issue| {
        issue.path == "error_rechecks/0/context" && issue.code == "missing_field"
    }));
}

#[test]
fn failed_error_recheck_requires_corrective_repair_before_new_cycle() {
    let mut json: serde_json::Value = serde_json::from_str(&spec_fixture()).unwrap();
    json["error_rechecks"] = serde_json::json!([
        {
            "error_target_id": "err-1",
            "recheck_id": "rc-1",
            "passed": false,
            "assistance_level": 0,
            "context": "pharmacy:recheck-ser-estar",
            "repair_applied": false
        }
    ]);
    let result = parse_lesson_result(&json.to_string()).unwrap();
    let issues = validate(&result);
    assert!(issues.iter().any(|issue| {
        issue.path == "error_rechecks/0/repair_applied" && issue.code == "missing_repair"
    }));
}

#[test]
fn failed_error_recheck_requires_named_repair_action() {
    let mut json: serde_json::Value = serde_json::from_str(&spec_fixture()).unwrap();
    json["error_rechecks"] = serde_json::json!([
        {
            "error_target_id": "err-1",
            "recheck_id": "rc-1",
            "passed": false,
            "assistance_level": 0,
            "context": "pharmacy:recheck-ser-estar",
            "repair_applied": true
        }
    ]);
    let result = parse_lesson_result(&json.to_string()).unwrap();
    let issues = validate(&result);
    assert!(issues.iter().any(|issue| {
        issue.path == "error_rechecks/0/repair_action" && issue.code == "missing_field"
    }));
}

#[test]
fn passed_error_recheck_cannot_claim_corrective_repair() {
    let mut json: serde_json::Value = serde_json::from_str(&spec_fixture()).unwrap();
    json["error_rechecks"] = serde_json::json!([
        {
            "error_target_id": "err-1",
            "recheck_id": "rc-1",
            "passed": true,
            "assistance_level": 0,
            "context": "pharmacy:recheck-ser-estar",
            "repair_applied": true,
            "repair_action": "contrast"
        }
    ]);
    let result = parse_lesson_result(&json.to_string()).unwrap();
    let issues = validate(&result);
    assert!(issues.iter().any(|issue| {
        issue.path == "error_rechecks/0/repair_applied" && issue.code == "inconsistent_repair"
    }));
}

#[test]
fn failed_error_recheck_accepts_named_repair_action() {
    let mut json: serde_json::Value = serde_json::from_str(&spec_fixture()).unwrap();
    json["error_rechecks"] = serde_json::json!([
        {
            "error_target_id": "err-1",
            "recheck_id": "rc-1",
            "passed": false,
            "assistance_level": 0,
            "context": "pharmacy:recheck-ser-estar",
            "repair_applied": true,
            "repair_action": "semantic_reframe"
        }
    ]);
    let result = parse_lesson_result(&json.to_string()).unwrap();
    let issues = validate(&result);
    assert!(
        issues.is_empty(),
        "named failed-recheck repair should validate: {issues:?}"
    );
}

#[test]
fn repair_action_requires_canonical_stable_identifier() {
    let mut json: serde_json::Value = serde_json::from_str(&spec_fixture()).unwrap();
    json["error_rechecks"] = serde_json::json!([
        {
            "error_target_id": "err-1",
            "recheck_id": "rc-1",
            "passed": false,
            "assistance_level": 0,
            "context": "pharmacy:recheck-ser-estar",
            "repair_applied": true,
            "repair_action": "Semantic Reframe!"
        }
    ]);
    json["recommendation_resolutions"] = serde_json::json!([
        {
            "recommendation_id": "rec-1",
            "status": "accepted",
            "repair_action": "Cue Fade!"
        }
    ]);

    let result = parse_lesson_result(&json.to_string()).unwrap();
    let issues = validate(&result);
    assert!(issues.iter().any(|issue| {
        issue.path == "error_rechecks/0/repair_action" && issue.code == "invalid_field"
    }));
    assert!(issues.iter().any(|issue| {
        issue.path == "recommendation_resolutions/0/repair_action" && issue.code == "invalid_field"
    }));
}

#[test]
fn intervention_immediate_outcome_cannot_be_diagnostic() {
    let mut result = parse_lesson_result(&spec_fixture()).unwrap();
    result.interventions[0].evidence_index = Some(0);
    let issues = validate(&result);
    assert!(
        issues.iter().any(|issue| {
            issue.path == "interventions/0/evidence_index"
                && issue.code == "inconsistent_intervention"
        }),
        "an intervention immediate outcome must be post-intervention, not the diagnostic baseline"
    );
}

#[test]
fn communicative_task_counts_require_known_success_rubric() {
    let mut result = parse_lesson_result(&spec_fixture()).unwrap();
    {
        let metrics = result.metrics.as_mut().unwrap();
        metrics.task_attempt_count = 2;
        metrics.task_success_count = 1;
        metrics.task_success_rubric = None;
    }
    let issues = validate(&result);
    assert!(issues.iter().any(|issue| {
        issue.path == "metrics/task_success_rubric" && issue.code == "missing_field"
    }));

    result.metrics.as_mut().unwrap().task_success_rubric =
        Some(tutor_contract::CommunicativeTaskRubric::GoalAchievementV1);
    let issues = validate(&result);
    assert!(
        !issues
            .iter()
            .any(|issue| issue.path == "metrics/task_success_rubric")
    );
}

#[test]
fn independent_task_success_rubric_is_versioned_and_backward_compatible() {
    assert_eq!(
        tutor_contract::communicative_task_rubric_id(
            tutor_contract::CommunicativeTaskRubric::GoalAchievementV1,
        ),
        tutor_contract::COMMUNICATIVE_TASK_SUCCESS_RUBRIC_V1,
    );
    assert_eq!(
        tutor_contract::communicative_task_rubric_id(
            tutor_contract::CommunicativeTaskRubric::IndependentGoalAchievementV2,
        ),
        tutor_contract::COMMUNICATIVE_TASK_SUCCESS_RUBRIC_V2,
    );

    let v2: tutor_contract::CommunicativeTaskRubric =
        serde_json::from_str(r#""goal_completed_independently_v2""#).unwrap();
    assert_eq!(
        v2,
        tutor_contract::CommunicativeTaskRubric::IndependentGoalAchievementV2
    );

    let v1: tutor_contract::CommunicativeTaskRubric =
        serde_json::from_str(r#""goal_completed_without_target_model_v1""#).unwrap();
    assert_eq!(
        v1,
        tutor_contract::CommunicativeTaskRubric::GoalAchievementV1
    );
}

#[test]
fn communicative_task_counts_require_explicit_rubric() {
    let mut result = parse_lesson_result(&spec_fixture()).unwrap();
    let metrics = result.metrics.as_mut().unwrap();
    metrics.task_attempt_count = 2;
    metrics.task_success_count = 1;
    let issues = validate(&result);
    assert!(issues.iter().any(|issue| {
        issue.code == "missing_field" && issue.path == "metrics/task_success_rubric"
    }));
}

#[test]
fn unknown_fields_rejected() {
    let json = spec_fixture().replace("\"schema_version\"", "\"evil_extra\":1,\"schema_version\"");
    assert!(parse_lesson_result(&json).is_err());
}

#[test]
fn wrong_schema_version_flagged() {
    let json = spec_fixture().replace("\"schema_version\":\"2\"", "\"schema_version\":\"9\"");
    let result = parse_lesson_result(&json).unwrap();
    let issues = validate(&result);
    assert!(
        issues
            .iter()
            .any(|i| i.code == "unsupported_schema_version")
    );
}

#[test]
fn create_without_card_flagged() {
    let json = spec_fixture().replace(
        "\"card\":",
        "\"reason\":\"broken create\",\"ignored_card\":",
    );
    // renaming "card" produces unknown field → parse error, so instead build
    // a create mutation without card directly:
    assert!(parse_lesson_result(&json).is_err());
    let mut result = parse_lesson_result(&spec_fixture()).unwrap();
    result.card_mutations[0].card = None;
    let issues = validate(&result);
    assert!(
        issues
            .iter()
            .any(|i| i.code == "missing_field" && i.path.ends_with("/card"))
    );
}

#[test]
fn suspend_requires_card_id() {
    let mut result = parse_lesson_result(&spec_fixture()).unwrap();
    result.card_mutations[0].operation = MutationOperation::Suspend;
    result.card_mutations[0].card_id = None;
    let issues = validate(&result);
    assert!(issues.iter().any(|i| i.path.ends_with("/card_id")));
}

#[test]
fn oversized_collections_and_texts_flagged() {
    let mut result = parse_lesson_result(&spec_fixture()).unwrap();
    let item = result.new_learning_items[0].clone();
    result.new_learning_items = vec![item; bounds::MAX_NEW_ITEMS + 1];
    result.new_learning_items[0].target_text = "x".repeat(bounds::MAX_SHORT_TEXT + 1);
    result.error_occurrences[0].original_production = "y".repeat(bounds::MAX_SHORT_TEXT + 1);
    let issues = validate(&result);
    assert!(
        issues
            .iter()
            .any(|i| i.code == "over_limit" && i.path == "new_learning_items")
    );
    assert!(issues.iter().filter(|i| i.code == "too_long").count() >= 2);
}

#[test]
fn out_of_range_scores_flagged() {
    let mut result = parse_lesson_result(&spec_fixture()).unwrap();
    if let Some(m) = &mut result.metrics {
        m.confidence = Some(1.5);
    }
    if let Some(card) = &mut result.card_mutations[0].card {
        card.priority_score = Some(-0.2);
    }
    let issues = validate(&result);
    assert!(issues.iter().any(|i| i.path == "metrics/confidence"));
    assert!(issues.iter().any(|i| i.path.ends_with("priority_score")));
}

#[test]
fn garbage_inputs_error_not_panic() {
    for garbage in [
        "",
        "{",
        "null",
        "[]",
        "42",
        "{\"schema_version\":1}",
        "\u{0}\u{1}",
    ] {
        let _ = parse_lesson_result(garbage); // must return Err, never panic
    }
}

#[test]
fn same_lesson_client_item_reference_validates() {
    let mut result = parse_lesson_result(&spec_fixture()).unwrap();
    result.new_learning_items[0].client_item_key = Some("new-cough".to_owned());
    for evidence in &mut result.evidence {
        evidence.learning_item_id = None;
        evidence.client_item_key = Some("new-cough".to_owned());
    }
    result.evidence[0].context = Some("pharmacy:ask-for-remedy".to_owned());
    result.interventions[0].learning_item_id = None;
    result.interventions[0].client_item_key = Some("new-cough".to_owned());
    let issues = validate(&result);
    assert!(issues.is_empty(), "unexpected issues: {issues:?}");
}

#[test]
fn unknown_same_lesson_client_item_reference_is_rejected() {
    let mut result = parse_lesson_result(&spec_fixture()).unwrap();
    result.new_learning_items[0].client_item_key = Some("new-cough".to_owned());
    result.evidence[0].learning_item_id = None;
    result.evidence[0].client_item_key = Some("missing".to_owned());
    let issues = validate(&result);
    assert!(issues.iter().any(
        |issue| issue.code == "unknown_reference" && issue.path == "evidence/0/client_item_key"
    ));
}

#[test]
fn transfer_requires_context_family() {
    let mut json: serde_json::Value = serde_json::from_str(&spec_fixture()).unwrap();
    json["evidence"][0]["kind"] = serde_json::json!("transfer");
    json["evidence"][0]["assistance_level"] = serde_json::json!(0);
    json["evidence"][0]["context"] = serde_json::json!("pharmacy:ask-help");
    let result = parse_lesson_result(&json.to_string()).unwrap();
    let issues = validate(&result);
    assert!(issues.iter().any(|issue| {
        issue.code == "missing_field" && issue.path == "evidence/0/context_family"
    }));
}

#[test]
fn transfer_context_family_requires_stable_identifier() {
    let mut json: serde_json::Value = serde_json::from_str(&spec_fixture()).unwrap();
    json["evidence"][0]["kind"] = serde_json::json!("transfer");
    json["evidence"][0]["assistance_level"] = serde_json::json!(0);
    json["evidence"][0]["probe_phase"] = serde_json::json!("transfer");
    json["evidence"][0]["context_family"] = serde_json::json!("Service.Ask_Help");
    let result = parse_lesson_result(&json.to_string()).unwrap();
    let issues = validate(&result);
    assert!(issues.iter().any(|issue| {
        issue.code == "invalid_field" && issue.path == "evidence/0/context_family"
    }));
}

#[test]
fn assisted_spontaneous_production_is_rejected() {
    let mut json: serde_json::Value = serde_json::from_str(&spec_fixture()).unwrap();
    json["evidence"][0]["assistance_level"] = serde_json::json!(2);
    let result = parse_lesson_result(&json.to_string()).unwrap();
    let issues = validate(&result);
    assert!(issues.iter().any(|issue| {
        issue.code == "inconsistent_evidence" && issue.path == "evidence/0/assistance_level"
    }));
}

#[test]
fn prompted_production_cannot_be_marked_unaided() {
    let mut json: serde_json::Value = serde_json::from_str(&spec_fixture()).unwrap();
    json["evidence"][0]["kind"] = serde_json::json!("prompted_production");
    json["evidence"][0]["assistance_level"] = serde_json::json!(0);
    let result = parse_lesson_result(&json.to_string()).unwrap();
    let issues = validate(&result);
    assert!(issues.iter().any(|issue| {
        issue.code == "inconsistent_evidence" && issue.path == "evidence/0/assistance_level"
    }));
}

#[test]
fn listening_attempt_requires_item_reference_and_audio_source() {
    let mut result = parse_lesson_result(&spec_fixture()).unwrap();
    result.listening_attempts[0].learning_item_id = None;
    result.listening_attempts[0].client_item_key = None;
    result.listening_attempts[0].source_reference = None;

    let issues = validate(&result);
    assert!(issues.iter().any(|issue| {
        issue.path == "listening_attempts/0" && issue.code == "invalid_reference"
    }));
    assert!(issues.iter().any(|issue| {
        issue.path == "listening_attempts/0/source_reference" && issue.code == "missing_field"
    }));
}

#[test]
fn listening_probe_requires_real_source_reference() {
    let mut result = parse_lesson_result(&spec_fixture()).unwrap();
    result.listening_attempts[0].source_reference = None;
    let issues = validate(&result);
    assert!(issues.iter().any(|issue| {
        issue.path == "listening_attempts/0/source_reference" && issue.code == "missing_field"
    }));
}

#[test]
fn response_latency_is_optional_when_not_instrumented() {
    let mut result = parse_lesson_result(&spec_fixture()).unwrap();
    result.evidence[0].response_millis = None;
    result.listening_attempts[0].response_millis = None;
    let issues = validate(&result);
    assert!(
        issues
            .iter()
            .all(|issue| !issue.path.ends_with("/response_millis")),
        "unobservable latency must not force the tutor to invent timing: {issues:?}"
    );
}

#[test]
fn correction_records_do_not_count_toward_measurement_coverage() {
    let mut json: serde_json::Value = serde_json::from_str(&spec_fixture()).unwrap();
    json["interventions"] = serde_json::json!([]);
    json["evidence"][0]["kind"] = serde_json::json!("correction");
    json["evidence"][0]["probe_phase"] = serde_json::json!("diagnostic");
    json["evidence"][1]["kind"] = serde_json::json!("correction");
    json["evidence"][1]["probe_phase"] = serde_json::json!("delayed_retrieval");
    json["listening_attempts"][0]["probe_phase"] = serde_json::json!("practice");
    let result = parse_lesson_result(&json.to_string()).unwrap();

    assert_eq!(
        tutor_contract::measurement_coverage_counts(&result),
        (0, 0),
        "correction audit records must not manufacture diagnostic or measurement coverage"
    );
}

#[test]
fn insufficient_measurement_coverage_does_not_invalidate_lesson() {
    let mut result = parse_lesson_result(&spec_fixture()).unwrap();
    result.listening_attempts[0].probe_phase = Some(tutor_contract::ProbePhase::Practice);
    let issues = validate(&result);
    assert!(
        issues
            .iter()
            .all(|issue| issue.code != "insufficient_measurement_coverage"),
        "coverage controls calibration eligibility, not whether the lesson may be persisted: {issues:?}"
    );
}

#[test]
fn one_diagnostic_plus_delayed_measurement_is_sufficient_coverage() {
    let mut result = parse_lesson_result(&spec_fixture()).unwrap();
    result.listening_attempts[0].probe_phase = Some(tutor_contract::ProbePhase::DelayedRetrieval);
    assert_eq!(tutor_contract::measurement_coverage_counts(&result), (1, 2));
}

#[test]
fn pronunciation_signals_are_optional_without_audio_observation() {
    let mut result = parse_lesson_result(&spec_fixture()).unwrap();
    result.evidence[0].pronunciation_intelligible = None;
    result.evidence[0].pronunciation_repeat_required = None;
    let issues = validate(&result);
    assert!(
        issues
            .iter()
            .all(|issue| !issue.path.contains("pronunciation_")),
        "text-only production must not force invented pronunciation evidence: {issues:?}"
    );
}

#[test]
fn pronunciation_signals_must_be_recorded_as_a_pair_when_observed() {
    let mut result = parse_lesson_result(&spec_fixture()).unwrap();
    result.evidence[0].pronunciation_intelligible = Some(true);
    result.evidence[0].pronunciation_repeat_required = None;
    let issues = validate(&result);
    assert!(issues.iter().any(|issue| {
        issue.path == "evidence/0/pronunciation_repeat_required"
            && issue.code == "incomplete_metric"
    }));
}
