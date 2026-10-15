//! Lesson-completion planner tests (WBS 4.4): success + failure paths,
//! idempotency, dedup, escalation, server-side mastery re-check.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use application::{
    ErrorAction, ItemRef, LessonCompletionContext, LessonOutcome, PlanError, plan_lesson_completion,
};
use learning_core::{
    CoreErrorCategory, ErrorTarget, ErrorTargetId, ErrorTargetStatus, EvidenceId, EvidenceKind,
    LanguagePackId, LanguageProfileId, LanguageTag, LearnerId, LearnerLanguageProfile,
    LearningItem, LearningItemEvidence, LearningItemId, LearningItemStatus, LearningItemType,
    Lesson, LessonId, LessonSource, LessonStatus, ProfileStatus, Timestamp,
};
use tutor_contract::parse_lesson_result;

const NOW: Timestamp = Timestamp(1_780_000_000);

fn profile() -> LearnerLanguageProfile {
    LearnerLanguageProfile {
        id: LanguageProfileId("llp-1".to_owned()),
        learner_id: LearnerId("learner-1".to_owned()),
        language_pack_id: LanguagePackId("es-ES".to_owned()),
        source_language: LanguageTag("ru".to_owned()),
        target_locale: LanguageTag("es-ES".to_owned()),
        estimated_level: Some("A2".to_owned()),
        current_lesson_number: 7,
        status: ProfileStatus::Active,
        is_default: true,
        preferences: serde_json::json!({}),
    }
}

fn base_result_json() -> serde_json::Value {
    serde_json::json!({
      "schema_version": "1",
      "client_operation_id": "op-1",
      "language_profile_id": "llp-1",
      "language_pack_id": "es-ES",
      "language_pack_version": "0.1.0",
      "lesson": { "scenario": "pharmacy" },
      "new_learning_items": [
        { "item_type": "phrase", "target_text": "¿Tiene algo para la tos?" },
        { "item_type": "phrase", "target_text": "Buenos días" }
      ],
      "evidence": [
        { "learning_item_id": "li-known", "kind": "spontaneous_production",
          "success": true, "assistance_level": 0, "context": "pharmacy:request" }
      ],
      "error_occurrences": [
        { "error_target_id": "et-1", "core_category": "grammar",
          "original_production": "estoy médico", "corrected_production": "soy médico" },
        { "core_category": "vocabulary",
          "original_production": "x", "corrected_production": "y" }
      ],
      "mastery_changes": [
        { "learning_item_id": "li-known", "new_status": "provisionally_mastered" }
      ]
    })
}

fn known_item(id: &str, normalized: &str) -> LearningItem {
    LearningItem {
        id: LearningItemId(id.to_owned()),
        language_profile_id: LanguageProfileId("llp-1".to_owned()),
        language_pack_id: LanguagePackId("es-ES".to_owned()),
        item_type: LearningItemType::Phrase,
        target_text: normalized.to_owned(),
        normalized_target_text: normalized.to_owned(),
        prompt_text: None,
        prompt_language: None,
        status: LearningItemStatus::Learning,
        language_metadata: serde_json::json!({}),
        introduced_lesson_id: None,
    }
}

fn target(id: &str, count: u32) -> ErrorTarget {
    ErrorTarget {
        id: ErrorTargetId(id.to_owned()),
        language_profile_id: LanguageProfileId("llp-1".to_owned()),
        language_pack_id: LanguagePackId("es-ES".to_owned()),
        language_pack_version: semver::Version::new(0, 1, 0),
        core_category: CoreErrorCategory::Grammar,
        pack_error_code: None,
        original_production: "estoy médico".to_owned(),
        corrected_production: "soy médico".to_owned(),
        explanation: None,
        occurrence_count: count,
        method_change_required: false,
        status: ErrorTargetStatus::Active,
    }
}

fn spontaneous(item: &str, lesson: &str) -> LearningItemEvidence {
    LearningItemEvidence {
        id: EvidenceId(format!("ev-{lesson}")),
        learning_item_id: LearningItemId(item.to_owned()),
        lesson_id: Some(learning_core::LessonId(lesson.to_owned())),
        kind: EvidenceKind::SpontaneousProduction,
        success: true,
        detail: None,
        pack_metadata: Some(serde_json::json!({
            "context_key": "baseline",
            "assistance_level": 0
        })),
        recorded_at: NOW,
    }
}

