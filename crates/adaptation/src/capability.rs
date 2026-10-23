//! Item-level capability projections derived from immutable learning evidence.
//!
//! Capability values are derived from persisted evidence and are not persisted.
//! recomputed from bounded recent evidence whenever the tutor needs a plan.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

const DAY_SECONDS: f64 = 86_400.0;

/// Evidence dimensions that matter for communicative ability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityKind {
    /// Recognition or meaning identification without production.
    Recognition,
    /// Understanding the target when heard.
    Listening,
    /// Production that required one or more tutor cues.
    PromptedProduction,
    /// Unaided production in the current communicative context.
    SpontaneousProduction,
    /// Successful use in a meaningfully different context.
    Transfer,
}

/// One normalized evidence observation used by the projection.
#[derive(Debug, Clone, PartialEq)]
pub struct EvidenceObservation {
    /// Capability demonstrated by this observation.
    pub capability: CapabilityKind,
    /// Whether the attempt succeeded.
    pub success: bool,
    /// Assistance level from 0 (unaided) to 3 (model/reveal), when known.
    pub assistance_level: Option<u8>,
    /// Response latency in milliseconds, when observed.
    pub response_millis: Option<u32>,
    /// Stable concrete communicative context key, when applicable.
    pub context_key: Option<String>,
    /// Stable semantic transfer family, supplied by the language pack/tutor.
    pub context_family: Option<String>,
    /// Observation time as a Unix timestamp in seconds.
    pub recorded_at_unix: i64,
}

/// Compact estimate for one item × capability cell.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CapabilityEstimate {
    /// Number of observations represented by the estimate.
    pub attempts: u32,
    /// Number of successful observations.
    pub successes: u32,
    /// Recency- and age-weighted success rate.
    pub weighted_success_rate: Option<f64>,
    /// Success rate among attempts explicitly recorded as unaided.
    pub unaided_success_rate: Option<f64>,
    /// Recency- and age-weighted response latency.
    pub latency_weighted_millis: Option<u32>,
    /// Number of most recent consecutive qualified successful observations.
    pub qualified_success_streak: u8,
    /// Number of distinct successful concrete contexts.
    pub distinct_success_contexts: u16,
    /// Number of distinct successful semantic context families.
    pub distinct_success_families: u16,
    /// Successful concrete context keys retained for tutor continuity.
    pub successful_contexts: Vec<String>,
    /// Successful semantic families retained for transfer planning.
    pub successful_context_families: Vec<String>,
    /// Evidence quantity + freshness confidence from 0 to 1.
    pub evidence_strength: f64,
    /// Most recent observation timestamp.
    pub last_observed_at_unix: Option<i64>,
    /// Outcome of the most recent observation.
    pub last_observed_success: Option<bool>,
    /// Whether the most recent observation is a qualified success.
    pub last_observed_qualified_success: Option<bool>,
}

/// Item-level learner state used by task scoring.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ItemCapabilityState {
    /// Persistent learning-item ID.
    pub learning_item_id: String,
    /// Target-language text shown to the tutor.
    pub target_text: String,
    /// Current learning-item lifecycle status.
    pub status: String,
    /// Recognition capability estimate.
    pub recognition: CapabilityEstimate,
    /// Listening capability estimate.
    pub listening: CapabilityEstimate,
    /// Prompted-production capability estimate.
    pub prompted_production: CapabilityEstimate,
    /// Spontaneous-production capability estimate.
    pub spontaneous_production: CapabilityEstimate,
    /// Cross-context transfer capability estimate.
    pub transfer: CapabilityEstimate,
}

/// Builds one item state from a bounded evidence history.
#[must_use]
pub fn project_item_capabilities(
    learning_item_id: impl Into<String>,
    target_text: impl Into<String>,
    status: impl Into<String>,
    evidence: &[EvidenceObservation],
    now_unix: i64,
) -> ItemCapabilityState {
    ItemCapabilityState {
        learning_item_id: learning_item_id.into(),
        target_text: target_text.into(),
        status: status.into(),
        recognition: estimate(evidence, CapabilityKind::Recognition, now_unix),
        listening: estimate(evidence, CapabilityKind::Listening, now_unix),
        prompted_production: estimate(evidence, CapabilityKind::PromptedProduction, now_unix),
        spontaneous_production: estimate(evidence, CapabilityKind::SpontaneousProduction, now_unix),
        transfer: estimate(evidence, CapabilityKind::Transfer, now_unix),
    }
}

