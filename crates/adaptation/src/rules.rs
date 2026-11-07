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
pub(crate) const REDESIGN_AGAIN_THRESHOLD: usize = 3;
/// Hard-count threshold for latency-based redesign.
pub(crate) const REDESIGN_HARD_THRESHOLD: usize = 3;
/// Latency threshold for a Hard-driven redesign.
pub(crate) const HIGH_LATENCY_MILLIS: u32 = 10_000;
const AGAIN: u8 = 1;
const HARD: u8 = 2;

/// Emit card recommendations without allocating a collection.
pub fn visit_card_rules<'a>(
    signals: CardSignalView<'a>,
    mut emit: impl FnMut(RecommendationView<'a>),
) {
    let again_count = count(signals.recent_grades, AGAIN);
    let hard_count = count(signals.recent_grades, HARD);
    let last_failed = signals.recent_grades.first().copied() == Some(AGAIN);
    if last_failed && let Some(item) = signals.learning_item_id {
        emit(RecommendationView {
            subject_type: "learning_item",
            subject_id: item,
            action: "practice_in_lesson",
            detail: "failed production review — bring the phrase into the next speaking session",
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
    }
}

fn count(grades: &[u8], grade: u8) -> usize {
    grades
        .iter()
        .filter(|candidate| **candidate == grade)
        .count()
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
        }
    }

    #[test]
    fn failed_review_emits_practice() {
        let mut actions = Vec::new();
        visit_card_rules(signals(&[1, 3, 3]), |rec| actions.push(rec.action));
        assert!(actions.contains(&"practice_in_lesson"));
    }

    #[test]
    fn review_rules_do_not_reinfer_spontaneous_capability_from_lifetime_evidence() {
        let value = signals(&[3, 3, 3, 3]);
        let mut actions = Vec::new();
        visit_card_rules(value, |rec| actions.push(rec.action));
        assert!(
            !actions.contains(&"practice_in_lesson") && !actions.contains(&"redesign_card"),
            "spontaneous capability belongs to recency/episode-aware tutor_tasks, not review rules"
        );
    }

    #[test]
    fn review_rules_do_not_reinfer_listening_capability_from_lifetime_evidence() {
        let value = signals(&[3, 3, 3, 3]);
        let mut actions = Vec::new();
        visit_card_rules(value, |rec| actions.push(rec.action));
        assert!(
            !actions.contains(&"create_listening_card"),
            "listening diagnosis belongs to item-level tutor_tasks"
        );
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
}
