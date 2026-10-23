//! Deterministic fallback scheduler (05-review-engine.md §2).
//! multiplicative scheme, integer arithmetic only, always available. Ships
//! regardless of the FSRS gate outcome; same trait, same persisted shape.

use crate::{
    Grade, InitialReviewInput, ReviewEventInput, ReviewScheduler, ScheduleError, ScheduledState,
    SchedulerSettings, Timestamp, clamp_interval,
};

const ALGORITHM: &str = "fallback";
const ALGORITHM_VERSION: &str = "fallback-1";
const DAY: i64 = 86_400;

/// Grade-driven multiplicative intervals: Again → 10 min (relearning),
/// Hard → ×1.2, Good → ×2.5, Easy → ×3.5, floored at 1 day (2 days for Easy).
#[derive(Debug, Default, Clone, Copy)]
pub struct FallbackScheduler;

impl FallbackScheduler {
    fn next_interval_seconds(previous: i64, grade: Grade) -> i64 {
        match grade {
            Grade::Again => crate::MIN_INTERVAL_SECONDS,
            Grade::Hard => (previous * 6 / 5).max(DAY),
            Grade::Good => (previous * 5 / 2).max(DAY),
            Grade::Easy => (previous * 7 / 2).max(2 * DAY),
        }
    }
}

impl ReviewScheduler for FallbackScheduler {
    fn initial_state(&self, input: InitialReviewInput) -> Result<ScheduledState, ScheduleError> {
        Ok(ScheduledState {
            algorithm: ALGORITHM.to_owned(),
            algorithm_version: ALGORITHM_VERSION.to_owned(),
            parameter_set_id: None,
            difficulty: None,
            stability: None,
            last_review_at: None,
            due_at: Some(input.now),
            scheduled_interval_seconds: None,
            lapse_count: 0,
            review_count: 0,
            state_version: 1,
        })
    }

    fn apply_review(
        &self,
        current: &ScheduledState,
        event: &ReviewEventInput,
        settings: &SchedulerSettings,
    ) -> Result<ScheduledState, ScheduleError> {
        let previous = current.scheduled_interval_seconds.unwrap_or(0);
        let raw = Self::next_interval_seconds(previous, event.grade);
        #[allow(clippy::cast_precision_loss)] // intervals « 2^52
        let interval_seconds = clamp_interval(raw as f64, settings, ALGORITHM)?;
        Ok(ScheduledState {
            algorithm: ALGORITHM.to_owned(),
            algorithm_version: ALGORITHM_VERSION.to_owned(),
            parameter_set_id: current.parameter_set_id.clone(),
            difficulty: None,
            stability: None,
            last_review_at: Some(event.answered_at),
            due_at: Some(Timestamp(event.answered_at.0 + interval_seconds)),
            scheduled_interval_seconds: Some(interval_seconds),
            lapse_count: current.lapse_count + u32::from(event.grade == Grade::Again),
            review_count: current.review_count + 1,
            state_version: current.state_version + 1,
        })
    }

    /// Exponential decay pinned to the scheduled interval: R = 0.9^(t/interval),
    /// so retrievability is exactly `desired_retention`'s default at the due
    /// instant. Used for deterministic queue ordering before FSRS state exists.
    fn predicted_retrievability(
        &self,
        state: &ScheduledState,
        at: Timestamp,
    ) -> Result<f64, ScheduleError> {
        let last = state
            .last_review_at
            .ok_or_else(|| ScheduleError::InvalidState("no last review".to_owned()))?;
        let interval = state
            .scheduled_interval_seconds
            .filter(|i| *i > 0)
            .ok_or_else(|| ScheduleError::InvalidState("no scheduled interval".to_owned()))?;
        #[allow(clippy::cast_precision_loss)] // second counts « 2^52
        let elapsed = (at.0.saturating_sub(last.0)).max(0) as f64 / interval as f64;
        Ok(0.9_f64.powf(elapsed))
    }
}
