//! Deterministic lesson planning from learner evidence.

use serde::Serialize;

/// Aggregated signals from recent completed lessons.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct RecentLearningSignals {
    /// Number of completed lessons represented in this aggregate.
    pub lessons: u8,
    /// Fraction of active lesson time spent speaking, from 0 to 1.
    pub speaking_ratio: Option<f64>,
    /// Unaided recalls divided by all recall attempts, from 0 to 1.
    pub unaided_recall_rate: Option<f64>,
    /// Mean listening score, from 0 to 1.
    pub listening_score: Option<f64>,
    /// Mean observed answer latency in milliseconds.
    pub avg_answer_millis: Option<u32>,
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
    /// Prioritized scenarios borrowed from the caller.
    pub priority_scenarios: &'a [&'a str],
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
    /// Remaining daily budget for newly activated review cards.
    pub new_card_budget: u8,
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
const BASE_PROBES: [&str; 2] = [
    "Start with 2–4 unaided retrieval/production probes before teaching or hinting.",
    "Vary the situation enough to distinguish memorized recall from transfer.",
];
const BASELINE_PROBE: &str =
    "Treat the first minutes as baseline calibration; do not infer ability from CEFR label alone.";
const SUCCESS_CRITERIA: [&str; 4] = [
    "Target response is produced unaided before model/reveal.",
    "Target is retrieved again after an intervening task.",
    "At least one target is tested in a meaningfully different communicative context.",
    "Any material plan change is justified by evidence recorded in the lesson result.",
];

/// Fixed-capacity plan. Variable input scenarios are borrowed; priority rationales
/// are the only owned strings created by the planner.
#[derive(Debug)]
pub struct AdaptiveLessonPlanFixed<'a> {
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
    /// priority scenarios used by the corresponding adaptation pass.
    pub priority_scenarios: &'a [&'a str],
    include_baseline_probe: bool,
}

/// Borrow the ranked actions retained by this bounded plan.
#[must_use]
pub fn plan_priorities<'store, 'text>(
    plan: &'store AdaptiveLessonPlanFixed<'text>,
) -> &'store [PlanPriorityFixed] {
    &plan.priorities[..plan.priority_len]
}

/// Count diagnostic probes, including the optional baseline probe.
#[must_use]
pub fn plan_diagnostic_probe_count(plan: &AdaptiveLessonPlanFixed<'_>) -> usize {
    2 + usize::from(plan.include_baseline_probe)
}

/// Return the diagnostic probe at the given index, if one is present.
#[must_use]
pub fn plan_diagnostic_probe(
    plan: &AdaptiveLessonPlanFixed<'_>,
    index: usize,
) -> Option<&'static str> {
    match index {
        0 => Some(BASE_PROBES[0]),
        1 => Some(BASE_PROBES[1]),
        2 if plan.include_baseline_probe => Some(BASELINE_PROBE),
        _ => None,
    }
}

