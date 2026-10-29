//! Pure mutation planning, including invalid requests rejected before SQL admission.
use super::*;
use crate::operations::card_plan::*;

/// Fuzz bounded mutation plans independently of SQLite admission.
pub fn exercise_card_model(data: &[u8]) {
    let seed = data.first().copied().unwrap_or(0);
    lesson_model(seed);
    let mut state = new_card_plan_state(seed % 4);
    let statuses = [
        "new",
        "review",
        "suspended",
        "retired",
        "candidate",
        "learning",
    ];
    for (i, status) in statuses.iter().enumerate() {
        let id = format!("card-{i}");
        let answers = if seed & 1 == 0 {
            r#"["one","two","ni\u00f1o","quote\"","line\nbreak"]"#
        } else {
            r#"["one","two"]"#
        };
        add_existing_card_json(
            &mut state,
            &id,
            status,
            "production",
            "prompt",
            "answer",
            answers,
        )
        .expect("valid stored card");
        let index = find_card(&state, &id).expect("inserted id");
        assert_eq!(card_prompt(&state, index), "prompt");
        assert_eq!(card_answer(&state, index), "answer");
        for j in 0..card_accepted_len(&state, index) {
            assert!(!card_accepted_at(&state, index, j).is_empty());
        }
    }
    for bad in ["null", "[42]", r#"["bad", "\q"]"#, r#"["partial""#] {
        assert!(
            add_existing_card_json(&mut state, "bad", "review", "unknown", "p", "a", bad).is_err()
        );
        assert!(find_card(&state, "bad").is_none());
    }
    let payload = rich_lesson_result("p", "0.2.0", "op".into(), None, 0).card_mutations[0]
        .card
        .clone()
        .expect("payload");
    let operations = [
        tutor_contract::MutationOperation::Create,
        tutor_contract::MutationOperation::Update,
        tutor_contract::MutationOperation::Suspend,
        tutor_contract::MutationOperation::Restore,
        tutor_contract::MutationOperation::Retire,
        tutor_contract::MutationOperation::AddAcceptedAnswer,
        tutor_contract::MutationOperation::RemoveAcceptedAnswer,
        tutor_contract::MutationOperation::Split,
        tutor_contract::MutationOperation::Merge,
        tutor_contract::MutationOperation::Link,
        tutor_contract::MutationOperation::AddListeningVariant,
    ];
    let input = if data.is_empty() { &[0][..] } else { data };
    let mut mutations = Vec::new();
    for (i, b) in input
        .iter()
        .copied()
        .take(tutor_contract::bounds::MAX_CARD_MUTATIONS)
        .enumerate()
    {
        let op = operations[usize::from(b) % operations.len()];
        let mut card = payload.clone();
        card.prompt = "prompt".into();
        card.answer = if b & 32 == 0 {
            "answer".into()
        } else {
            "changed".into()
        };
        let mutation = tutor_contract::CardMutation {
            operation: op,
            client_operation_id: format!("m-{i}"),
            card_id: if b & 64 != 0 {
                None
            } else {
                Some(format!("card-{}", b % 8))
            },
            card: if b & 128 == 0 { Some(card) } else { None },
            accepted_answer: match b % 4 {
                0 => None,
                1 => Some("".into()),
                2 => Some("one".into()),
                _ => Some("new".into()),
            },
            reason: None,
        };
        if b & 16 == 0 {
            set_card_content_hash(&mut state, i, "duplicate", b & 8 != 0).expect("admitted hash");
        }
        mutations.push(mutation);
    }
    mark_existing_card_hash(&mut state, "missing");
    let a = plan_card_mutations(&mutations, &state).expect("bounded mutation plan");
    let b = plan_card_mutations(&mutations, &state).expect("deterministic plan");
    assert_eq!(a.len, mutations.len());
    for i in 0..a.len {
        assert_eq!(a.verdict[i], b.verdict[i]);
        assert_eq!(a.effect[i], b.effect[i]);
        assert_eq!(card_plan_source_index(&a, i), i);
        let _ = card_plan_status(&a, i);
        let _ = card_plan_note(&a, i);
    }
    assert!(
        set_card_content_hash(
            &mut state,
            tutor_contract::bounds::MAX_CARD_MUTATIONS,
            "x",
            false
        )
        .is_err()
    );
}

pub(super) fn lesson_model(seed: u8) {
    use crate::operations::lesson_plan::*;
    let mut state = new_lesson_plan_state("p", "es-ES", u32::from(seed));
    let mut result = rich_lesson_result("p", "0.2.0", "pure-plan".into(), None, 0);
    add_known_lesson_item(&mut state, "li-fuzz").expect("known item");
    for (i, item) in result.new_learning_items.iter().enumerate() {
        set_lesson_normalized_item(&mut state, i, &item.target_text).expect("normalize item");
    }
    add_lesson_mastery_signal(
        &mut state,
        "li-fuzz",
        seed & 1 == 0,
        &["family-a", "family-b", "family-c", "family-a", ""],
    )
    .expect("mastery history");
    set_lesson_mastery_latest_transfer_failed(&mut state, "li-fuzz", seed & 2 != 0)
        .expect("transfer history");
    result.mastery_changes[0].new_status = match seed % 7 {
        0 => learning_core::LearningItemStatus::Candidate,
        1 => learning_core::LearningItemStatus::Introduced,
        2 => learning_core::LearningItemStatus::Learning,
        3 => learning_core::LearningItemStatus::TransferPending,
        4 => learning_core::LearningItemStatus::ProvisionallyMastered,
        5 => learning_core::LearningItemStatus::Mastered,
        _ => learning_core::LearningItemStatus::Suspended,
    };
    let _ = plan_lesson_completion(&result, &state);
}
