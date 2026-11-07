//! Candidate-task scoring over item-level capability projections.

use crate::{CapabilityEstimateView, ItemCapabilityView};

const AUTOMATICITY_RETRIEVAL_THRESHOLD_MILLIS: u32 = 5_000;

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

    let spontaneous_uncertain = needs_diagnostic(spontaneous)
        || decision_fragile(
            spontaneous.unaided_success_rate,
            spontaneous.unaided_evidence_weight,
            0.60,
        )
        || decision_fragile(
            spontaneous.unaided_success_rate,
            spontaneous.unaided_evidence_weight,
            0.70,
        )
        || decision_fragile(
            spontaneous.weighted_success_rate,
            spontaneous.effective_evidence_weight,
            0.75,
        );
    if spontaneous_uncertain {
        let diagnostic_score = (66 + uncertainty_points(spontaneous))
            .saturating_add(u8::from(primary_objective == "probe_prompt_independence") * 8)
            .min(100);
        emit(view(
            item,
            "diagnostic_spontaneous",
            diagnostic_score,
            "spontaneous ability is uncertain or stale; probe without hints before teaching",
            "spontaneous_production",
        ));
    }

    let prompted_established = !needs_diagnostic(prompted)
        && !decision_fragile(
            prompted.weighted_success_rate,
            prompted.effective_evidence_weight,
            0.70,
        );
    let prompt_gap_established = !spontaneous_uncertain
        && prompted_established
        && prompted.weighted_success_rate.unwrap_or(0.0) >= 0.70
        && spontaneous.unaided_success_rate.unwrap_or(0.0) < 0.60;
    if prompt_gap_established {
        emit(view(
            item,
            "fade_prompts",
            98,
            "prompted production is stronger than explicitly unaided production",
            "spontaneous_production",
        ));
    }

    if !spontaneous_uncertain
        && !prompt_gap_established
        && spontaneous.attempts > 0
        && spontaneous.unaided_success_rate.unwrap_or(0.0) < 0.70
    {
        let gap = gap_points(spontaneous.unaided_success_rate, 0.70, 24);
        emit(view(
            item,
            "spontaneous_retrieval",
            boosted(68 + gap, primary_objective, "probe_prompt_independence"),
            "explicitly unaided production is below the target range",
            "spontaneous_production",
        ));
    }

    let listening_uncertain = needs_diagnostic(listening)
        || listening.unaided_evidence_episodes < 2
        || decision_fragile(
            listening.weighted_success_rate,
            listening.effective_evidence_weight,
            0.75,
        )
        || decision_fragile(
            listening.unaided_success_rate,
            listening.unaided_evidence_weight,
            0.70,
        );
    let listening_gap = listening.weighted_success_rate.unwrap_or(0.0) < 0.75
        || listening.unaided_success_rate.unwrap_or(0.0) < 0.70;
    let listening_variation_gap =
        listening.attempts >= 2 && listening.distinct_success_contexts < 2;
    if listening_uncertain || (!listening_gap && listening_variation_gap) {
        let base = 62
            + gap_points(listening.unaided_success_rate, 0.70, 24)
            + uncertainty_points(listening) / 2;
        emit(view(
            item,
            "listening_probe",
            base,
            "listening state is uncertain/stale or needs an independent source/context check",
            "listening",
        ));
    } else if listening_gap {
        let base = 76 + gap_points(listening.unaided_success_rate, 0.70, 18);
        emit(view(
            item,
            "listening_repair",
            base,
            "independent listening weakness is established; repair before spending more time diagnosing it",
            "listening",
        ));
    }

    let spontaneous_ready = crate::capability::spontaneous_ready_for_transfer(spontaneous);
    if !spontaneous_uncertain
        && spontaneous_ready
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
        .is_some_and(|latency| latency > AUTOMATICITY_RETRIEVAL_THRESHOLD_MILLIS)
        && spontaneous.latency_evidence_episodes >= 2
        && spontaneous
            .weighted_success_rate
            .is_some_and(|rate| rate >= 0.80)
        && spontaneous
            .unaided_success_rate
            .is_some_and(|rate| rate >= 0.80)
        && spontaneous.evidence_strength >= 0.65
        && spontaneous.qualified_success_streak >= 2
    {
        emit(view(
            item,
            "automaticity_drill",
            82,
            "unaided production is often correct but still slow",
            "spontaneous_production",
        ));
    }
}

