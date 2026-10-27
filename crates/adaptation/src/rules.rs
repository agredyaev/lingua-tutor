//! Deterministic adaptive recommendation rules as explicit scalar passes.

/// Borrowed signal view used by the allocation-free card-rule pass.
#[derive(Debug, Clone, Copy)]
pub struct CardSignalView<'a> {
    /// Stable persisted card ID borrowed from the boundary.
    pub card_id: &'a str,
    /// Persisted learning-item ID borrowed from the boundary.
    pub learning_item_id: Option<&'a str>,
    /// Recent review grades, newest first.
    pub recent_grades: &'a [u8],
    /// Mean response latency.
    pub avg_response_millis: Option<u32>,
    /// Failed spontaneous-production observations.
    pub spontaneous_failures: u32,
    /// Successful spontaneous-production observations.
    pub spontaneous_successes: u32,
    /// Successful prompted-production observations.
    pub prompted_successes: u32,
    /// Listening failures.
    pub listening_failures: u32,
    /// Total graded reviews.
    pub review_count: u32,
}

/// Borrowed recommendation emitted directly into the persistence pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecommendationView<'a> {
    /// Subject category.
    pub subject_type: &'static str,
    /// Persisted subject ID borrowed from the caller.
    pub subject_id: &'a str,
    /// Stable recommendation action.
    pub action: &'static str,
    /// Static recommendation detail.
    pub detail: &'static str,
}

/// Window size the local runtime must respect when building card signals.
pub const GRADE_WINDOW: usize = 10;
/// Again-count threshold for card redesign.
pub const REDESIGN_AGAIN_THRESHOLD: usize = 3;
/// Hard-count threshold for latency-based redesign.
pub const REDESIGN_HARD_THRESHOLD: usize = 3;
/// Latency threshold for a Hard-driven redesign.
pub const HIGH_LATENCY_MILLIS: u32 = 10_000;
/// Review volume required before low-utility retirement can be proposed.
pub const RETIREMENT_MIN_REVIEWS: u32 = 10;

const AGAIN: u8 = 1;
const HARD: u8 = 2;
const ERROR_ESCALATION: u32 = 3;
const ERROR_METHOD_CHANGE: u32 = 5;

/// Emit card recommendations without allocating a collection.
pub fn visit_card_rules<'a>(
    signals: CardSignalView<'a>,
    mut emit: impl FnMut(RecommendationView<'a>),
) {
    let again_count = count(signals.recent_grades, AGAIN);
    let hard_count = count(signals.recent_grades, HARD);
    let last_failed = signals.recent_grades.first().copied() == Some(AGAIN);
    let successes = signals
        .recent_grades
        .iter()
        .filter(|grade| **grade > AGAIN)
        .count();
    let success_rate = if signals.recent_grades.is_empty() {
        0.0
    } else {
        #[allow(clippy::cast_precision_loss)]
        {
            successes as f64 / signals.recent_grades.len() as f64
        }
    };

    if last_failed && let Some(item) = signals.learning_item_id {
        emit(RecommendationView {
            subject_type: "learning_item",
            subject_id: item,
            action: "practice_in_lesson",
            detail: "failed production review — bring the phrase into the next speaking session",
        });
    }
    if success_rate >= 0.8
        && signals.spontaneous_failures > 0
        && let Some(item) = signals.learning_item_id
    {
        emit(RecommendationView {
            subject_type: "learning_item",
            subject_id: item,
            action: "schedule_transfer_practice",
            detail: "review passes but spontaneous speech fails — schedule role-play transfer",
        });
    }
    if signals.listening_failures >= 2
        && let Some(item) = signals.learning_item_id
    {
        emit(RecommendationView {
            subject_type: "listening_item",
            subject_id: item,
            action: "create_listening_card",
            detail: "repeated listening failures — create/activate a listening card",
        });
    }
    if signals.prompted_successes >= 2
        && signals.spontaneous_successes == 0
        && let Some(item) = signals.learning_item_id
    {
        emit(RecommendationView {
            subject_type: "learning_item",
            subject_id: item,
            action: "create_transfer_target",
            detail: "produced only with hints — create a transfer target",
        });
    }

    if again_count >= REDESIGN_AGAIN_THRESHOLD {
        emit(RecommendationView {
            subject_type: "card",
            subject_id: signals.card_id,
            action: "redesign_card",
            detail: "redesign candidate: repeated Again grades in the recent window",
        });
    } else if hard_count >= REDESIGN_HARD_THRESHOLD
        && signals.avg_response_millis.unwrap_or(0) >= HIGH_LATENCY_MILLIS
    {
        emit(RecommendationView {
            subject_type: "card",
            subject_id: signals.card_id,
            action: "redesign_card",
            detail: "redesign candidate: frequent Hard with high response time",
        });
    } else if success_rate >= 0.8 && signals.spontaneous_failures > 0 {
        emit(RecommendationView {
            subject_type: "card",
            subject_id: signals.card_id,
            action: "redesign_card",
            detail: "redesign candidate: review success but conversational failure",
        });
    }

    if signals.review_count >= RETIREMENT_MIN_REVIEWS
        && success_rate >= 0.9
        && signals.spontaneous_successes == 0
        && signals.spontaneous_failures == 0
    {
        emit(RecommendationView {
            subject_type: "card",
            subject_id: signals.card_id,
            action: "consider_retirement",
            detail: "high review success but never used in conversation — low utility",
        });
    }
}

