//! Deterministic intervention-effect summaries from persisted planner snapshots.
//!
//! This does not auto-tune policy. It exposes delayed, observable effects so
//! thresholds and teaching methods can later be changed from evidence.

use crate::{CapabilityEstimate, CapabilityKind, ItemCapabilityState, TaskCandidate};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

/// One persisted pre-lesson snapshot used for calibration.
#[derive(Debug, Clone, PartialEq)]
pub struct CalibrationSnapshot {
    /// Snapshot time as Unix seconds.
    pub at_unix: i64,
    /// Item capability state before that lesson.
    pub item_states: Vec<ItemCapabilityState>,
    /// Ranked item-level actions proposed before that lesson.
    pub task_candidates: Vec<TaskCandidate>,
}

/// Aggregated delayed effect of one intervention action.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct InterventionEffect {
    /// Stable task/intervention code.
    pub action: String,
    /// Capability the action targets.
    pub capability: CapabilityKind,
    /// Comparable before→after samples.
    pub samples: u32,
    /// Previously unknown states that became observable.
    pub newly_observed: u32,
    /// Samples with material positive change.
    pub improved: u32,
    /// Samples with no material change.
    pub stable: u32,
    /// Samples with material negative change.
    pub declined: u32,
    /// Mean change in comparable capability success rate.
    pub mean_success_delta: Option<f64>,
    /// Number of new semantic transfer families observed after the action.
    pub transfer_family_gains: u32,
}

/// Summarizes effects between consecutive planner snapshots.
///
/// A pair is eligible only after `minimum_gap_seconds`, and only when the
/// targeted item received newer evidence between the two snapshots.
#[must_use]
pub fn summarize_intervention_effects(
    snapshots: &[CalibrationSnapshot],
    minimum_gap_seconds: i64,
) -> Vec<InterventionEffect> {
    #[derive(Default)]
    struct Acc {
        samples: u32,
        newly_observed: u32,
        improved: u32,
        stable: u32,
        declined: u32,
        delta_sum: f64,
        transfer_family_gains: u32,
    }

    let mut ordered: Vec<&CalibrationSnapshot> = snapshots.iter().collect();
    ordered.sort_by_key(|snapshot| snapshot.at_unix);

    let mut acc: BTreeMap<(String, CapabilityKind), Acc> = BTreeMap::new();

    for pair in ordered.windows(2) {
        let before = pair[0];
        let after = pair[1];
        if after.at_unix.saturating_sub(before.at_unix) < minimum_gap_seconds {
            continue;
        }

        let before_by_id: BTreeMap<&str, &ItemCapabilityState> = before
            .item_states
            .iter()
            .map(|state| (state.learning_item_id.as_str(), state))
            .collect();
        let after_by_id: BTreeMap<&str, &ItemCapabilityState> = after
            .item_states
            .iter()
            .map(|state| (state.learning_item_id.as_str(), state))
            .collect();

        let mut seen = BTreeSet::new();
        for candidate in &before.task_candidates {
            let Some(capability) = evidence_capability(&candidate.expected_evidence) else {
                continue;
            };
            if !seen.insert((
                candidate.task_type.clone(),
                candidate.learning_item_id.clone(),
                capability,
            )) {
                continue;
            }

            let Some(before_state) = before_by_id.get(candidate.learning_item_id.as_str()) else {
                continue;
            };
            let Some(after_state) = after_by_id.get(candidate.learning_item_id.as_str()) else {
                continue;
            };
            let before_estimate = estimate_for(before_state, capability);
            let after_estimate = estimate_for(after_state, capability);

            let before_at = before_estimate.last_observed_at_unix.unwrap_or(i64::MIN);
            let after_at = after_estimate.last_observed_at_unix.unwrap_or(i64::MIN);
            if after_at <= before_at {
                continue;
            }

            let bucket = acc
                .entry((candidate.task_type.clone(), capability))
                .or_default();

            if capability == CapabilityKind::Transfer {
                bucket.transfer_family_gains =
                    bucket.transfer_family_gains.saturating_add(u32::from(
                        after_estimate.distinct_success_families
                            > before_estimate.distinct_success_families,
                    ));
            }

            match (
                comparable_rate(before_estimate, capability),
                comparable_rate(after_estimate, capability),
            ) {
                (Some(before_rate), Some(after_rate)) => {
                    let delta = after_rate - before_rate;
                    bucket.samples = bucket.samples.saturating_add(1);
                    bucket.delta_sum += delta;
                    if delta > 0.08 {
                        bucket.improved = bucket.improved.saturating_add(1);
                    } else if delta < -0.08 {
                        bucket.declined = bucket.declined.saturating_add(1);
                    } else {
                        bucket.stable = bucket.stable.saturating_add(1);
                    }
                }
                (None, Some(_)) => {
                    bucket.newly_observed = bucket.newly_observed.saturating_add(1);
                }
                _ => {}
            }
        }
    }

    let mut out: Vec<InterventionEffect> = acc
        .into_iter()
        .map(|((action, capability), bucket)| InterventionEffect {
            action,
            capability,
            samples: bucket.samples,
            newly_observed: bucket.newly_observed,
            improved: bucket.improved,
            stable: bucket.stable,
            declined: bucket.declined,
            mean_success_delta: (bucket.samples > 0)
                .then_some(bucket.delta_sum / f64::from(bucket.samples)),
            transfer_family_gains: bucket.transfer_family_gains,
        })
        .collect();

    out.sort_by(|left, right| {
        let left_n = left.samples.saturating_add(left.newly_observed);
        let right_n = right.samples.saturating_add(right.newly_observed);
        right_n
            .cmp(&left_n)
            .then_with(|| right.improved.cmp(&left.improved))
            .then_with(|| left.action.cmp(&right.action))
    });
    out
}

