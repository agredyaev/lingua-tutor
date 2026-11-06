use adaptation::{evidence_capability, new_card_budget, DailySignal};
use review_engine::{
    apply_review, initial_state, new_scheduler_scratch, Grade, ReviewEventInput, SchedulerSettings,
    Timestamp, FALLBACK_ID, FSRS_ID, MIN_INTERVAL_SECONDS,
};

fn text(data: &[u8]) -> String {
    String::from_utf8_lossy(data).into_owned()
}

pub fn exercise(data: &[u8]) {
    let a = text(data.get(..data.len().min(128)).unwrap_or(data));
    let split = data.len() / 2;
    let b = text(&data[split..]);

    let intent = match data.first().copied().unwrap_or(0) % 4 {
        0 => learning_core::CardIntent::Production,
        1 => learning_core::CardIntent::Listening,
        2 => learning_core::CardIntent::Contrast,
        _ => learning_core::CardIntent::Transfer,
    };
    let mut answers = vec![a.clone(), b.clone()];
    let hash_a = learning_core::card_content_hash(intent, &a, &b, &answers);
    answers.reverse();
    let hash_b = learning_core::card_content_hash(intent, &a, &b, &answers);
    assert_eq!(hash_a, hash_b);

    let count = u32::from(data.get(3).copied().unwrap_or(0));
    let resolved = data.get(4).copied().unwrap_or(0) & 1 == 1;
    let _ = learning_core::error_escalation_due(count, resolved);
    let _ = learning_core::error_method_change_due(count, resolved);

    let base = data.get(5).copied().unwrap_or(8).min(64);
    let yesterday = data.get(6).copied().unwrap_or(base).min(64);
    let overdue = u32::from(data.get(7).copied().unwrap_or(0));
    let daily = DailySignal {
        review_minutes: u16::from(data.get(8).copied().unwrap_or(0)),
        recall_rate: f64::from(data.get(9).copied().unwrap_or(128)) / 255.0,
    };
    let budget = new_card_budget(base, yesterday, overdue, 12, Some((daily, daily)));
    assert!(budget <= base);
    let _ = evidence_capability(&a);

    let _ = Grade::try_from(data.first().copied().unwrap_or(0));
    let _ = review_engine::new_scheduler_scratch(99);
    let _ = review_engine::initial_state(99, Timestamp(0));
    for scheduler in [FSRS_ID, FALLBACK_ID] {
        let scratch = new_scheduler_scratch(scheduler).expect("scheduler scratch");
        let settings = SchedulerSettings::default();
        let mut state = initial_state(scheduler, Timestamp(1_800_000_000)).expect("initial state");
        let _ =
            review_engine::predicted_retrievability(scheduler, &state, Timestamp(1_800_000_000));
        let _ = review_engine::predicted_retrievability(99, &state, Timestamp(0));
        let _ = review_engine::apply_review(
            99,
            &scratch,
            &state,
            &ReviewEventInput {
                grade: Grade::Good,
                answered_at: Timestamp(0),
            },
            &settings,
        );
        let mut events = Vec::new();
        let mut now = 1_800_000_000_i64;
        for byte in data.iter().copied().take(32) {
            now = now.saturating_add(600 + i64::from(byte) * 60);
            let grade = Grade::try_from(byte % 4 + 1).expect("bounded grade");
            let event = ReviewEventInput {
                grade,
                answered_at: Timestamp(now),
            };
            events.push(event);
            let next = apply_review(scheduler, &scratch, &state, &event, &settings)
                .expect("valid review sequence");
            let repeat = apply_review(scheduler, &scratch, &state, &event, &settings)
                .expect("deterministic repeat");
            assert_eq!(next, repeat);
            let interval = next.scheduled_interval_seconds.expect("interval");
            assert!(interval >= MIN_INTERVAL_SECONDS);
            assert_eq!(next.due_at.expect("due").0, now + interval);
            assert!(next.state_version > state.state_version);
            let recall =
                review_engine::predicted_retrievability(scheduler, &next, Timestamp(now + 3600))
                    .expect("recall");
            assert!((0.0..=1.0).contains(&recall));
            state = next;
        }
        assert_eq!(
            state,
            review_engine::replay(scheduler, Timestamp(1_800_000_000), &events, &settings)
                .expect("replay")
        );
    }
}
