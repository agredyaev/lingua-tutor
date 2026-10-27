#![allow(clippy::items_after_test_module)] // Test fixtures are intentionally grouped before the processing pass.
//! Fixed-capacity card mutation planner.

#[cfg(test)]
// Test fixtures intentionally fail fast and use compact tuple snapshots.
#[allow(clippy::unwrap_used, clippy::type_complexity)]
mod tests {
    use super::*;

    fn create_mutation(op: &str, prompt: &str, answer: &str) -> tutor_contract::CardMutation {
        tutor_contract::CardMutation {
            operation: tutor_contract::MutationOperation::Create,
            client_operation_id: op.to_owned(),
            card_id: None,
            card: Some(tutor_contract::CardPayload {
                intent: language_pack_api::CardIntent::Production,
                prompt: prompt.to_owned(),
                answer: answer.to_owned(),
                accepted_answers: vec!["alt".to_owned()],
                prompt_language: Some("ru".to_owned()),
                explanation: None,
                scenario_tags: vec![],
                skill_tags: vec![],
                source_learning_item_id: None,
                source_error_target_id: None,
                creation_reason: "lesson".to_owned(),
                priority_score: None,
            }),
            accepted_answer: None,
            reason: None,
        }
    }

    fn state() -> CardPlanState {
        new_card_plan_state(2)
    }

    #[test]
    fn accepted_answers_json_preserves_escaped_unicode_and_order() {
        let mut state = state();
        let escaped =
            r#"["plain","line\nbreak","quote\"here","\\slash","ni\u00f1o","\ud83d\ude80"]"#;
        add_existing_card_json(
            &mut state,
            "card-esc",
            "review",
            "production",
            "prompt",
            "answer",
            escaped,
        )
        .unwrap();
        let index = find_card(&state, "card-esc").unwrap();
        let expected = [
            "plain",
            "line\nbreak",
            "quote\"here",
            "\\slash",
            "niño",
            "🚀",
        ];
        assert_eq!(card_accepted_len(&state, index), expected.len());
        for (i, answer) in expected.iter().enumerate() {
            assert_eq!(card_accepted_at(&state, index, i), *answer);
        }
        assert!(card_accepted_contains(&state, index, "  niño "));
    }

    #[test]
    fn invalid_or_oversized_accepted_answers_never_add_card() {
        let mut state = state();
        for json in ["null", "[42]", r#"["broken","\q"]"#, r#"["unfinished""#] {
            assert_eq!(
                add_existing_card_json(
                    &mut state,
                    "bad",
                    "review",
                    "production",
                    "prompt",
                    "answer",
                    json
                ),
                Err(CardPlanError::InvalidAcceptedAnswersJson),
            );
            assert_eq!(state.card_len, 0);
            assert!(find_card(&state, "bad").is_none());
        }
        let overflow = serde_json::to_string(
            &(0..=MAX_ACCEPTED)
                .map(|n| format!("answer-{n}"))
                .collect::<Vec<_>>(),
        )
        .unwrap();
        assert_eq!(
            add_existing_card_json(
                &mut state,
                "too-many",
                "review",
                "production",
                "prompt",
                "answer",
                &overflow
            ),
            Err(CardPlanError::InvalidAcceptedAnswersJson),
        );
        assert_eq!(state.card_len, 0);
        add_existing_card_json(
            &mut state,
            "valid",
            "review",
            "production",
            "prompt",
            "answer",
            r#"["one","two"]"#,
        )
        .unwrap();
        assert_eq!(find_card(&state, "valid"), Some(0));
        assert_eq!(card_accepted_len(&state, 0), 2);
    }

