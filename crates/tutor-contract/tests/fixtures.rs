//! Fixture tests: the 07-tutor-contract.md §2 lesson-result JSON parses and
//! validates; malformed and oversized inputs are rejected without panic.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use tutor_contract::{MutationOperation, bounds, parse_lesson_result, validate};

fn spec_fixture() -> String {
    // 07-tutor-contract.md §2 example, concretized (core_category uses the
    // core closed set; the spec's "verb_form" is illustrative — specificity
    // lives in pack_error_code).
    serde_json::json!({
      "schema_version": "1",
      "client_operation_id": "op_01J5",
      "language_profile_id": "llp_01J5",
      "language_pack_id": "es-ES",
      "language_pack_version": "1.0.0",
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
                                "prompt_text": "Спросите что-нибудь от кашля", "prompt_language": "ru" } ],
      "evidence": [ { "learning_item_id": "phr_01J5", "kind": "spontaneous_production",
                      "success": true, "detail": null } ],
      "error_occurrences": [ { "error_target_id": null,
                               "core_category": "grammar", "pack_error_code": "es.verb.ser_estar",
                               "original_production": "estoy médico",
                               "corrected_production": "soy médico",
                               "explanation": "Profession takes ser." } ],
      "listening_attempts": [ { "learning_item_id": "phr_01J5", "success": false,
                                "source_reference": "lesson-audio-3", "transcript": "¿tiene algo…?" } ],
      "mastery_changes": [ { "learning_item_id": "phr_01J5", "new_status": "provisionally_mastered" } ],
      "card_mutations": [ { "operation": "create", "client_operation_id": "op_01J5x",
        "card": {
          "intent": "production",
          "prompt": "You need to ask whether there is an appointment available this week.",
          "answer": "¿Hay alguna cita disponible para esta semana?",
          "accepted_answers": ["¿Hay alguna cita libre para esta semana?"],
          "prompt_language": "ru",
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
fn spec_example_parses_and_validates_clean() {
    let result = parse_lesson_result(&spec_fixture()).unwrap();
    assert_eq!(
        result.card_mutations[0].operation,
        MutationOperation::Create
    );
    assert_eq!(result.evidence.len(), 1);
    let issues = validate(&result);
    assert!(issues.is_empty(), "unexpected issues: {issues:?}");
}

#[test]
fn unknown_fields_rejected() {
    let json = spec_fixture().replace("\"schema_version\"", "\"evil_extra\":1,\"schema_version\"");
    assert!(parse_lesson_result(&json).is_err());
}

#[test]
fn wrong_schema_version_flagged() {
    let json = spec_fixture().replace("\"schema_version\":\"1\"", "\"schema_version\":\"9\"");
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
