//! Deterministic lesson planning from learner evidence.
//!
//! The planner does not choose language-specific content. It chooses the
//! pedagogical action mix and capacity; the active language pack supplies
//! scenarios and candidate phrases.

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
    /// Mean tutor-estimated learner confidence, from 0 to 1.
    pub confidence: Option<f64>,
}

/// Learner preferences used as planner constraints.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct PlannerPreferences {
    /// Optional cap on newly introduced items in one tutor lesson.
    pub max_new_items_per_lesson: Option<u8>,
    /// Optional preferred difficulty: supported, balanced, or stretch.
    pub preferred_difficulty: Option<String>,
    /// Preferred scenarios ranked ahead of catalog order.
    pub priority_scenarios: Vec<String>,
}

/// Language-neutral inputs used to plan the next tutor lesson.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct LessonPlannerInput {
    /// Review items currently due.
    pub due_reviews: u32,
    /// Mature review/relearning items that are overdue.
    pub overdue_reviews: u32,
    /// Scheduled recurring-error rechecks due now.
    pub due_error_rechecks: u32,
    /// Unresolved recurring error targets.
    pub active_errors: u32,
    /// Error targets whose current teaching method must change.
    pub method_change_errors: u32,
    /// Pending recommendations derived from evidence.
    pub open_recommendations: u32,
    /// Effective new-card budget after backlog protection.
    pub new_card_budget: u8,
    /// Aggregated recent lesson performance.
    pub recent: RecentLearningSignals,
    /// Explicit learner preferences.
    pub preferences: PlannerPreferences,
}

/// One ranked pedagogical action in the lesson plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlanPriority {
    /// Stable machine-readable action code.
    pub action: String,
    /// Deterministic priority from 0 to 100.
    pub score: u8,
    /// Evidence-based explanation for the score.
    pub rationale: String,
}

/// Structured next-lesson policy consumed by the tutor.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AdaptiveLessonPlan {
    /// Highest-ranked pedagogical objective.
    pub primary_objective: String,
    /// supported, balanced, or stretch.
    pub difficulty: String,
    /// Maximum newly introduced items for this lesson.
    pub max_new_items: u8,
    /// Ranked candidate pedagogical actions.
    pub priorities: Vec<PlanPriority>,
    /// Required diagnostic behavior at lesson start.
    pub diagnostic_probes: Vec<String>,
    /// Evidence-oriented criteria for considering the lesson successful.
    pub success_criteria: Vec<String>,
    /// Recent signals used by the planner.
    pub recent_signals: RecentLearningSignals,
    /// Learner-prioritized real-life scenarios.
    pub priority_scenarios: Vec<String>,
}

/// Builds a deterministic next-lesson policy from the current learner state.
#[must_use]
pub fn plan_next_lesson(input: &LessonPlannerInput) -> AdaptiveLessonPlan {
    let mut priorities = Vec::new();

    push_if(
        &mut priorities,
        input.method_change_errors > 0,
        "change_method",
        100,
        format!(
            "{} unresolved error target(s) require a different teaching method",
            input.method_change_errors
        ),
    );
    push_if(
        &mut priorities,
        input.due_error_rechecks > 0,
        "run_error_rechecks",
        96,
        format!(
            "{} scheduled error recheck(s) are due",
            input.due_error_rechecks
        ),
    );

    if let Some(rate) = input.recent.unaided_recall_rate
        && rate < 0.60
    {
        priorities.push(priority(
            "reduce_prompt_dependence",
            92,
            format!("unaided recall rate is {:.0}%", rate * 100.0),
        ));
    }
    if let Some(score) = input.recent.listening_score
        && score < 0.70
    {
        priorities.push(priority(
            "strengthen_listening",
            88,
            format!("recent listening score is {:.0}%", score * 100.0),
        ));
    }
    if let Some(latency) = input.recent.avg_answer_millis
        && latency > 5_000
    {
        priorities.push(priority(
            "build_automaticity",
            84,
            format!("mean answer latency is {} ms", latency),
        ));
    }
    if let Some(confidence) = input.recent.confidence
        && confidence < 0.55
    {
        priorities.push(priority(
            "stabilize_confidence",
            82,
            format!("recent confidence is {:.0}%", confidence * 100.0),
        ));
    }
    if let Some(ratio) = input.recent.speaking_ratio
        && ratio < 0.45
    {
        priorities.push(priority(
            "increase_speaking_share",
            80,
            format!("learner speaking ratio is {:.0}%", ratio * 100.0),
        ));
    }

    push_if(
        &mut priorities,
        input.active_errors > 0,
        "repair_recurring_errors",
        78,
        format!("{} active recurring error target(s)", input.active_errors),
    );
    push_if(
        &mut priorities,
        input.overdue_reviews > 0,
        "consolidate_overdue_material",
        76,
        format!("{} review item(s) are overdue", input.overdue_reviews),
    );
    push_if(
        &mut priorities,
        input.open_recommendations > 0,
        "apply_adaptive_recommendations",
        72,
        format!(
            "{} evidence-based recommendation(s) are open",
            input.open_recommendations
        ),
    );
    push_if(
        &mut priorities,
        input.due_reviews > 0,
        "retrieve_due_material",
        68,
        format!("{} review item(s) are due", input.due_reviews),
    );

    let mut max_new_items = input
        .preferences
        .max_new_items_per_lesson
        .unwrap_or(3)
        .min(input.new_card_budget)
        .min(5);
    if input.overdue_reviews > 0 || input.method_change_errors > 0 || input.new_card_budget == 0 {
        max_new_items = 0;
    }
    if max_new_items > 0 {
        priorities.push(priority(
            "introduce_high_utility_material",
            55,
            format!("capacity permits up to {max_new_items} new item(s)"),
        ));
    }

    if priorities.is_empty() {
        priorities.push(priority(
            "diagnose_and_expand",
            50,
            "no urgent weakness is detected; probe retained skills, then use one scenario from learner priorities",
        ));
    }

    priorities.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.action.cmp(&right.action))
    });

    let primary_objective = priorities[0].action.clone();
    let difficulty = choose_difficulty(input, &primary_objective);

    let mut diagnostic_probes = vec![
        "Start with 2–4 unaided retrieval/production probes before teaching or hinting.".to_owned(),
        "Vary the situation enough to distinguish memorized recall from transfer.".to_owned(),
    ];
    if input.recent.lessons < 2 {
        diagnostic_probes.push(
            "Treat the first minutes as baseline calibration; do not infer ability from CEFR label alone."
                .to_owned(),
        );
    }

    let success_criteria = vec![
        "Target response is produced unaided before model/reveal.".to_owned(),
        "Target is retrieved again after an intervening task.".to_owned(),
        "At least one target is tested in a meaningfully different communicative context."
            .to_owned(),
        "Any material plan change is justified by evidence recorded in the lesson result."
            .to_owned(),
    ];

    AdaptiveLessonPlan {
        primary_objective,
        difficulty,
        max_new_items,
        priorities,
        diagnostic_probes,
        success_criteria,
        recent_signals: input.recent,
        priority_scenarios: input.preferences.priority_scenarios.clone(),
    }
}

