//! Fixed-seed property suite for FSRS scheduling invariants.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use proptest::prelude::*;
use proptest::test_runner::{Config, RngAlgorithm, TestRng, TestRunner};
use review_engine::{
    Grade, MIN_INTERVAL_SECONDS, ReviewEventInput, SchedulerSettings, Timestamp, apply_review,
    initial_state, new_scheduler_scratch, predicted_retrievability,
};
const T0: i64 = 1_780_000_000;
fn grade_strategy() -> impl Strategy<Value = Grade> {
    prop_oneof![
        Just(Grade::Again),
        Just(Grade::Hard),
        Just(Grade::Good),
        Just(Grade::Easy)
    ]
}
fn sequence_strategy() -> impl Strategy<Value = Vec<(Grade, i64)>> {
    prop::collection::vec((grade_strategy(), 0_i64..400 * 86_400), 1..40)
}
#[test]
fn fsrs_never_produces_invalid_intervals() {
    let scratch = new_scheduler_scratch().unwrap();
    let settings = SchedulerSettings::default();
    let max_seconds = i64::from(settings.max_interval_days) * 86_400;
    let mut runner = TestRunner::new_with_rng(
        Config {
            cases: 128,
            failure_persistence: None,
            ..Config::default()
        },
        TestRng::deterministic_rng(RngAlgorithm::ChaCha),
    );
    runner
        .run(&sequence_strategy(), |seq| {
            let mut state = initial_state(Timestamp(T0));
            let mut now = T0;
            let mut lapses = 0_u32;
            for (i, (grade, gap)) in seq.iter().enumerate() {
                now += gap;
                let event = ReviewEventInput {
                    grade: *grade,
                    answered_at: Timestamp(now),
                };
                let next = apply_review(&scratch, &state, &event, &settings).unwrap();
                let repeat = apply_review(&scratch, &state, &event, &settings).unwrap();
                prop_assert_eq!(&next, &repeat);
                let interval = next.scheduled_interval_seconds.unwrap();
                prop_assert!((MIN_INTERVAL_SECONDS..=max_seconds).contains(&interval));
                prop_assert_eq!(next.due_at.unwrap().0, now + interval);
                prop_assert_eq!(next.review_count, u32::try_from(i).unwrap() + 1);
                lapses += u32::from(*grade == Grade::Again);
                prop_assert_eq!(next.lapse_count, lapses);
                prop_assert!(next.state_version > state.state_version);
                prop_assert_ne!(
                    review_engine::state_hash(&next),
                    review_engine::state_hash(&state)
                );
                if let Some(stability) = next.stability {
                    prop_assert!(stability.is_finite() && stability > 0.0);
                }
                let r = predicted_retrievability(&next, Timestamp(now + 86_400)).unwrap();
                prop_assert!((0.0..=1.0).contains(&r));
                state = next;
            }
            Ok(())
        })
        .expect("scheduler invariant violated");
}
