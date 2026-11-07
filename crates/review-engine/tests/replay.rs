//! Deterministic replay (05-review-engine.md §7).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use review_engine::{
    Grade, ReviewEventInput, SchedulerSettings, Timestamp, apply_review, initial_state,
    new_scheduler_scratch, replay,
};
const CREATED: Timestamp = Timestamp(1_780_000_000);
#[test]
fn zero_event_replay_reproduces_activation_state() {
    let original = initial_state(CREATED);
    let replayed = replay(CREATED, &[], &SchedulerSettings::default()).unwrap();
    assert_eq!(
        review_engine::state_hash(&replayed),
        review_engine::state_hash(&original)
    );
    assert_eq!(replayed, original);
}
#[test]
fn replay_matches_sequential_application() {
    let scratch = new_scheduler_scratch().unwrap();
    let settings = SchedulerSettings::default();
    let events = [
        ReviewEventInput {
            grade: Grade::Good,
            answered_at: Timestamp(CREATED.0 + 60),
        },
        ReviewEventInput {
            grade: Grade::Again,
            answered_at: Timestamp(CREATED.0 + 90_000),
        },
        ReviewEventInput {
            grade: Grade::Easy,
            answered_at: Timestamp(CREATED.0 + 200_000),
        },
    ];
    let mut expected = initial_state(CREATED);
    for event in &events {
        expected = apply_review(&scratch, &expected, event, &settings).unwrap();
    }
    let replayed = replay(CREATED, &events, &settings).unwrap();
    assert_eq!(
        review_engine::state_hash(&replayed),
        review_engine::state_hash(&expected)
    );
    assert_eq!(replayed, expected);
}
#[test]
fn replay_is_a_pure_function_of_its_inputs() {
    let settings = SchedulerSettings::default();
    let events = [ReviewEventInput {
        grade: Grade::Hard,
        answered_at: Timestamp(CREATED.0 + 3600),
    }];
    assert_eq!(
        replay(CREATED, &events, &settings).unwrap(),
        replay(CREATED, &events, &settings).unwrap()
    );
}
