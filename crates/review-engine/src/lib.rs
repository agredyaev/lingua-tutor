//! Language-neutral spaced-repetition processing built around one persisted FSRS algorithm.
//! Third-party scheduler types remain request-local and never cross this crate boundary.

mod fsrs_adapter;

/// Minimum review interval after grading: ten minutes.
pub const MIN_INTERVAL_SECONDS: i64 = 600;

/// Persisted scheduler algorithm identifier.
pub const ALGORITHM: &str = "fsrs";

/// Persisted scheduler implementation version.
pub const ALGORITHM_VERSION: &str = "fsrs-rs-6.6.1";

/// UTC instant represented as Unix seconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Timestamp(pub i64);

/// Learner review grade.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Grade {
    /// Recall failed.
    Again = 1,
    /// Recall succeeded with substantial effort.
    Hard = 2,
    /// Recall succeeded with normal effort.
    Good = 3,
    /// Recall was immediate and confident.
    Easy = 4,
}

impl TryFrom<u8> for Grade {
    type Error = ScheduleError;

    fn try_from(value: u8) -> Result<Self, ScheduleError> {
        match value {
            1 => Ok(Self::Again),
            2 => Ok(Self::Hard),
            3 => Ok(Self::Good),
            4 => Ok(Self::Easy),
            other => Err(ScheduleError::InvalidState(format!(
                "grade {other} outside 1..=4"
            ))),
        }
    }
}

/// Scalar scheduling state persisted in `card_schedule_state`.
#[derive(Debug, Clone, PartialEq)]
pub struct ScheduledState {
    /// Persisted scheduler algorithm.
    pub algorithm: String,
    /// Persisted scheduler implementation version.
    pub algorithm_version: String,
    /// FSRS difficulty, absent before the first review.
    pub difficulty: Option<f64>,
    /// FSRS stability in days, absent before the first review.
    pub stability: Option<f64>,
    /// Last graded review instant.
    pub last_review_at: Option<Timestamp>,
    /// Next due instant.
    pub due_at: Option<Timestamp>,
    /// Interval that produced `due_at`.
    pub scheduled_interval_seconds: Option<i64>,
    /// Number of lapse transitions.
    pub lapse_count: u32,
    /// Number of applied graded reviews.
    pub review_count: u32,
    /// Monotonic state version used by persistence.
    pub state_version: u32,
}

/// Compute a deterministic hash of all persisted scheduler state columns.
///
/// The hash is used for stale-write conflict detection and replay verification.
#[must_use]
pub fn state_hash(state: &ScheduledState) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut eat = |bytes: &[u8]| {
        for byte in bytes {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100_0000_01b3);
        }
        hash ^= 0xff;
        hash = hash.wrapping_mul(0x100_0000_01b3);
    };
    eat(state.algorithm.as_bytes());
    eat(state.algorithm_version.as_bytes());
    // Preserve the removed v7 metadata field's empty hash slot so existing
    // persisted state hashes remain valid across the v8 schema cleanup.
    eat(b"-");
    eat(&state.difficulty.map_or([0xff; 8], f64::to_be_bytes));
    eat(&state.stability.map_or([0xff; 8], f64::to_be_bytes));
    eat(&state
        .last_review_at
        .map_or([0xff; 8], |value| value.0.to_be_bytes()));
    eat(&state
        .due_at
        .map_or([0xff; 8], |value| value.0.to_be_bytes()));
    eat(&state
        .scheduled_interval_seconds
        .map_or([0xff; 8], i64::to_be_bytes));
    eat(&state.lapse_count.to_be_bytes());
    eat(&state.review_count.to_be_bytes());
    eat(&state.state_version.to_be_bytes());
    format!("{hash:016x}")
}

/// One graded review to apply to the current scheduler state.
#[derive(Debug, Clone, Copy)]
pub struct ReviewEventInput {
    /// Learner grade.
    pub grade: Grade,
    /// Instant at which the answer was graded.
    pub answered_at: Timestamp,
}