/// Return the fixed success criteria used to assess this plan.
#[must_use]
pub const fn plan_success_criteria(_plan: &AdaptiveLessonPlanFixed<'_>) -> &'static [&'static str] {
    &SUCCESS_CRITERIA
}

/// Canonical bounded planner.
#[must_use]
pub fn plan_next_lesson_fixed<'a>(
    input: &LessonPlannerInputView<'a>,
) -> AdaptiveLessonPlanFixed<'a> {
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
    if let Some(rate) = input.recent.unaided_recall_rate
        && rate < 0.60
    {
        push(
            "reduce_prompt_dependence",
            92,
            format!("unaided recall rate is {:.0}%", rate * 100.0),
        );
    }
    if let Some(score) = input.recent.listening_score
        && score < 0.70
    {
        push(
            "strengthen_listening",
            88,
            format!("recent listening score is {:.0}%", score * 100.0),
        );
    }
    if let Some(rate) = input.recent.communicative_task_success_rate
        && rate < 0.70
    {
        push(
            "improve_communicative_task_success",
            86,
            format!("communicative task success is {:.0}%", rate * 100.0),
        );
    }
    if let Some(confidence) = input.recent.confidence
        && confidence < 0.45
    {
        push(
            "stabilize_confidence",
            60,
            format!("recent confidence is {:.0}%", confidence * 100.0),
        );
    }
    if let Some(ratio) = input.recent.speaking_ratio
        && ratio < 0.35
    {
        push(
            "increase_speaking_share",
            58,
            format!("learner speaking ratio is {:.0}%", ratio * 100.0),
        );
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
        .min(input.new_card_budget)
        .min(5);
    if input.method_change_errors > 0 || input.new_card_budget == 0 {
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
        priority_scenarios: input.preferences.priority_scenarios,
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
        "change_method" | "reduce_prompt_dependence" | "stabilize_confidence"
    ) {
        return "supported";
    }
    if let Some(preferred) = input.preferences.preferred_difficulty {
        match preferred {
            "supported" => return "supported",
            "balanced" => return "balanced",
            "stretch" => return "stretch",
            _ => {}
        }
    }
    let strong = input
        .recent
        .unaided_recall_rate
        .is_some_and(|value| value >= 0.80)
        && input
            .recent
            .listening_score
            .is_none_or(|value| value >= 0.75)
        && input
            .recent
            .communicative_task_success_rate
            .is_none_or(|value| value >= 0.80)
        && input.overdue_reviews <= 5;
    if strong { "stretch" } else { "balanced" }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input<'a>(
        method_change_errors: u32,
        due_rechecks: u32,
        new_budget: u8,
        scenarios: &'a [&'a str],
    ) -> LessonPlannerInputView<'a> {
        LessonPlannerInputView {
            due_reviews: 0,
            overdue_reviews: 0,
            due_error_rechecks: due_rechecks,
            active_errors: 0,
            method_change_errors,
            open_recommendations: 0,
            new_card_budget: new_budget,
            recent: RecentLearningSignals::default(),
            preferences: PlannerPreferencesView {
                max_new_items_per_lesson: None,
                preferred_difficulty: None,
                priority_scenarios: scenarios,
            },
        }
    }

    #[test]
    fn one_overdue_review_reduces_but_does_not_block_new_material() {
        let mut value = input(0, 0, 8, &[]);
        value.overdue_reviews = 1;
        let plan = plan_next_lesson_fixed(&value);
        assert!(plan.max_new_items > 0);
        assert!(plan.max_new_items < 3);
    }

    #[test]
    fn method_change_and_rechecks_beat_new_material() {
        let input = input(1, 2, 8, &[]);
        let plan = plan_next_lesson_fixed(&input);
        assert_eq!(plan.primary_objective, "change_method");
        assert_eq!(plan.max_new_items, 0);
        assert_eq!(plan.difficulty, "supported");
        assert_eq!(plan_priorities(&plan)[0].action, "change_method");
        assert_eq!(plan_priorities(&plan)[1].action, "run_error_rechecks");
    }

    #[test]
    fn weak_unaided_recall_prioritizes_prompt_independence() {
        let mut input = input(0, 0, 8, &[]);
        input.recent = RecentLearningSignals {
            lessons: 3,
            unaided_recall_rate: Some(0.4),
            listening_score: Some(0.85),
            ..RecentLearningSignals::default()
        };
        let plan = plan_next_lesson_fixed(&input);
        assert_eq!(plan.primary_objective, "reduce_prompt_dependence");
        assert_eq!(plan.max_new_items, 3);
    }

    #[test]
    fn healthy_state_uses_preference_and_scenarios_without_owned_copy() {
        let scenarios = ["transport"];
        let mut input = input(0, 0, 8, &scenarios);
        input.recent = RecentLearningSignals {
            lessons: 4,
            unaided_recall_rate: Some(0.9),
            listening_score: Some(0.9),
            confidence: Some(0.8),
            speaking_ratio: Some(0.6),
            avg_answer_millis: Some(2_000),
            communicative_task_success_rate: Some(0.9),
        };
        input.preferences.max_new_items_per_lesson = Some(2);
        input.preferences.preferred_difficulty = Some("stretch");
        let plan = plan_next_lesson_fixed(&input);
        assert_eq!(plan.primary_objective, "introduce_high_utility_material");
        assert_eq!(plan.max_new_items, 2);
        assert_eq!(plan.difficulty, "stretch");
        assert_eq!(plan.priority_scenarios, ["transport"]);
    }

    #[test]
    fn baseline_probe_is_included_for_new_history() {
        let input = input(0, 0, 0, &[]);
        let plan = plan_next_lesson_fixed(&input);
        assert_eq!(plan_diagnostic_probe_count(&plan), 3);
        assert!(plan_diagnostic_probe(&plan, 2).is_some());
        assert_eq!(plan_success_criteria(&plan).len(), 4);
    }
}