    #[test]
    fn create_consumes_budget_then_leaves_candidate() {
        let mutations = vec![
            create_mutation("op-1", "p1", "a1"),
            create_mutation("op-2", "p2", "a2"),
            create_mutation("op-3", "p3", "a3"),
        ];
        let plan = plan_card_mutations(&mutations, &state()).unwrap();

        assert_eq!(plan.len, 3);
        assert_eq!(plan.verdict[0], CardVerdict::Applied);
        assert_eq!(plan.verdict[1], CardVerdict::Applied);
        assert_eq!(plan.verdict[2], CardVerdict::Warned);
        assert_eq!(plan.effect[0], CardEffectKind::Create);
        assert_eq!(plan.effect[2], CardEffectKind::Create);
        assert_eq!(card_plan_status(&plan, 0), "new");
        assert_eq!(card_plan_status(&plan, 1), "new");
        assert_eq!(card_plan_status(&plan, 2), "candidate");
    }

    #[test]
    fn duplicate_existing_and_duplicate_in_same_batch_are_rejected() {
        let mut state = state();
        let existing = create_mutation("seed", "same", "answer");
        let payload = existing.card.as_ref().unwrap();
        let hash = learning_core::card_content_hash(
            payload.intent,
            &payload.prompt,
            &payload.answer,
            &payload.accepted_answers,
        );
        set_card_content_hash(&mut state, 0, &hash, true).unwrap();

        let mutations = vec![
            create_mutation("op-1", "same", "answer"),
            create_mutation("op-2", "new", "answer"),
            create_mutation("op-3", "new", "answer"),
        ];

        let plan = plan_card_mutations(&mutations, &state).unwrap();
        assert_eq!(plan.verdict[0], CardVerdict::Rejected);
        assert_eq!(plan.effect[0], CardEffectKind::None);
        assert_eq!(plan.verdict[1], CardVerdict::Applied);
        assert_eq!(plan.verdict[2], CardVerdict::Rejected);
    }

    #[test]
    fn update_allows_content_only_but_rejects_semantic_change() {
        let mut state = state();
        add_existing_card(
            &mut state,
            "card-1",
            "review",
            "production",
            "answer",
            &["alt"],
        )
        .unwrap();

        let mut cosmetic = create_mutation("op-1", "new prompt", "answer");
        cosmetic.operation = tutor_contract::MutationOperation::Update;
        cosmetic.card_id = Some("card-1".to_owned());

        let mut semantic = cosmetic.clone();
        semantic.client_operation_id = "op-2".to_owned();
        semantic.card.as_mut().unwrap().answer = "different".to_owned();

        let plan = plan_card_mutations(&[cosmetic, semantic], &state).unwrap();
        assert_eq!(plan.effect[0], CardEffectKind::UpdateContent);
        assert_eq!(plan.verdict[0], CardVerdict::Applied);
        assert_eq!(plan.effect[1], CardEffectKind::None);
        assert_eq!(plan.verdict[1], CardVerdict::Rejected);
    }

    #[test]
    fn suspend_restore_retire_follow_old_transition_rules() {
        let mut state = state();
        add_existing_card(&mut state, "card-review", "review", "production", "a", &[]).unwrap();
        add_existing_card(
            &mut state,
            "card-suspended",
            "suspended",
            "production",
            "a",
            &[],
        )
        .unwrap();

        let mk = |operation, op: &str, card: &str| tutor_contract::CardMutation {
            operation,
            client_operation_id: op.to_owned(),
            card_id: Some(card.to_owned()),
            card: None,
            accepted_answer: None,
            reason: Some("reason".to_owned()),
        };

        let mutations = [
            mk(
                tutor_contract::MutationOperation::Suspend,
                "op-1",
                "card-review",
            ),
            mk(
                tutor_contract::MutationOperation::Restore,
                "op-2",
                "card-suspended",
            ),
            mk(
                tutor_contract::MutationOperation::Retire,
                "op-3",
                "card-review",
            ),
            mk(
                tutor_contract::MutationOperation::Restore,
                "op-4",
                "card-review",
            ),
        ];

        let plan = plan_card_mutations(&mutations, &state).unwrap();
        assert_eq!(plan.effect[0], CardEffectKind::SetStatus);
        assert_eq!(card_plan_status(&plan, 0), "suspended");
        assert_eq!(plan.effect[1], CardEffectKind::SetStatus);
        assert_eq!(card_plan_status(&plan, 1), "review");
        assert_eq!(plan.effect[2], CardEffectKind::SetStatus);
        assert_eq!(card_plan_status(&plan, 2), "retired");
        assert_eq!(plan.verdict[3], CardVerdict::Rejected);
    }

