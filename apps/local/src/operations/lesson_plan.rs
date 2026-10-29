#![allow(clippy::items_after_test_module)] // Test fixtures are intentionally grouped before the processing pass.
//! Fixed-capacity lesson-completion planner.
//!
//! Tests define behavior first. The implementation must not delegate to the
//! old Vec/domain-object planner.

#[cfg(test)]
// Test fixtures intentionally fail fast and use compact tuple snapshots.
#[allow(clippy::unwrap_used, clippy::type_complexity)]
mod tests {
    use super::*;

    fn base_result() -> tutor_contract::LessonResult {
        tutor_contract::parse_lesson_result(
            &serde_json::json!({
                "schema_version": "2",
                "client_operation_id": "op-1",
                "language_profile_id": "llp-1",
                "language_pack_id": "es-ES",
                "language_pack_version": "0.2.0",
                "lesson": { "scenario": "pharmacy" },
                "new_learning_items": [
                    { "item_type": "phrase", "target_text": "¿Tiene algo para la tos?" },
                    { "item_type": "phrase", "target_text": "Buenos días" }
                ],
                "evidence": [
                    { "learning_item_id": "li-known",
                      "kind": "spontaneous_production",
                      "success": true,
                      "assistance_level": 0,
                      "context": "pharmacy:request" }
                ],
                "error_occurrences": [
                    { "error_target_id": "et-1", "core_category": "grammar",
                      "original_production": "estoy médico",
                      "corrected_production": "soy médico" },
                    { "core_category": "vocabulary",
                      "original_production": "x", "corrected_production": "y" }
                ],
                "mastery_changes": [
                    { "learning_item_id": "li-known",
                      "new_status": "provisionally_mastered" }
                ]
            })
            .to_string(),
        )
        .unwrap()
    }

    fn state() -> LessonPlanState {
        let mut state = new_lesson_plan_state("llp-1", "es-ES", 7);
        set_lesson_normalized_item(&mut state, 0, "¿tiene algo para la tos?").unwrap();
        set_lesson_normalized_item(&mut state, 1, "buenos días").unwrap();
        add_lesson_existing_normalized(&mut state, "buenos días", "li-dup").unwrap();
        add_known_lesson_item(&mut state, "li-known").unwrap();
        add_lesson_error_target(&mut state, "et-1", 2, false).unwrap();
        add_lesson_mastery_signal(&mut state, "li-known", true, &[]).unwrap();
        state
    }

    #[test]
    fn basic_plan_matches_old_planner_semantics_without_vec_plan_state() {
        let result = base_result();
        let plan = plan_lesson_completion(&result, &state()).unwrap();

        assert_eq!(plan.lesson_kind, LessonPlanKind::CreateCompleted);
        assert_eq!(plan.lesson_number, 8);

        assert_eq!(plan.new_items_len, 1);
        assert_eq!(plan.new_item_source[0], 0);
        assert_eq!(
            lesson_normalized_new(&state(), slot(plan.new_item_source[0])),
            "¿tiene algo para la tos?"
        );

        assert_eq!(plan.reused_items_len, 1);
        assert_eq!(plan.reused_item_source[0], 1);
        assert_eq!(plan_reused_item_id(&plan, &state(), 0), "li-dup");

        assert_eq!(plan.evidence_len, 1);
        assert_eq!(plan.evidence_item[0], ResolvedItem::Known(0));
        assert_eq!(
            plan_resolved_item_id(&plan, &state(), plan.evidence_item[0]),
            "li-known"
        );

        assert_eq!(plan.error_len, 2);
        assert!(plan.error_target_present[0]);
        assert_eq!(
            result.error_occurrences[0].error_target_id.as_deref(),
            Some("et-1")
        );
        assert_eq!(plan.error_new_count[0], 3);
        assert!(plan.error_new_count[0] >= learning_core::ERROR_ESCALATION_THRESHOLD);
        assert!(!plan.error_method_change[0]);
        assert!(!plan.error_target_present[1]);

        assert_eq!(plan.status_len, 1);
        assert_eq!(
            result.mastery_changes[slot(plan.status_source[0])].learning_item_id,
            "li-known"
        );
        assert_eq!(
            result.mastery_changes[slot(plan.status_source[0])].new_status,
            learning_core::LearningItemStatus::ProvisionallyMastered
        );
    }

    #[test]
    fn duplicate_is_noop() {
        let result = base_result();
        let mut state = state();
        state.duplicate_import = true;
        let plan = plan_lesson_completion(&result, &state).unwrap();

        assert_eq!(plan.lesson_kind, LessonPlanKind::DuplicateNoop);
        assert_eq!(plan.new_items_len, 0);
        assert_eq!(plan.evidence_len, 0);
        assert_eq!(plan.error_len, 0);
        assert_eq!(plan.status_len, 0);
    }