fn estimate(
    evidence: &[EvidenceObservation],
    capability: CapabilityKind,
    now_unix: i64,
) -> CapabilityEstimate {
    let mut samples: Vec<&EvidenceObservation> = evidence
        .iter()
        .filter(|sample| sample.capability == capability)
        .collect();
    samples.sort_by_key(|sample| std::cmp::Reverse(sample.recorded_at_unix));

    let attempts = u32::try_from(samples.len()).unwrap_or(u32::MAX);
    let successes =
        u32::try_from(samples.iter().filter(|sample| sample.success).count()).unwrap_or(u32::MAX);

    let mut weighted_total = 0.0;
    let mut weighted_success = 0.0;
    let mut weighted_latency_total = 0.0;
    let mut weighted_latency = 0.0;
    let mut unaided_attempts = 0_u32;
    let mut unaided_successes = 0_u32;
    let mut contexts = BTreeSet::new();
    let mut families = BTreeSet::new();

    let qualified_success_streak = u8::try_from(
        samples
            .iter()
            .take_while(|sample| qualified_success(sample, capability))
            .count()
            .min(usize::from(u8::MAX)),
    )
    .unwrap_or(u8::MAX);

    for (index, sample) in samples.iter().enumerate() {
        let order_weight = 0.90_f64.powi(i32::try_from(index).unwrap_or(i32::MAX));
        let age_seconds = now_unix.saturating_sub(sample.recorded_at_unix).max(0) as f64;
        let age_days = age_seconds / DAY_SECONDS;
        let age_weight = 0.5_f64.powf(age_days / half_life_days(capability));
        let weight = order_weight * age_weight;

        weighted_total += weight;
        if sample.success {
            weighted_success += weight;
            if let Some(context) = non_empty(sample.context_key.as_deref()) {
                contexts.insert(context.to_owned());
            }
            if let Some(family) = non_empty(sample.context_family.as_deref()) {
                families.insert(family.to_owned());
            }
        }

        // Unknown assistance is not proof of independence.
        if sample.assistance_level == Some(0) {
            unaided_attempts = unaided_attempts.saturating_add(1);
            if sample.success {
                unaided_successes = unaided_successes.saturating_add(1);
            }
        }

        if let Some(latency) = sample.response_millis {
            weighted_latency_total += weight;
            weighted_latency += weight * f64::from(latency);
        }
    }

    let weighted_success_rate = (weighted_total > 0.0).then_some(weighted_success / weighted_total);
    let unaided_success_rate = (unaided_attempts > 0)
        .then_some(f64::from(unaided_successes) / f64::from(unaided_attempts));
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let latency_weighted_millis = (weighted_latency_total > 0.0)
        .then_some((weighted_latency / weighted_latency_total).round() as u32);

    CapabilityEstimate {
        attempts,
        successes,
        weighted_success_rate,
        unaided_success_rate,
        latency_weighted_millis,
        qualified_success_streak,
        distinct_success_contexts: u16::try_from(contexts.len()).unwrap_or(u16::MAX),
        distinct_success_families: u16::try_from(families.len()).unwrap_or(u16::MAX),
        successful_contexts: contexts.into_iter().collect(),
        successful_context_families: families.into_iter().collect(),
        // Four fresh observations saturate confidence. Old evidence naturally fades.
        evidence_strength: (weighted_total / 4.0).min(1.0),
        last_observed_at_unix: samples.first().map(|sample| sample.recorded_at_unix),
        last_observed_success: samples.first().map(|sample| sample.success),
        last_observed_qualified_success: samples
            .first()
            .map(|sample| qualified_success(sample, capability)),
    }
}

fn non_empty(value: Option<&str>) -> Option<&str> {
    value.filter(|value| !value.trim().is_empty())
}

fn half_life_days(capability: CapabilityKind) -> f64 {
    match capability {
        CapabilityKind::Recognition => 60.0,
        CapabilityKind::Listening => 35.0,
        CapabilityKind::PromptedProduction => 21.0,
        CapabilityKind::SpontaneousProduction => 28.0,
        CapabilityKind::Transfer => 42.0,
    }
}

