//! Deterministic lesson planning from learner evidence.

use serde::Serialize;

/// Aggregated signals from recent completed lessons.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct RecentLearningSignals {
    /// Number of completed lessons represented in this aggregate.
    pub lessons: u8,
    /// Number of measured spontaneous-production attempts.
    pub recall_attempts: u16,
    /// Number of distinct recent lessons containing spontaneous-production evidence.
    pub recall_lessons: u8,
    /// Explicitly unaided spontaneous-production success rate, balanced per lesson.
    pub unaided_recall_rate: Option<f64>,
    /// Whether relevant spontaneous-production lesson evidence spans the anti-massing gap.
    pub recall_temporally_distributed: bool,
    /// Number of explicit communicative-task attempts under the current strong rubric. Telemetry/rationale only; not evidence-strength authority.
    pub communicative_task_attempts: u16,
    /// Number of distinct recent lessons containing strong-rubric task attempts. Canonical episode-count authority for task-outcome strength.
    pub communicative_task_lessons: u8,
    /// Fraction of explicit communicative tasks completed successfully.
    pub communicative_task_success_rate: Option<f64>,
    /// Whether strong-rubric task lesson evidence spans the anti-massing gap.
    pub communicative_task_temporally_distributed: bool,
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
const MIN_ESTABLISHED_TASK_EPISODES: u8 = 3;
/// Priority that marks a sufficiently established communicative-task failure.
pub const ESTABLISHED_TASK_FAILURE_PRIORITY: u8 = 98;
/// Priority that marks sufficiently established spontaneous-production weakness.
pub const ESTABLISHED_SPONTANEOUS_WEAKNESS_PRIORITY: u8 = 88;
const BASE_PROBES: [&str; 1] = [
    "Use one minimal unaided probe that can change the next decision: discriminate the highest-priority bottleneck or revalidate the stale capability before teaching or hinting.",
];
const BASELINE_PROBE: &str =
    "Treat the first minutes as baseline calibration; do not infer ability from CEFR label alone.";
