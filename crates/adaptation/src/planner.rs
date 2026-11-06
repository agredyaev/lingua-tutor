//! Deterministic lesson planning from learner evidence.

use serde::Serialize;

/// Aggregated signals from recent completed lessons.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct RecentLearningSignals {
    /// Number of completed lessons represented in this aggregate.
    pub lessons: u8,
    /// Fraction of active lesson time spent speaking, from 0 to 1.
    pub speaking_ratio: Option<f64>,
    /// Number of observed recall attempts contributing to the prompt-dependence signal.
    pub recall_attempts: u16,
    /// Number of distinct recent lessons that contained at least one recall attempt.
    pub recall_lessons: u8,
    /// Unaided recalls divided by all recall attempts, from 0 to 1.
    pub unaided_recall_rate: Option<f64>,
    /// Number of explicit communicative-task attempts under the current strong rubric.
    pub communicative_task_attempts: u16,
    /// Number of distinct recent lessons containing strong-rubric task attempts.
    pub communicative_task_lessons: u8,
    /// Fraction of explicit communicative tasks completed successfully.
    pub communicative_task_success_rate: Option<f64>,
    /// Mean tutor-estimated learner confidence, from 0 to 1. Diagnostic only.
    pub confidence: Option<f64>,
}

/// Borrowed preferences used by the canonical bounded planner.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct PlannerPreferencesView<'a> {
    /// Optional cap on newly introduced items in one tutor lesson.
    pub max_new_items_per_lesson: Option<u8>,
    /// Optional preferred difficulty.
    pub preferred_difficulty: Option<&'a str>,
}

/// Borrowed scalar planner input used by the canonical bounded planner.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct LessonPlannerInputView<'a> {
    /// Number of cards scheduled for review in the planning window.
    pub due_reviews: u32,
    /// overdue reviews used by the corresponding adaptation pass.
    pub overdue_reviews: u32,
    /// Number of scheduled error-target rechecks due now.
    pub due_error_rechecks: u32,
    /// active errors used by the corresponding adaptation pass.
    pub active_errors: u32,
    /// Number of recurring errors requiring a change of teaching method.
    pub method_change_errors: u32,
    /// open recommendations used by the corresponding adaptation pass.
    pub open_recommendations: u32,
    /// recent used by the corresponding adaptation pass.
    pub recent: RecentLearningSignals,
    /// Learner preferences used to bound and prioritize the lesson plan.
    pub preferences: PlannerPreferencesView<'a>,
}

/// Fixed ranked action used by the canonical planner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub struct PlanPriorityFixed {
    /// Stable identifier of the proposed tutoring or review action.
    pub action: &'static str,
    /// score used by the corresponding adaptation pass.
    pub score: u8,
    /// Evidence-based explanation for the action or recommendation.
    pub rationale: String,
}

const MAX_PRIORITIES: usize = 12;
const BASE_PROBES: [&str; 1] = [
    "Use one minimal unaided probe that can change the next decision: discriminate the highest-priority bottleneck or revalidate the stale capability before teaching or hinting.",
];
const BASELINE_PROBE: &str =
    "Treat the first minutes as baseline calibration; do not infer ability from CEFR label alone.";
const SUCCESS_CRITERIA: [&str; 4] = [
    "Target response is produced unaided before model/reveal.",
    "Target is retrieved again after an intervening task.",
    "At least one target is tested in a meaningfully different communicative context.",
    "Any material plan change is justified by evidence recorded in the lesson result.",
];

/// Fixed-capacity plan. Priority rationales are the only owned strings created by the planner.
#[derive(Debug)]
pub struct AdaptiveLessonPlanFixed {
    /// Highest-priority learning objective selected for the lesson.
    pub primary_objective: &'static str,
    /// difficulty used by the corresponding adaptation pass.
    pub difficulty: &'static str,
    /// Maximum number of learning items to introduce during this lesson.
    pub max_new_items: u8,
    priorities: [PlanPriorityFixed; MAX_PRIORITIES],
    priority_len: usize,
    /// Learning signals used when selecting this plan.
    pub recent_signals: RecentLearningSignals,
    include_baseline_probe: bool,
}

