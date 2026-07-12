//! Review orchestration logic (05-review-engine.md §3, §8): queue ordering
//! and card status transitions. Scheduling math stays in `review-engine`.

use learning_core::{CardId, CardStatus, Timestamp};
use review_engine::Grade;

/// Overdue margin marking a review card "significantly overdue" (05 §8
/// priority 2). Design decision: 2 days past due.
pub const SIGNIFICANT_OVERDUE_SECONDS: i64 = 2 * 86_400;

/// Queue candidate assembled by the worker from `cards` + `card_schedule_state`.
#[derive(Debug, Clone)]
pub struct QueueCandidate {
    /// Card ID.
    pub card_id: CardId,
    /// Card lifecycle status.
    pub status: CardStatus,
    /// Due instant, when scheduled.
    pub due_at: Option<Timestamp>,
    /// Predicted recall probability at `now` (lower = riskier).
    pub retrievability: Option<f64>,
}

/// Card status transition per grade (05-review-engine.md §3, §5).
/// `Suspended`/`Retired`/`Candidate` are not reviewable; returns `None`.
#[must_use]
pub fn next_card_status(current: CardStatus, grade: Grade) -> Option<CardStatus> {
    let passed = grade != Grade::Again;
    match current {
        CardStatus::New => Some(CardStatus::Learning),
        CardStatus::Learning => Some(if passed {
            CardStatus::Review
        } else {
            CardStatus::Learning
        }),
        CardStatus::Review => Some(if passed {
            CardStatus::Review
        } else {
            CardStatus::Relearning
        }),
        CardStatus::Relearning => Some(if passed {
            CardStatus::Review
        } else {
            CardStatus::Relearning
        }),
        CardStatus::Candidate | CardStatus::Suspended | CardStatus::Retired => None,
    }
}

/// Builds the ordered due queue (05 §8):
/// 1 relearning; 2 significantly overdue review; 3 due review by forgetting
/// risk (ascending retrievability); 4 due learning; 5 new within budget.
/// Bounded by `max_batch`.
#[must_use]
pub fn build_queue(
    candidates: &[QueueCandidate],
    now: Timestamp,
    new_card_budget: u8,
    max_batch: u16,
) -> Vec<CardId> {
    let due = |c: &&QueueCandidate| c.due_at.is_some_and(|d| d.0 <= now.0);

    let relearning: Vec<&QueueCandidate> = candidates
        .iter()
        .filter(|c| c.status == CardStatus::Relearning)
        .filter(due)
        .collect();
    let (overdue, due_review): (Vec<&QueueCandidate>, Vec<&QueueCandidate>) = candidates
        .iter()
        .filter(|c| c.status == CardStatus::Review)
        .filter(due)
        .partition(|c| {
            c.due_at
                .is_some_and(|d| now.0 - d.0 >= SIGNIFICANT_OVERDUE_SECONDS)
        });
    let mut due_review = due_review;
    due_review.sort_by(|a, b| {
        let risk_a = a.retrievability.unwrap_or(0.0);
        let risk_b = b.retrievability.unwrap_or(0.0);
        risk_a
            .partial_cmp(&risk_b)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let learning: Vec<&QueueCandidate> = candidates
        .iter()
        .filter(|c| c.status == CardStatus::Learning)
        .filter(due)
        .collect();
    let new_cards: Vec<&QueueCandidate> = candidates
        .iter()
        .filter(|c| c.status == CardStatus::New)
        .take(usize::from(new_card_budget))
        .collect();

    relearning
        .into_iter()
        .chain(overdue)
        .chain(due_review)
        .chain(learning)
        .chain(new_cards)
        .take(usize::from(max_batch))
        .map(|c| c.card_id.clone())
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    const NOW: Timestamp = Timestamp(1_780_000_000);

    fn candidate(
        id: &str,
        status: CardStatus,
        due_offset: Option<i64>,
        retrievability: Option<f64>,
    ) -> QueueCandidate {
        QueueCandidate {
            card_id: CardId(id.to_owned()),
            status,
            due_at: due_offset.map(|o| Timestamp(NOW.0 + o)),
            retrievability,
        }
    }

    #[test]
    fn queue_priorities_follow_spec_order() {
        let candidates = vec![
            candidate("new-1", CardStatus::New, None, None),
            candidate("review-risky", CardStatus::Review, Some(-3_600), Some(0.70)),
            candidate("review-safe", CardStatus::Review, Some(-3_600), Some(0.92)),
            candidate("overdue", CardStatus::Review, Some(-3 * 86_400), Some(0.50)),
            candidate("learning", CardStatus::Learning, Some(-60), None),
            candidate("relearning", CardStatus::Relearning, Some(-60), None),
            candidate("future", CardStatus::Review, Some(86_400), Some(0.99)),
            candidate("suspended", CardStatus::Suspended, Some(-60), None),
        ];
        let queue = build_queue(&candidates, NOW, 8, 50);
        let ids: Vec<&str> = queue.iter().map(|c| c.0.as_str()).collect();
        assert_eq!(
            ids,
            [
                "relearning",
                "overdue",
                "review-risky",
                "review-safe",
                "learning",
                "new-1"
            ]
        );
    }

    #[test]
    fn new_cards_respect_budget_and_batch_is_bounded() {
        let mut candidates: Vec<QueueCandidate> = (0..10)
            .map(|i| candidate(&format!("new-{i}"), CardStatus::New, None, None))
            .collect();
        candidates.extend((0..60).map(|i| {
            candidate(
                &format!("due-{i}"),
                CardStatus::Review,
                Some(-60),
                Some(0.9),
            )
        }));
        let queue = build_queue(&candidates, NOW, 3, 50);
        assert_eq!(queue.len(), 50, "bounded by max_review_batch");
        let news = queue.iter().filter(|c| c.0.starts_with("new-")).count();
        assert_eq!(news, 0, "batch fills with due cards before new");

        let small = build_queue(&candidates[..10], NOW, 3, 50);
        assert_eq!(small.len(), 3, "new cards capped by daily budget");
    }

    #[test]
    fn status_transitions_match_v1_table() {
        use Grade::{Again, Good};
        assert_eq!(
            next_card_status(CardStatus::New, Again),
            Some(CardStatus::Learning)
        );
        assert_eq!(
            next_card_status(CardStatus::Learning, Good),
            Some(CardStatus::Review)
        );
        assert_eq!(
            next_card_status(CardStatus::Learning, Again),
            Some(CardStatus::Learning)
        );
        assert_eq!(
            next_card_status(CardStatus::Review, Again),
            Some(CardStatus::Relearning)
        );
        assert_eq!(
            next_card_status(CardStatus::Relearning, Good),
            Some(CardStatus::Review)
        );
        assert_eq!(next_card_status(CardStatus::Suspended, Good), None);
        assert_eq!(next_card_status(CardStatus::Candidate, Good), None);
    }
}