    #[test]
    fn existing_card_intent_preserves_update_semantics_for_all_variants() {
        for (stored, kind) in [
            ("production", language_pack_api::CardIntent::Production),
            ("listening", language_pack_api::CardIntent::Listening),
            ("contrast", language_pack_api::CardIntent::Contrast),
            ("transfer", language_pack_api::CardIntent::Transfer),
        ] {
            let mut state = state();
            add_existing_card(&mut state, "card", "review", stored, "answer", &[]).unwrap();
            let mut update = create_mutation("same", "prompt", "answer");
            update.operation = tutor_contract::MutationOperation::Update;
            update.card_id = Some("card".to_owned());
            update.card.as_mut().unwrap().intent = kind;
            let plan = plan_card_mutations(&[update.clone()], &state).unwrap();
            assert_eq!(plan.effect[0], CardEffectKind::UpdateContent, "{stored}");
            update.card.as_mut().unwrap().intent = match kind {
                language_pack_api::CardIntent::Production => {
                    language_pack_api::CardIntent::Listening
                }
                _ => language_pack_api::CardIntent::Production,
            };
            let plan = plan_card_mutations(&[update], &state).unwrap();
            assert_eq!(plan.verdict[0], CardVerdict::Rejected, "{stored}");
        }
        let mut state = state();
        add_existing_card(&mut state, "card", "review", "unknown", "answer", &[]).unwrap();
        let mut update = create_mutation("invalid", "prompt", "answer");
        update.operation = tutor_contract::MutationOperation::Update;
        update.card_id = Some("card".to_owned());
        assert_eq!(
            plan_card_mutations(&[update], &state).unwrap().verdict[0],
            CardVerdict::Rejected
        );
    }

    #[test]
    fn accepted_answer_add_remove_respects_current_state() {
        let mut state = state();
        add_existing_card(
            &mut state,
            "card-1",
            "review",
            "production",
            "answer",
            &["one", "two"],
        )
        .unwrap();

        let mk = |operation, op: &str, answer: &str| tutor_contract::CardMutation {
            operation,
            client_operation_id: op.to_owned(),
            card_id: Some("card-1".to_owned()),
            card: None,
            accepted_answer: Some(answer.to_owned()),
            reason: None,
        };

        let plan = plan_card_mutations(
            &[
                mk(
                    tutor_contract::MutationOperation::AddAcceptedAnswer,
                    "a",
                    "three",
                ),
                mk(
                    tutor_contract::MutationOperation::AddAcceptedAnswer,
                    "b",
                    "one",
                ),
                mk(
                    tutor_contract::MutationOperation::RemoveAcceptedAnswer,
                    "c",
                    "two",
                ),
                mk(
                    tutor_contract::MutationOperation::RemoveAcceptedAnswer,
                    "d",
                    "missing",
                ),
            ],
            &state,
        )
        .unwrap();

        assert_eq!(plan.effect[0], CardEffectKind::AddAcceptedAnswer);
        assert_eq!(plan.verdict[1], CardVerdict::Rejected);
        assert_eq!(plan.effect[2], CardEffectKind::RemoveAcceptedAnswer);
        assert_eq!(plan.verdict[3], CardVerdict::Rejected);
    }