const SUCCESS_CRITERIA: [&str; 4] = [
    "The primary communicative objective is attempted before targeted support; record the real task outcome when the lesson contains an explicit communicative task.",
    "If an intervention is used, its post-intervention check measures the same target and capability without repeating the same support.",
    "Delayed retrieval or transfer is checked when it is due or can change the next retention/transfer decision; it is not a per-lesson quota.",
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

fn include_generic_diagnostic_probe(plan: &AdaptiveLessonPlanFixed) -> bool {
    plan.primary_objective != "run_error_rechecks"
}

/// Count diagnostic probes, including the optional baseline probe.
#[must_use]
pub fn plan_diagnostic_probe_count(plan: &AdaptiveLessonPlanFixed) -> usize {
    if include_generic_diagnostic_probe(plan) {
        1 + usize::from(plan.include_baseline_probe)
    } else {
        0
    }
}

/// Return the diagnostic probe at the given index, if one is present.
#[must_use]
pub fn plan_diagnostic_probe(plan: &AdaptiveLessonPlanFixed, index: usize) -> Option<&'static str> {
    if !include_generic_diagnostic_probe(plan) {
        return None;
    }
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

    let task_failure_rate = input
        .recent
        .communicative_task_success_rate
        .filter(|rate| *rate < 0.70);
    let established_task_failure = task_failure_rate.is_some()
        && input.recent.communicative_task_lessons >= MIN_ESTABLISHED_TASK_EPISODES;
    let spontaneous_failure_rate = input.recent.unaided_recall_rate.filter(|rate| *rate < 0.60);
    let established_spontaneous_weakness = spontaneous_failure_rate.is_some()
        && input.recent.recall_attempts >= 3
        && input.recent.recall_lessons >= 2;

    if input.method_change_errors > 0 {
        push(
            "change_method",
            97,
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
    if let Some(rate) = task_failure_rate {
        let attempts = input.recent.communicative_task_attempts;
        let score = if established_task_failure {
            ESTABLISHED_TASK_FAILURE_PRIORITY
        } else {
            84
        };
        push(
            "improve_communicative_task_success",
            score,
            format!(
                "communicative task success is {:.0}% across {} task-bearing lesson episode(s); {} v2 attempt(s) were reported",
                rate * 100.0,
                input.recent.communicative_task_lessons,
                attempts
            ),
        );
    }
    if let Some(rate) = spontaneous_failure_rate {
        push(
            "probe_prompt_independence",
            if established_spontaneous_weakness {
                ESTABLISHED_SPONTANEOUS_WEAKNESS_PRIORITY
            } else {
                64
            },
            format!(
                "unaided spontaneous production is {:.0}%; diagnose the same-item bottleneck before choosing an intervention",
                rate * 100.0
            ),
        );
    }
    if input.active_errors > 0 {
        push(
            "repair_recurring_errors",
            78,
            format!("{} active recurring error target(s)", input.active_errors),
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
    let mut max_new_items = input
        .preferences
        .max_new_items_per_lesson
        .unwrap_or(3)
        .min(5);
    if input.method_change_errors > 0 {
        max_new_items = 0;
    } else if established_task_failure || established_spontaneous_weakness {
        max_new_items = max_new_items.min(1);
    } else {
        max_new_items = apply_overdue_pressure(max_new_items, input.overdue_reviews);
    }
    if max_new_items > 0 && !established_task_failure && !established_spontaneous_weakness {
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
        "change_method" | "improve_communicative_task_success"
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
    if primary == "run_error_rechecks" {
        return "balanced";
    }

    // Stretch is an evidence claim, not merely a preference. A short history
    // stays balanced even when its first observations are perfect.
    let strong = input.recent.lessons >= 3
        && input.recent.recall_attempts >= 3
        && input.recent.recall_lessons >= 2
        && input.recent.recall_temporally_distributed
        && input
            .recent
            .unaided_recall_rate
            .is_some_and(|value| value >= 0.80)
        && input.recent.communicative_task_temporally_distributed
        && input
            .recent
            .communicative_task_success_rate
            .is_some_and(|value| {
                input.recent.communicative_task_lessons >= MIN_ESTABLISHED_TASK_EPISODES
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
    fn review_backlog_throttles_lesson_load_but_does_not_become_tutor_objective() {
        let mut value = input(0, 0);
        value.due_reviews = 25;
        value.overdue_reviews = 10;
        value.recent = RecentLearningSignals {
            lessons: 4,
            recall_attempts: 4,
            recall_lessons: 2,
            unaided_recall_rate: Some(0.9),
            recall_temporally_distributed: true,
            communicative_task_attempts: 4,
            communicative_task_lessons: 2,
            communicative_task_success_rate: Some(0.9),
            communicative_task_temporally_distributed: true,
            ..RecentLearningSignals::default()
        };

        let plan = plan_next_lesson_fixed(&value);
        assert_eq!(plan.primary_objective, "introduce_high_utility_material");
        assert_eq!(plan.max_new_items, 2);
        assert!(
            plan_priorities(&plan).iter().all(|priority| {
                !matches!(
                    priority.action,
                    "consolidate_overdue_material" | "retrieve_due_material"
                )
            }),
            "FSRS review backlog is handled by the separate review queue; lesson planner should only use it as load pressure"
        );
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
    fn profile_spontaneous_weakness_requests_probe_not_prompt_diagnosis() {
        let mut input = input(0, 0);
        input.recent = RecentLearningSignals {
            lessons: 4,
            recall_attempts: 4,
            recall_lessons: 2,
            unaided_recall_rate: Some(0.40),
            ..RecentLearningSignals::default()
        };
        let plan = plan_next_lesson_fixed(&input);
        assert_eq!(
            plan.primary_objective, "probe_prompt_independence",
            "profile-wide spontaneous weakness can request a discriminating probe but cannot prove same-item prompt dependence"
        );
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
        assert_eq!(plan.primary_objective, "probe_prompt_independence");
        assert_eq!(plan.max_new_items, 1);
    }

    #[test]
    fn healthy_state_uses_scalar_preferences() {
        let mut input = input(0, 0);
        input.recent = RecentLearningSignals {
            lessons: 4,
            recall_attempts: 4,
            recall_lessons: 2,
            unaided_recall_rate: Some(0.9),
            recall_temporally_distributed: true,
            confidence: Some(0.8),
            communicative_task_attempts: 4,
            communicative_task_lessons: 3,
            communicative_task_success_rate: Some(0.9),
            communicative_task_temporally_distributed: true,
        };
        input.preferences.max_new_items_per_lesson = Some(2);
        input.preferences.preferred_difficulty = Some("stretch");
        let plan = plan_next_lesson_fixed(&input);
        assert_eq!(plan.primary_objective, "introduce_high_utility_material");
        assert_eq!(plan.max_new_items, 2);
        assert_eq!(plan.difficulty, "stretch");
    }

    #[test]
    fn massed_success_lessons_do_not_authorize_stretch() {
        let mut input = input(0, 0);
        input.recent = RecentLearningSignals {
            lessons: 4,
            recall_attempts: 4,
            recall_lessons: 2,
            unaided_recall_rate: Some(1.0),
            recall_temporally_distributed: false,
            communicative_task_attempts: 4,
            communicative_task_lessons: 2,
            communicative_task_success_rate: Some(1.0),
            communicative_task_temporally_distributed: false,
            ..RecentLearningSignals::default()
        };
        input.preferences.preferred_difficulty = Some("stretch");

        let plan = plan_next_lesson_fixed(&input);
        assert_eq!(
            plan.difficulty, "balanced",
            "massed lesson IDs cannot prove durable readiness for stretch"
        );
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
            communicative_task_lessons: 3,
            communicative_task_success_rate: Some(0.40),
            confidence: Some(0.20),
            ..RecentLearningSignals::default()
        };
        let plan = plan_next_lesson_fixed(&value);
        assert_eq!(plan.primary_objective, "improve_communicative_task_success");
        assert_eq!(
            plan.max_new_items, 1,
            "established task failure keeps only the bottleneck-specific repair slot"
        );
    }

    #[test]
    fn inflated_within_lesson_task_counts_cannot_establish_failure_without_three_task_episodes() {
        let mut value = input(0, 0);
        value.recent = RecentLearningSignals {
            lessons: 4,
            communicative_task_attempts: 99,
            communicative_task_lessons: 2,
            communicative_task_success_rate: Some(0.40),
            ..RecentLearningSignals::default()
        };
        let plan = plan_next_lesson_fixed(&value);
        assert_eq!(
            plan_priorities(&plan)
                .iter()
                .find(|priority| priority.action == "improve_communicative_task_success")
                .map(|priority| priority.score),
            Some(84),
            "aggregate within-lesson counts are telemetry; they must not substitute for independent task-bearing lesson episodes"
        );
        assert_eq!(
            plan.max_new_items, 3,
            "two task-bearing lessons remain sparse evidence until the task outcome has a third independent episode"
        );
    }

    #[test]
    fn established_task_failure_keeps_one_missing_material_repair_slot() {
        let mut value = input(0, 0);
        value.recent = RecentLearningSignals {
            lessons: 4,
            recall_attempts: 4,
            recall_lessons: 2,
            unaided_recall_rate: Some(0.90),
            communicative_task_attempts: 4,
            communicative_task_lessons: 3,
            communicative_task_success_rate: Some(0.40),
            ..RecentLearningSignals::default()
        };
        let plan = plan_next_lesson_fixed(&value);
        assert_eq!(plan.primary_objective, "improve_communicative_task_success");
        assert_eq!(plan.max_new_items, 1);
        assert!(
            plan_priorities(&plan)
                .iter()
                .all(|priority| priority.action != "introduce_high_utility_material"),
            "the one-item reserve is for diagnosed repair, not optional expansion"
        );
    }

    #[test]
    fn established_communicative_task_failure_outranks_due_error_recheck() {
        let mut value = input(0, 1);
        value.recent = RecentLearningSignals {
            lessons: 4,
            communicative_task_attempts: 4,
            communicative_task_lessons: 3,
            communicative_task_success_rate: Some(0.40),
            ..RecentLearningSignals::default()
        };
        let plan = plan_next_lesson_fixed(&value);
        assert_eq!(plan.primary_objective, "improve_communicative_task_success");
        assert_eq!(plan_priorities(&plan)[1].action, "run_error_rechecks");
    }

    #[test]
    fn established_communicative_task_failure_outranks_method_change_maintenance() {
        let mut value = input(1, 0);
        value.recent = RecentLearningSignals {
            lessons: 4,
            communicative_task_attempts: 4,
            communicative_task_lessons: 3,
            communicative_task_success_rate: Some(0.40),
            ..RecentLearningSignals::default()
        };
        let plan = plan_next_lesson_fixed(&value);
        assert_eq!(plan.primary_objective, "improve_communicative_task_success");
        assert_eq!(plan_priorities(&plan)[1].action, "change_method");
    }

    #[test]
    fn established_spontaneous_weakness_outranks_single_lesson_task_failure() {
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
        assert_eq!(plan.primary_objective, "probe_prompt_independence");
    }

    #[test]
    fn established_spontaneous_weakness_outranks_sparse_task_failure() {
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
        assert_eq!(plan.primary_objective, "probe_prompt_independence");
        let task_score = plan_priorities(&plan)
            .iter()
            .find(|priority| priority.action == "improve_communicative_task_success")
            .map(|priority| priority.score);
        assert!(task_score.is_some_and(|score| score < 88));
    }

    #[test]
    fn confidence_is_diagnostic_not_a_lesson_objective() {
        let mut value = input(0, 0);
        value.recent = RecentLearningSignals {
            lessons: 4,
            confidence: Some(0.10),
            ..RecentLearningSignals::default()
        };
        let plan = plan_next_lesson_fixed(&value);
        assert_eq!(plan.primary_objective, "introduce_high_utility_material");
        assert!(
            plan_priorities(&plan)
                .iter()
                .all(|priority| priority.action != "stabilize_confidence")
        );
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

    #[test]
    fn due_error_recheck_never_uses_stretch_difficulty() {
        let mut value = input(0, 1);
        value.recent = RecentLearningSignals {
            lessons: 4,
            recall_attempts: 4,
            recall_lessons: 2,
            unaided_recall_rate: Some(1.0),
            communicative_task_attempts: 4,
            communicative_task_lessons: 2,
            communicative_task_success_rate: Some(1.0),
            ..RecentLearningSignals::default()
        };
        value.preferences.preferred_difficulty = Some("stretch");

        let plan = plan_next_lesson_fixed(&value);
        assert_eq!(plan.primary_objective, "run_error_rechecks");
        assert_eq!(
            plan.difficulty, "balanced",
            "scheduled rechecks must not confound retention measurement with stretch difficulty"
        );
    }

    #[test]
    fn due_error_recheck_does_not_add_a_redundant_generic_probe() {
        let mut value = input(0, 1);
        value.recent.lessons = 5;
        let plan = plan_next_lesson_fixed(&value);
        assert_eq!(plan.primary_objective, "run_error_rechecks");
        assert_eq!(
            plan_diagnostic_probe_count(&plan),
            0,
            "the scheduled unaided recheck is already the discriminating measurement"
        );
    }

    #[test]
    fn success_criteria_do_not_turn_delayed_retrieval_or_transfer_into_quotas() {
        let input = input(0, 1);
        let plan = plan_next_lesson_fixed(&input);
        let criteria = plan_success_criteria(&plan);
        assert!(
            criteria
                .iter()
                .any(|criterion| criterion.contains("primary communicative objective")),
            "success must be anchored in the lesson's real task outcome"
        );
        assert!(
            criteria.iter().any(|criterion| {
                criterion.contains("Delayed retrieval or transfer") && criterion.contains("when")
            }),
            "retention/transfer checks must be conditional, not mandatory every lesson"
        );
        assert!(
            criteria
                .iter()
                .all(|criterion| *criterion
                    != "Target is retrieved again after an intervening task.")
        );
    }
}