    #[test]
    fn open_lesson_is_completed_instead_of_creating_a_new_one() {
        let result = base_result();
        let mut state = state();
        set_lesson_existing(&mut state, "lesson-open");
        let plan = plan_lesson_completion(&result, &state).unwrap();

        assert_eq!(plan.lesson_kind, LessonPlanKind::CompleteExisting);
        assert_eq!(lesson_existing_id(&state), "lesson-open");
    }

    #[test]
    fn unknown_existing_item_rejects_atomically() {
        let mut result = base_result();
        result.evidence[0].learning_item_id = Some("li-ghost".to_owned());
        assert_eq!(
            plan_lesson_completion(&result, &state()).unwrap_err(),
            LessonPlanError::UnknownLearningItem("li-ghost".to_owned())
        );
    }

    #[test]
    fn client_item_key_resolves_to_new_item_plan_index() {
        let mut result = base_result();
        result.new_learning_items[0].client_item_key = Some("new-cough".to_owned());
        result.evidence[0].learning_item_id = None;
        result.evidence[0].client_item_key = Some("new-cough".to_owned());
        result.mastery_changes.clear();

        let plan = plan_lesson_completion(&result, &state()).unwrap();
        assert_eq!(plan.evidence_len, 1);
        assert_eq!(plan.evidence_item[0], ResolvedItem::New(0));
    }

    #[test]
    fn canonical_error_thresholds_are_three_and_five() {
        let result = base_result();

        let mut state3 = state();
        replace_lesson_error_target_count(&mut state3, "et-1", 2).unwrap();
        let plan3 = plan_lesson_completion(&result, &state3).unwrap();
        assert_eq!(plan3.error_new_count[0], 3);
        assert!(plan3.error_new_count[0] >= learning_core::ERROR_ESCALATION_THRESHOLD);
        assert!(!plan3.error_method_change[0]);

        let mut state5 = state();
        replace_lesson_error_target_count(&mut state5, "et-1", 4).unwrap();
        let plan5 = plan_lesson_completion(&result, &state5).unwrap();
        assert_eq!(plan5.error_new_count[0], 5);
        assert!(plan5.error_new_count[0] >= learning_core::ERROR_ESCALATION_THRESHOLD);
        assert!(plan5.error_method_change[0]);
    }

    #[test]
    fn fresh_failed_transfer_blocks_full_mastery_even_with_three_historical_families() {
        let mut result = base_result();
        result.mastery_changes[0].new_status = learning_core::LearningItemStatus::Mastered;
        result.evidence.clear();
        result.evidence.push(tutor_contract::EvidenceEntry {
            learning_item_id: Some("li-known".to_owned()),
            client_item_key: None,
            context: Some("new-context".to_owned()),
            context_family: Some("service.ask_help".to_owned()),
            kind: learning_core::EvidenceKind::Transfer,
            success: false,
            assistance_level: Some(0),
            response_millis: Some(1_500),
            probe_phase: Some(tutor_contract::ProbePhase::Transfer),
            pronunciation_intelligible: Some(true),
            pronunciation_repeat_required: Some(false),
            detail: None,
        });

        let mut state = state();
        add_lesson_mastery_signal(
            &mut state,
            "li-known",
            true,
            &[
                "service.ask_help",
                "commerce.request_item",
                "transport.ask_stop",
            ],
        )
        .unwrap();

        let plan = plan_lesson_completion(&result, &state).unwrap();
        assert_eq!(plan.status_len, 0);
        assert_eq!(plan.warning_len, 1);
    }

    #[test]
    fn full_mastery_uses_three_distinct_unaided_transfer_families() {
        let mut result = base_result();
        result.mastery_changes[0].new_status = learning_core::LearningItemStatus::Mastered;
        result.evidence = vec![tutor_contract::EvidenceEntry {
            learning_item_id: Some("li-known".to_owned()),
            client_item_key: None,
            context: Some("pharmacy:help".to_owned()),
            context_family: Some("service.ask_help".to_owned()),
            kind: learning_core::EvidenceKind::Transfer,
            success: true,
            assistance_level: Some(0),
            response_millis: None,
            probe_phase: Some(tutor_contract::ProbePhase::Transfer),
            pronunciation_intelligible: Some(true),
            pronunciation_repeat_required: Some(false),
            detail: None,
        }];

        let mut state = state();
        add_lesson_mastery_signal(
            &mut state,
            "li-known",
            true,
            &["transport.ask_stop", "commerce.request_item"],
        )
        .unwrap();

        let plan = plan_lesson_completion(&result, &state).unwrap();
        assert_eq!(plan.status_len, 1);
        assert_eq!(
            result.mastery_changes[slot(plan.status_source[0])].new_status,
            learning_core::LearningItemStatus::Mastered
        );
    }