fn count(grades: &[u8], grade: u8) -> usize {
    grades
        .iter()
        .filter(|candidate| **candidate == grade)
        .count()
}

/// Return the escalation action for an error occurrence count.
#[must_use]
pub const fn error_escalation_action(occurrence_count: u32) -> Option<&'static str> {
    if occurrence_count >= ERROR_METHOD_CHANGE {
        Some("require_method_change")
    } else if occurrence_count >= ERROR_ESCALATION {
        Some("activate_mini_target")
    } else {
        None
    }
}

/// Return failed-recheck fields without copying the target ID.
#[must_use]
pub fn recheck_failed_fields(offset_days: u32) -> (&'static str, &'static str, String) {
    (
        "error_target",
        "practice_again",
        format!("failed the {offset_days}-day recheck — back to active practice"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signals<'a>(grades: &'a [u8]) -> CardSignalView<'a> {
        CardSignalView {
            card_id: "crd-1",
            learning_item_id: Some("li-1"),
            recent_grades: grades,
            avg_response_millis: None,
            spontaneous_failures: 0,
            spontaneous_successes: 0,
            prompted_successes: 0,
            listening_failures: 0,
            review_count: grades.len() as u32,
        }
    }

    #[test]
    fn failed_review_emits_practice() {
        let mut actions = Vec::new();
        visit_card_rules(signals(&[1, 3, 3]), |rec| actions.push(rec.action));
        assert!(actions.contains(&"practice_in_lesson"));
    }

    #[test]
    fn repeated_again_emits_redesign() {
        let mut actions = Vec::new();
        visit_card_rules(signals(&[1, 1, 3, 1, 3]), |rec| actions.push(rec.action));
        assert!(actions.contains(&"redesign_card"));
    }

    #[test]
    fn hard_requires_latency_for_redesign() {
        let mut slow = signals(&[2, 2, 3, 2]);
        slow.avg_response_millis = Some(12_000);
        let mut slow_redesign = false;
        visit_card_rules(slow, |rec| slow_redesign |= rec.action == "redesign_card");
        assert!(slow_redesign);

        let mut fast = signals(&[2, 2, 3, 2]);
        fast.avg_response_millis = Some(3_000);
        let mut fast_redesign = false;
        visit_card_rules(fast, |rec| fast_redesign |= rec.action == "redesign_card");
        assert!(!fast_redesign);
    }

    #[test]
    fn scalar_error_rules_preserve_thresholds() {
        assert_eq!(error_escalation_action(2), None);
        assert_eq!(error_escalation_action(3), Some("activate_mini_target"));
        assert_eq!(error_escalation_action(5), Some("require_method_change"));
        assert_eq!(recheck_failed_fields(3).1, "practice_again");
    }
}
