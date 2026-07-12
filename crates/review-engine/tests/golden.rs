//! Golden scheduler vectors (12-testing.md §2): 4 grades from fresh, initial
//! schedule, lapse. FSRS expectations recorded from fsrs-rs 6.6.1 with
//! DEFAULT_PARAMETERS (spike, docs/adr/0004-fsrs-go.md); any change on a
//! crate bump is a breaking algorithm change and must be a conscious decision.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use review_engine::{
    FallbackScheduler, FsrsScheduler, Grade, InitialReviewInput, ReviewEventInput, ReviewScheduler,
    ScheduledState, SchedulerSettings, Timestamp,
};

const T0: Timestamp = Timestamp(1_780_000_000);

fn fresh(scheduler: &dyn ReviewScheduler) -> ScheduledState {
    scheduler
        .initial_state(InitialReviewInput { now: T0 })
        .unwrap()
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
    for scheduler in schedulers() {
        let state = fresh(scheduler.as_ref());
        assert_eq!(state.due_at, Some(T0));
        assert_eq!(state.review_count, 0);
        assert_eq!(state.scheduled_interval_seconds, None);
    }
}

#[test]
fn fsrs_four_grades_from_fresh_match_recorded_vectors() {
    let scheduler = FsrsScheduler::new().unwrap();
    let settings = SchedulerSettings::default();
    // (grade, stability, difficulty, interval_days) from fsrs-rs 6.6.1 defaults.
    let expected = [
        (Grade::Again, 0.212, 6.4133, 0.212),
        (Grade::Hard, 1.2931, 5.112_170_7, 1.2931),
        (Grade::Good, 2.3065, 2.118_104, 2.3065),
        (Grade::Easy, 8.2956, 1.0, 8.2956),
    ];
    for (grade, stability, difficulty, interval_days) in expected {
        let state = scheduler
            .apply_review(&fresh(&scheduler), &review(grade, T0), &settings)
            .unwrap();
        approx(state.stability.unwrap(), stability);
        approx(state.difficulty.unwrap(), difficulty);
        let expected_seconds = (interval_days * 86_400.0_f64).round().max(600.0) as i64;
        assert_eq!(
            state.scheduled_interval_seconds,
            Some(expected_seconds),
            "{grade:?}"
        );
        assert_eq!(state.due_at, Some(Timestamp(T0.0 + expected_seconds)));
        assert_eq!(state.review_count, 1);
        assert_eq!(state.lapse_count, u32::from(grade == Grade::Again));
    }
}

#[test]
fn fsrs_lapse_after_good_matches_recorded_vector() {
    let scheduler = FsrsScheduler::new().unwrap();
    let settings = SchedulerSettings::default();
    let good = scheduler
        .apply_review(&fresh(&scheduler), &review(Grade::Good, T0), &settings)
        .unwrap();
    let two_days_later = Timestamp(T0.0 + 2 * 86_400);
    let lapsed = scheduler
        .apply_review(&good, &review(Grade::Again, two_days_later), &settings)
        .unwrap();
    approx(lapsed.stability.unwrap(), 0.607_580_1);
    approx(lapsed.difficulty.unwrap(), 7.394_502);
    assert_eq!(lapsed.lapse_count, 1);
    assert_eq!(lapsed.review_count, 2);
    assert_eq!(lapsed.state_version, good.state_version + 1);
}

#[test]
fn fsrs_retrievability_after_good_at_two_days() {
    let scheduler = FsrsScheduler::new().unwrap();
    let settings = SchedulerSettings::default();
    let good = scheduler
        .apply_review(&fresh(&scheduler), &review(Grade::Good, T0), &settings)
        .unwrap();
    let r = scheduler
        .predicted_retrievability(&good, Timestamp(T0.0 + 2 * 86_400))
        .unwrap();
    approx(r, 0.909_493_2);
}

#[test]
fn fallback_four_grades_from_fresh() {
    let scheduler = FallbackScheduler;
    let settings = SchedulerSettings::default();
    let expected = [
        (Grade::Again, 600),
        (Grade::Hard, 86_400),
        (Grade::Good, 86_400),
        (Grade::Easy, 172_800),
    ];
    for (grade, seconds) in expected {
        let state = scheduler
            .apply_review(&fresh(&scheduler), &review(grade, T0), &settings)
            .unwrap();
        assert_eq!(state.scheduled_interval_seconds, Some(seconds), "{grade:?}");
        assert_eq!(state.due_at, Some(Timestamp(T0.0 + seconds)));
    }
}

#[test]
fn fallback_growth_and_lapse_sequence() {
    let scheduler = FallbackScheduler;
    let settings = SchedulerSettings::default();
    let mut state = fresh(&scheduler);
    let mut at = T0;
    // Good, Good: 1d -> 2.5d
    for expected in [86_400, 216_000] {
        at = Timestamp(at.0 + 86_400);
        state = scheduler
            .apply_review(&state, &review(Grade::Good, at), &settings)
            .unwrap();
        assert_eq!(state.scheduled_interval_seconds, Some(expected));
    }
    // Lapse: back to 10 minutes, lapse counted.
    at = Timestamp(at.0 + 86_400);
    state = scheduler
        .apply_review(&state, &review(Grade::Again, at), &settings)
        .unwrap();
    assert_eq!(state.scheduled_interval_seconds, Some(600));
    assert_eq!(state.lapse_count, 1);
    // Retrievability at due instant == 0.9 by construction.
    let r = scheduler
        .predicted_retrievability(&state, Timestamp(at.0 + 600))
        .unwrap();
    assert!((r - 0.9).abs() < 1e-9);
}

#[test]
fn state_hash_changes_on_any_review_and_is_stable() {
    for scheduler in schedulers() {
        let settings = SchedulerSettings::default();
        let initial = fresh(scheduler.as_ref());
        let reviewed = scheduler
            .apply_review(&initial, &review(Grade::Good, T0), &settings)
            .unwrap();
        assert_eq!(initial.state_hash(), initial.state_hash(), "deterministic");
        assert_ne!(
            initial.state_hash(),
            reviewed.state_hash(),
            "review must change hash"
        );
    }
}

fn schedulers() -> Vec<Box<dyn ReviewScheduler>> {
    vec![
        Box::new(FsrsScheduler::new().unwrap()),
        Box::new(FallbackScheduler),
    ]
}