    #[test]
    fn latest_transfer_attempt_in_lesson_controls_mastery_gate() {
        let mut result = base_result();
        result.mastery_changes[0].new_status = learning_core::LearningItemStatus::Mastered;
        let mut state = state();
        add_lesson_mastery_signal(
            &mut state,
            "li-known",
            true,
            &[
                "service.ask_help",
                "commerce.request_item",
                "transport.ask_stop",
            ],
        )
        .unwrap();

        let transfer = |success: bool, family: &str| tutor_contract::EvidenceEntry {
            learning_item_id: Some("li-known".to_owned()),
            client_item_key: None,
            context: Some("ordered-transfer".to_owned()),
            context_family: Some(family.to_owned()),
            kind: learning_core::EvidenceKind::Transfer,
            success,
            assistance_level: Some(0),
            response_millis: Some(1_200),
            probe_phase: Some(tutor_contract::ProbePhase::Transfer),
            pronunciation_intelligible: Some(true),
            pronunciation_repeat_required: Some(false),
            detail: None,
        };

        result.evidence = vec![
            transfer(true, "communication.clarify"),
            transfer(false, "service.ask_help"),
        ];
        let blocked = plan_lesson_completion(&result, &state).unwrap();
        assert_eq!(blocked.status_len, 0);

        result.evidence.reverse();
        let accepted = plan_lesson_completion(&result, &state).unwrap();
        assert_eq!(accepted.status_len, 1);
    }

    #[test]
    fn latest_transfer_attempt_in_result_controls_mastery() {
        let mut result = base_result();
        result.mastery_changes[0].new_status = learning_core::LearningItemStatus::Mastered;
        result.evidence = vec![
            tutor_contract::EvidenceEntry {
                learning_item_id: Some("li-known".to_owned()),
                client_item_key: None,
                context: Some("first".to_owned()),
                context_family: Some("service.ask_help".to_owned()),
                kind: learning_core::EvidenceKind::Transfer,
                success: false,
                assistance_level: Some(0),
                response_millis: Some(1_800),
                probe_phase: Some(tutor_contract::ProbePhase::Transfer),
                pronunciation_intelligible: Some(true),
                pronunciation_repeat_required: Some(false),
                detail: None,
            },
            tutor_contract::EvidenceEntry {
                learning_item_id: Some("li-known".to_owned()),
                client_item_key: None,
                context: Some("second".to_owned()),
                context_family: Some("communication.clarify".to_owned()),
                kind: learning_core::EvidenceKind::Transfer,
                success: true,
                assistance_level: Some(0),
                response_millis: Some(1_200),
                probe_phase: Some(tutor_contract::ProbePhase::Transfer),
                pronunciation_intelligible: Some(true),
                pronunciation_repeat_required: Some(false),
                detail: None,
            },
        ];

        let mut state = state();
        add_lesson_mastery_signal(
            &mut state,
            "li-known",
            true,
            &[
                "service.ask_help",
                "commerce.request_item",
                "transport.ask_stop",
            ],
        )
        .unwrap();

        let recovered = plan_lesson_completion(&result, &state).unwrap();
        assert_eq!(recovered.status_len, 1);
        assert_eq!(recovered.warning_len, 0);

        result.evidence.swap(0, 1);
        let blocked = plan_lesson_completion(&result, &state).unwrap();
        assert_eq!(blocked.status_len, 0);
        assert_eq!(blocked.warning_len, 1);
    }

    #[test]
    fn historical_latest_transfer_failure_blocks_mastery_until_new_unaided_transfer_success() {
        let mut result = base_result();
        result.mastery_changes[0].new_status = learning_core::LearningItemStatus::Mastered;
        result.evidence.clear();

        let mut state = state();
        add_lesson_mastery_signal(
            &mut state,
            "li-known",
            true,
            &[
                "service.ask_help",
                "commerce.request_item",
                "transport.ask_stop",
            ],
        )
        .unwrap();
        set_lesson_mastery_latest_transfer_failed(&mut state, "li-known", true).unwrap();

        let blocked = plan_lesson_completion(&result, &state).unwrap();
        assert_eq!(blocked.status_len, 0);
        assert_eq!(blocked.warning_len, 1);

        result.evidence.push(tutor_contract::EvidenceEntry {
            learning_item_id: Some("li-known".to_owned()),
            client_item_key: None,
            context: Some("new-setting".to_owned()),
            context_family: Some("communication.clarify".to_owned()),
            kind: learning_core::EvidenceKind::Transfer,
            success: true,
            assistance_level: Some(0),
            response_millis: Some(1_400),
            probe_phase: Some(tutor_contract::ProbePhase::Transfer),
            pronunciation_intelligible: Some(true),
            pronunciation_repeat_required: Some(false),
            detail: None,
        });
        let recovered = plan_lesson_completion(&result, &state).unwrap();
        assert_eq!(recovered.status_len, 1);
        assert_eq!(recovered.warning_len, 0);
    }