/// Borrow the ranked actions retained by this bounded plan.
#[must_use]
pub fn plan_priorities(plan: &AdaptiveLessonPlanFixed) -> &[PlanPriorityFixed] {
    &plan.priorities[..plan.priority_len]
}

/// Count diagnostic probes, including the optional baseline probe.
#[must_use]
pub fn plan_diagnostic_probe_count(plan: &AdaptiveLessonPlanFixed) -> usize {
    1 + usize::from(plan.include_baseline_probe)
}

/// Return the diagnostic probe at the given index, if one is present.
#[must_use]
pub fn plan_diagnostic_probe(plan: &AdaptiveLessonPlanFixed, index: usize) -> Option<&'static str> {
    match index {
        0 => Some(BASE_PROBES[0]),
        1 if plan.include_baseline_probe => Some(BASELINE_PROBE),
        _ => None,
    }
}

/// Return the fixed success criteria used to assess this plan.
#[must_use]
pub const fn plan_success_criteria(_plan: &AdaptiveLessonPlanFixed) -> &'static [&'static str] {
    &SUCCESS_CRITERIA
}

/// Canonical bounded planner.
#[must_use]
pub fn plan_next_lesson_fixed(input: &LessonPlannerInputView<'_>) -> AdaptiveLessonPlanFixed {
    let mut priorities: [PlanPriorityFixed; MAX_PRIORITIES] =
        std::array::from_fn(|_| PlanPriorityFixed::default());
    let mut len = 0usize;

    let mut push = |action: &'static str, score: u8, rationale: String| {
        debug_assert!(len < MAX_PRIORITIES);
        priorities[len] = PlanPriorityFixed {
            action,
            score,
            rationale,
        };
        len += 1;
    };

    if input.method_change_errors > 0 {
        push(
            "change_method",
            100,
            format!(
                "{} unresolved error target(s) require a different teaching method",
                input.method_change_errors
            ),
        );
    }
    if input.due_error_rechecks > 0 {
        push(
            "run_error_rechecks",
            96,
            format!(
                "{} scheduled error recheck(s) are due",
                input.due_error_rechecks
            ),
        );
    }
    if let Some(rate) = input.recent.communicative_task_success_rate
        && rate < 0.70
    {
        let attempts = input.recent.communicative_task_attempts;
        let distributed = input.recent.communicative_task_lessons >= 2;
        let score = if attempts >= 3 && distributed { 94 } else { 84 };
        push(
            "improve_communicative_task_success",
            score,
            format!(
                "communicative task success is {:.0}% across {} v2 task attempt(s)",
                rate * 100.0,
                attempts
            ),
        );
    }
    if let Some(rate) = input.recent.unaided_recall_rate
        && rate < 0.60
    {
        let attempts = input.recent.recall_attempts;
        if attempts >= 3 && input.recent.recall_lessons >= 2 {
            push(
                "reduce_prompt_dependence",
                90,
                format!(
                    "unaided recall rate is {:.0}% across {attempts} recall attempt(s)",
                    rate * 100.0
                ),
            );
        } else if attempts > 0 {
            push(
                "probe_prompt_independence",
                64,
                format!(
                    "unaided recall rate is {:.0}% but only {attempts} recall attempt(s) are observed",
                    rate * 100.0
                ),
            );
        }
    }
    if input.active_errors > 0 {
        push(
            "repair_recurring_errors",
            78,
            format!("{} active recurring error target(s)", input.active_errors),
        );
    }
    if input.overdue_reviews > 0 {
        push(
            "consolidate_overdue_material",
            76,
            format!("{} review item(s) are overdue", input.overdue_reviews),
        );
    }
    if input.open_recommendations > 0 {
        push(
            "apply_adaptive_recommendations",
            72,
            format!(
                "{} evidence-based recommendation(s) are open",
                input.open_recommendations
            ),
        );
    }
    if input.due_reviews > 0 {
        push(
            "retrieve_due_material",
            68,
            format!("{} review item(s) are due", input.due_reviews),
        );
    }

    let mut max_new_items = input
        .preferences
        .max_new_items_per_lesson
        .unwrap_or(3)
        .min(5);
    if input.method_change_errors > 0 {
        max_new_items = 0;
    } else {
        max_new_items = apply_overdue_pressure(max_new_items, input.overdue_reviews);
    }
    if max_new_items > 0 {
        push(
            "introduce_high_utility_material",
            55,
            format!("capacity permits up to {max_new_items} new item(s)"),
        );
    }

    if len == 0 {
        priorities[0] = PlanPriorityFixed {
            action: "diagnose_and_expand",
            score: 50,
            rationale:
                "no urgent weakness is detected; probe retained skills, then expand one useful scenario"
                    .to_owned(),
        };
        len = 1;
    }

    priorities[..len].sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.action.cmp(right.action))
    });

    let primary_objective = priorities[0].action;
    let difficulty = choose_difficulty_fixed(input, primary_objective);

    AdaptiveLessonPlanFixed {
        primary_objective,
        difficulty,
        max_new_items,
        priorities,
        priority_len: len,
        recent_signals: input.recent,
        include_baseline_probe: input.recent.lessons < 2,
    }
}

