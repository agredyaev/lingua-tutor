//! Stateless FSRS-6 processing passes over persisted scalar scheduling state.
//! Pinned fsrs-rs dependency remains a request-local external computation resource.
use crate::{
    Grade, ReviewEventInput, ScheduleError, ScheduledState, SchedulerSettings, Timestamp,
    clamp_interval,
};

pub(crate) const ALGORITHM: &str = "fsrs";
pub(crate) const ALGORITHM_VERSION: &str = "fsrs-rs-6.6.1";

pub(crate) fn init_fsrs() -> Result<fsrs::FSRS, ScheduleError> {
    fsrs::FSRS::new(&fsrs::DEFAULT_PARAMETERS)
        .map_err(|error| ScheduleError::InvalidState(format!("fsrs init: {error}")))
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

pub(crate) fn initial_state(now: Timestamp) -> ScheduledState {
    ScheduledState {
        algorithm: ALGORITHM.to_owned(),
        algorithm_version: ALGORITHM_VERSION.to_owned(),
        parameter_set_id: None,
        difficulty: None,
        stability: None,
        last_review_at: None,
        due_at: Some(now),
        scheduled_interval_seconds: None,
        lapse_count: 0,
        review_count: 0,
        state_version: 1,
    }
}

pub(crate) fn apply_review(
    inner: &fsrs::FSRS,
    current: &ScheduledState,
    event: &ReviewEventInput,
    settings: &SchedulerSettings,
) -> Result<ScheduledState, ScheduleError> {
    #[allow(clippy::cast_possible_truncation)] // retention is 0.0..=1.0
    let retention = settings.desired_retention as f32;
    let next = inner
        .next_states(
            memory_of(current),
            retention,
            elapsed_days(current, event.answered_at),
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

pub(crate) fn predicted_retrievability(
    state: &ScheduledState,
    at: Timestamp,
) -> Result<f64, ScheduleError> {
    let memory = memory_of(state)
        .ok_or_else(|| ScheduleError::InvalidState("no memory state yet".to_owned()))?;
    let last = state
        .last_review_at
        .ok_or_else(|| ScheduleError::InvalidState("no last review".to_owned()))?;
    #[allow(clippy::cast_precision_loss)] // day counts are tiny vs f32 range
    let days = (at.0.saturating_sub(last.0)).max(0) as f32 / 86_400.0;
    Ok(f64::from(fsrs::current_retrievability(
        memory,
        days,
        fsrs::DEFAULT_PARAMETERS[20],
    )))
}