fn priority(action: &str, score: u8, rationale: impl Into<String>) -> PlanPriority {
    PlanPriority {
        action: action.to_owned(),
        score,
        rationale: rationale.into(),
    }
}

fn push_if(
    priorities: &mut Vec<PlanPriority>,
    condition: bool,
    action: &str,
    score: u8,
    rationale: String,
) {
    if condition {
        priorities.push(priority(action, score, rationale));
    }
}

fn choose_difficulty(input: &LessonPlannerInput, primary: &str) -> String {
    if matches!(
        primary,
        "change_method" | "reduce_prompt_dependence" | "stabilize_confidence"
    ) {
        return "supported".to_owned();
    }
    if let Some(preferred) = input.preferences.preferred_difficulty.as_deref()
        && matches!(preferred, "supported" | "balanced" | "stretch")
    {
        return preferred.to_owned();
    }
    let strong = input
        .recent
        .unaided_recall_rate
        .is_some_and(|value| value >= 0.80)
        && input
            .recent
            .listening_score
            .is_none_or(|value| value >= 0.75)
        && input.recent.confidence.is_none_or(|value| value >= 0.65)
        && input.overdue_reviews == 0;
    if strong {
        "stretch".to_owned()
    } else {
        "balanced".to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn method_change_and_rechecks_beat_new_material() {
        let input = LessonPlannerInput {
            method_change_errors: 1,
            due_error_rechecks: 2,
            new_card_budget: 8,
            ..LessonPlannerInput::default()
        };
        let plan = plan_next_lesson(&input);
        assert_eq!(plan.primary_objective, "change_method");
        assert_eq!(plan.max_new_items, 0);
        assert_eq!(plan.difficulty, "supported");
    }

    #[test]
    fn weak_unaided_recall_prioritizes_prompt_independence() {
        let input = LessonPlannerInput {
            new_card_budget: 8,
            recent: RecentLearningSignals {
                lessons: 3,
                unaided_recall_rate: Some(0.4),
                listening_score: Some(0.85),
                ..RecentLearningSignals::default()
            },
            ..LessonPlannerInput::default()
        };
        let plan = plan_next_lesson(&input);
        assert_eq!(plan.primary_objective, "reduce_prompt_dependence");
        assert_eq!(plan.max_new_items, 3);
    }

    #[test]
    fn healthy_state_allows_small_new_material_budget() {
        let input = LessonPlannerInput {
            new_card_budget: 8,
            recent: RecentLearningSignals {
                lessons: 4,
                unaided_recall_rate: Some(0.9),
                listening_score: Some(0.9),
                confidence: Some(0.8),
                speaking_ratio: Some(0.6),
                avg_answer_millis: Some(2_000),
            },
            preferences: PlannerPreferences {
                max_new_items_per_lesson: Some(2),
                preferred_difficulty: None,
                priority_scenarios: vec!["transport".to_owned()],
            },
            ..LessonPlannerInput::default()
        };
        let plan = plan_next_lesson(&input);
        assert_eq!(plan.primary_objective, "introduce_high_utility_material");
        assert_eq!(plan.max_new_items, 2);
        assert_eq!(plan.difficulty, "stretch");
    }
}