fn actionable_status(status: &str) -> bool {
    matches!(
        status,
        "introduced" | "learning" | "transfer_pending" | "provisionally_mastered" | "mastered"
    )
}

fn needs_diagnostic(estimate: CapabilityEstimateView<'_>) -> bool {
    estimate.attempts == 0
        || estimate.independent_evidence_episodes < 3
        || estimate.evidence_strength < 0.45
}

fn uncertainty_points(estimate: CapabilityEstimateView<'_>) -> u8 {
    ((1.0 - estimate.evidence_strength).clamp(0.0, 1.0) * 18.0).round() as u8
}

fn decision_fragile(rate: Option<f64>, weight: f64, threshold: f64) -> bool {
    let Some(rate) = rate else {
        return true;
    };
    if weight <= f64::EPSILON {
        return true;
    }
    let current = rate >= threshold;
    let after_success = ((rate * weight + 1.0) / (weight + 1.0)) >= threshold;
    let after_failure = ((rate * weight) / (weight + 1.0)) >= threshold;
    current != after_success || current != after_failure
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
                attempts: 6,
                weighted_success_rate: Some(prompted),
                evidence_strength: 0.9,
                effective_evidence_weight: 5.0,
                independent_evidence_episodes: 6,
                ..CapabilityEstimateFixed::default()
            },
            spontaneous_production: CapabilityEstimateFixed {
                attempts: u32::from(spontaneous.is_some()),
                weighted_success_rate: spontaneous,
                unaided_success_rate: spontaneous,
                evidence_strength: 0.8,
                effective_evidence_weight: 3.2,
                unaided_evidence_weight: 3.2,
                independent_evidence_episodes: 3,
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
    fn slow_but_inaccurate_production_does_not_trigger_automaticity_drill() {
        let mut item = state("learning", 0.9, Some(0.65));
        item.spontaneous_production.attempts = 4;
        item.spontaneous_production.latency_weighted_millis = Some(9_000);
        item.spontaneous_production.evidence_strength = 0.8;
        item.spontaneous_production.qualified_success_streak = 2;
        let mut automaticity = false;
        visit_task_candidates(crate::item_capability_view(&item), "x", |candidate| {
            automaticity |= candidate.task_type == "automaticity_drill";
        });
        assert!(!automaticity);
    }

    #[test]
    fn raw_unaided_rate_cannot_hide_decayed_accuracy_for_automaticity() {
        let mut item = state("learning", 0.9, Some(0.80));
        item.spontaneous_production.attempts = 10;
        item.spontaneous_production.weighted_success_rate = Some(0.76);
        item.spontaneous_production.unaided_success_rate = Some(0.80);
        item.spontaneous_production.latency_weighted_millis = Some(9_000);
        item.spontaneous_production.evidence_strength = 0.9;
        item.spontaneous_production.independent_evidence_episodes = 10;
        item.spontaneous_production.qualified_success_streak = 2;

        let mut automaticity = false;
        visit_task_candidates(crate::item_capability_view(&item), "x", |candidate| {
            automaticity |= candidate.task_type == "automaticity_drill";
        });

        assert!(
            !automaticity,
            "automaticity work must wait until recency-weighted accuracy is also at least 80%"
        );
    }

    #[test]
    fn one_latency_sample_cannot_trigger_automaticity_drill() {
        let mut item = state("learning", 0.9, Some(0.90));
        item.spontaneous_production.attempts = 6;
        item.spontaneous_production.latency_weighted_millis = Some(9_000);
        item.spontaneous_production.latency_evidence_episodes = 1;
        item.spontaneous_production.evidence_strength = 0.9;
        item.spontaneous_production.independent_evidence_episodes = 6;
        item.spontaneous_production.qualified_success_streak = 3;

        let mut automaticity = false;
        visit_task_candidates(crate::item_capability_view(&item), "x", |candidate| {
            automaticity |= candidate.task_type == "automaticity_drill";
        });

        assert!(
            !automaticity,
            "automaticity must not be diagnosed from a single measured latency episode"
        );
    }

    #[test]
    fn slow_accurate_distributed_production_can_trigger_automaticity_drill() {
        let mut item = state("learning", 0.9, Some(0.90));
        item.spontaneous_production.attempts = 4;
        item.spontaneous_production.latency_weighted_millis = Some(9_000);
        item.spontaneous_production.latency_evidence_episodes = 2;
        item.spontaneous_production.evidence_strength = 0.8;
        item.spontaneous_production.qualified_success_streak = 2;
        let mut automaticity = false;
        visit_task_candidates(crate::item_capability_view(&item), "x", |candidate| {
            automaticity |= candidate.task_type == "automaticity_drill";
        });
        assert!(automaticity);
    }

    #[test]
    fn assisted_listening_volume_cannot_hide_sparse_unaided_evidence() {
        let mut item = state("learning", 0.9, Some(0.90));
        item.listening.attempts = 6;
        item.listening.weighted_success_rate = Some(0.90);
        item.listening.unaided_success_rate = Some(0.0);
        item.listening.evidence_strength = 0.90;
        item.listening.effective_evidence_weight = 5.0;
        item.listening.unaided_evidence_weight = 1.0;
        item.listening.unaided_evidence_episodes = 1;
        item.listening.independent_evidence_episodes = 6;
        item.listening.qualified_success_streak = 0;

        let mut probe = false;
        let mut repair = false;
        visit_task_candidates(
            crate::item_capability_view(&item),
            "introduce_high_utility_material",
            |candidate| match candidate.task_type {
                "listening_probe" => probe = true,
                "listening_repair" => repair = true,
                _ => {}
            },
        );

        assert!(
            probe,
            "one unaided listening episode is still diagnostically sparse"
        );
        assert!(
            !repair,
            "assisted volume must not make sparse unaided listening look like an established independent gap"
        );
    }

    #[test]
    fn established_listening_gap_uses_repair_not_another_diagnostic_probe() {
        let mut item = state("learning", 0.9, Some(0.90));
        item.listening.attempts = 6;
        item.listening.weighted_success_rate = Some(0.20);
        item.listening.unaided_success_rate = Some(0.20);
        item.listening.evidence_strength = 0.90;
        item.listening.effective_evidence_weight = 5.0;
        item.listening.unaided_evidence_weight = 5.0;
        item.listening.unaided_evidence_episodes = 6;
        item.listening.independent_evidence_episodes = 6;
        item.listening.qualified_success_streak = 0;

        let mut probe = false;
        let mut repair = false;
        visit_task_candidates(
            crate::item_capability_view(&item),
            "introduce_high_utility_material",
            |candidate| match candidate.task_type {
                "listening_probe" => probe = true,
                "listening_repair" => repair = true,
                _ => {}
            },
        );

        assert!(
            repair,
            "established listening weakness should trigger intervention"
        );
        assert!(
            !probe,
            "do not spend another slot diagnosing an already stable gap"
        );
    }

    #[test]
    fn diagnostic_uncertainty_blocks_transfer_promotion() {
        let mut item = state("learning", 0.9, Some(0.95));
        item.spontaneous_production.attempts = 2;
        item.spontaneous_production.evidence_strength = 0.48;
        item.spontaneous_production.independent_evidence_episodes = 2;
        item.spontaneous_production.qualified_success_streak = 2;

        let mut diagnostic = false;
        let mut transfer = false;
        visit_task_candidates(
            crate::item_capability_view(&item),
            "diagnose_and_expand",
            |candidate| match candidate.task_type {
                "diagnostic_spontaneous" => diagnostic = true,
                "transfer_practice" => transfer = true,
                _ => {}
            },
        );

        assert!(diagnostic);
        assert!(
            !transfer,
            "the same state cannot be both diagnostically uncertain and ready for transfer promotion"
        );
    }

    #[test]
    fn transfer_candidate_and_spaced_focus_share_one_readiness_boundary() {
        let mut item = state("learning", 0.95, Some(0.79));
        item.spontaneous_production.attempts = 4;
        item.spontaneous_production.weighted_success_rate = Some(0.79);
        item.spontaneous_production.unaided_success_rate = Some(0.76);
        item.spontaneous_production.evidence_strength = 0.50;
        item.spontaneous_production.effective_evidence_weight = 20.0;
        item.spontaneous_production.unaided_evidence_weight = 20.0;
        item.spontaneous_production.independent_evidence_episodes = 4;
        item.spontaneous_production.qualified_success_streak = 2;

        let mut candidate = false;
        visit_task_candidates(
            crate::item_capability_view(&item),
            "diagnose_and_expand",
            |task| candidate |= task.task_type == "transfer_practice",
        );

        let mut focus = false;
        crate::visit_tutor_focus_decisions(crate::item_capability_view(&item), 1_000, |thread| {
            focus |= thread.capability == crate::CapabilityKind::Transfer;
        });

        assert_eq!(
            candidate, focus,
            "transfer must not be emitted as an unscheduled practice candidate before the same readiness boundary opens its spaced focus thread"
        );
    }

    #[test]
    fn aggregate_rates_without_distributed_success_do_not_unlock_transfer() {
        let mut item = state("learning", 0.9, Some(0.95));
        item.spontaneous_production.attempts = 6;
        item.spontaneous_production.evidence_strength = 0.75;
        item.spontaneous_production.independent_evidence_episodes = 3;
        item.spontaneous_production.qualified_success_streak = 1;

        let mut transfer = false;
        visit_task_candidates(
            crate::item_capability_view(&item),
            "diagnose_and_expand",
            |candidate| {
                transfer |= candidate.task_type == "transfer_practice";
            },
        );
        assert!(!transfer);
    }

    #[test]
    fn retrieval_latency_threshold_is_independent_of_target_word_count() {
        let mut short = state("learning", 0.9, Some(0.90));
        short.target_text = "sí";
        short.spontaneous_production.attempts = 4;
        short.spontaneous_production.latency_weighted_millis = Some(7_000);
        short.spontaneous_production.latency_evidence_episodes = 2;
        short.spontaneous_production.evidence_strength = 0.8;
        short.spontaneous_production.qualified_success_streak = 2;

        let mut long = short;
        long.target_text = "una frase bastante más larga con muchas palabras conocidas";

        let mut short_drill = false;
        let mut long_drill = false;
        visit_task_candidates(crate::item_capability_view(&short), "x", |candidate| {
            short_drill |= candidate.task_type == "automaticity_drill";
        });
        visit_task_candidates(crate::item_capability_view(&long), "x", |candidate| {
            long_drill |= candidate.task_type == "automaticity_drill";
        });

        assert_eq!(short_drill, long_drill);
    }

    #[test]
    fn sparse_prompted_success_cannot_establish_prompt_dependence() {
        let mut item = state("learning", 1.0, Some(0.20));
        item.prompted_production.attempts = 1;
        item.prompted_production.weighted_success_rate = Some(1.0);
        item.prompted_production.evidence_strength = 0.25;
        item.prompted_production.effective_evidence_weight = 1.0;
        item.prompted_production.independent_evidence_episodes = 1;

        item.spontaneous_production.attempts = 6;
        item.spontaneous_production.weighted_success_rate = Some(0.20);
        item.spontaneous_production.unaided_success_rate = Some(0.20);
        item.spontaneous_production.evidence_strength = 0.90;
        item.spontaneous_production.effective_evidence_weight = 5.0;
        item.spontaneous_production.unaided_evidence_weight = 5.0;
        item.spontaneous_production.independent_evidence_episodes = 6;

        let mut fade = false;
        let mut retrieval = false;
        visit_task_candidates(
            crate::item_capability_view(&item),
            "probe_prompt_independence",
            |candidate| match candidate.task_type {
                "fade_prompts" => fade = true,
                "spontaneous_retrieval" => retrieval = true,
                _ => {}
            },
        );

        assert!(
            !fade,
            "prompt dependence needs established prompted evidence too"
        );
        assert!(
            retrieval,
            "the established unaided retrieval gap still needs work"
        );
    }

    #[test]
    fn decision_fragile_spontaneous_state_probes_before_committing_to_retrieval() {
        let mut item = state("learning", 0.95, Some(0.67));
        item.spontaneous_production.attempts = 3;
        item.spontaneous_production.weighted_success_rate = Some(0.67);
        item.spontaneous_production.unaided_success_rate = Some(0.67);
        item.spontaneous_production.evidence_strength = 0.68;
        item.spontaneous_production.effective_evidence_weight = 2.72;
        item.spontaneous_production.unaided_evidence_weight = 2.72;
        item.spontaneous_production.independent_evidence_episodes = 3;
        item.spontaneous_production.qualified_success_streak = 1;

        let mut diagnostic = false;
        let mut retrieval = false;
        visit_task_candidates(
            crate::item_capability_view(&item),
            "probe_prompt_independence",
            |candidate| match candidate.task_type {
                "diagnostic_spontaneous" => diagnostic = true,
                "spontaneous_retrieval" => retrieval = true,
                _ => {}
            },
        );

        assert!(
            diagnostic,
            "when one fresh independent observation can flip the action boundary, measure before committing"
        );
        assert!(!retrieval);
    }

    #[test]
    fn uncertain_spontaneous_state_probes_before_intervention() {
        let mut item = state("learning", 0.95, Some(0.20));
        item.spontaneous_production.attempts = 1;
        item.spontaneous_production.evidence_strength = 0.20;
        item.spontaneous_production.independent_evidence_episodes = 1;

        let mut diagnostic = false;
        let mut fade = false;
        let mut retrieval = false;
        visit_task_candidates(
            crate::item_capability_view(&item),
            "probe_prompt_independence",
            |candidate| match candidate.task_type {
                "diagnostic_spontaneous" => diagnostic = true,
                "fade_prompts" => fade = true,
                "spontaneous_retrieval" => retrieval = true,
                _ => {}
            },
        );

        assert!(diagnostic);
        assert!(!fade);
        assert!(!retrieval);
    }

    #[test]
    fn prompt_independence_objective_boosts_diagnostic_probe() {
        let mut item = state("learning", 0.95, Some(0.50));
        item.spontaneous_production.attempts = 1;
        item.spontaneous_production.evidence_strength = 0.20;
        item.spontaneous_production.independent_evidence_episodes = 1;

        let mut neutral_score = None;
        visit_task_candidates(crate::item_capability_view(&item), "x", |candidate| {
            if candidate.task_type == "diagnostic_spontaneous" {
                neutral_score = Some(candidate.score);
            }
        });

        let mut objective_score = None;
        visit_task_candidates(
            crate::item_capability_view(&item),
            "probe_prompt_independence",
            |candidate| {
                if candidate.task_type == "diagnostic_spontaneous" {
                    objective_score = Some(candidate.score);
                }
            },
        );

        assert_eq!(
            objective_score,
            neutral_score.map(|score| score.saturating_add(8).min(100)),
            "planner probe_prompt_independence objective must raise the matching diagnostic task"
        );
    }

    #[test]
    fn two_conflicting_independent_episodes_still_probe_before_intervention() {
        let mut item = state("learning", 0.95, Some(0.50));
        item.spontaneous_production.attempts = 2;
        item.spontaneous_production.evidence_strength = 0.48;
        item.spontaneous_production.independent_evidence_episodes = 2;

        let mut diagnostic = false;
        let mut fade = false;
        let mut retrieval = false;
        visit_task_candidates(
            crate::item_capability_view(&item),
            "probe_prompt_independence",
            |candidate| match candidate.task_type {
                "diagnostic_spontaneous" => diagnostic = true,
                "fade_prompts" => fade = true,
                "spontaneous_retrieval" => retrieval = true,
                _ => {}
            },
        );

        assert!(diagnostic);
        assert!(!fade);
        assert!(!retrieval);
    }

    #[test]
    fn prompt_gap_emits_boosted_fade_prompts() {
        let item = state("learning", 1.0, Some(0.3));
        let mut best = None;
        let mut generic_retrieval = false;
        visit_task_candidates(
            crate::item_capability_view(&item),
            "introduce_high_utility_material",
            |candidate| match candidate.task_type {
                "fade_prompts" => best = Some(candidate.score),
                "spontaneous_retrieval" => generic_retrieval = true,
                _ => {}
            },
        );
        assert_eq!(best, Some(98));
        assert!(
            !generic_retrieval,
            "an established same-item prompt gap should select prompt fading, not compete with generic retrieval"
        );
    }
}

