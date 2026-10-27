//! Spaced tutor-focus threads derived from item capability evidence.

use crate::{CapabilityEstimateView, CapabilityKind, ItemCapabilityView, TextSetRef};

const DAY_SECONDS: i64 = 86_400;

/// Borrowed focus view emitted by the canonical per-item pass.
#[derive(Debug)]
pub struct TutorFocusThreadView<'a> {
    /// Learning item ID borrowed from the item state.
    pub learning_item_id: &'a str,
    /// Target text borrowed from the item state.
    pub target_text: &'a str,
    /// Capability being trained.
    pub capability: CapabilityKind,
    /// Spacing stage.
    pub stage: u8,
    /// Due instant.
    pub due_at_unix: i64,
    /// Due-now flag.
    pub due_now: bool,
    /// Successful contexts borrowed from the capability state.
    pub successful_contexts: TextSetRef<'a>,
    /// Successful context families borrowed from the capability state.
    pub successful_context_families: TextSetRef<'a>,
    /// Continuation rule. Dynamic only when existing contexts must be named.
    pub next_context_policy: String,
    /// Evidence-based explanation.
    pub rationale: String,
}

/// Decision-only focus view. No explanation string is allocated before top-K
/// selection; consumers may render it after selecting the final focus records.
#[derive(Debug, Clone, Copy)]
pub struct TutorFocusDecisionView<'a> {
    /// Stable identifier of the referenced learning item.
    pub learning_item_id: &'a str,
    /// target text used by the corresponding adaptation pass.
    pub target_text: &'a str,
    /// Skill dimension targeted by this task or observation.
    pub capability: CapabilityKind,
    /// stage used by the corresponding adaptation pass.
    pub stage: u8,
    /// Next scheduled practice time as a Unix timestamp in seconds.
    pub due_at_unix: i64,
    /// due now used by the corresponding adaptation pass.
    pub due_now: bool,
    /// Evidence-derived capability estimate for the focus decision.
    pub estimate: CapabilityEstimateView<'a>,
}

/// Emit focus decisions for one item without constructing an intermediate collection.
pub fn visit_tutor_focus_decisions<'a>(
    item: ItemCapabilityView<'a>,
    now_unix: i64,
    mut emit: impl FnMut(TutorFocusDecisionView<'a>),
) {
    if !focusable_status(item.status) {
        return;
    }
    emit_if_needed(
        item,
        CapabilityKind::Listening,
        item.listening,
        now_unix,
        &mut emit,
    );
    emit_if_needed(
        item,
        CapabilityKind::SpontaneousProduction,
        item.spontaneous_production,
        now_unix,
        &mut emit,
    );
    let spontaneous_ready = item
        .spontaneous_production
        .weighted_success_rate
        .is_some_and(|rate| rate >= 0.80)
        && item
            .spontaneous_production
            .unaided_success_rate
            .is_some_and(|rate| rate >= 0.75)
        && item.spontaneous_production.evidence_strength >= 0.55;
    if spontaneous_ready {
        emit_if_needed(
            item,
            CapabilityKind::Transfer,
            item.transfer,
            now_unix,
            &mut emit,
        );
    }
}

/// Emits all required focus threads for one item without allocating a collection.
pub fn visit_tutor_focus_threads<'a>(
    item: ItemCapabilityView<'a>,
    now_unix: i64,
    mut emit: impl FnMut(TutorFocusThreadView<'a>),
) {
    visit_tutor_focus_decisions(item, now_unix, |decision| {
        emit(TutorFocusThreadView {
            learning_item_id: decision.learning_item_id,
            target_text: decision.target_text,
            capability: decision.capability,
            stage: decision.stage,
            due_at_unix: decision.due_at_unix,
            due_now: decision.due_now,
            successful_contexts: decision.estimate.successful_contexts,
            successful_context_families: decision.estimate.successful_context_families,
            next_context_policy: format_focus_policy(
                decision.capability,
                decision.estimate.successful_contexts,
                decision.estimate.successful_context_families,
            ),
            rationale: format_focus_rationale(
                decision.capability,
                decision.estimate.weighted_success_rate,
                decision.estimate.evidence_strength,
                decision.stage,
                decision.estimate.distinct_success_contexts,
                decision.estimate.distinct_success_families,
            ),
        });
    });
}

fn focusable_status(status: &str) -> bool {
    matches!(
        status,
        "introduced" | "learning" | "transfer_pending" | "provisionally_mastered" | "mastered"
    )
}

fn emit_if_needed<'a>(
    item: ItemCapabilityView<'a>,
    capability: CapabilityKind,
    estimate: CapabilityEstimateView<'a>,
    now_unix: i64,
    emit: &mut impl FnMut(TutorFocusDecisionView<'a>),
) {
    if consolidated(capability, estimate) {
        return;
    }

    let stage = spacing_stage(estimate);
    let interval_days = match stage {
        0 => 0,
        1 => 1,
        2 => 3,
        3 => 7,
        _ => 14,
    };
    let due_at_unix = estimate.last_observed_at_unix.map_or(now_unix, |at| {
        at.saturating_add(i64::from(interval_days) * DAY_SECONDS)
    });
    let due_now = due_at_unix <= now_unix;

    emit(TutorFocusDecisionView {
        learning_item_id: item.learning_item_id,
        target_text: item.target_text,
        capability,
        stage,
        due_at_unix,
        due_now,
        estimate,
    });
}

