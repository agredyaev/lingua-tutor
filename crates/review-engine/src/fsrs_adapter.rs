//! FSRS adapter: wraps the pinned `fsrs` crate behind [`ReviewScheduler`].
//! GO decision + wasm32/size/determinism evidence: docs/adr/0004-fsrs-go.md.

use crate::{
    Grade, InitialReviewInput, ReviewEventInput, ReviewScheduler, ScheduleError, ScheduledState,
    SchedulerSettings, Timestamp, clamp_interval,
};

const ALGORITHM: &str = "fsrs";
const ALGORITHM_VERSION: &str = "fsrs-rs-6.6.1";

/// FSRS-6 scheduler with the crate's reviewed default parameters
/// (`scheduler_parameter_sets.origin = 'default'`, 05-review-engine.md §11).
pub struct FsrsScheduler {
    inner: fsrs::FSRS,
    /// Kept for `current_retrievability` (decay lives in parameters[20]).
    decay: f32,
}

impl FsrsScheduler {
    /// Scheduler with FSRS-6 default parameters.
    ///
    /// # Errors
    ///
    /// [`ScheduleError::InvalidState`] when the parameter vector is rejected
    /// by the fsrs crate (cannot happen with the shipped defaults).
    pub fn new() -> Result<Self, ScheduleError> {
        let inner = fsrs::FSRS::new(&fsrs::DEFAULT_PARAMETERS)
            .map_err(|e| ScheduleError::InvalidState(format!("fsrs init: {e}")))?;
        Ok(Self {
            inner,
            decay: fsrs::DEFAULT_PARAMETERS[20],
        })
    }

    fn memory_of(state: &ScheduledState) -> Option<fsrs::MemoryState> {
        match (state.stability, state.difficulty) {
            (Some(s), Some(d)) => Some(fsrs::MemoryState {
                #[allow(clippy::cast_possible_truncation)] // fsrs is f32-native
                stability: s as f32,
                #[allow(clippy::cast_possible_truncation)]
                difficulty: d as f32,
            }),
            _ => None,
        }
    }

    fn elapsed_days(state: &ScheduledState, answered_at: Timestamp) -> u32 {
        state
            .last_review_at
            .map(|last| (answered_at.0.saturating_sub(last.0)).max(0) / 86_400)
            .and_then(|d| u32::try_from(d).ok())
            .unwrap_or(0)
    }
}

impl ReviewScheduler for FsrsScheduler {
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
        #[allow(clippy::cast_possible_truncation)] // retention is 0.0..=1.0
        let retention = settings.desired_retention as f32;
        let next = self
            .inner
            .next_states(
                Self::memory_of(current),
                retention,
                Self::elapsed_days(current, event.answered_at),
            )
            .map_err(|e| ScheduleError::InvalidState(format!("fsrs next_states: {e}")))?;
        let chosen = match event.grade {
            Grade::Again => next.again,
            Grade::Hard => next.hard,
            Grade::Good => next.good,
            Grade::Easy => next.easy,
        };
        let interval_seconds =
            clamp_interval(f64::from(chosen.interval) * 86_400.0, settings, ALGORITHM)?;
        Ok(ScheduledState {
            algorithm: ALGORITHM.to_owned(),
            algorithm_version: ALGORITHM_VERSION.to_owned(),
            parameter_set_id: current.parameter_set_id.clone(),
            difficulty: Some(f64::from(chosen.memory.difficulty)),
            stability: Some(f64::from(chosen.memory.stability)),
            last_review_at: Some(event.answered_at),
            due_at: Some(Timestamp(event.answered_at.0 + interval_seconds)),
            scheduled_interval_seconds: Some(interval_seconds),
            lapse_count: current.lapse_count + u32::from(event.grade == Grade::Again),
            review_count: current.review_count + 1,
            state_version: current.state_version + 1,
        })
    }

    fn predicted_retrievability(
        &self,
        state: &ScheduledState,
        at: Timestamp,
    ) -> Result<f64, ScheduleError> {
        let memory = Self::memory_of(state)
            .ok_or_else(|| ScheduleError::InvalidState("no memory state yet".to_owned()))?;
        let last = state
            .last_review_at
            .ok_or_else(|| ScheduleError::InvalidState("no last review".to_owned()))?;
        #[allow(clippy::cast_precision_loss)] // day counts are tiny vs f32 range
        let days = (at.0.saturating_sub(last.0)).max(0) as f32 / 86_400.0;
        Ok(f64::from(fsrs::current_retrievability(
            memory, days, self.decay,
        )))
    }
}
