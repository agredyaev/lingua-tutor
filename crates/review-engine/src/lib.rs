//! Spaced-repetition subsystem: explicit FSRS/fallback processing passes,
//! deterministic fallback (05-review-engine.md). Language-neutral; no IO;
//! third-party scheduler types never cross this crate's boundary.

mod fallback;
mod fsrs_adapter;

/// Minimum interval after any review: 10 minutes.
pub const MIN_INTERVAL_SECONDS: i64 = 600;

/// UTC instant, unix seconds. RFC 3339 TEXT conversion happens at the
/// repository boundary (04-database.md §1), never here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Timestamp(pub i64);

/// Review grade (05-review-engine.md §4). `Hard` is a passing grade;
/// forgetting = `Again`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Grade {
    /// Not recalled correctly before reveal.
    Again = 1,
    /// Correct with substantial effort.
    Hard = 2,
    /// Correct with normal effort.
    Good = 3,
    /// Immediate, confident, fully correct.
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

/// Scheduling state persisted in `card_schedule_state` (04-database.md).
#[derive(Debug, Clone, PartialEq)]
pub struct ScheduledState {
    /// Algorithm identifier, e.g. `fsrs` or `fallback`.
    pub algorithm: String,
    /// Exact algorithm/crate version producing this state.
    pub algorithm_version: String,
    /// Active parameter set; assigned by the native review write pass on persist, not here.
    pub parameter_set_id: Option<String>,
    /// FSRS difficulty; `None` before first review or for fallback.
    pub difficulty: Option<f64>,
    /// FSRS stability in days; `None` before first review or for fallback.
    pub stability: Option<f64>,
    /// Last graded review.
    pub last_review_at: Option<Timestamp>,
    /// Next due instant; `None` only for never-scheduled states.
    pub due_at: Option<Timestamp>,
    /// Interval that produced `due_at`.
    pub scheduled_interval_seconds: Option<i64>,
    /// Times the card fell back to relearning.
    pub lapse_count: u32,
    /// Graded reviews applied.
    pub review_count: u32,
    /// Monotonic version, bumped per applied review.
    pub state_version: u32,
}

/// Deterministic hash for conflict detection, reproducibility tests and
/// audit (`prior_state_hash` / `result_state_hash`, 05-review-engine.md §7).
/// Floats hashed by exact bit pattern — no formatting ambiguity.
///
/// # Invariants
///
/// Equal states hash equal; any field change changes the hash (FNV-1a 64).
#[must_use]
pub fn state_hash(state: &ScheduledState) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut eat = |bytes: &[u8]| {
        for b in bytes {
            hash ^= u64::from(*b);
            hash = hash.wrapping_mul(0x100_0000_01b3);
        }
        hash ^= 0xff; // field separator
        hash = hash.wrapping_mul(0x100_0000_01b3);
    };
    eat(state.algorithm.as_bytes());
    eat(state.algorithm_version.as_bytes());
    eat(state.parameter_set_id.as_deref().unwrap_or("-").as_bytes());
    eat(&state.difficulty.map_or([0xff; 8], f64::to_be_bytes));
    eat(&state.stability.map_or([0xff; 8], f64::to_be_bytes));
    eat(&state
        .last_review_at
        .map_or([0xff; 8], |t| t.0.to_be_bytes()));
    eat(&state.due_at.map_or([0xff; 8], |t| t.0.to_be_bytes()));
    eat(&state
        .scheduled_interval_seconds
        .map_or([0xff; 8], i64::to_be_bytes));
    eat(&state.lapse_count.to_be_bytes());
    eat(&state.review_count.to_be_bytes());
    eat(&state.state_version.to_be_bytes());
    format!("{hash:016x}")
}
/// One graded review to apply.
#[derive(Debug, Clone, Copy)]
pub struct ReviewEventInput {
    /// Learner grade.
    pub grade: Grade,
    /// When the answer was graded.
    pub answered_at: Timestamp,
}

/// Scheduler tuning (05-review-engine.md §6). Defaults are the reviewed v1
/// values; no other configuration knobs exist.
#[derive(Debug, Clone, PartialEq)]
pub struct SchedulerSettings {
    /// Target recall probability at review time.
    pub desired_retention: f64,
    /// New-card introduction budget per day.
    pub max_new_cards_per_day: u8,
    /// Soft daily review-minutes window (min, max).
    pub target_review_minutes: (u8, u8),
    /// Hard cap per queue batch.
    pub max_review_batch: u16,
    /// Interval ceiling.
    pub max_interval_days: u32,
    /// IANA timezone name; day-boundary logic lives in the native review processing pass.
    pub timezone: String,
}

