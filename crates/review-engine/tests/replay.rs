//! Deterministic replay (05-review-engine.md §7): undo recomputes state by
//! replaying surviving events from the card's activation instant. The result
//! must be byte-identical to the pre-grade state no matter when the replay
//! runs — a wall-clock-seeded replay caused stale-hash 409s (e2e flake).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use review_engine::{
    FsrsScheduler, Grade, InitialReviewInput, ReviewEventInput, ReviewScheduler, SchedulerSettings,
    Timestamp, replay,
};

const CREATED: Timestamp = Timestamp(1_780_000_000);

#[test]
fn zero_event_replay_reproduces_activation_state() {
    let scheduler = FsrsScheduler::new().unwrap();
    let original = scheduler
        .initial_state(InitialReviewInput { now: CREATED })
        .unwrap();
    let replayed = replay(&scheduler, CREATED, &[], &SchedulerSettings::default()).unwrap();
    assert_eq!(replayed.state_hash(), original.state_hash());
    assert_eq!(replayed, original);
}

#[test]
fn replay_matches_sequential_application() {
    let scheduler = FsrsScheduler::new().unwrap();
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
    let mut expected = scheduler
        .initial_state(InitialReviewInput { now: CREATED })
        .unwrap();
    for event in &events {
        expected = scheduler.apply_review(&expected, event, &settings).unwrap();
    }
    let replayed = replay(&scheduler, CREATED, &events, &settings).unwrap();
    assert_eq!(replayed.state_hash(), expected.state_hash());
    assert_eq!(replayed, expected);
}

#[test]
fn replay_is_a_pure_function_of_its_inputs() {
    let scheduler = FsrsScheduler::new().unwrap();
    let settings = SchedulerSettings::default();
    let events = [ReviewEventInput {
        grade: Grade::Hard,
        answered_at: Timestamp(CREATED.0 + 3600),
    }];
    let first = replay(&scheduler, CREATED, &events, &settings).unwrap();
    let second = replay(&scheduler, CREATED, &events, &settings).unwrap();
    assert_eq!(first, second);
}