    #[test]
    fn full_mastery_rejects_duplicate_context_family() {
        let mut result = base_result();
        result.mastery_changes[0].new_status = learning_core::LearningItemStatus::Mastered;
        result.evidence = vec![tutor_contract::EvidenceEntry {
            learning_item_id: Some("li-known".to_owned()),
            client_item_key: None,
            context: Some("another".to_owned()),
            context_family: Some("transport.ask_stop".to_owned()),
            kind: learning_core::EvidenceKind::Transfer,
            success: true,
            assistance_level: Some(0),
            response_millis: None,
            probe_phase: Some(tutor_contract::ProbePhase::Transfer),
            pronunciation_intelligible: Some(true),
            pronunciation_repeat_required: Some(false),
            detail: None,
        }];

        let mut state = state();
        add_lesson_mastery_signal(
            &mut state,
            "li-known",
            true,
            &["transport.ask_stop", "commerce.request_item"],
        )
        .unwrap();

        let plan = plan_lesson_completion(&result, &state).unwrap();
        assert_eq!(plan.status_len, 0);
        assert_eq!(plan.warning_len, 1);
    }
}

const MAX_NEW: usize = tutor_contract::bounds::MAX_NEW_ITEMS;
const MAX_EVIDENCE: usize = tutor_contract::bounds::MAX_EVIDENCE;
const MAX_ERRORS: usize = tutor_contract::bounds::MAX_ERROR_OCCURRENCES;
const MAX_LISTENING: usize = tutor_contract::bounds::MAX_LISTENING_ATTEMPTS;
const MAX_MASTERY: usize = tutor_contract::bounds::MAX_MASTERY_CHANGES;
const MAX_REFERENCED_ITEMS: usize = MAX_EVIDENCE + MAX_LISTENING + MAX_MASTERY;

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
struct TextRef {
    start: usize,
    len: usize,
}

fn push_text(text: &mut String, value: &str) -> TextRef {
    let start = text.len();
    text.push_str(value);
    TextRef {
        start,
        len: value.len(),
    }
}

fn text_at(text: &str, reference: TextRef) -> &str {
    &text[reference.start..reference.start + reference.len]
}

pub(crate) struct LessonPlanState {
    profile_id: TextRef,
    pack_id: TextRef,
    current_lesson_number: u32,
    pub duplicate_import: bool,
    text: String,
    existing_lesson_present: bool,
    existing_lesson_id: TextRef,

    normalized_len: usize,
    normalized_new: [TextRef; MAX_NEW],

    existing_normalized_len: usize,
    existing_normalized_text: [TextRef; MAX_NEW],
    existing_normalized_id: [TextRef; MAX_NEW],

    known_item_len: usize,
    known_item_id: [TextRef; MAX_REFERENCED_ITEMS],

    error_len: usize,
    error_id: [TextRef; MAX_ERRORS],
    error_count: [u32; MAX_ERRORS],
    error_resolved: [bool; MAX_ERRORS],

    mastery_len: usize,
    mastery_item_id: [TextRef; MAX_MASTERY],
    mastery_any_unaided_productive: [bool; MAX_MASTERY],
    mastery_latest_transfer_failed: [bool; MAX_MASTERY],
    mastery_family_count: [u8; MAX_MASTERY],
    mastery_family_0: [TextRef; MAX_MASTERY],
    mastery_family_1: [TextRef; MAX_MASTERY],
    mastery_family_2: [TextRef; MAX_MASTERY],
}

pub(crate) fn new_lesson_plan_state(
    profile_id: &str,
    pack_id: &str,
    current_lesson_number: u32,
) -> LessonPlanState {
    let mut text = String::new();
    let profile_id = push_text(&mut text, profile_id);
    let pack_id = push_text(&mut text, pack_id);
    LessonPlanState {
        profile_id,
        pack_id,
        current_lesson_number,
        duplicate_import: false,
        text,
        existing_lesson_present: false,
        existing_lesson_id: TextRef::default(),
        normalized_len: 0,
        normalized_new: [TextRef::default(); MAX_NEW],
        existing_normalized_len: 0,
        existing_normalized_text: [TextRef::default(); MAX_NEW],
        existing_normalized_id: [TextRef::default(); MAX_NEW],
        known_item_len: 0,
        known_item_id: [TextRef::default(); MAX_REFERENCED_ITEMS],
        error_len: 0,
        error_id: [TextRef::default(); MAX_ERRORS],
        error_count: [0; MAX_ERRORS],
        error_resolved: [false; MAX_ERRORS],
        mastery_len: 0,
        mastery_item_id: [TextRef::default(); MAX_MASTERY],
        mastery_any_unaided_productive: [false; MAX_MASTERY],
        mastery_latest_transfer_failed: [false; MAX_MASTERY],
        mastery_family_count: [0; MAX_MASTERY],
        mastery_family_0: [TextRef::default(); MAX_MASTERY],
        mastery_family_1: [TextRef::default(); MAX_MASTERY],
        mastery_family_2: [TextRef::default(); MAX_MASTERY],
    }
}

pub(crate) fn lesson_profile_id(state: &LessonPlanState) -> &str {
    text_at(&state.text, state.profile_id)
}

fn lesson_pack_id(state: &LessonPlanState) -> &str {
    text_at(&state.text, state.pack_id)
}

pub(crate) fn set_lesson_existing(state: &mut LessonPlanState, id: &str) {
    state.existing_lesson_present = true;
    state.existing_lesson_id = push_text(&mut state.text, id);
}