fn open_lesson(id: &str) -> Lesson {
    Lesson {
        id: LessonId(id.to_owned()),
        language_profile_id: LanguageProfileId("llp-1".to_owned()),
        language_pack_id: LanguagePackId("es-ES".to_owned()),
        language_pack_version: semver::Version::new(0, 1, 0),
        lesson_number: 8,
        status: LessonStatus::Active,
        scenario: "pharmacy".to_owned(),
        planned_goal: String::new(),
        source: LessonSource::Mcp,
        started_at: Some(NOW),
        completed_at: None,
        duration_seconds: None,
        tutor_summary: None,
        next_goal: None,
        idempotency_key: None,
    }
}

fn ctx() -> LessonCompletionContext {
    LessonCompletionContext {
        profile: profile(),
        now: NOW,
        existing_lesson: None,
        duplicate_import: false,
        items_by_normalized: vec![known_item("li-dup", "buenos días")],
        normalized_new_items: vec![
            "¿tiene algo para la tos?".to_owned(),
            "buenos días".to_owned(),
        ],
        known_item_ids: vec![LearningItemId("li-known".to_owned())],
        error_targets: vec![target("et-1", 2)],
        evidence_for_mastery: vec![(
            LearningItemId("li-known".to_owned()),
            vec![spontaneous("li-known", "l1")],
        )],
    }
}

#[test]
fn plan_creates_lesson_dedups_items_and_escalates_errors() {
    let result = parse_lesson_result(&base_result_json().to_string()).unwrap();
    let plan = plan_lesson_completion(&result, &ctx()).unwrap();

    assert_eq!(
        plan.lesson,
        LessonOutcome::CreateCompleted { lesson_number: 8 }
    );
    // Item 0 is new; item 1 matches existing normalized text → reused.
    assert_eq!(plan.new_items.len(), 1);
    assert_eq!(plan.new_items[0].source_index, 0);
    assert_eq!(
        plan.reused_items,
        vec![(1, LearningItemId("li-dup".to_owned()))]
    );
    // Evidence resolves to the known item.
    assert_eq!(
        plan.evidence[0].item,
        ItemRef::Existing(LearningItemId("li-known".to_owned()))
    );
    // Known target 2 → 3: escalation, no method change yet; second occurrence is new.
    assert_eq!(
        plan.error_actions[0],
        ErrorAction::Register {
            source_index: 0,
            target_id: ErrorTargetId("et-1".to_owned()),
            new_count: 3,
            escalate: true,
            method_change: false,
        }
    );
    assert_eq!(
        plan.error_actions[1],
        ErrorAction::NewTarget { source_index: 1 }
    );
    // Mastery proposal accepted: evidence trail has spontaneous production.
    assert_eq!(
        plan.item_status_changes,
        vec![(
            LearningItemId("li-known".to_owned()),
            LearningItemStatus::ProvisionallyMastered
        )]
    );
    assert!(plan.warnings.is_empty());
}

#[test]
fn completion_without_lesson_id_attaches_to_the_open_lesson_not_a_new_one() {
    // `base_result_json()`'s `lesson` object carries no `lesson_id` — the
    // common MCP shape (the tutor doesn't echo back what `start_lesson`
    // returned). Before this fix, that always took the `CreateCompleted`
    // branch and computed `lesson_number = current_lesson_number + 1`,
    // colliding (UNIQUE constraint) with the `active` row `start_lesson`
    // already reserved that exact number for. It must instead resolve to
    // the profile's open lesson.
    let result = parse_lesson_result(&base_result_json().to_string()).unwrap();
    assert!(result.lesson.lesson_id.is_none());
    let mut context = ctx();
    context.existing_lesson = Some(open_lesson("les-open"));
    let plan = plan_lesson_completion(&result, &context).unwrap();
    assert_eq!(
        plan.lesson,
        LessonOutcome::CompleteExisting(LessonId("les-open".to_owned()))
    );
}

#[test]
fn duplicate_import_is_noop() {
    let result = parse_lesson_result(&base_result_json().to_string()).unwrap();
    let mut context = ctx();
    context.duplicate_import = true;
    let plan = plan_lesson_completion(&result, &context).unwrap();
    assert_eq!(plan.lesson, LessonOutcome::DuplicateNoop);
    assert!(plan.new_items.is_empty() && plan.error_actions.is_empty());
}

#[test]
fn unknown_evidence_reference_rejects_whole_import() {
    let mut json = base_result_json();
    json["evidence"][0]["learning_item_id"] = serde_json::json!("li-ghost");
    let result = parse_lesson_result(&json.to_string()).unwrap();
    assert_eq!(
        plan_lesson_completion(&result, &ctx()),
        Err(PlanError::UnknownLearningItem("li-ghost".to_owned()))
    );
}

