//! Card mutation workflow tests (12-testing.md §3): quality gate, duplicate
//! detection, activation budget, edit classes, audit completeness.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use application::{MutationContext, MutationEffect, Verdict, plan_card_mutations};
use learning_core::{
    Card, CardActor, CardId, CardIntent, CardStatus, LanguagePackId, LanguageProfileId,
    LanguageTag, Timestamp,
};
use tutor_contract::{CardMutation, CardPayload, MutationOperation};

fn existing_card(id: &str, answer: &str, status: CardStatus) -> Card {
    Card {
        id: CardId(id.to_owned()),
        language_profile_id: LanguageProfileId("llp-1".to_owned()),
        language_pack_id: LanguagePackId("es-ES".to_owned()),
        language_pack_version: semver::Version::new(0, 1, 0),
        intent: CardIntent::Production,
        status,
        prompt: "Ask for water".to_owned(),
        prompt_language: LanguageTag("ru".to_owned()),
        answer: answer.to_owned(),
        answer_language: LanguageTag("es".to_owned()),
        accepted_answers: vec!["dame agua".to_owned()],
        answer_evaluation_mode: "exact".to_owned(),
        explanation: None,
        pronunciation_note: None,
        source_learning_item_id: None,
        source_error_target_id: None,
        source_lesson_id: None,
        creation_reason: "test".to_owned(),
        priority_score: None,
        content_hash: Card::content_hash_of(
            CardIntent::Production,
            "Ask for water",
            answer,
            &["dame agua".to_owned()],
        ),
        content_version: 1,
        parent_card_id: None,
        suspended_reason: None,
        created_by: CardActor::Tutor,
    }
}

fn payload(prompt: &str, answer: &str) -> CardPayload {
    CardPayload {
        intent: CardIntent::Production,
        prompt: prompt.to_owned(),
        answer: answer.to_owned(),
        accepted_answers: vec!["dame agua".to_owned()],
        prompt_language: Some("ru".to_owned()),
        explanation: None,
        scenario_tags: vec![],
        skill_tags: vec![],
        source_learning_item_id: None,
        source_error_target_id: None,
        creation_reason: "failed retrieval".to_owned(),
        priority_score: Some(0.8),
    }
}

fn create(op_id: &str, prompt: &str, answer: &str) -> CardMutation {
    CardMutation {
        operation: MutationOperation::Create,
        client_operation_id: op_id.to_owned(),
        card_id: None,
        card: Some(payload(prompt, answer)),
        accepted_answer: None,
        reason: None,
    }
}

fn ctx(existing: Vec<Card>, budget: u8) -> MutationContext {
    MutationContext {
        existing_cards: existing,
        remaining_activation_budget: budget,
        now: Timestamp(1_780_000_000),
    }
}

#[test]
fn duplicate_create_rejected_never_silent() {
    let existing = existing_card("crd-1", "¿Me da agua?", CardStatus::Review);
    let mutations = [create("op-1", "Ask for water", "¿Me da agua?")];
    let planned = plan_card_mutations(&mutations, &ctx(vec![existing], 8));
    assert_eq!(planned[0].verdict, Verdict::Rejected);
    assert!(planned[0].note.as_deref().unwrap().contains("crd-1"));
    assert!(planned[0].effect.is_none());
}

#[test]
fn in_batch_duplicate_rejected() {
    let mutations = [
        create("op-1", "Ask for water", "¿Me da agua?"),
        create("op-2", "ask for WATER", "¿me da agua?"), // same content hash
    ];
    let planned = plan_card_mutations(&mutations, &ctx(vec![], 8));
    assert_eq!(planned[0].verdict, Verdict::Applied);
    assert_eq!(planned[1].verdict, Verdict::Rejected);
}

#[test]
fn activation_budget_exhaustion_leaves_candidates() {
    let mutations = [
        create("op-1", "p1", "a1"),
        create("op-2", "p2", "a2"),
        create("op-3", "p3", "a3"),
    ];
    let planned = plan_card_mutations(&mutations, &ctx(vec![], 2));
    let statuses: Vec<_> = planned
        .iter()
        .map(|p| match p.effect.as_ref().unwrap() {
            MutationEffect::CreateCard { initial_status, .. } => *initial_status,
            other => panic!("unexpected effect {other:?}"),
        })
        .collect();
    assert_eq!(
        statuses,
        [CardStatus::New, CardStatus::New, CardStatus::Candidate]
    );
    assert_eq!(planned[2].verdict, Verdict::Warned);
}