    #[test]
    fn unsupported_mutations_are_audited_as_rejected() {
        let state = state();
        for operation in [
            tutor_contract::MutationOperation::Split,
            tutor_contract::MutationOperation::Merge,
            tutor_contract::MutationOperation::Link,
            tutor_contract::MutationOperation::AddListeningVariant,
        ] {
            let mutation = tutor_contract::CardMutation {
                operation,
                client_operation_id: format!("{operation:?}"),
                card_id: None,
                card: None,
                accepted_answer: None,
                reason: None,
            };
            let plan = plan_card_mutations(&[mutation], &state).unwrap();
            assert_eq!(plan.verdict[0], CardVerdict::Rejected);
            assert_eq!(plan.effect[0], CardEffectKind::None);
        }
    }
}

const MAX_MUTATIONS: usize = tutor_contract::bounds::MAX_CARD_MUTATIONS;
const MAX_ACCEPTED: usize = tutor_contract::bounds::MAX_ACCEPTED_ANSWERS;
const MAX_EXISTING: usize = MAX_MUTATIONS * 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CardVerdict {
    Applied,
    Rejected,
    Warned,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CardEffectKind {
    None,
    Create,
    UpdateContent,
    SetStatus,
    AddAcceptedAnswer,
    RemoveAcceptedAnswer,
}

#[derive(Clone, Copy, Default)]
pub(super) struct TextRef {
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

pub(super) fn text_at(text: &str, reference: TextRef) -> &str {
    &text[reference.start..reference.start + reference.len]
}

pub(crate) struct CardPlanState {
    remaining_activation_budget: u8,
    text: String,

    content_hash: [TextRef; MAX_MUTATIONS],
    content_hash_present: [bool; MAX_MUTATIONS],
    content_hash_existing: [bool; MAX_MUTATIONS],

    card_len: usize,
    card_id: [TextRef; MAX_EXISTING],
    card_status: [TextRef; MAX_EXISTING],
    card_intent: [Option<language_pack_api::CardIntent>; MAX_EXISTING],
    card_prompt: [TextRef; MAX_EXISTING],
    card_answer: [TextRef; MAX_EXISTING],
    accepted_count: [u8; MAX_EXISTING],
    accepted_answers: [TextRef; MAX_EXISTING * MAX_ACCEPTED],
}

pub(crate) fn new_card_plan_state(remaining_activation_budget: u8) -> CardPlanState {
    CardPlanState {
        remaining_activation_budget,
        text: String::new(),
        content_hash: [TextRef::default(); MAX_MUTATIONS],
        content_hash_present: [false; MAX_MUTATIONS],
        content_hash_existing: [false; MAX_MUTATIONS],
        card_len: 0,
        card_id: [TextRef::default(); MAX_EXISTING],
        card_status: [TextRef::default(); MAX_EXISTING],
        card_intent: [None; MAX_EXISTING],
        card_prompt: [TextRef::default(); MAX_EXISTING],
        card_answer: [TextRef::default(); MAX_EXISTING],
        accepted_count: [0; MAX_EXISTING],
        accepted_answers: [TextRef::default(); MAX_EXISTING * MAX_ACCEPTED],
    }
}

/// Preserve the admitted digest across duplicate checks and SQL writes.
pub(crate) fn set_card_content_hash(
    state: &mut CardPlanState,
    index: usize,
    hash: &str,
    exists_in_database: bool,
) -> Result<(), CardPlanError> {
    if index >= MAX_MUTATIONS {
        return Err(CardPlanError::CapacityExceeded("mutation_hashes"));
    }
    state.content_hash[index] = push_text(&mut state.text, hash);
    state.content_hash_present[index] = true;
    state.content_hash_existing[index] = exists_in_database;
    Ok(())
}

pub(crate) fn card_hash_at(state: &CardPlanState, index: usize) -> Option<&str> {
    state.content_hash_present[index].then(|| text_at(&state.text, state.content_hash[index]))
}

pub(crate) fn mark_existing_card_hash(state: &mut CardPlanState, hash: &str) {
    for index in 0..MAX_MUTATIONS {
        if card_hash_at(state, index).is_some_and(|value| value == hash) {
            state.content_hash_existing[index] = true;
        }
    }
}

#[cfg(test)]
pub(crate) fn add_existing_card(
    state: &mut CardPlanState,
    id: &str,
    status: &str,
    intent: &str,
    answer: &str,
    accepted_answers: &[&str],
) -> Result<(), CardPlanError> {
    add_existing_card_full(state, id, status, intent, "", answer, accepted_answers)
}

#[cfg(test)]
pub(crate) fn add_existing_card_full(
    state: &mut CardPlanState,
    id: &str,
    status: &str,
    intent: &str,
    prompt: &str,
    answer: &str,
    accepted_answers: &[&str],
) -> Result<(), CardPlanError> {
    if state.card_len == MAX_EXISTING {
        return Err(CardPlanError::CapacityExceeded("existing_cards"));
    }
    if accepted_answers.len() > MAX_ACCEPTED {
        return Err(CardPlanError::CapacityExceeded("accepted_answers"));
    }

    let i = push_existing_card_fields(state, id, status, intent, prompt, answer);
    state.accepted_count[i] = accepted_answers.len() as u8;
    let base = i * MAX_ACCEPTED;
    for (offset, accepted) in accepted_answers.iter().enumerate() {
        state.accepted_answers[base + offset] = push_text(&mut state.text, accepted);
    }
    Ok(())
}

fn push_existing_card_fields(
    state: &mut CardPlanState,
    id: &str,
    status: &str,
    intent: &str,
    prompt: &str,
    answer: &str,
) -> usize {
    let i = state.card_len;
    state.card_id[i] = push_text(&mut state.text, id);
    state.card_status[i] = push_text(&mut state.text, status);
    state.card_intent[i] = crate::repositories::card_intent_from_db(intent);
    state.card_prompt[i] = push_text(&mut state.text, prompt);
    state.card_answer[i] = push_text(&mut state.text, answer);
    state.card_len += 1;
    i
}

pub(crate) fn add_existing_card_json(
    state: &mut CardPlanState,
    id: &str,
    status: &str,
    intent: &str,
    prompt: &str,
    answer: &str,
    accepted_answers_json: &str,
) -> Result<(), CardPlanError> {
    if state.card_len == MAX_EXISTING {
        return Err(CardPlanError::CapacityExceeded("existing_cards"));
    }
    // Stream decoded elements into the destination arena, retaining only
    // offsets. An invalid array rolls back the appended text and leaves
    // the card count unchanged.
    let base = state.card_len * MAX_ACCEPTED;
    let count = decode_accepted_answers(
        accepted_answers_json,
        &mut state.text,
        &mut state.accepted_answers[base..base + MAX_ACCEPTED],
    )
    .map_err(|_| CardPlanError::InvalidAcceptedAnswersJson)?;
    let i = push_existing_card_fields(state, id, status, intent, prompt, answer);
    state.accepted_count[i] = count as u8;
    Ok(())
}

fn find_card_index(state: &CardPlanState, id: &str) -> Option<usize> {
    (0..state.card_len).position(|i| text_at(&state.text, state.card_id[i]) == id)
}

fn card_status_at(state: &CardPlanState, index: usize) -> &str {
    text_at(&state.text, state.card_status[index])
}

fn card_accepted_contains(state: &CardPlanState, card_index: usize, answer: &str) -> bool {
    let base = card_index * MAX_ACCEPTED;
    let count = state.accepted_count[card_index] as usize;
    (0..count).any(|offset| {
        text_at(&state.text, state.accepted_answers[base + offset])
            .trim()
            .eq_ignore_ascii_case(answer.trim())
    })
}

pub(crate) fn find_card(state: &CardPlanState, id: &str) -> Option<usize> {
    find_card_index(state, id)
}

pub(crate) fn card_prompt(state: &CardPlanState, index: usize) -> &str {
    text_at(&state.text, state.card_prompt[index])
}

pub(crate) fn card_answer(state: &CardPlanState, index: usize) -> &str {
    text_at(&state.text, state.card_answer[index])
}

pub(crate) fn card_intent(
    state: &CardPlanState,
    index: usize,
) -> Option<language_pack_api::CardIntent> {
    state.card_intent[index]
}

pub(crate) fn card_accepted_len(state: &CardPlanState, index: usize) -> usize {
    state.accepted_count[index] as usize
}

pub(crate) fn card_accepted_at(state: &CardPlanState, index: usize, offset: usize) -> &str {
    text_at(
        &state.text,
        state.accepted_answers[index * MAX_ACCEPTED + offset],
    )
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum CardPlanError {
    CapacityExceeded(&'static str),
    InvalidAcceptedAnswersJson,
}

pub(crate) struct CardPlan {
    pub len: usize,
    pub source_index: [u32; MAX_MUTATIONS],
    pub verdict: [CardVerdict; MAX_MUTATIONS],
    pub effect: [CardEffectKind; MAX_MUTATIONS],
    status: [&'static str; MAX_MUTATIONS],
    note: [Option<&'static str>; MAX_MUTATIONS],
}

fn empty_card_plan() -> CardPlan {
    CardPlan {
        len: 0,
        source_index: [0; MAX_MUTATIONS],
        verdict: [CardVerdict::Rejected; MAX_MUTATIONS],
        effect: [CardEffectKind::None; MAX_MUTATIONS],
        status: [""; MAX_MUTATIONS],
        note: [None; MAX_MUTATIONS],
    }
}

fn dense_source_index(index: usize) -> Result<u32, CardPlanError> {
    u32::try_from(index).map_err(|_| CardPlanError::CapacityExceeded("card_source_index"))
}

fn source_slot(index: u32) -> usize {
    match usize::try_from(index) {
        Ok(index) => index,
        Err(_) => unreachable!("u32 dense index must fit usize on supported targets"),
    }
}

fn append_card_plan_entry(
    plan: &mut CardPlan,
    source_index: usize,
    verdict: CardVerdict,
    effect: CardEffectKind,
    status: Option<&'static str>,
    note: Option<&'static str>,
) -> Result<(), CardPlanError> {
    if plan.len == MAX_MUTATIONS {
        return Err(CardPlanError::CapacityExceeded("card_plan"));
    }
    let i = plan.len;
    plan.source_index[i] = dense_source_index(source_index)?;
    plan.verdict[i] = verdict;
    plan.effect[i] = effect;
    plan.status[i] = status.unwrap_or_default();
    plan.note[i] = note;
    plan.len += 1;
    Ok(())
}

pub(crate) fn card_plan_source_index(plan: &CardPlan, index: usize) -> usize {
    source_slot(plan.source_index[index])
}

pub(crate) fn card_plan_status(plan: &CardPlan, index: usize) -> &str {
    plan.status[index]
}

pub(crate) fn card_plan_note(plan: &CardPlan, index: usize) -> &str {
    plan.note[index].unwrap_or_default()
}

pub(crate) fn plan_card_mutations(
    mutations: &[tutor_contract::CardMutation],
    state: &CardPlanState,
) -> Result<CardPlan, CardPlanError> {
    let mut plan = empty_card_plan();
    let mut budget = state.remaining_activation_budget;
    let mut seen_len = 0usize;
    let mut seen_source = [0u32; MAX_MUTATIONS];

    for (source_index, mutation) in mutations.iter().enumerate() {
        use tutor_contract::MutationOperation;

        match mutation.operation {
            MutationOperation::Create => {
                let Some(payload) = mutation.card.as_ref() else {
                    append_card_plan_entry(
                        &mut plan,
                        source_index,
                        CardVerdict::Rejected,
                        CardEffectKind::None,
                        None,
                        Some("create without card payload"),
                    )?;
                    continue;
                };

                let fallback_hash;
                let hash = if let Some(hash) = card_hash_at(state, source_index) {
                    hash
                } else {
                    fallback_hash = learning_core::card_content_hash(
                        payload.intent,
                        &payload.prompt,
                        &payload.answer,
                        &payload.accepted_answers,
                    );
                    fallback_hash.as_str()
                };
                let duplicate_existing = state.content_hash_existing[source_index];
                let duplicate_batch = (0..seen_len).any(|i| {
                    let previous = source_slot(seen_source[i]);
                    if let Some(admitted) = card_hash_at(state, previous) {
                        admitted == hash
                    } else {
                        // Standalone planner tests may bypass SQL admission.
                        let Some(previous_payload) = mutations[previous].card.as_ref() else {
                            return false;
                        };
                        learning_core::card_content_hash(
                            previous_payload.intent,
                            &previous_payload.prompt,
                            &previous_payload.answer,
                            &previous_payload.accepted_answers,
                        ) == hash
                    }
                });

                if duplicate_existing || duplicate_batch {
                    append_card_plan_entry(
                        &mut plan,
                        source_index,
                        CardVerdict::Rejected,
                        CardEffectKind::None,
                        None,
                        Some(if duplicate_existing {
                            "duplicate existing card"
                        } else {
                            "duplicate of earlier create in this batch"
                        }),
                    )?;
                    continue;
                }

                seen_source[seen_len] = dense_source_index(source_index)?;
                seen_len += 1;

                let (status, verdict, note) = if budget > 0 {
                    budget -= 1;
                    ("new", CardVerdict::Applied, None)
                } else {
                    (
                        "candidate",
                        CardVerdict::Warned,
                        Some("activation budget exhausted; card stays candidate"),
                    )
                };

                append_card_plan_entry(
                    &mut plan,
                    source_index,
                    verdict,
                    CardEffectKind::Create,
                    Some(status),
                    note,
                )?;
            }
            MutationOperation::Update => {
                let Some(card_id) = mutation.card_id.as_deref() else {
                    reject(&mut plan, source_index, "unknown card_id")?;
                    continue;
                };
                let Some(card_index) = find_card_index(state, card_id) else {
                    reject(&mut plan, source_index, "unknown card_id")?;
                    continue;
                };
                let Some(payload) = mutation.card.as_ref() else {
                    reject(&mut plan, source_index, "update without card payload")?;
                    continue;
                };

                let semantic_change = !payload
                    .answer
                    .trim()
                    .eq_ignore_ascii_case(card_answer(state, card_index).trim())
                    || Some(payload.intent) != card_intent(state, card_index);

                if semantic_change {
                    reject(
                        &mut plan,
                        source_index,
                        "semantic edit requires explicit confirmation",
                    )?;
                } else {
                    append_card_plan_entry(
                        &mut plan,
                        source_index,
                        CardVerdict::Applied,
                        CardEffectKind::UpdateContent,
                        None,
                        None,
                    )?;
                }
            }
            MutationOperation::Suspend | MutationOperation::Restore | MutationOperation::Retire => {
                let Some(card_id) = mutation.card_id.as_deref() else {
                    reject(&mut plan, source_index, "unknown card_id")?;
                    continue;
                };
                let Some(card_index) = find_card_index(state, card_id) else {
                    reject(&mut plan, source_index, "unknown card_id")?;
                    continue;
                };

                let current = card_status_at(state, card_index);
                let (target, valid) = match mutation.operation {
                    MutationOperation::Suspend => ("suspended", current != "retired"),
                    MutationOperation::Restore => ("review", current == "suspended"),
                    MutationOperation::Retire => ("retired", true),
                    _ => unreachable!(),
                };

                if valid {
                    append_card_plan_entry(
                        &mut plan,
                        source_index,
                        CardVerdict::Applied,
                        CardEffectKind::SetStatus,
                        Some(target),
                        None,
                    )?;
                } else {
                    reject(&mut plan, source_index, "invalid status transition")?;
                }
            }
            MutationOperation::AddAcceptedAnswer | MutationOperation::RemoveAcceptedAnswer => {
                let Some(card_id) = mutation.card_id.as_deref() else {
                    reject(&mut plan, source_index, "unknown card_id")?;
                    continue;
                };
                let Some(card_index) = find_card_index(state, card_id) else {
                    reject(&mut plan, source_index, "unknown card_id")?;
                    continue;
                };
                let Some(answer) = mutation
                    .accepted_answer
                    .as_deref()
                    .filter(|answer| !answer.trim().is_empty())
                else {
                    reject(&mut plan, source_index, "missing accepted_answer")?;
                    continue;
                };

                let exists = card_accepted_contains(state, card_index, answer);
                match mutation.operation {
                    MutationOperation::AddAcceptedAnswer if !exists => append_card_plan_entry(
                        &mut plan,
                        source_index,
                        CardVerdict::Applied,
                        CardEffectKind::AddAcceptedAnswer,
                        None,
                        None,
                    )?,
                    MutationOperation::RemoveAcceptedAnswer if exists => append_card_plan_entry(
                        &mut plan,
                        source_index,
                        CardVerdict::Applied,
                        CardEffectKind::RemoveAcceptedAnswer,
                        None,
                        None,
                    )?,
                    _ => reject(
                        &mut plan,
                        source_index,
                        "accepted answer already in requested state",
                    )?,
                }
            }
            MutationOperation::Split
            | MutationOperation::Merge
            | MutationOperation::Link
            | MutationOperation::AddListeningVariant => {
                let note = match mutation.operation {
                    MutationOperation::Split => {
                        "Split not supported in M2; proposed via redesign loop"
                    }
                    MutationOperation::Merge => {
                        "Merge not supported in M2; proposed via redesign loop"
                    }
                    MutationOperation::Link => {
                        "Link not supported in M2; proposed via redesign loop"
                    }
                    MutationOperation::AddListeningVariant => {
                        "AddListeningVariant not supported in M2; proposed via redesign loop"
                    }
                    _ => unreachable!(),
                };
                reject(&mut plan, source_index, note)?;
            }
        }
    }

    Ok(plan)
}

fn reject(
    plan: &mut CardPlan,
    source_index: usize,
    note: &'static str,
) -> Result<(), CardPlanError> {
    append_card_plan_entry(
        plan,
        source_index,
        CardVerdict::Rejected,
        CardEffectKind::None,
        None,
        Some(note),
    )
}

// JSON boundary only; no separate retained answer collection. The parser
// writes accepted-answer text directly into CardPlanState's existing arena.
struct AcceptedAnswerSeed<'a> {
    text: &'a mut String,
    values: &'a mut [TextRef],
    count: &'a mut usize,
}

impl<'de> serde::de::DeserializeSeed<'de> for AcceptedAnswerSeed<'_> {
    type Value = ();

    fn deserialize<D: serde::Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
        deserializer.deserialize_seq(self)
    }
}

impl<'de> serde::de::Visitor<'de> for AcceptedAnswerSeed<'_> {
    type Value = ();

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("a bounded array of accepted-answer strings")
    }

    fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
        while let Some(value) = seq.next_element::<String>()? {
            if *self.count == self.values.len() {
                return Err(<A::Error as serde::de::Error>::custom(
                    "accepted answers exceed fixed capacity",
                ));
            }
            self.values[*self.count] = push_text(self.text, &value);
            *self.count += 1;
        }
        Ok(())
    }
}

/// Decode directly into the lifetime owner and caller-provided compact offsets.
/// A parsing or capacity error restores the text arena to its prior length.
pub(super) fn decode_accepted_answers(
    json: &str,
    text: &mut String,
    refs: &mut [TextRef],
) -> Result<usize, serde_json::Error> {
    let previous_len = text.len();
    let mut count = 0usize;
    let mut deserializer = serde_json::Deserializer::from_str(json);
    let result = serde::de::DeserializeSeed::deserialize(
        AcceptedAnswerSeed {
            text,
            values: refs,
            count: &mut count,
        },
        &mut deserializer,
    )
    .and_then(|()| deserializer.end());
    if result.is_err() {
        text.truncate(previous_len);
    }
    result.map(|()| count)
}