pub(crate) fn set_lesson_normalized_item(
    state: &mut LessonPlanState,
    index: usize,
    normalized: &str,
) -> Result<(), LessonPlanError> {
    if index >= MAX_NEW {
        return Err(LessonPlanError::CapacityExceeded("normalized_new_items"));
    }
    state.normalized_new[index] = push_text(&mut state.text, normalized);
    state.normalized_len = state.normalized_len.max(index + 1);
    Ok(())
}

pub(crate) fn add_lesson_existing_normalized(
    state: &mut LessonPlanState,
    normalized: &str,
    id: &str,
) -> Result<(), LessonPlanError> {
    if state.existing_normalized_len == MAX_NEW {
        return Err(LessonPlanError::CapacityExceeded("existing_normalized"));
    }
    let i = state.existing_normalized_len;
    state.existing_normalized_text[i] = push_text(&mut state.text, normalized);
    state.existing_normalized_id[i] = push_text(&mut state.text, id);
    state.existing_normalized_len += 1;
    Ok(())
}

pub(crate) fn add_known_lesson_item(
    state: &mut LessonPlanState,
    id: &str,
) -> Result<(), LessonPlanError> {
    if (0..state.known_item_len).any(|i| text_at(&state.text, state.known_item_id[i]) == id) {
        return Ok(());
    }
    if state.known_item_len == MAX_REFERENCED_ITEMS {
        return Err(LessonPlanError::CapacityExceeded("known_items"));
    }
    state.known_item_id[state.known_item_len] = push_text(&mut state.text, id);
    state.known_item_len += 1;
    Ok(())
}

pub(crate) fn add_lesson_error_target(
    state: &mut LessonPlanState,
    id: &str,
    count: u32,
    resolved: bool,
) -> Result<(), LessonPlanError> {
    if state.error_len == MAX_ERRORS {
        return Err(LessonPlanError::CapacityExceeded("error_targets"));
    }
    let i = state.error_len;
    state.error_id[i] = push_text(&mut state.text, id);
    state.error_count[i] = count;
    state.error_resolved[i] = resolved;
    state.error_len += 1;
    Ok(())
}

#[cfg(test)]
pub(crate) fn replace_lesson_error_target_count(
    state: &mut LessonPlanState,
    id: &str,
    count: u32,
) -> Result<(), LessonPlanError> {
    let Some(i) = (0..state.error_len).position(|i| text_at(&state.text, state.error_id[i]) == id)
    else {
        return Err(LessonPlanError::UnknownErrorTarget(id.to_owned()));
    };
    state.error_count[i] = count;
    Ok(())
}

pub(crate) fn add_lesson_mastery_signal(
    state: &mut LessonPlanState,
    item_id: &str,
    any_unaided_productive: bool,
    families: &[&str],
) -> Result<(), LessonPlanError> {
    let i = if let Some(i) = (0..state.mastery_len)
        .position(|i| text_at(&state.text, state.mastery_item_id[i]) == item_id)
    {
        i
    } else {
        if state.mastery_len == MAX_MASTERY {
            return Err(LessonPlanError::CapacityExceeded("mastery_signals"));
        }
        let i = state.mastery_len;
        state.mastery_item_id[i] = push_text(&mut state.text, item_id);
        state.mastery_len += 1;
        i
    };

    state.mastery_any_unaided_productive[i] |= any_unaided_productive;
    for family in families {
        add_lesson_mastery_family(state, i, family);
    }
    Ok(())
}

pub(crate) fn set_lesson_mastery_latest_transfer_failed(
    state: &mut LessonPlanState,
    item_id: &str,
    failed: bool,
) -> Result<(), LessonPlanError> {
    add_lesson_mastery_signal(state, item_id, false, &[])?;
    let Some(index) = (0..state.mastery_len)
        .position(|index| text_at(&state.text, state.mastery_item_id[index]) == item_id)
    else {
        return Err(LessonPlanError::UnknownLearningItem(item_id.to_owned()));
    };
    state.mastery_latest_transfer_failed[index] = failed;
    Ok(())
}

fn add_lesson_mastery_family(state: &mut LessonPlanState, index: usize, family: &str) {
    if family.trim().is_empty() || state.mastery_family_count[index] >= 3 {
        return;
    }
    let count = usize::from(state.mastery_family_count[index]);
    let exists = (0..count).any(|offset| lesson_mastery_family(state, index, offset) == family);
    if exists {
        return;
    }
    let reference = push_text(&mut state.text, family);
    match count {
        0 => state.mastery_family_0[index] = reference,
        1 => state.mastery_family_1[index] = reference,
        2 => state.mastery_family_2[index] = reference,
        _ => unreachable!(),
    }
    state.mastery_family_count[index] += 1;
}

pub(crate) fn lesson_normalized_new(state: &LessonPlanState, index: usize) -> &str {
    text_at(&state.text, state.normalized_new[index])
}

fn lesson_existing_normalized_text(state: &LessonPlanState, index: usize) -> &str {
    text_at(&state.text, state.existing_normalized_text[index])
}

