//! Golden FSRS scheduler vectors recorded from fsrs-rs 6.6.1 defaults.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use review_engine::{
    Grade, ReviewEventInput, ScheduledState, SchedulerSettings, Timestamp, apply_review,
    initial_state, new_scheduler_scratch, predicted_retrievability,
};
const T0: Timestamp = Timestamp(1_780_000_000);
fn fresh() -> ScheduledState {
    initial_state(T0)
}
fn review(grade: Grade, at: Timestamp) -> ReviewEventInput {
    ReviewEventInput {
        grade,
        answered_at: at,
    }
}
fn approx(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 1e-4,
        "expected {expected}, got {actual}"
    );
}
#[test]
fn initial_state_is_due_immediately() {
    let state = fresh();
    assert_eq!(state.due_at, Some(T0));
    assert_eq!(state.review_count, 0);
    assert_eq!(state.scheduled_interval_seconds, None);
}
#[test]
fn fsrs_four_grades_from_fresh_match_recorded_vectors() {
    let scratch = new_scheduler_scratch().unwrap();
    let settings = SchedulerSettings::default();
    let expected = [
        (Grade::Again, 0.212, 6.4133, 0.212),
        (Grade::Hard, 1.2931, 5.112_170_7, 1.2931),
        (Grade::Good, 2.3065, 2.118_104, 2.3065),
        (Grade::Easy, 8.2956, 1.0, 8.2956),
    ];
    for (grade, stability, difficulty, interval_days) in expected {
        let state = apply_review(&scratch, &fresh(), &review(grade, T0), &settings).unwrap();
        approx(state.stability.unwrap(), stability);
        approx(state.difficulty.unwrap(), difficulty);
        let expected_seconds = (interval_days * 86_400.0_f64).round().max(600.0) as i64;
        assert_eq!(state.scheduled_interval_seconds, Some(expected_seconds));
        assert_eq!(state.due_at, Some(Timestamp(T0.0 + expected_seconds)));
        assert_eq!(state.review_count, 1);
        assert_eq!(state.lapse_count, u32::from(grade == Grade::Again));
    }
}
#[test]
fn fsrs_lapse_after_good_matches_recorded_vector() {
    let scratch = new_scheduler_scratch().unwrap();
    let settings = SchedulerSettings::default();
    let good = apply_review(&scratch, &fresh(), &review(Grade::Good, T0), &settings).unwrap();
    let two_days_later = Timestamp(T0.0 + 2 * 86_400);
    let lapsed = apply_review(
        &scratch,
        &good,
        &review(Grade::Again, two_days_later),
        &settings,
    )
    .unwrap();
    approx(lapsed.stability.unwrap(), 0.607_580_1);
    approx(lapsed.difficulty.unwrap(), 7.394_502);
    assert_eq!(lapsed.lapse_count, 1);
    assert_eq!(lapsed.review_count, 2);
}
#[test]
fn fsrs_retrievability_after_good_at_two_days() {
    let scratch = new_scheduler_scratch().unwrap();
    let settings = SchedulerSettings::default();
    let good = apply_review(&scratch, &fresh(), &review(Grade::Good, T0), &settings).unwrap();
    approx(
        predicted_retrievability(&good, Timestamp(T0.0 + 2 * 86_400)).unwrap(),
        0.909_493_2,
    );
}
#[test]
fn state_hash_changes_on_review_and_is_stable() {
    let scratch = new_scheduler_scratch().unwrap();
    let initial = fresh();
    let reviewed = apply_review(
        &scratch,
        &initial,
        &review(Grade::Good, T0),
        &SchedulerSettings::default(),
    )
    .unwrap();
    assert_eq!(
        review_engine::state_hash(&initial),
        review_engine::state_hash(&initial)
    );
    assert_ne!(
        review_engine::state_hash(&initial),
        review_engine::state_hash(&reviewed)
    );
}

#[test]
fn v7_state_hash_layout_survives_parameter_metadata_cleanup() {
    let state = initial_state(T0);
    assert_eq!(
        review_engine::state_hash(&state),
        "76f85c283c7af879",
        "removing unused scheduler metadata must not invalidate persisted v7 state hashes"
    );
}