fn consolidated(capability: CapabilityKind, estimate: CapabilityEstimateView<'_>) -> bool {
    match capability {
        CapabilityKind::Listening => {
            estimate
                .weighted_success_rate
                .is_some_and(|rate| rate >= 0.85)
                && estimate.evidence_strength >= 0.65
        }
        CapabilityKind::SpontaneousProduction => {
            estimate
                .weighted_success_rate
                .is_some_and(|rate| rate >= 0.85)
                && estimate
                    .unaided_success_rate
                    .is_some_and(|rate| rate >= 0.80)
                && estimate.evidence_strength >= 0.70
                && estimate.qualified_success_streak >= 2
        }
        CapabilityKind::Transfer => {
            estimate
                .weighted_success_rate
                .is_some_and(|rate| rate >= 0.80)
                && estimate
                    .unaided_success_rate
                    .is_some_and(|rate| rate >= 0.80)
                && estimate.distinct_success_families >= 3
                && estimate.evidence_strength >= 0.65
                && estimate.qualified_success_streak >= 2
        }
        CapabilityKind::Recognition | CapabilityKind::PromptedProduction => true,
    }
}

fn spacing_stage(estimate: CapabilityEstimateView<'_>) -> u8 {
    if estimate.attempts == 0 || estimate.last_observed_qualified_success != Some(true) {
        return 0;
    }
    estimate.qualified_success_streak.min(4)
}

/// Render the next-context rule from already successful contexts and families.
pub fn format_focus_policy(
    capability: CapabilityKind,
    successful_contexts: TextSetRef<'_>,
    successful_context_families: TextSetRef<'_>,
) -> String {
    match capability {
        CapabilityKind::Transfer => {
            if successful_context_families.is_empty() {
                "Use a meaningfully different situation and record both context and context_family."
                    .to_owned()
            } else {
                format!(
                    "Use a new semantic context family; avoid already successful families: {}",
                    join_texts(successful_context_families)
                )
            }
        }
        CapabilityKind::SpontaneousProduction => {
            if successful_contexts.is_empty() {
                "Use a real communicative context and record assistance_level=0 only if truly unaided."
                    .to_owned()
            } else {
                format!(
                    "Vary the concrete situation; avoid only repeating: {}",
                    join_texts(successful_contexts)
                )
            }
        }
        CapabilityKind::Listening => {
            "Change wording, speaker characteristics, or surrounding context; do not only repeat the same audio/text."
                .to_owned()
        }
        CapabilityKind::Recognition | CapabilityKind::PromptedProduction => {
            "Probe without revealing the answer first.".to_owned()
        }
    }
}

fn join_texts(values: TextSetRef<'_>) -> String {
    let mut out = String::new();
    for index in 0..values.len() {
        let Some(value) = values.get(index) else {
            continue;
        };
        if !out.is_empty() {
            out.push_str(", ");
        }
        out.push_str(value);
    }
    out
}

/// Render an explanation from capability strength, retention stage, and context diversity.
pub fn format_focus_rationale(
    capability: CapabilityKind,
    weighted_success_rate: Option<f64>,
    evidence_strength: f64,
    stage: u8,
    distinct_success_contexts: u16,
    distinct_success_families: u16,
) -> String {
    let rate = weighted_success_rate.map_or_else(
        || "unknown".to_owned(),
        |value| format!("{:.0}%", value * 100.0),
    );
    format!(
        "{capability:?}: success={rate}, evidence_strength={evidence_strength:.2}, spacing_stage={stage}, contexts={distinct_success_contexts}, families={distinct_success_families}",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CapabilityEstimateFixed, ItemCapabilityStateFixed};

    fn item<'a>(
        id: &'a str,
        status: &'a str,
        listening: CapabilityEstimateFixed<'a>,
        spontaneous: CapabilityEstimateFixed<'a>,
        transfer: CapabilityEstimateFixed<'a>,
    ) -> ItemCapabilityStateFixed<'a> {
        ItemCapabilityStateFixed {
            learning_item_id: id,
            target_text: "target",
            status,
            recognition: CapabilityEstimateFixed::default(),
            listening,
            prompted_production: CapabilityEstimateFixed::default(),
            spontaneous_production: spontaneous,
            transfer,
        }
    }

    #[test]
    fn candidate_items_do_not_emit_focus() {
        let state = item(
            "li",
            "candidate",
            CapabilityEstimateFixed::default(),
            CapabilityEstimateFixed::default(),
            CapabilityEstimateFixed::default(),
        );
        let mut count = 0;
        visit_tutor_focus_decisions(crate::item_capability_view(&state), 100, |_| count += 1);
        assert_eq!(count, 0);
    }

    #[test]
    fn missing_listening_evidence_is_due_now() {
        let state = item(
            "li",
            "learning",
            CapabilityEstimateFixed::default(),
            CapabilityEstimateFixed {
                attempts: 1,
                weighted_success_rate: Some(0.9),
                unaided_success_rate: Some(0.9),
                evidence_strength: 0.8,
                qualified_success_streak: 2,
                last_observed_qualified_success: Some(true),
                ..CapabilityEstimateFixed::default()
            },
            CapabilityEstimateFixed::default(),
        );
        let mut found = None;
        visit_tutor_focus_decisions(crate::item_capability_view(&state), 100, |decision| {
            if decision.capability == CapabilityKind::Listening {
                found = Some((decision.due_now, decision.stage));
            }
        });
        assert_eq!(found, Some((true, 0)));
    }
}