/// Scheduler settings read from the learner profile.
#[derive(Debug, Clone, PartialEq)]
pub struct SchedulerSettings {
    /// Target recall probability.
    pub desired_retention: f64,
    /// Maximum newly introduced cards per local day.
    pub max_new_cards_per_day: u8,
    /// Soft daily review-minute range.
    pub target_review_minutes: (u8, u8),
    /// Maximum cards returned in one review batch.
    pub max_review_batch: u16,
    /// Maximum FSRS interval in days.
    pub max_interval_days: u32,
    /// IANA timezone name used by native day-boundary processing.
    pub timezone: String,
}

impl Default for SchedulerSettings {
    fn default() -> Self {
        Self {
            desired_retention: 0.90,
            max_new_cards_per_day: 8,
            target_review_minutes: (8, 12),
            max_review_batch: 50,
            max_interval_days: 36_500,
            timezone: "Europe/Madrid".to_owned(),
        }
    }
}

/// Scheduler processing error.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ScheduleError {
    /// Scheduler produced an invalid interval.
    #[error("invalid interval from {algorithm}: {detail}")]
    InvalidInterval {
        /// Producing algorithm.
        algorithm: String,
        /// Validation failure details.
        detail: String,
    },
    /// Persisted or computed state is invalid.
    #[error("invalid scheduling state: {0}")]
    InvalidState(String),
}

/// Request-local FSRS computation scratch.
///
/// The external FSRS object is never persisted or retained in domain state.
pub struct SchedulerScratch {
    fsrs: fsrs::FSRS,
}

/// Create request-local FSRS scratch.
///
/// # Errors
///
/// Returns an error if the pinned FSRS parameter vector cannot initialize.
pub fn new_scheduler_scratch() -> Result<SchedulerScratch, ScheduleError> {
    Ok(SchedulerScratch {
        fsrs: fsrs_adapter::init_fsrs()?,
    })
}

/// Construct a fresh FSRS state due immediately.
#[must_use]
pub fn initial_state(at: Timestamp) -> ScheduledState {
    fsrs_adapter::initial_state(at)
}

/// Apply one graded review.
///
/// # Errors
///
/// Returns an error when FSRS rejects the state or produces an invalid interval.
pub fn apply_review(
    scratch: &SchedulerScratch,
    state: &ScheduledState,
    event: &ReviewEventInput,
    settings: &SchedulerSettings,
) -> Result<ScheduledState, ScheduleError> {
    fsrs_adapter::apply_review(&scratch.fsrs, state, event, settings)
}

/// Predict current recall probability for a reviewed card.
///
/// # Errors
///
/// Returns an error when the state has no valid FSRS memory state.
pub fn predicted_retrievability(
    state: &ScheduledState,
    at: Timestamp,
) -> Result<f64, ScheduleError> {
    fsrs_adapter::predicted_retrievability(state, at)
}

/// Replay effective review events deterministically from card activation.
///
/// # Errors
///
/// Returns the first scheduler error encountered during replay.
pub fn replay(
    activated_at: Timestamp,
    events: &[ReviewEventInput],
    settings: &SchedulerSettings,
) -> Result<ScheduledState, ScheduleError> {
    let scratch = new_scheduler_scratch()?;
    let mut state = initial_state(activated_at);
    for event in events {
        state = apply_review(&scratch, &state, event, settings)?;
    }
    Ok(state)
}

/// Clamp a scheduler-produced interval to persisted bounds.
fn clamp_interval(
    raw_seconds: f64,
    settings: &SchedulerSettings,
    algorithm: &str,
) -> Result<i64, ScheduleError> {
    if !raw_seconds.is_finite() {
        return Err(ScheduleError::InvalidInterval {
            algorithm: algorithm.to_owned(),
            detail: format!("non-finite interval {raw_seconds}"),
        });
    }
    let max = i64::from(settings.max_interval_days) * 86_400;
    #[allow(clippy::cast_possible_truncation)]
    Ok((raw_seconds.round() as i64).clamp(MIN_INTERVAL_SECONDS, max))
}
