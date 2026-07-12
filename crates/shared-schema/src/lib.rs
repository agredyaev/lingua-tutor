//! DTOs shared between `apps/web` and `apps/worker`: serde types mirroring
//! the `/api/v1` contract (08-http-api.md). No domain logic, no IO.
//! Field names must match worker JSON exactly — round-trip tests enforce it.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Response body of `GET /api/v1/health`.
///
/// Stub shape for M0; extended with D1 connectivity, schema version,
/// scheduler version, migration and deployment metadata at WBS 10.5.
///
/// # Examples
///
/// ```
/// let health = shared_schema::HealthResponse::ok("0.1.0");
/// assert_eq!(health.status, "ok");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HealthResponse {
    /// `"ok"` when the Worker is serving requests.
    pub status: String,
    /// Deployment version (crate version at build time).
    pub version: String,
}

impl HealthResponse {
    /// Healthy response for the given deployment version.
    #[must_use]
    pub fn ok(version: &str) -> Self {
        Self {
            status: "ok".to_owned(),
            version: version.to_owned(),
        }
    }
}

/// Standard API error body (08-http-api.md §1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiErrorBody {
    /// Error envelope.
    pub error: ApiErrorDetail,
}

/// Error envelope content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiErrorDetail {
    /// Stable machine code, e.g. `conflict_stale_state`.
    pub code: String,
    /// Human-readable message.
    pub message: String,
}

/// Typed client-side API failure (09-frontend.md §2 error rule).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiError {
    /// Server returned the standard error body.
    Api {
        /// HTTP status.
        status: u16,
        /// Stable error code.
        code: String,
        /// Message for display.
        message: String,
    },
    /// Network failure / no response.
    Network(String),
    /// Response body did not match the expected DTO.
    Decode(String),
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Api { code, message, .. } => write!(f, "{code}: {message}"),
            Self::Network(message) => write!(f, "network error: {message}"),
            Self::Decode(message) => write!(f, "bad response: {message}"),
        }
    }
}

impl std::error::Error for ApiError {}

/// Language pack summary (subset of the manifest JSON).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LanguagePackDto {
    /// Stable pack ID.
    pub id: String,
    /// Human-readable name.
    pub display_name: String,
    /// Target locale.
    pub target_locale: String,
    /// Supported source languages.
    pub supported_source_languages: Vec<String>,
    /// Default source language.
    pub default_source_language: String,
}

/// Language profile (mirrors `learning-core` serde shape).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LanguageProfileDto {
    /// Profile ID.
    pub id: String,
    /// Selected pack.
    pub language_pack_id: String,
    /// Instruction language.
    pub source_language: String,
    /// Target locale.
    pub target_locale: String,
    /// Level label.
    pub estimated_level: Option<String>,
    /// Last assigned lesson number.
    pub current_lesson_number: u32,
    /// `active` | `paused` | `archived`.
    pub status: String,
    /// Default profile flag.
    pub is_default: bool,
}

/// `GET /api/v1/dashboard` payload (08-http-api.md §3). Exceptions first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DashboardDto {
    /// Actionable exceptions, ordered by urgency.
    pub exceptions: Vec<String>,
    /// Cards due right now.
    pub due_cards: u32,
    /// Remaining new-card budget today.
    pub new_card_budget: u8,
    /// Graded reviews today.
    pub reviews_today: u32,
    /// Review time today, seconds.
    pub review_seconds_today: u32,
    /// Active error targets.
    pub active_error_targets: u32,
    /// Error rechecks due.
    pub due_error_rechecks: u32,
    /// Next lesson number.
    pub next_lesson_number: u32,
    /// Next lesson goal from the last completed lesson.
    pub next_lesson_goal: Option<String>,
    /// Item counts by status.
    pub items_by_status: BTreeMap<String, u32>,
}

/// One card as served by `/api/v1/cards` (mirrors `learning-core::Card`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CardDto {
    /// Card ID.
    pub id: String,
    /// Intent.
    pub intent: String,
    /// Lifecycle status.
    pub status: String,
    /// Prompt.
    pub prompt: String,
    /// Prompt language.
    pub prompt_language: String,
    /// Preferred answer.
    pub answer: String,
    /// Accepted alternatives.
    pub accepted_answers: Vec<String>,
    /// Explanation.
    pub explanation: Option<String>,
    /// Why the card exists.
    pub creation_reason: String,
    /// Edit version.
    pub content_version: u32,
    /// Suspension reason when suspended.
    pub suspended_reason: Option<String>,
}

/// One lesson as served by `/api/v1/lessons`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LessonDto {
    /// Lesson ID.
    pub id: String,
    /// Number within profile.
    pub lesson_number: u32,
    /// `planned` | `active` | `completed` | `abandoned`.
    pub status: String,
    /// Scenario.
    pub scenario: String,
    /// Tutor summary.
    pub tutor_summary: Option<String>,
    /// Next goal.
    pub next_goal: Option<String>,
}

/// Review queue response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QueueDto {
    /// Session the queue belongs to.
    pub session_id: String,
    /// Remaining new-card budget.
    pub new_card_budget: u8,
    /// Ordered batch.
    pub queue: Vec<QueueCardDto>,
}