fn qualified_success(sample: &EvidenceObservation, capability: CapabilityKind) -> bool {
    if !sample.success {
        return false;
    }
    match capability {
        CapabilityKind::Recognition | CapabilityKind::Listening => true,
        CapabilityKind::PromptedProduction => {
            sample.assistance_level.is_some_and(|level| level > 0)
        }
        CapabilityKind::SpontaneousProduction => sample.assistance_level == Some(0),
        CapabilityKind::Transfer => {
            sample.assistance_level == Some(0)
                && non_empty(sample.context_family.as_deref()).is_some()
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    const NOW: i64 = 100 * 86_400;

    fn obs(
        capability: CapabilityKind,
        success: bool,
        assistance: Option<u8>,
        latency: Option<u32>,
        context: Option<&str>,
        family: Option<&str>,
        age_days: i64,
    ) -> EvidenceObservation {
        EvidenceObservation {
            capability,
            success,
            assistance_level: assistance,
            response_millis: latency,
            context_key: context.map(str::to_owned),
            context_family: family.map(str::to_owned),
            recorded_at_unix: NOW - age_days * 86_400,
        }
    }

    #[test]
    fn unknown_assistance_never_counts_as_unaided() {
        let state = project_item_capabilities(
            "li-1",
            "hola",
            "learning",
            &[obs(
                CapabilityKind::SpontaneousProduction,
                true,
                None,
                Some(1500),
                Some("c1"),
                Some("greeting"),
                0,
            )],
            NOW,
        );
        assert_eq!(state.spontaneous_production.unaided_success_rate, None);
        assert_eq!(state.spontaneous_production.qualified_success_streak, 0);
    }

    #[test]
    fn old_evidence_loses_strength_with_calendar_time() {
        let fresh = project_item_capabilities(
            "fresh",
            "hola",
            "learning",
            &[
                obs(
                    CapabilityKind::SpontaneousProduction,
                    true,
                    Some(0),
                    None,
                    Some("c1"),
                    Some("greeting"),
                    0,
                ),
                obs(
                    CapabilityKind::SpontaneousProduction,
                    true,
                    Some(0),
                    None,
                    Some("c2"),
                    Some("greeting"),
                    1,
                ),
                obs(
                    CapabilityKind::SpontaneousProduction,
                    true,
                    Some(0),
                    None,
                    Some("c3"),
                    Some("greeting"),
                    2,
                ),
            ],
            NOW,
        );
        let stale = project_item_capabilities(
            "stale",
            "hola",
            "learning",
            &[
                obs(
                    CapabilityKind::SpontaneousProduction,
                    true,
                    Some(0),
                    None,
                    Some("c1"),
                    Some("greeting"),
                    180,
                ),
                obs(
                    CapabilityKind::SpontaneousProduction,
                    true,
                    Some(0),
                    None,
                    Some("c2"),
                    Some("greeting"),
                    181,
                ),
                obs(
                    CapabilityKind::SpontaneousProduction,
                    true,
                    Some(0),
                    None,
                    Some("c3"),
                    Some("greeting"),
                    182,
                ),
            ],
            NOW,
        );
        assert!(
            fresh.spontaneous_production.evidence_strength
                > stale.spontaneous_production.evidence_strength * 10.0
        );
    }

    #[test]
    fn transfer_requires_semantic_family_for_qualified_success() {
        let without_family = project_item_capabilities(
            "li-1",
            "target",
            "learning",
            &[obs(
                CapabilityKind::Transfer,
                true,
                Some(0),
                None,
                Some("bus:route1"),
                None,
                0,
            )],
            NOW,
        );
        assert_eq!(without_family.transfer.qualified_success_streak, 0);

        let with_family = project_item_capabilities(
            "li-1",
            "target",
            "learning",
            &[obs(
                CapabilityKind::Transfer,
                true,
                Some(0),
                None,
                Some("bus:route1"),
                Some("transport.ask_stop"),
                0,
            )],
            NOW,
        );
        assert_eq!(with_family.transfer.qualified_success_streak, 1);
        assert_eq!(with_family.transfer.distinct_success_families, 1);
    }

    #[test]
    fn recent_evidence_has_more_weight_than_old_evidence() {
        let state = project_item_capabilities(
            "li-1",
            "hola",
            "learning",
            &[
                obs(
                    CapabilityKind::SpontaneousProduction,
                    false,
                    Some(0),
                    None,
                    Some("c1"),
                    Some("greeting"),
                    5,
                ),
                obs(
                    CapabilityKind::SpontaneousProduction,
                    true,
                    Some(0),
                    None,
                    Some("c2"),
                    Some("greeting"),
                    2,
                ),
                obs(
                    CapabilityKind::SpontaneousProduction,
                    true,
                    Some(0),
                    None,
                    Some("c3"),
                    Some("greeting"),
                    0,
                ),
            ],
            NOW,
        );
        assert!(state.spontaneous_production.weighted_success_rate.unwrap() > 2.0 / 3.0);
    }
}