fn lesson_existing_normalized_id(state: &LessonPlanState, index: usize) -> &str {
    text_at(&state.text, state.existing_normalized_id[index])
}

fn lesson_known_item_id(state: &LessonPlanState, index: usize) -> &str {
    text_at(&state.text, state.known_item_id[index])
}

fn lesson_error_id(state: &LessonPlanState, index: usize) -> &str {
    text_at(&state.text, state.error_id[index])
}

fn lesson_mastery_item_id(state: &LessonPlanState, index: usize) -> &str {
    text_at(&state.text, state.mastery_item_id[index])
}

fn lesson_mastery_family(state: &LessonPlanState, index: usize, offset: usize) -> &str {
    let reference = match offset {
        0 => state.mastery_family_0[index],
        1 => state.mastery_family_1[index],
        2 => state.mastery_family_2[index],
        _ => unreachable!(),
    };
    text_at(&state.text, reference)
}

pub(crate) fn lesson_existing_id(state: &LessonPlanState) -> &str {
    text_at(&state.text, state.existing_lesson_id)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LessonPlanKind {
    DuplicateNoop,
    CompleteExisting,
    CreateCompleted,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum LessonPlanError {
    ProfileMismatch(String),
    PackMismatch(String),
    UnknownLearningItem(String),
    UnknownClientItemKey(String),
    UnknownLesson(String),
    UnknownErrorTarget(String),
    InvalidContext(&'static str),
    CapacityExceeded(&'static str),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ResolvedItem {
    Known(u32),
    Reused(u32),
    New(u32),
}

#[derive(Debug)]
pub(crate) struct LessonPlan {
    pub lesson_kind: LessonPlanKind,
    pub lesson_number: u32,

    pub new_items_len: usize,
    pub new_item_source: [u32; MAX_NEW],

    pub reused_items_len: usize,
    pub reused_item_source: [u32; MAX_NEW],
    reused_existing_index: [u32; MAX_NEW],

    pub evidence_len: usize,
    pub evidence_item: [ResolvedItem; MAX_EVIDENCE],

    pub listening_len: usize,
    pub listening_item: [Option<ResolvedItem>; MAX_LISTENING],

    pub error_len: usize,
    pub error_target_present: [bool; MAX_ERRORS],
    pub error_new_count: [u32; MAX_ERRORS],
    pub error_method_change: [bool; MAX_ERRORS],

    pub status_len: usize,
    pub status_source: [u32; MAX_MASTERY],

    pub warning_len: usize,
    pub warning_source: [u32; MAX_MASTERY],
}

fn dense_index(index: usize, label: &'static str) -> Result<u32, LessonPlanError> {
    u32::try_from(index).map_err(|_| LessonPlanError::CapacityExceeded(label))
}

fn slot(index: u32) -> usize {
    match usize::try_from(index) {
        Ok(index) => index,
        Err(_) => unreachable!("u32 dense index must fit usize on supported targets"),
    }
}

fn empty_lesson_plan() -> LessonPlan {
    LessonPlan {
        lesson_kind: LessonPlanKind::CreateCompleted,
        lesson_number: 0,
        new_items_len: 0,
        new_item_source: [0; MAX_NEW],
        reused_items_len: 0,
        reused_item_source: [0; MAX_NEW],
        reused_existing_index: [0; MAX_NEW],
        evidence_len: 0,
        evidence_item: [ResolvedItem::Known(0); MAX_EVIDENCE],
        listening_len: 0,
        listening_item: [None; MAX_LISTENING],
        error_len: 0,
        error_target_present: [false; MAX_ERRORS],
        error_new_count: [0; MAX_ERRORS],
        error_method_change: [false; MAX_ERRORS],
        status_len: 0,
        status_source: [0; MAX_MASTERY],
        warning_len: 0,
        warning_source: [0; MAX_MASTERY],
    }
}

pub(crate) fn plan_reused_item_id<'a>(
    plan: &LessonPlan,
    state: &'a LessonPlanState,
    index: usize,
) -> &'a str {
    lesson_existing_normalized_id(state, slot(plan.reused_existing_index[index]))
}

pub(crate) fn plan_resolved_item_id<'a>(
    plan: &LessonPlan,
    state: &'a LessonPlanState,
    reference: ResolvedItem,
) -> &'a str {
    match reference {
        ResolvedItem::Known(index) => lesson_known_item_id(state, slot(index)),
        ResolvedItem::Reused(index) => plan_reused_item_id(plan, state, slot(index)),
        ResolvedItem::New(_) => unreachable!("new item uses generated ID"),
    }
}