/// One queue entry with everything the review screen needs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QueueCardDto {
    /// Card ID.
    pub card_id: String,
    /// Intent.
    pub intent: String,
    /// Lifecycle status.
    pub status: String,
    /// Prompt (front).
    pub prompt: String,
    /// Prompt language.
    pub prompt_language: String,
    /// Answer (back, hidden until reveal).
    pub answer: String,
    /// Accepted alternatives.
    pub accepted_answers: Vec<String>,
    /// Explanation shown after grading.
    pub explanation: Option<String>,
    /// Pronunciation note.
    pub pronunciation_note: Option<String>,
    /// Hash the grade submission must echo.
    pub prior_state_hash: Option<String>,
}

/// Grade submission result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventResultDto {
    /// Event ID.
    pub event_id: String,
    /// Graded card.
    pub card_id: String,
    /// Card status after the grade (absent on duplicate replay).
    #[serde(default)]
    pub card_status: Option<String>,
    /// True when this was an idempotent replay.
    #[serde(default)]
    pub duplicate: bool,
}

/// Import validate response (08-http-api.md §7).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ValidateResponseDto {
    /// Token required by apply.
    pub validation_token: String,
    /// Token expiry.
    pub expires_at: String,
    /// Counts preview.
    pub preview: ImportPreviewDto,
    /// Non-fatal warnings.
    pub warnings: Vec<String>,
}

/// Counts shown before apply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportPreviewDto {
    /// Scenario of the incoming lesson.
    pub lesson_scenario: String,
    /// New items.
    pub new_learning_items: usize,
    /// Evidence entries.
    pub evidence: usize,
    /// Error occurrences.
    pub error_occurrences: usize,
    /// Listening attempts.
    pub listening_attempts: usize,
    /// Card mutations (destructive ones highlighted by the UI).
    pub card_mutations: usize,
}

/// Import apply response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApplyResponseDto {
    /// Created/completed lesson.
    #[serde(default)]
    pub lesson_id: Option<String>,
    /// Duplicate marker (idempotent replay).
    #[serde(default)]
    pub status: Option<String>,
    /// Per-mutation outcomes.
    #[serde(default)]
    pub card_mutations: Vec<MutationOutcomeDto>,
    /// Warnings (e.g. downgraded mastery proposals).
    #[serde(default)]
    pub warnings: Vec<String>,
}

/// One card-mutation outcome from an apply.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MutationOutcomeDto {
    /// Mutation idempotency key.
    pub client_operation_id: String,
    /// `applied` | `rejected` | `warned`.
    pub verdict: String,
    /// Rejection/warning note.
    pub note: Option<String>,
    /// Affected card.
    pub card_id: Option<String>,
}

/// Review summary (today).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SummaryDto {
    /// Graded reviews today.
    pub reviews_today: u32,
    /// Grade distribution.
    pub grade_distribution: GradeDistributionDto,
    /// Time spent, seconds.
    pub review_seconds_today: u32,
}

/// Grade counts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GradeDistributionDto {
    /// Grade 1.
    pub again: u32,
    /// Grade 2.
    pub hard: u32,
    /// Grade 3.
    pub good: u32,
    /// Grade 4.
    pub easy: u32,
}

/// Due forecast.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForecastDto {
    /// Horizon in days.
    pub days: i64,
    /// Due count per day, index 0 = today.
    pub due_per_day: Vec<u32>,
}

/// Answer evaluation verdict from the active pack.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvaluationDto {
    /// `correct` | `acceptable_variant` | `incorrect`.
    pub verdict: String,
    /// Which accepted answer matched.
    pub matched_answer: Option<String>,
    /// Optional pack feedback.
    pub feedback: Option<String>,
}

/// Error target as served by `/api/v1/error-targets`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ErrorTargetDto {
    /// Target ID.
    pub id: String,
    /// Core category.
    pub core_category: String,
    /// Pack taxonomy code.
    pub pack_error_code: Option<String>,
    /// Learner's production.
    pub original_production: String,
    /// Corrected form.
    pub corrected_production: String,
    /// Sightings.
    pub occurrence_count: u32,
    /// Method change required flag.
    pub method_change_required: bool,
    /// `active` | `recheck` | `resolved` | `escalated`.
    pub status: String,
}

#[cfg(test)]
#[allow(clippy::unwrap_used)] // tests exempt (16-rust-guidelines.md §2)
mod tests {
    use super::*;

    #[test]
    fn health_response_serializes_to_contract_shape() {
        let health = HealthResponse::ok("0.1.0");
        let json = serde_json::to_value(&health).unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "status": "ok", "version": "0.1.0" })
        );
    }

    #[test]
    fn health_response_round_trips() {
        let health = HealthResponse::ok("0.1.0");
        let text = serde_json::to_string(&health).unwrap();
        let back: HealthResponse = serde_json::from_str(&text).unwrap();
        assert_eq!(back, health);
    }

    #[test]
    fn error_body_matches_api_shape() {
        let body: ApiErrorBody = serde_json::from_str(
            r#"{"error":{"code":"conflict_stale_state","message":"…","details":{}}}"#,
        )
        .unwrap();
        assert_eq!(body.error.code, "conflict_stale_state");
    }

    #[test]
    fn queue_card_parses_worker_shape() {
        let dto: QueueCardDto = serde_json::from_str(
            r#"{"accepted_answers":[],"answer":"a","answer_evaluation_mode":"exact",
                "card_id":"crd_1","explanation":null,"intent":"production",
                "prior_state_hash":"h","prompt":"p","prompt_language":"ru",
                "pronunciation_note":null,"status":"new"}"#,
        )
        .unwrap();
        assert_eq!(dto.card_id, "crd_1");
        assert_eq!(dto.prior_state_hash.as_deref(), Some("h"));
    }
}