// FUZZ SUPPORT BELOW: excluded from production line coverage.
#[cfg(fuzzing)]
/// Exercise task-candidate scoring branches from the system-state fuzz harness.
pub fn fuzz_support() {
    let estimate = |rate: Option<f64>, strength: f64, attempts: u32| CapabilityEstimateView {
        attempts,
        successes: attempts,
        weighted_success_rate: rate,
        unaided_success_rate: rate,
        latency_weighted_millis: Some(6_500),
        latency_evidence_episodes: 2,
        qualified_success_streak: 3,
        distinct_success_contexts: 2,
        distinct_success_families: 2,
        successful_contexts: &["shop", "home"],
        successful_context_families: &["commerce", "domestic"],
        evidence_strength: strength,
        effective_evidence_weight: 5.0,
        unaided_evidence_weight: 5.0,
        unaided_evidence_episodes: 4,
        independent_evidence_episodes: 4,
        last_observed_at_unix: Some(100),
        last_observed_success: Some(true),
        last_observed_qualified_success: Some(true),
    };
    let strong = estimate(Some(0.9), 0.8, 4);
    let weak = estimate(Some(0.4), 0.8, 4);
    let uncertain = estimate(None, 0.2, 0);

    let prompt_gap = ItemCapabilityView {
        learning_item_id: "prompt-gap",
        target_text: "quiero agua",
        status: "learning",
        recognition: strong,
        listening: strong,
        prompted_production: strong,
        spontaneous_production: weak,
        transfer: weak,
    };
    visit_task_candidates(prompt_gap, "probe_prompt_independence", |_| {});

    let listening_gap = ItemCapabilityView {
        learning_item_id: "listening-gap",
        target_text: "donde",
        status: "learning",
        recognition: strong,
        listening: weak,
        prompted_production: strong,
        spontaneous_production: strong,
        transfer: weak,
    };
    visit_task_candidates(listening_gap, "diagnose_and_expand", |_| {});

    let transfer_gap = ItemCapabilityView {
        learning_item_id: "transfer-gap",
        target_text: "necesito ayuda",
        status: "transfer_pending",
        recognition: strong,
        listening: strong,
        prompted_production: strong,
        spontaneous_production: strong,
        transfer: weak,
    };
    visit_task_candidates(transfer_gap, "diagnose_and_expand", |_| {});

    let diagnostic = ItemCapabilityView {
        learning_item_id: "diagnostic",
        target_text: "hola",
        status: "introduced",
        recognition: uncertain,
        listening: uncertain,
        prompted_production: uncertain,
        spontaneous_production: uncertain,
        transfer: uncertain,
    };
    visit_task_candidates(diagnostic, "probe_prompt_independence", |_| {});
    let mut inactive = diagnostic;
    inactive.status = "retired";
    visit_task_candidates(inactive, "probe_prompt_independence", |_| {});

    let _ = needs_diagnostic(uncertain);
    let _ = uncertainty_points(uncertain);
    let _ = decision_fragile(None, 1.0, 0.7);
    let _ = decision_fragile(Some(0.8), 0.0, 0.7);
    let _ = decision_fragile(Some(0.7), 1.0, 0.7);
    let _ = gap_points(None, 0.8, 18);
    let _ = boosted(95, "x", "x");
}
