//! Deterministic adaptive rules as explicit Rust (02-workspace.md §2: no
//! interpreted rule language): backlog protection + new-card budget
//! throttling (05-review-engine.md §9) and the 07 §7 recommendation table.

mod calibration;
mod capability;
mod focus;
mod planner;
mod queue;
mod rules;
mod tasks;

pub use calibration::{InterventionEffect, summarize_intervention_history};
pub use capability::{
    CapabilityEstimateFixed, CapabilityEstimateView, CapabilityKind, ContextSetView,
    ItemCapabilityStateFixed, ItemCapabilityView, MAX_CAPABILITY_OBSERVATIONS, TextSetRef,
    capability_estimate_view, context_set_slice, item_capability_view,
    project_item_capabilities_columns,
};
pub use focus::{
    TutorFocusDecisionView, TutorFocusThreadView, format_focus_policy, format_focus_rationale,
    visit_tutor_focus_decisions, visit_tutor_focus_threads,
};
pub use planner::{
    AdaptiveLessonPlanFixed, LessonPlannerInputView, PlanPriorityFixed, PlannerPreferencesView,
    RecentLearningSignals, plan_diagnostic_probe, plan_diagnostic_probe_count,
    plan_next_lesson_fixed, plan_priorities, plan_success_criteria,
};
pub use queue::{TutorTask, criterion, default_action, default_focus_score, evidence_capability};
pub use rules::{
    CardSignalView, GRADE_WINDOW, RecommendationView, error_escalation_action,
    recheck_failed_fields, visit_card_rules,
};
pub use tasks::{TaskCandidateView, visit_task_candidates};

/// One study day's signals, most recent first.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DailySignal {
    /// Minutes spent reviewing.
    pub review_minutes: u16,
    /// Active-recall success rate 0..=1 across graded reviews.
    pub recall_rate: f64,
}

/// Overdue-card count above which new cards pause (05 §9). Design decision.
pub const OVERDUE_PAUSE_THRESHOLD: u32 = 50;
/// Recall rate below which budget halves.
pub const RECALL_RATE_FLOOR: f64 = 0.70;

/// Today's effective new-card budget (05-review-engine.md §9):
/// - overdue backlog above threshold → 0 (pause);
/// - review time > 12 min on 2 consecutive study days → halve;
/// - recall < 70% on 2 consecutive study days → halve;
/// - backlog cleared → restore gradually: at most `yesterday_budget + 1`,
///   never an instant jump back to base.
#[must_use]
pub fn new_card_budget(
    base_budget: u8,
    yesterday_budget: u8,
    overdue_count: u32,
    review_minutes_limit: u16,
    last_two_days: Option<(DailySignal, DailySignal)>,
) -> u8 {
    if overdue_count > OVERDUE_PAUSE_THRESHOLD {
        return 0;
    }
    let mut budget = base_budget;
    if let Some((a, b)) = last_two_days {
        if a.review_minutes > review_minutes_limit && b.review_minutes > review_minutes_limit {
            budget /= 2;
        }
        if a.recall_rate < RECALL_RATE_FLOOR && b.recall_rate < RECALL_RATE_FLOOR {
            budget /= 2;
        }
    }
    // Gradual restore: never grow more than +1 per day.
    if budget > yesterday_budget.saturating_add(1) {
        budget = yesterday_budget.saturating_add(1);
    }
    budget
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    const CALM: DailySignal = DailySignal {
        review_minutes: 8,
        recall_rate: 0.9,
    };
    const HEAVY: DailySignal = DailySignal {
        review_minutes: 15,
        recall_rate: 0.9,
    };
    const WEAK: DailySignal = DailySignal {
        review_minutes: 8,
        recall_rate: 0.6,
    };

    #[test]
    fn overdue_backlog_pauses_new_cards() {
        assert_eq!(new_card_budget(8, 8, 51, 12, Some((CALM, CALM))), 0);
    }

    #[test]
    fn two_heavy_days_halve_budget() {
        assert_eq!(new_card_budget(8, 8, 0, 12, Some((HEAVY, HEAVY))), 4);
        // One heavy day is not enough.
        assert_eq!(new_card_budget(8, 8, 0, 12, Some((HEAVY, CALM))), 8);
    }

    #[test]
    fn two_weak_recall_days_halve_budget_and_stack_with_time() {
        assert_eq!(new_card_budget(8, 8, 0, 12, Some((WEAK, WEAK))), 4);
        let both = DailySignal {
            review_minutes: 15,
            recall_rate: 0.6,
        };
        assert_eq!(new_card_budget(8, 8, 0, 12, Some((both, both))), 2);
    }

    #[test]
    fn restore_is_gradual_not_instant() {
        // Yesterday paused (0) → today at most 1, even if signals are calm.
        assert_eq!(new_card_budget(8, 0, 0, 12, Some((CALM, CALM))), 1);
        assert_eq!(new_card_budget(8, 3, 0, 12, Some((CALM, CALM))), 4);
        assert_eq!(new_card_budget(8, 8, 0, 12, Some((CALM, CALM))), 8);
    }

    #[test]
    fn no_history_means_base_budget_capped_by_gradual_restore() {
        assert_eq!(new_card_budget(8, 8, 0, 12, None), 8);
    }
}