#[test]
fn semantic_update_rejected_clarifying_applied() {
    let existing = existing_card("crd-1", "¿Me da agua?", CardStatus::Review);
    let clarifying = CardMutation {
        operation: MutationOperation::Update,
        client_operation_id: "op-1".to_owned(),
        card_id: Some("crd-1".to_owned()),
        card: Some(payload("Ask politely for water", "¿Me da agua?")),
        accepted_answer: None,
        reason: None,
    };
    let semantic = CardMutation {
        card: Some(payload("Ask for water", "¿Me pone un vaso de agua?")),
        client_operation_id: "op-2".to_owned(),
        ..clarifying.clone()
    };
    let planned = plan_card_mutations(&[clarifying, semantic], &ctx(vec![existing], 8));
    assert_eq!(
        planned[0].verdict,
        Verdict::Applied,
        "clarifying edit: same answer"
    );
    assert_eq!(
        planned[1].verdict,
        Verdict::Rejected,
        "semantic edit blocked"
    );
    assert!(planned[1].note.as_deref().unwrap().contains("semantic"));
}

#[test]
fn status_transitions_validated() {
    let suspended = existing_card("crd-s", "a1", CardStatus::Suspended);
    let retired = existing_card("crd-r", "a2", CardStatus::Retired);
    let make = |op, card_id: &str, op_id: &str| CardMutation {
        operation: op,
        client_operation_id: op_id.to_owned(),
        card_id: Some(card_id.to_owned()),
        card: None,
        accepted_answer: None,
        reason: Some("test".to_owned()),
    };
    let planned = plan_card_mutations(
        &[
            make(MutationOperation::Restore, "crd-s", "op-1"),
            make(MutationOperation::Suspend, "crd-r", "op-2"), // retired → invalid
            make(MutationOperation::Retire, "crd-s", "op-3"),
            make(MutationOperation::Suspend, "crd-ghost", "op-4"),
        ],
        &ctx(vec![suspended, retired], 8),
    );
    assert_eq!(planned[0].verdict, Verdict::Applied);
    assert_eq!(planned[1].verdict, Verdict::Rejected);
    assert_eq!(planned[2].verdict, Verdict::Applied);
    assert_eq!(planned[3].verdict, Verdict::Rejected, "unknown card");
}

#[test]
fn accepted_answer_add_remove_idempotent_guard() {
    let existing = existing_card("crd-1", "¿Me da agua?", CardStatus::Review);
    let make = |op, answer: &str, op_id: &str| CardMutation {
        operation: op,
        client_operation_id: op_id.to_owned(),
        card_id: Some("crd-1".to_owned()),
        card: None,
        accepted_answer: Some(answer.to_owned()),
        reason: None,
    };
    let planned = plan_card_mutations(
        &[
            make(MutationOperation::AddAcceptedAnswer, "quiero agua", "op-1"),
            make(MutationOperation::AddAcceptedAnswer, "Dame agua", "op-2"), // exists
            make(MutationOperation::RemoveAcceptedAnswer, "dame agua", "op-3"),
            make(MutationOperation::RemoveAcceptedAnswer, "no existe", "op-4"),
        ],
        &ctx(vec![existing], 8),
    );
    assert_eq!(planned[0].verdict, Verdict::Applied);
    assert_eq!(planned[1].verdict, Verdict::Rejected);
    assert_eq!(planned[2].verdict, Verdict::Applied);
    assert_eq!(planned[3].verdict, Verdict::Rejected);
}

#[test]
fn unsupported_m2_operations_audited_as_rejected() {
    let mutations = [CardMutation {
        operation: MutationOperation::Split,
        client_operation_id: "op-1".to_owned(),
        card_id: Some("crd-1".to_owned()),
        card: None,
        accepted_answer: None,
        reason: None,
    }];
    let planned = plan_card_mutations(&mutations, &ctx(vec![], 8));
    assert_eq!(planned.len(), 1, "every attempt gets an audit row");
    assert_eq!(planned[0].verdict, Verdict::Rejected);
}
