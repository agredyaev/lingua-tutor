//! Candidate-task scoring over item-level capability projections.

use crate::{AdaptiveLessonPlan, CapabilityEstimate, ItemCapabilityState};
use serde::{Deserialize, Serialize};

/// Concrete next action that the tutor can execute.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskCandidate {
    /// Stable task code understood by the tutor methodology.
    pub task_type: String,
    /// Learning item targeted by this task.
    pub learning_item_id: String,
    /// Target-language text for tutor orientation.
    pub target_text: String,
    /// Deterministic priority from 0 to 100.
    pub score: u8,
    /// Evidence-based reason for scheduling the task.
    pub rationale: String,
    /// Evidence kind expected if the task is executed.
    pub expected_evidence: String,
}

/// Scores and ranks concrete item-level learning actions.
#[must_use]
pub fn score_task_candidates(
    items: &[ItemCapabilityState],
    plan: &AdaptiveLessonPlan,
    limit: usize,
) -> Vec<TaskCandidate> {
    let mut out = Vec::new();
    for item in items.iter().filter(|item| actionable_status(&item.status)) {
        score_item(item, plan, &mut out);
    }
    out.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.learning_item_id.cmp(&right.learning_item_id))
            .then_with(|| left.task_type.cmp(&right.task_type))
    });
    out.truncate(limit);
    out
}

fn actionable_status(status: &str) -> bool {
    matches!(
        status,
        "introduced" | "learning" | "transfer_pending" | "provisionally_mastered" | "mastered"
    )
}

fn score_item(item: &ItemCapabilityState, plan: &AdaptiveLessonPlan, out: &mut Vec<TaskCandidate>) {
    let listening = &item.listening;
    let prompted = &item.prompted_production;
    let spontaneous = &item.spontaneous_production;
    let transfer = &item.transfer;

    if needs_diagnostic(spontaneous) {
        push(
            out,
            item,
            "diagnostic_spontaneous",
            boosted(
                66 + uncertainty_points(spontaneous),
                plan,
                "reduce_prompt_dependence",
            ),
            "spontaneous ability is uncertain or stale; probe without hints before teaching",
            "spontaneous_production",
        );
    }

    if prompted.weighted_success_rate.unwrap_or(0.0) >= 0.70
        && spontaneous.unaided_success_rate.unwrap_or(0.0) < 0.60
    {
        push(
            out,
            item,
            "fade_prompts",
            boosted(90, plan, "reduce_prompt_dependence"),
            "prompted production is stronger than explicitly unaided production",
            "spontaneous_production",
        );
    }

    if spontaneous.attempts > 0 && spontaneous.unaided_success_rate.unwrap_or(0.0) < 0.70 {
        let gap = gap_points(spontaneous.unaided_success_rate, 0.70, 24);
        push(
            out,
            item,
            "spontaneous_retrieval",
            boosted(68 + gap, plan, "reduce_prompt_dependence"),
            "explicitly unaided production is below the target range",
            "spontaneous_production",
        );
    }

    if listening.attempts == 0 || listening.weighted_success_rate.unwrap_or(0.0) < 0.75 {
        let base = 62
            + gap_points(listening.weighted_success_rate, 0.75, 24)
            + uncertainty_points(listening) / 2;
        push(
            out,
            item,
            "listening_probe",
            boosted(base, plan, "strengthen_listening"),
            "listening evidence is weak, uncertain, stale, or missing",
            "listening",
        );
    }

    let spontaneous_ready = spontaneous
        .weighted_success_rate
        .is_some_and(|rate| rate >= 0.75)
        && spontaneous
            .unaided_success_rate
            .is_some_and(|rate| rate >= 0.70)
        && spontaneous.evidence_strength >= 0.45;
    if spontaneous_ready
        && (transfer.distinct_success_families < 3
            || transfer.weighted_success_rate.unwrap_or(0.0) < 0.80
            || transfer.evidence_strength < 0.65)
    {
        let base = 78 + gap_points(transfer.weighted_success_rate, 0.80, 18);
        push(
            out,
            item,
            "transfer_practice",
            boosted(base, plan, "diagnose_and_expand"),
            "spontaneous production is stable enough to test a new semantic context family",
            "transfer",
        );
    }

    if spontaneous
        .latency_weighted_millis
        .is_some_and(|latency| latency > 5_000)
        && spontaneous
            .unaided_success_rate
            .is_some_and(|rate| rate >= 0.60)
    {
        push(
            out,
            item,
            "automaticity_drill",
            boosted(82, plan, "build_automaticity"),
            "unaided production is often correct but still slow",
            "spontaneous_production",
        );
    }
}