fn comparable_rate(estimate: &CapabilityEstimate, capability: CapabilityKind) -> Option<f64> {
    match capability {
        CapabilityKind::SpontaneousProduction | CapabilityKind::Transfer => {
            estimate.unaided_success_rate
        }
        CapabilityKind::Recognition
        | CapabilityKind::Listening
        | CapabilityKind::PromptedProduction => estimate.weighted_success_rate,
    }
}

fn estimate_for(state: &ItemCapabilityState, capability: CapabilityKind) -> &CapabilityEstimate {
    match capability {
        CapabilityKind::Recognition => &state.recognition,
        CapabilityKind::Listening => &state.listening,
        CapabilityKind::PromptedProduction => &state.prompted_production,
        CapabilityKind::SpontaneousProduction => &state.spontaneous_production,
        CapabilityKind::Transfer => &state.transfer,
    }
}

fn evidence_capability(value: &str) -> Option<CapabilityKind> {
    match value {
        "recognition" => Some(CapabilityKind::Recognition),
        "listening" => Some(CapabilityKind::Listening),
        "prompted_production" => Some(CapabilityKind::PromptedProduction),
        "spontaneous_production" => Some(CapabilityKind::SpontaneousProduction),
        "transfer" => Some(CapabilityKind::Transfer),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn estimate(rate: Option<f64>, observed_at: i64, families: u16) -> CapabilityEstimate {
        CapabilityEstimate {
            attempts: u32::from(rate.is_some()),
            successes: u32::from(rate.is_some_and(|value| value >= 0.5)),
            weighted_success_rate: rate,
            unaided_success_rate: rate,
            latency_weighted_millis: None,
            qualified_success_streak: u8::from(rate.is_some_and(|value| value >= 0.5)),
            distinct_success_contexts: families,
            distinct_success_families: families,
            successful_contexts: vec![],
            successful_context_families: vec![],
            evidence_strength: 0.8,
            last_observed_at_unix: rate.map(|_| observed_at),
            last_observed_success: rate.map(|value| value >= 0.5),
            last_observed_qualified_success: rate.map(|value| value >= 0.5),
        }
    }

    fn state(rate: Option<f64>, observed_at: i64) -> ItemCapabilityState {
        ItemCapabilityState {
            learning_item_id: "li-1".to_owned(),
            target_text: "target".to_owned(),
            status: "learning".to_owned(),
            recognition: CapabilityEstimate::default(),
            listening: CapabilityEstimate::default(),
            prompted_production: CapabilityEstimate::default(),
            spontaneous_production: estimate(rate, observed_at, 0),
            transfer: CapabilityEstimate::default(),
        }
    }

    fn candidate() -> TaskCandidate {
        TaskCandidate {
            task_type: "fade_prompts".to_owned(),
            learning_item_id: "li-1".to_owned(),
            target_text: "target".to_owned(),
            score: 90,
            rationale: "gap".to_owned(),
            expected_evidence: "spontaneous_production".to_owned(),
        }
    }

    #[test]
    fn delayed_improvement_is_counted() {
        let snapshots = vec![
            CalibrationSnapshot {
                at_unix: 0,
                item_states: vec![state(Some(0.3), 0)],
                task_candidates: vec![candidate()],
            },
            CalibrationSnapshot {
                at_unix: 10 * 60 * 60,
                item_states: vec![state(Some(0.8), 10 * 60 * 60)],
                task_candidates: vec![],
            },
        ];
        let effects = summarize_intervention_effects(&snapshots, 6 * 60 * 60);
        assert_eq!(effects.len(), 1);
        assert_eq!(effects[0].samples, 1);
        assert_eq!(effects[0].improved, 1);
        assert!(
            effects[0]
                .mean_success_delta
                .is_some_and(|delta| delta > 0.4)
        );
    }

    #[test]
    fn same_session_change_is_not_used_for_calibration() {
        let snapshots = vec![
            CalibrationSnapshot {
                at_unix: 0,
                item_states: vec![state(Some(0.3), 0)],
                task_candidates: vec![candidate()],
            },
            CalibrationSnapshot {
                at_unix: 60,
                item_states: vec![state(Some(0.8), 60)],
                task_candidates: vec![],
            },
        ];
        assert!(summarize_intervention_effects(&snapshots, 6 * 60 * 60).is_empty());
    }
}
