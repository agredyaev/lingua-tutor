//! Spaced-repetition subsystem: `ReviewScheduler` trait, FSRS adapter,
//! deterministic fallback (05-review-engine.md). Language-neutral; no IO;
//! third-party scheduler types never cross this crate's boundary.

mod fallback;
mod fsrs_adapter;

pub use fallback::FallbackScheduler;
pub use fsrs_adapter::FsrsScheduler;

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
    /// Active parameter set; assigned by `application` on persist, not here.
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

impl ScheduledState {
    /// Deterministic hash for conflict detection, reproducibility tests and
    /// audit (`prior_state_hash` / `result_state_hash`, 05-review-engine.md §7).
    /// Floats hashed by exact bit pattern — no formatting ambiguity.
    ///
    /// # Invariants
    ///
    /// Equal states hash equal; any field change changes the hash (FNV-1a 64).
    #[must_use]
    pub fn state_hash(&self) -> String {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        let mut eat = |bytes: &[u8]| {
            for b in bytes {
                hash ^= u64::from(*b);
                hash = hash.wrapping_mul(0x100_0000_01b3);
            }
            hash ^= 0xff; // field separator
            hash = hash.wrapping_mul(0x100_0000_01b3);
        };
        eat(self.algorithm.as_bytes());
        eat(self.algorithm_version.as_bytes());
        eat(self.parameter_set_id.as_deref().unwrap_or("-").as_bytes());
        eat(&self.difficulty.map_or([0xff; 8], f64::to_be_bytes));
        eat(&self.stability.map_or([0xff; 8], f64::to_be_bytes));
        eat(&self.last_review_at.map_or([0xff; 8], |t| t.0.to_be_bytes()));
        eat(&self.due_at.map_or([0xff; 8], |t| t.0.to_be_bytes()));
        eat(&self
            .scheduled_interval_seconds
            .map_or([0xff; 8], i64::to_be_bytes));
        eat(&self.lapse_count.to_be_bytes());
        eat(&self.review_count.to_be_bytes());
        eat(&self.state_version.to_be_bytes());
        format!("{hash:016x}")
    }
}

/// Input to [`ReviewScheduler::initial_state`].
#[derive(Debug, Clone, Copy)]
pub struct InitialReviewInput {
    /// Introduction instant; the card becomes due immediately — "second
    /// daily session or next practical window" (05-review-engine.md §5).
    pub now: Timestamp,
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
    /// IANA timezone name; day-boundary logic lives in `application`.
    pub timezone: String,
}

impl Default for SchedulerSettings {
    fn default() -> Self {
        Self {
            desired_retention: 0.90,
            max_new_cards_per_day: 8,
            target_review_minutes: (8, 12),
            max_review_batch: 50,
            max_interval_days: 1825, // 5 years — conservative ceiling
            timezone: "Europe/Madrid".to_owned(),
        }
    }
}

/// Scheduling failure. Worker maps this to an API error; never a panic
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

/// Application-owned scheduling contract (05-review-engine.md §1).
pub trait ReviewScheduler {
    /// State for a freshly introduced card; due immediately.
    ///
    /// # Errors
    ///
    /// [`ScheduleError`] when the algorithm cannot produce a valid state.
    fn initial_state(&self, input: InitialReviewInput) -> Result<ScheduledState, ScheduleError>;

    /// Applies one graded review, returning the successor state.
    ///
    /// # Errors
    ///
    /// [`ScheduleError::InvalidInterval`] when the produced interval is
    /// non-finite, non-positive, or absurd; [`ScheduleError::InvalidState`]
    /// when `current` is inconsistent.
    fn apply_review(
        &self,
        current: &ScheduledState,
        event: &ReviewEventInput,
        settings: &SchedulerSettings,
    ) -> Result<ScheduledState, ScheduleError>;

    /// Predicted recall probability at `at`.
    ///
    /// # Errors
    ///
    /// [`ScheduleError::InvalidState`] when the state has no review history.
    fn predicted_retrievability(
        &self,
        state: &ScheduledState,
        at: Timestamp,
    ) -> Result<f64, ScheduleError>;
}

/// Recomputes a card's state by replaying `events` from the card's
/// activation instant (05-review-engine.md §7 undo). Pure function of its
/// inputs — it must NOT observe the wall clock, so a replay run at any later
/// time reproduces the exact pre-event state and hash.
///
/// # Errors
///
/// Propagates [`ScheduleError`] from the underlying scheduler.
pub fn replay(
    scheduler: &dyn ReviewScheduler,
    activated_at: Timestamp,
    events: &[ReviewEventInput],
    settings: &SchedulerSettings,
) -> Result<ScheduledState, ScheduleError> {
    let mut state = scheduler.initial_state(InitialReviewInput { now: activated_at })?;
    for event in events {
        state = scheduler.apply_review(&state, event, settings)?;
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