impl Default for SchedulerSettings {
    fn default() -> Self {
        Self {
            desired_retention: 0.90,
            max_new_cards_per_day: 8,
            target_review_minutes: (8, 12),
            max_review_batch: 50,
            max_interval_days: 36_500, // 100 years; scheduler, not cap pressure, controls mature intervals
            timezone: "Europe/Madrid".to_owned(),
        }
    }
}

/// Scheduling failure. The runtime maps this to an operation error; never a panic
/// (16-rust-guidelines.md §2).
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ScheduleError {
    /// Algorithm produced a non-finite or non-positive interval.
    #[error("invalid interval from {algorithm}: {detail}")]
    InvalidInterval {
        /// Producing algorithm.
        algorithm: String,
        /// What was wrong.
        detail: String,
    },
    /// State does not carry what the operation needs.
    #[error("invalid scheduling state: {0}")]
    InvalidState(String),
}

/// Stable integer algorithm IDs used only in request-local dispatch.
pub const FSRS_ID: u32 = 0;
/// Deterministic offline-safe fallback algorithm.
pub const FALLBACK_ID: u32 = 1;

/// Request-local FSRS computation resource; no domain object graph is retained.
pub struct SchedulerScratch {
    fsrs: Option<fsrs::FSRS>,
}

/// Prepare scratch once per stream of graded events.
///
/// # Errors
/// Returns an error when the external FSRS engine rejects its pinned parameters.
pub fn new_scheduler_scratch(id: u32) -> Result<SchedulerScratch, ScheduleError> {
    let fsrs = match id {
        FSRS_ID => Some(fsrs_adapter::init_fsrs()?),
        FALLBACK_ID => None,
        _ => {
            return Err(ScheduleError::InvalidState(format!(
                "unknown scheduler id {id}"
            )));
        }
    };
    Ok(SchedulerScratch { fsrs })
}

/// Convert the database algorithm name to its stable integer ID.
#[must_use]
pub fn algorithm_id(name: &str) -> Option<u32> {
    match name {
        "fsrs" => Some(FSRS_ID),
        "fallback" => Some(FALLBACK_ID),
        _ => None,
    }
}

/// Construct the first state without allocating a scheduler object.
///
/// # Errors
/// Rejects an unknown algorithm ID.
pub fn initial_state(id: u32, at: Timestamp) -> Result<ScheduledState, ScheduleError> {
    match id {
        FSRS_ID => Ok(fsrs_adapter::initial_state(at)),
        FALLBACK_ID => Ok(fallback::initial_state(at)),
        _ => Err(ScheduleError::InvalidState(format!(
            "unknown scheduler id {id}"
        ))),
    }
}

/// Advance one graded review over the supplied current state.
///
/// # Errors
/// Propagates invalid FSRS states, interval errors and unrecognized IDs.
pub fn apply_review(
    id: u32,
    scratch: &SchedulerScratch,
    state: &ScheduledState,
    event: &ReviewEventInput,
    settings: &SchedulerSettings,
) -> Result<ScheduledState, ScheduleError> {
    match id {
        FSRS_ID => {
            let engine = scratch
                .fsrs
                .as_ref()
                .ok_or_else(|| ScheduleError::InvalidState("missing FSRS scratch".to_owned()))?;
            fsrs_adapter::apply_review(engine, state, event, settings)
        }
        FALLBACK_ID => fallback::apply_review(state, event, settings),
        _ => Err(ScheduleError::InvalidState(format!(
            "unknown scheduler id {id}"
        ))),
    }
}

/// Predict recall probability using the selected scheduling pass.
///
/// # Errors
/// Propagates invalid state and unrecognized algorithm ID.
pub fn predicted_retrievability(
    id: u32,
    state: &ScheduledState,
    at: Timestamp,
) -> Result<f64, ScheduleError> {
    match id {
        FSRS_ID => fsrs_adapter::predicted_retrievability(state, at),
        FALLBACK_ID => fallback::predicted_retrievability(state, at),
        _ => Err(ScheduleError::InvalidState(format!(
            "unknown scheduler id {id}"
        ))),
    }
}

/// Replay effective events deterministically from a card's activation instant.
///
/// # Errors
/// Propagates errors from the chosen pass.
pub fn replay(
    id: u32,
    activated_at: Timestamp,
    events: &[ReviewEventInput],
    settings: &SchedulerSettings,
) -> Result<ScheduledState, ScheduleError> {
    let scratch = new_scheduler_scratch(id)?;
    let mut state = initial_state(id, activated_at)?;
    for event in events {
        state = apply_review(id, &scratch, &state, event, settings)?;
    }
    Ok(state)
}

/// Clamps a produced interval into `[MIN_INTERVAL_SECONDS, max_interval_days]`,
/// rejecting non-finite input. Shared by both schedulers.
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
    #[allow(clippy::cast_possible_truncation)] // bounded by max above before cast
    Ok((raw_seconds.round() as i64).clamp(MIN_INTERVAL_SECONDS, max))
}