fn needs_diagnostic(estimate: &CapabilityEstimate) -> bool {
    estimate.attempts == 0 || estimate.evidence_strength < 0.45
}

fn uncertainty_points(estimate: &CapabilityEstimate) -> u8 {
    ((1.0 - estimate.evidence_strength).clamp(0.0, 1.0) * 18.0).round() as u8
}

fn gap_points(rate: Option<f64>, target: f64, max: u8) -> u8 {
    let rate = rate.unwrap_or(0.0);
    let gap = ((target - rate) / target).clamp(0.0, 1.0);
    (gap * f64::from(max)).round() as u8
}

fn boosted(score: u8, plan: &AdaptiveLessonPlan, matching_objective: &str) -> u8 {
    score
        .saturating_add(u8::from(plan.primary_objective == matching_objective) * 8)
        .min(100)
}

fn push(
    out: &mut Vec<TaskCandidate>,
    item: &ItemCapabilityState,
    task_type: &str,
    score: u8,
    rationale: &str,
    expected_evidence: &str,
) {
    out.push(TaskCandidate {
        task_type: task_type.to_owned(),
        learning_item_id: item.learning_item_id.clone(),
        target_text: item.target_text.clone(),
        score,
        rationale: rationale.to_owned(),
        expected_evidence: expected_evidence.to_owned(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn estimate(rate: Option<f64>, unaided: Option<f64>, strength: f64) -> CapabilityEstimate {
        CapabilityEstimate {
            attempts: u32::from(rate.is_some()),
            successes: u32::from(rate.is_some_and(|value| value >= 0.5)),
            weighted_success_rate: rate,
            unaided_success_rate: unaided,
            latency_weighted_millis: None,
            qualified_success_streak: u8::from(rate.is_some_and(|value| value >= 0.5)),
            distinct_success_contexts: 0,
            distinct_success_families: 0,
            successful_contexts: vec![],
            successful_context_families: vec![],
            evidence_strength: strength,
            last_observed_at_unix: None,
            last_observed_success: rate.map(|value| value >= 0.5),
            last_observed_qualified_success: rate.map(|value| value >= 0.5),
        }
    }

    fn item(status: &str, prompted: f64, spontaneous_unaided: Option<f64>) -> ItemCapabilityState {
        ItemCapabilityState {
            learning_item_id: "li-1".to_owned(),
            target_text: "target".to_owned(),
            status: status.to_owned(),
            recognition: estimate(Some(1.0), None, 0.8),
            listening: estimate(Some(0.9), None, 0.8),
            prompted_production: estimate(Some(prompted), None, 0.8),
            spontaneous_production: estimate(spontaneous_unaided, spontaneous_unaided, 0.8),
            transfer: estimate(None, None, 0.0),
        }
    }

    fn plan(primary: &str) -> AdaptiveLessonPlan {
        AdaptiveLessonPlan {
            primary_objective: primary.to_owned(),
            difficulty: "balanced".to_owned(),
            max_new_items: 2,
            priorities: vec![],
            diagnostic_probes: vec![],
            success_criteria: vec![],
            recent_signals: crate::RecentLearningSignals::default(),
            priority_scenarios: vec![],
        }
    }

    #[test]
    fn candidates_are_not_tutor_tasks() {
        assert!(
            score_task_candidates(&[item("candidate", 1.0, Some(0.2))], &plan("x"), 10).is_empty()
        );
    }

    #[test]
    fn prompted_success_plus_spontaneous_gap_prioritizes_prompt_fading() {
        let candidates = score_task_candidates(
            &[item("learning", 1.0, Some(0.3))],
            &plan("reduce_prompt_dependence"),
            10,
        );
        assert_eq!(candidates[0].task_type, "fade_prompts");
        assert_eq!(candidates[0].score, 98);
    }

    #[test]
    fn unknown_unaided_state_is_not_treated_as_success() {
        let candidates = score_task_candidates(
            &[item("learning", 1.0, None)],
            &plan("reduce_prompt_dependence"),
            10,
        );
        assert!(
            candidates
                .iter()
                .any(|candidate| candidate.task_type == "fade_prompts")
        );
    }
}