fn apply_overdue_pressure(base: u8, overdue: u32) -> u8 {
    match overdue {
        0 => base,
        1..=5 => base.saturating_sub(1),
        6..=20 => base.div_ceil(2),
        21..=50 => base.min(1),
        _ => 0,
    }
}

fn choose_difficulty_fixed(input: &LessonPlannerInputView<'_>, primary: &str) -> &'static str {
    if matches!(
        primary,
        "change_method" | "reduce_prompt_dependence" | "improve_communicative_task_success"
    ) {
        return "supported";
    }
    if let Some(preferred) = input.preferences.preferred_difficulty {
        match preferred {
            "supported" => return "supported",
            "balanced" => return "balanced",
            _ => {}
        }
    }

    // Stretch is an evidence claim, not merely a preference. A short history
    // stays balanced even when its first observations are perfect.
    let strong = input.recent.lessons >= 3
        && input.recent.recall_attempts >= 3
        && input.recent.recall_lessons >= 2
        && input
            .recent
            .unaided_recall_rate
            .is_some_and(|value| value >= 0.80)
        && input
            .recent
            .communicative_task_success_rate
            .is_some_and(|value| {
                input.recent.communicative_task_attempts >= 3
                    && input.recent.communicative_task_lessons >= 2
                    && value >= 0.80
            })
        && input.overdue_reviews <= 5;
    if strong { "stretch" } else { "balanced" }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(method_change_errors: u32, due_rechecks: u32) -> LessonPlannerInputView<'static> {
        LessonPlannerInputView {
            due_reviews: 0,
            overdue_reviews: 0,
            due_error_rechecks: due_rechecks,
            active_errors: 0,
            method_change_errors,
            open_recommendations: 0,
            recent: RecentLearningSignals::default(),
            preferences: PlannerPreferencesView {
                max_new_items_per_lesson: None,
                preferred_difficulty: None,
            },
        }
    }

    #[test]
    fn review_card_budget_does_not_gate_lesson_new_material() {
        let value = input(0, 0);
        let plan = plan_next_lesson_fixed(&value);
        assert_eq!(plan.max_new_items, 3);
    }

    #[test]
    fn one_overdue_review_reduces_but_does_not_block_new_material() {
        let mut value = input(0, 0);
        value.overdue_reviews = 1;
        let plan = plan_next_lesson_fixed(&value);
        assert!(plan.max_new_items > 0);
        assert!(plan.max_new_items < 3);
    }

    #[test]
    fn method_change_and_rechecks_beat_new_material() {
        let input = input(1, 2);
        let plan = plan_next_lesson_fixed(&input);
        assert_eq!(plan.primary_objective, "change_method");
        assert_eq!(plan.max_new_items, 0);
        assert_eq!(plan.difficulty, "supported");
        assert_eq!(plan_priorities(&plan)[0].action, "change_method");
        assert_eq!(plan_priorities(&plan)[1].action, "run_error_rechecks");
    }

    #[test]
    fn sparse_recall_evidence_probes_before_declaring_prompt_dependence() {
        let mut input = input(0, 0);
        input.recent = RecentLearningSignals {
            lessons: 2,
            recall_attempts: 1,
            unaided_recall_rate: Some(0.0),
            ..RecentLearningSignals::default()
        };
        let plan = plan_next_lesson_fixed(&input);
        assert_eq!(plan.primary_objective, "probe_prompt_independence");
    }

    #[test]
    fn single_lesson_recall_cluster_probes_before_declaring_prompt_dependence() {
        let mut input = input(0, 0);
        input.recent = RecentLearningSignals {
            lessons: 4,
            recall_attempts: 4,
            recall_lessons: 1,
            unaided_recall_rate: Some(0.25),
            ..RecentLearningSignals::default()
        };
        let plan = plan_next_lesson_fixed(&input);
        assert_eq!(plan.primary_objective, "probe_prompt_independence");
    }

    #[test]
    fn weak_unaided_recall_prioritizes_prompt_independence() {
        let mut input = input(0, 0);
        input.recent = RecentLearningSignals {
            lessons: 3,
            recall_attempts: 4,
            recall_lessons: 2,
            unaided_recall_rate: Some(0.4),
            ..RecentLearningSignals::default()
        };
        let plan = plan_next_lesson_fixed(&input);
        assert_eq!(plan.primary_objective, "reduce_prompt_dependence");
        assert_eq!(plan.max_new_items, 3);
    }

    #[test]
    fn healthy_state_uses_scalar_preferences() {
        let mut input = input(0, 0);
        input.recent = RecentLearningSignals {
            lessons: 4,
            recall_attempts: 4,
            recall_lessons: 2,
            unaided_recall_rate: Some(0.9),
            confidence: Some(0.8),
            speaking_ratio: Some(0.6),
            communicative_task_attempts: 4,
            communicative_task_lessons: 2,
            communicative_task_success_rate: Some(0.9),
        };
        input.preferences.max_new_items_per_lesson = Some(2);
        input.preferences.preferred_difficulty = Some("stretch");
        let plan = plan_next_lesson_fixed(&input);
        assert_eq!(plan.primary_objective, "introduce_high_utility_material");
        assert_eq!(plan.max_new_items, 2);
        assert_eq!(plan.difficulty, "stretch");
    }

    #[test]
    fn communicative_task_failure_outranks_profile_proxy_weaknesses() {
        let mut value = input(0, 0);
        value.recent = RecentLearningSignals {
            lessons: 4,
            recall_attempts: 4,
            recall_lessons: 2,
            unaided_recall_rate: Some(0.40),
            communicative_task_attempts: 4,
            communicative_task_lessons: 2,
            communicative_task_success_rate: Some(0.40),
            confidence: Some(0.20),
            speaking_ratio: Some(0.20),
        };
        let plan = plan_next_lesson_fixed(&value);
        assert_eq!(plan.primary_objective, "improve_communicative_task_success");
    }

    #[test]
    fn single_lesson_task_failure_does_not_outrank_distributed_prompt_dependence() {
        let mut value = input(0, 0);
        value.recent = RecentLearningSignals {
            lessons: 4,
            recall_attempts: 4,
            recall_lessons: 2,
            unaided_recall_rate: Some(0.40),
            communicative_task_attempts: 4,
            communicative_task_lessons: 1,
            communicative_task_success_rate: Some(0.0),
            ..RecentLearningSignals::default()
        };
        let plan = plan_next_lesson_fixed(&value);
        assert_eq!(plan.primary_objective, "reduce_prompt_dependence");
    }

    #[test]
    fn sparse_task_failure_does_not_outrank_established_prompt_dependence() {
        let mut value = input(0, 0);
        value.recent = RecentLearningSignals {
            lessons: 4,
            recall_attempts: 4,
            recall_lessons: 2,
            unaided_recall_rate: Some(0.40),
            communicative_task_attempts: 1,
            communicative_task_success_rate: Some(0.0),
            ..RecentLearningSignals::default()
        };
        let plan = plan_next_lesson_fixed(&value);
        assert_eq!(plan.primary_objective, "reduce_prompt_dependence");
        let task_score = plan_priorities(&plan)
            .iter()
            .find(|priority| priority.action == "improve_communicative_task_success")
            .map(|priority| priority.score);
        assert!(task_score.is_some_and(|score| score < 90));
    }

    #[test]
    fn confidence_and_speaking_share_are_diagnostic_not_lesson_objectives() {
        let mut value = input(0, 0);
        value.recent = RecentLearningSignals {
            lessons: 4,
            confidence: Some(0.10),
            speaking_ratio: Some(0.10),
            ..RecentLearningSignals::default()
        };
        let plan = plan_next_lesson_fixed(&value);
        assert_eq!(plan.primary_objective, "introduce_high_utility_material");
        assert!(plan_priorities(&plan).iter().all(|priority| !matches!(
            priority.action,
            "stabilize_confidence" | "increase_speaking_share"
        )));
    }

    #[test]
    fn single_lesson_success_clusters_block_stretch() {
        let mut value = input(0, 0);
        value.recent = RecentLearningSignals {
            lessons: 5,
            recall_attempts: 4,
            recall_lessons: 1,
            unaided_recall_rate: Some(1.0),
            communicative_task_attempts: 4,
            communicative_task_lessons: 1,
            communicative_task_success_rate: Some(1.0),
            ..RecentLearningSignals::default()
        };
        value.preferences.preferred_difficulty = Some("stretch");
        let plan = plan_next_lesson_fixed(&value);
        assert_eq!(plan.difficulty, "balanced");
    }

    #[test]
    fn sparse_recall_evidence_blocks_stretch() {
        let mut value = input(0, 0);
        value.recent = RecentLearningSignals {
            lessons: 5,
            recall_attempts: 1,
            unaided_recall_rate: Some(1.0),
            communicative_task_attempts: 4,
            communicative_task_success_rate: Some(1.0),
            ..RecentLearningSignals::default()
        };
        value.preferences.preferred_difficulty = Some("stretch");
        let plan = plan_next_lesson_fixed(&value);
        assert_eq!(plan.difficulty, "balanced");
    }

    #[test]
    fn missing_communicative_task_evidence_blocks_stretch() {
        let mut value = input(0, 0);
        value.recent = RecentLearningSignals {
            lessons: 5,
            recall_attempts: 4,
            unaided_recall_rate: Some(1.0),
            ..RecentLearningSignals::default()
        };
        value.preferences.preferred_difficulty = Some("stretch");
        let plan = plan_next_lesson_fixed(&value);
        assert_eq!(plan.difficulty, "balanced");
    }

    #[test]
    fn sparse_communicative_task_evidence_blocks_stretch() {
        let mut value = input(0, 0);
        value.recent = RecentLearningSignals {
            lessons: 5,
            recall_attempts: 4,
            unaided_recall_rate: Some(1.0),
            communicative_task_attempts: 1,
            communicative_task_success_rate: Some(1.0),
            ..RecentLearningSignals::default()
        };
        value.preferences.preferred_difficulty = Some("stretch");
        let plan = plan_next_lesson_fixed(&value);
        assert_eq!(plan.difficulty, "balanced");
    }

    #[test]
    fn short_history_never_selects_stretch_difficulty() {
        let mut value = input(0, 0);
        value.recent = RecentLearningSignals {
            lessons: 1,
            recall_attempts: 1,
            unaided_recall_rate: Some(1.0),
            communicative_task_success_rate: Some(1.0),
            confidence: Some(1.0),
            speaking_ratio: Some(0.8),
            ..RecentLearningSignals::default()
        };
        value.preferences.preferred_difficulty = Some("stretch");
        let plan = plan_next_lesson_fixed(&value);
        assert_eq!(plan.difficulty, "balanced");
    }

    #[test]
    fn established_history_uses_one_initial_diagnostic_probe() {
        let mut value = input(0, 0);
        value.recent.lessons = 5;
        let plan = plan_next_lesson_fixed(&value);
        assert_eq!(
            plan_diagnostic_probe_count(&plan),
            1,
            "known learner state should not force a second diagnostic before intervention"
        );
    }

    #[test]
    fn baseline_probe_is_included_for_new_history() {
        let input = input(0, 0);
        let plan = plan_next_lesson_fixed(&input);
        assert_eq!(plan_diagnostic_probe_count(&plan), 2);
        assert!(plan_diagnostic_probe(&plan, 1).is_some());
        assert_eq!(plan_success_criteria(&plan).len(), 4);
    }
}
