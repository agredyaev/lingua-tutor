//! Spaced tutor-focus threads derived from item capability evidence.

use crate::{CapabilityEstimateView, CapabilityKind, ItemCapabilityView, TextSetRef};

const DAY_SECONDS: i64 = 86_400;

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
    let spontaneous_ready =
        crate::capability::spontaneous_ready_for_transfer(item.spontaneous_production);
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
                && estimate
                    .unaided_success_rate
                    .is_some_and(|rate| rate >= 0.80)
                && estimate.evidence_strength >= 0.65
                && estimate.qualified_success_streak >= 2
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
    fn assisted_listening_does_not_consolidate() {
        let state = item(
            "li",
            "learning",
            CapabilityEstimateFixed {
                attempts: 3,
                successes: 3,
                weighted_success_rate: Some(1.0),
                unaided_success_rate: None,
                evidence_strength: 0.9,
                qualified_success_streak: 0,
                last_observed_at_unix: Some(100),
                last_observed_qualified_success: Some(false),
                ..CapabilityEstimateFixed::default()
            },
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
        let mut listening = None;
        visit_tutor_focus_decisions(crate::item_capability_view(&state), 200, |decision| {
            if decision.capability == CapabilityKind::Listening {
                listening = Some((decision.due_now, decision.stage));
            }
        });
        assert_eq!(listening, Some((true, 0)));
    }

    #[test]
    fn transfer_focus_requires_distributed_spontaneous_success() {
        let state = item(
            "li",
            "learning",
            CapabilityEstimateFixed {
                attempts: 3,
                weighted_success_rate: Some(0.95),
                unaided_success_rate: Some(0.95),
                evidence_strength: 0.8,
                qualified_success_streak: 2,
                last_observed_at_unix: Some(100),
                last_observed_qualified_success: Some(true),
                ..CapabilityEstimateFixed::default()
            },
            CapabilityEstimateFixed {
                attempts: 6,
                weighted_success_rate: Some(0.95),
                unaided_success_rate: Some(0.95),
                evidence_strength: 0.8,
                independent_evidence_episodes: 3,
                qualified_success_streak: 1,
                last_observed_at_unix: Some(100),
                last_observed_qualified_success: Some(true),
                ..CapabilityEstimateFixed::default()
            },
            CapabilityEstimateFixed::default(),
        );

        let mut transfer = false;
        visit_tutor_focus_decisions(crate::item_capability_view(&state), 200, |decision| {
            transfer |= decision.capability == CapabilityKind::Transfer;
        });
        assert!(!transfer);
    }

    #[test]
    fn decay_reopens_consolidated_spontaneous_capability_for_revalidation() {
        let capability = [CapabilityKind::SpontaneousProduction; 4];
        let success = [true; 4];
        let assistance = [0; 4];
        let present = [true; 4];
        let millis = [1_000; 4];
        let contexts = [None; 4];
        let families = [None; 4];
        let episodes = [Some("les-4"), Some("les-3"), Some("les-2"), Some("les-1")];
        let base = 4 * DAY_SECONDS;
        let at = [
            base,
            base - 13 * 60 * 60,
            base - 26 * 60 * 60,
            base - 39 * 60 * 60,
        ];

        let fresh = crate::project_item_capabilities_columns(
            "li",
            "target",
            "mastered",
            4,
            &capability,
            &success,
            &assistance,
            &present,
            &millis,
            &present,
            &contexts,
            &families,
            &episodes,
            &at,
            base,
        );
        let mut fresh_spontaneous = false;
        visit_tutor_focus_decisions(crate::item_capability_view(&fresh), base, |decision| {
            fresh_spontaneous |= decision.capability == CapabilityKind::SpontaneousProduction;
        });
        assert!(
            !fresh_spontaneous,
            "fresh strong distributed production should be consolidated"
        );

        let later = base + 28 * DAY_SECONDS;
        let stale = crate::project_item_capabilities_columns(
            "li",
            "target",
            "mastered",
            4,
            &capability,
            &success,
            &assistance,
            &present,
            &millis,
            &present,
            &contexts,
            &families,
            &episodes,
            &at,
            later,
        );
        let mut stale_spontaneous = None;
        visit_tutor_focus_decisions(crate::item_capability_view(&stale), later, |decision| {
            if decision.capability == CapabilityKind::SpontaneousProduction {
                stale_spontaneous = Some((decision.due_now, decision.stage));
            }
        });

        assert_eq!(
            stale_spontaneous,
            Some((true, 4)),
            "calendar decay must reopen even previously consolidated mastery for due revalidation"
        );
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

// FUZZ SUPPORT BELOW: excluded from production line coverage.
#[cfg(fuzzing)]
/// Exercise focus scoring branches from the system-state fuzz harness.
pub fn fuzz_support(seed: u8) {
    let estimate = |rate: Option<f64>, strength: f64, streak: u8, families: u16, attempts: u32| {
        CapabilityEstimateView {
            attempts,
            successes: attempts,
            weighted_success_rate: rate,
            unaided_success_rate: rate,
            latency_weighted_millis: Some(6_000),
            latency_evidence_episodes: 2,
            qualified_success_streak: streak,
            distinct_success_contexts: 2,
            distinct_success_families: families,
            successful_contexts: &["shop", "home"],
            successful_context_families: &["commerce", "domestic"],
            evidence_strength: strength,
            effective_evidence_weight: 4.0,
            unaided_evidence_weight: 4.0,
            unaided_evidence_episodes: 4,
            independent_evidence_episodes: 4,
            last_observed_at_unix: Some(0),
            last_observed_success: Some(true),
            last_observed_qualified_success: Some(true),
        }
    };
    let weak = estimate(Some(0.5), 0.5, seed % 5, 1, 4);
    let spontaneous = estimate(Some(0.9), 0.8, 3, 2, 4);
    let transfer = if seed & 1 == 0 {
        estimate(Some(0.5), 0.5, 1, 1, 4)
    } else {
        estimate(Some(0.9), 0.8, 3, 3, 4)
    };
    let item = ItemCapabilityView {
        learning_item_id: "fuzz-item",
        target_text: "hablar",
        status: "learning",
        recognition: weak,
        listening: weak,
        prompted_production: weak,
        spontaneous_production: spontaneous,
        transfer,
    };
    visit_tutor_focus_decisions(item, 20 * DAY_SECONDS, |_| {});
    let _ = consolidated(CapabilityKind::Listening, estimate(Some(0.9), 0.8, 3, 3, 4));
    let _ = consolidated(
        CapabilityKind::SpontaneousProduction,
        estimate(Some(0.9), 0.8, 3, 3, 4),
    );
    let _ = consolidated(CapabilityKind::Transfer, estimate(Some(0.9), 0.8, 3, 3, 4));
    let _ = consolidated(CapabilityKind::Recognition, weak);
    let _ = consolidated(CapabilityKind::PromptedProduction, weak);
    let mut zero = weak;
    zero.attempts = 0;
    let _ = spacing_stage(zero);
    for streak in 1..=4 {
        let _ = spacing_stage(estimate(Some(0.5), 0.5, streak, 1, 4));
    }
    let _ = format_focus_policy(CapabilityKind::Transfer, &[], &[]);
    let _ = format_focus_policy(CapabilityKind::Transfer, &[], &["commerce", "travel"]);
    let _ = format_focus_policy(CapabilityKind::SpontaneousProduction, &[], &[]);
    let _ = format_focus_policy(
        CapabilityKind::SpontaneousProduction,
        &["shop", "home"],
        &[],
    );
    let _ = format_focus_policy(CapabilityKind::Listening, &[], &[]);
    let _ = format_focus_policy(CapabilityKind::Recognition, &[], &[]);
}