pub(crate) fn plan_lesson_completion(
    result: &tutor_contract::LessonResult,
    state: &LessonPlanState,
) -> Result<LessonPlan, LessonPlanError> {
    if result.language_profile_id != lesson_profile_id(state) {
        return Err(LessonPlanError::ProfileMismatch(
            result.language_profile_id.clone(),
        ));
    }
    if result.language_pack_id != lesson_pack_id(state) {
        return Err(LessonPlanError::PackMismatch(
            result.language_pack_id.clone(),
        ));
    }

    let mut plan = empty_lesson_plan();

    if state.duplicate_import {
        plan.lesson_kind = LessonPlanKind::DuplicateNoop;
        return Ok(plan);
    }

    if state.normalized_len != result.new_learning_items.len() {
        return Err(LessonPlanError::InvalidContext(
            "normalized_new_items length mismatch",
        ));
    }

    plan.lesson_kind = match (&result.lesson.lesson_id, state.existing_lesson_present) {
        (_, true) => LessonPlanKind::CompleteExisting,
        (Some(id), false) => return Err(LessonPlanError::UnknownLesson(id.clone())),
        (None, false) => {
            plan.lesson_number = state.current_lesson_number.saturating_add(1);
            LessonPlanKind::CreateCompleted
        }
    };

    split_items(state, &mut plan)?;
    resolve_evidence(result, state, &mut plan)?;
    resolve_listening(result, state, &mut plan)?;
    plan_errors(result, state, &mut plan)?;
    plan_mastery(result, state, &mut plan)?;

    Ok(plan)
}

fn split_items(state: &LessonPlanState, plan: &mut LessonPlan) -> Result<(), LessonPlanError> {
    for source_index in 0..state.normalized_len {
        let normalized = lesson_normalized_new(state, source_index);
        if let Some(existing_index) = (0..state.existing_normalized_len)
            .position(|i| lesson_existing_normalized_text(state, i) == normalized)
        {
            let i = plan.reused_items_len;
            if i == MAX_NEW {
                return Err(LessonPlanError::CapacityExceeded("reused_items"));
            }
            plan.reused_item_source[i] = dense_index(source_index, "reused_item_source")?;
            plan.reused_existing_index[i] = dense_index(existing_index, "reused_existing_index")?;
            plan.reused_items_len += 1;
        } else {
            let i = plan.new_items_len;
            if i == MAX_NEW {
                return Err(LessonPlanError::CapacityExceeded("new_items"));
            }
            plan.new_item_source[i] = dense_index(source_index, "new_item_source")?;
            plan.new_items_len += 1;
        }
    }
    Ok(())
}

fn resolve_evidence(
    result: &tutor_contract::LessonResult,
    state: &LessonPlanState,
    plan: &mut LessonPlan,
) -> Result<(), LessonPlanError> {
    for evidence in &result.evidence {
        if plan.evidence_len == MAX_EVIDENCE {
            return Err(LessonPlanError::CapacityExceeded("evidence"));
        }
        plan.evidence_item[plan.evidence_len] = resolve_reference(
            evidence.learning_item_id.as_deref(),
            evidence.client_item_key.as_deref(),
            result,
            state,
            plan,
        )?;
        plan.evidence_len += 1;
    }
    Ok(())
}

fn resolve_listening(
    result: &tutor_contract::LessonResult,
    state: &LessonPlanState,
    plan: &mut LessonPlan,
) -> Result<(), LessonPlanError> {
    for attempt in &result.listening_attempts {
        if plan.listening_len == MAX_LISTENING {
            return Err(LessonPlanError::CapacityExceeded("listening"));
        }
        if attempt.learning_item_id.is_some() || attempt.client_item_key.is_some() {
            plan.listening_item[plan.listening_len] = Some(resolve_reference(
                attempt.learning_item_id.as_deref(),
                attempt.client_item_key.as_deref(),
                result,
                state,
                plan,
            )?);
        }
        plan.listening_len += 1;
    }
    Ok(())
}

fn resolve_reference(
    learning_item_id: Option<&str>,
    client_item_key: Option<&str>,
    result: &tutor_contract::LessonResult,
    state: &LessonPlanState,
    plan: &LessonPlan,
) -> Result<ResolvedItem, LessonPlanError> {
    match (learning_item_id, client_item_key) {
        (Some(id), None) => {
            if let Some(index) =
                (0..state.known_item_len).position(|i| lesson_known_item_id(state, i) == id)
            {
                Ok(ResolvedItem::Known(dense_index(index, "known_item")?))
            } else {
                Err(LessonPlanError::UnknownLearningItem(id.to_owned()))
            }
        }
        (None, Some(key)) => {
            let Some(source_index) = result
                .new_learning_items
                .iter()
                .position(|item| item.client_item_key.as_deref() == Some(key))
            else {
                return Err(LessonPlanError::UnknownClientItemKey(key.to_owned()));
            };

            let source_key = dense_index(source_index, "new_learning_item_source")?;
            if let Some(index) = plan.reused_item_source[..plan.reused_items_len]
                .iter()
                .position(|candidate| *candidate == source_key)
            {
                return Ok(ResolvedItem::Reused(dense_index(
                    index,
                    "reused_plan_index",
                )?));
            }
            if let Some(plan_index) = plan.new_item_source[..plan.new_items_len]
                .iter()
                .position(|candidate| *candidate == source_key)
            {
                return Ok(ResolvedItem::New(dense_index(
                    plan_index,
                    "new_plan_index",
                )?));
            }
            Err(LessonPlanError::InvalidContext(
                "client item key did not resolve",
            ))
        }
        _ => Err(LessonPlanError::InvalidContext(
            "item reference must contain exactly one id/key",
        )),
    }
}

