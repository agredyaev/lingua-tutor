//! Candidate-task scoring over item-level capability projections.

use crate::{CapabilityEstimateView, ItemCapabilityView};

/// Borrowed candidate emitted by the canonical scoring pass.
#[derive(Debug, Clone, Copy)]
pub struct TaskCandidateView<'a> {
    /// Stable task code.
    pub task_type: &'static str,
    /// Learning item ID borrowed from the item state.
    pub learning_item_id: &'a str,
    /// Target text borrowed from the item state.
    pub target_text: &'a str,
    /// Deterministic priority from 0 to 100.
    pub score: u8,
    /// Static evidence-based rationale.
    pub rationale: &'static str,
    /// Evidence kind expected if the task is executed.
    pub expected_evidence: &'static str,
}

/// Emits concrete task candidates for one item without allocating a collection.
pub fn visit_task_candidates<'a>(
    item: ItemCapabilityView<'a>,
    primary_objective: &str,
    mut emit: impl FnMut(TaskCandidateView<'a>),
) {
    if !actionable_status(item.status) {
        return;
    }

    let listening = item.listening;
    let prompted = item.prompted_production;
    let spontaneous = item.spontaneous_production;
    let transfer = item.transfer;

    if needs_diagnostic(spontaneous) {
        emit(view(
            item,
            "diagnostic_spontaneous",
            boosted(
                66 + uncertainty_points(spontaneous),
                primary_objective,
                "reduce_prompt_dependence",
            ),
            "spontaneous ability is uncertain or stale; probe without hints before teaching",
            "spontaneous_production",
        ));
    }

    if prompted.weighted_success_rate.unwrap_or(0.0) >= 0.70
        && spontaneous.unaided_success_rate.unwrap_or(0.0) < 0.60
    {
        emit(view(
            item,
            "fade_prompts",
            boosted(90, primary_objective, "reduce_prompt_dependence"),
            "prompted production is stronger than explicitly unaided production",
            "spontaneous_production",
        ));
    }

    if spontaneous.attempts > 0 && spontaneous.unaided_success_rate.unwrap_or(0.0) < 0.70 {
        let gap = gap_points(spontaneous.unaided_success_rate, 0.70, 24);
        emit(view(
            item,
            "spontaneous_retrieval",
            boosted(68 + gap, primary_objective, "reduce_prompt_dependence"),
            "explicitly unaided production is below the target range",
            "spontaneous_production",
        ));
    }

    if listening.attempts == 0
        || listening.weighted_success_rate.unwrap_or(0.0) < 0.75
        || (listening.attempts >= 2 && listening.distinct_success_contexts < 2)
    {
        let base = 62
            + gap_points(listening.weighted_success_rate, 0.75, 24)
            + uncertainty_points(listening) / 2;
        emit(view(
            item,
            "listening_probe",
            boosted(base, primary_objective, "strengthen_listening"),
            "listening evidence is weak, stale, or lacks source/talker variation",
            "listening",
        ));
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
        emit(view(
            item,
            "transfer_practice",
            boosted(base, primary_objective, "diagnose_and_expand"),
            "spontaneous production is stable enough to test a new semantic context family",
            "transfer",
        ));
    }

    if spontaneous
        .latency_weighted_millis
        .is_some_and(|latency| latency > automaticity_threshold_millis(item.target_text))
        && spontaneous
            .unaided_success_rate
            .is_some_and(|rate| rate >= 0.60)
        && spontaneous.qualified_success_streak >= 2
    {
        emit(view(
            item,
            "automaticity_drill",
            boosted(82, primary_objective, "build_automaticity"),
            "unaided production is often correct but still slow",
            "spontaneous_production",
        ));
    }
}

fn automaticity_threshold_millis(target_text: &str) -> u32 {
    let words = u32::try_from(target_text.split_whitespace().count()).unwrap_or(u32::MAX);
    2_000_u32
        .saturating_add(words.saturating_mul(600))
        .clamp(3_000, 8_000)
}

fn actionable_status(status: &str) -> bool {
    matches!(
        status,
        "introduced" | "learning" | "transfer_pending" | "provisionally_mastered" | "mastered"
    )
}

fn needs_diagnostic(estimate: CapabilityEstimateView<'_>) -> bool {
    estimate.attempts == 0 || estimate.evidence_strength < 0.45
}

fn uncertainty_points(estimate: CapabilityEstimateView<'_>) -> u8 {
    ((1.0 - estimate.evidence_strength).clamp(0.0, 1.0) * 18.0).round() as u8
}

fn gap_points(rate: Option<f64>, target: f64, max: u8) -> u8 {
    let rate = rate.unwrap_or(0.0);
    let gap = ((target - rate) / target).clamp(0.0, 1.0);
    (gap * f64::from(max)).round() as u8
}

fn boosted(score: u8, primary_objective: &str, matching_objective: &str) -> u8 {
    score
        .saturating_add(u8::from(primary_objective == matching_objective) * 8)
        .min(100)
}

fn view<'a>(
    item: ItemCapabilityView<'a>,
    task_type: &'static str,
    score: u8,
    rationale: &'static str,
    expected_evidence: &'static str,
) -> TaskCandidateView<'a> {
    TaskCandidateView {
        task_type,
        learning_item_id: item.learning_item_id,
        target_text: item.target_text,
        score,
        rationale,
        expected_evidence,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CapabilityEstimateFixed, ItemCapabilityStateFixed};

    fn state<'a>(
        status: &'a str,
        prompted: f64,
        spontaneous: Option<f64>,
    ) -> ItemCapabilityStateFixed<'a> {
        ItemCapabilityStateFixed {
            learning_item_id: "li-1",
            target_text: "target",
            status,
            recognition: CapabilityEstimateFixed::default(),
            listening: CapabilityEstimateFixed {
                attempts: 1,
                weighted_success_rate: Some(0.9),
                evidence_strength: 0.8,
                ..CapabilityEstimateFixed::default()
            },
            prompted_production: CapabilityEstimateFixed {
                attempts: 2,
                weighted_success_rate: Some(prompted),
                evidence_strength: 0.8,
                ..CapabilityEstimateFixed::default()
            },
            spontaneous_production: CapabilityEstimateFixed {
                attempts: u32::from(spontaneous.is_some()),
                weighted_success_rate: spontaneous,
                unaided_success_rate: spontaneous,
                evidence_strength: 0.8,
                ..CapabilityEstimateFixed::default()
            },
            transfer: CapabilityEstimateFixed::default(),
        }
    }

    #[test]
    fn candidate_status_emits_nothing() {
        let item = state("candidate", 1.0, Some(0.2));
        let mut count = 0;
        visit_task_candidates(crate::item_capability_view(&item), "x", |_| count += 1);
        assert_eq!(count, 0);
    }

    #[test]
    fn prompt_gap_emits_boosted_fade_prompts() {
        let item = state("learning", 1.0, Some(0.3));
        let mut best = None;
        visit_task_candidates(
            crate::item_capability_view(&item),
            "reduce_prompt_dependence",
            |candidate| {
                if candidate.task_type == "fade_prompts" {
                    best = Some(candidate.score);
                }
            },
        );
        assert_eq!(best, Some(98));
    }
}