#[test]
fn profile_and_pack_mismatch_rejected() {
    let mut json = base_result_json();
    json["language_profile_id"] = serde_json::json!("llp-other");
    let result = parse_lesson_result(&json.to_string()).unwrap();
    assert!(matches!(
        plan_lesson_completion(&result, &ctx()),
        Err(PlanError::ProfileMismatch(_))
    ));

    let mut json = base_result_json();
    json["language_pack_id"] = serde_json::json!("fr-FR");
    let result = parse_lesson_result(&json.to_string()).unwrap();
    assert!(matches!(
        plan_lesson_completion(&result, &ctx()),
        Err(PlanError::PackMismatch(_))
    ));
}

#[test]
fn unsupported_mastery_proposal_downgraded_with_warning() {
    let mut json = base_result_json();
    json["mastery_changes"][0]["new_status"] = serde_json::json!("mastered");
    let result = parse_lesson_result(&json.to_string()).unwrap();
    // Trail has one context — full mastery requires three.
    let plan = plan_lesson_completion(&result, &ctx()).unwrap();
    assert!(plan.item_status_changes.is_empty());
    assert_eq!(plan.warnings.len(), 1);
}

#[test]
fn method_change_at_five_unresolved_occurrences() {
    let result = parse_lesson_result(&base_result_json().to_string()).unwrap();
    let mut context = ctx();
    context.error_targets = vec![target("et-1", 4)];
    let plan = plan_lesson_completion(&result, &context).unwrap();
    match &plan.error_actions[0] {
        ErrorAction::Register {
            new_count,
            escalate,
            method_change,
            ..
        } => {
            assert_eq!(*new_count, 5);
            assert!(*escalate && *method_change);
        }
        other => panic!("expected Register, got {other:?}"),
    }
}

#[test]
fn referenced_lesson_must_exist() {
    let mut json = base_result_json();
    json["lesson"]["lesson_id"] = serde_json::json!("les-ghost");
    let result = parse_lesson_result(&json.to_string()).unwrap();
    assert_eq!(
        plan_lesson_completion(&result, &ctx()),
        Err(PlanError::UnknownLesson("les-ghost".to_owned()))
    );
}

#[test]
fn same_lesson_evidence_resolves_new_item_key() {
    let mut json = base_result_json();
    json["new_learning_items"][0]["client_item_key"] = serde_json::json!("new-cough");
    json["evidence"] = serde_json::json!([
        {
            "client_item_key": "new-cough",
            "kind": "spontaneous_production",
            "success": true,
            "context": "pharmacy:ask-for-remedy"
        }
    ]);
    json["mastery_changes"] = serde_json::json!([]);
    let result = parse_lesson_result(&json.to_string()).unwrap();
    let plan = plan_lesson_completion(&result, &ctx()).unwrap();
    assert_eq!(plan.evidence.len(), 1);
    assert_eq!(plan.evidence[0].item, ItemRef::New(0));
}

#[test]
fn current_lesson_evidence_can_complete_context_mastery() {
    let mut context = ctx();
    context.evidence_for_mastery = vec![(
        LearningItemId("li-known".to_owned()),
        vec![
            LearningItemEvidence {
                id: EvidenceId("ev-old-1".to_owned()),
                learning_item_id: LearningItemId("li-known".to_owned()),
                lesson_id: Some(LessonId("l-old-1".to_owned())),
                kind: EvidenceKind::Transfer,
                success: true,
                detail: None,
                pack_metadata: Some(serde_json::json!({
                    "context_key": "bus:route1",
                    "context_family": "transport.ask_stop",
                    "assistance_level": 0
                })),
                recorded_at: NOW,
            },
            LearningItemEvidence {
                id: EvidenceId("ev-old-2".to_owned()),
                learning_item_id: LearningItemId("li-known".to_owned()),
                lesson_id: Some(LessonId("l-old-2".to_owned())),
                kind: EvidenceKind::Transfer,
                success: true,
                detail: None,
                pack_metadata: Some(serde_json::json!({
                    "context_key": "bakery:item",
                    "context_family": "commerce.request_item",
                    "assistance_level": 0
                })),
                recorded_at: NOW,
            },
        ],
    )];
    let mut json = base_result_json();
    json["evidence"] = serde_json::json!([
        {
            "learning_item_id": "li-known",
            "kind": "transfer",
            "success": true,
            "context": "pharmacy:help",
            "context_family": "service.ask_help",
            "assistance_level": 0,
            "probe_phase": "transfer"
        }
    ]);
    json["mastery_changes"][0]["new_status"] = serde_json::json!("mastered");
    let result = parse_lesson_result(&json.to_string()).unwrap();
    let plan = plan_lesson_completion(&result, &context).unwrap();
    assert_eq!(
        plan.item_status_changes,
        vec![(
            LearningItemId("li-known".to_owned()),
            LearningItemStatus::Mastered
        )]
    );
}