fn plan_errors(
    result: &tutor_contract::LessonResult,
    state: &LessonPlanState,
    plan: &mut LessonPlan,
) -> Result<(), LessonPlanError> {
    for occurrence in &result.error_occurrences {
        if plan.error_len == MAX_ERRORS {
            return Err(LessonPlanError::CapacityExceeded("errors"));
        }
        let i = plan.error_len;
        if let Some(raw_id) = occurrence.error_target_id.as_deref() {
            let target_index = (0..state.error_len)
                .position(|index| lesson_error_id(state, index) == raw_id)
                .ok_or_else(|| LessonPlanError::UnknownErrorTarget(raw_id.to_owned()))?;
            let new_count = state.error_count[target_index].saturating_add(1);
            let active = !state.error_resolved[target_index];
            plan.error_target_present[i] = true;
            plan.error_new_count[i] = new_count;
            plan.error_method_change[i] = learning_core::error_method_change_due_for(
                new_count,
                !active,
                occurrence.communicative_impact,
            );
        }
        plan.error_len += 1;
    }
    Ok(())
}

fn plan_mastery(
    result: &tutor_contract::LessonResult,
    state: &LessonPlanState,
    plan: &mut LessonPlan,
) -> Result<(), LessonPlanError> {
    for (source_index, proposal) in result.mastery_changes.iter().enumerate() {
        if !(0..state.known_item_len)
            .any(|i| lesson_known_item_id(state, i) == proposal.learning_item_id)
        {
            return Err(LessonPlanError::UnknownLearningItem(
                proposal.learning_item_id.clone(),
            ));
        }

        let signal_index = (0..state.mastery_len)
            .position(|i| lesson_mastery_item_id(state, i) == proposal.learning_item_id);

        let mut any_unaided = signal_index
            .map(|i| state.mastery_any_unaided_productive[i])
            .unwrap_or(false);
        let historical_transfer_failed = signal_index
            .map(|i| state.mastery_latest_transfer_failed[i])
            .unwrap_or(false);

        let mut family_count = 0usize;
        let mut families = [""; 3];

        if let Some(i) = signal_index {
            for offset in 0..usize::from(state.mastery_family_count[i]).min(3) {
                add_family_ref(
                    &mut family_count,
                    &mut families,
                    lesson_mastery_family(state, i, offset),
                );
            }
        }

        let mut fresh_productive_failure = false;
        let mut latest_fresh_transfer_qualified = None;
        for evidence in result
            .evidence
            .iter()
            .filter(|e| e.learning_item_id.as_deref() == Some(proposal.learning_item_id.as_str()))
        {
            let productive = matches!(
                evidence.kind,
                learning_core::EvidenceKind::SpontaneousProduction
                    | learning_core::EvidenceKind::Transfer
            );
            if productive && !evidence.success {
                fresh_productive_failure = true;
            }
            if evidence.kind == learning_core::EvidenceKind::Transfer {
                latest_fresh_transfer_qualified =
                    Some(evidence.success && evidence.assistance_level == Some(0));
            }
            if evidence.success && productive && evidence.assistance_level == Some(0) {
                any_unaided = true;
                if evidence.kind == learning_core::EvidenceKind::Transfer
                    && let Some(family) = evidence
                        .context_family
                        .as_deref()
                        .filter(|family| !family.trim().is_empty())
                {
                    add_family_ref(&mut family_count, &mut families, family);
                }
            }
        }

        let accepted = match proposal.new_status {
            learning_core::LearningItemStatus::ProvisionallyMastered => {
                any_unaided && !fresh_productive_failure
            }
            learning_core::LearningItemStatus::Mastered => {
                let latest_transfer_failed = latest_fresh_transfer_qualified == Some(false)
                    || (latest_fresh_transfer_qualified.is_none() && historical_transfer_failed);
                family_count >= 3 && !latest_transfer_failed
            }
            _ => true,
        };

        if accepted {
            if plan.status_len == MAX_MASTERY {
                return Err(LessonPlanError::CapacityExceeded("status_updates"));
            }
            let i = plan.status_len;
            plan.status_source[i] = dense_index(source_index, "status_source")?;
            plan.status_len += 1;
        } else {
            if plan.warning_len == MAX_MASTERY {
                return Err(LessonPlanError::CapacityExceeded("warnings"));
            }
            plan.warning_source[plan.warning_len] = dense_index(source_index, "warning_source")?;
            plan.warning_len += 1;
        }
    }
    Ok(())
}

fn add_family_ref<'a>(count: &mut usize, families: &mut [&'a str; 3], family: &'a str) {
    if family.trim().is_empty() || *count >= families.len() || families[..*count].contains(&family)
    {
        return;
    }
    families[*count] = family;
    *count += 1;
}
