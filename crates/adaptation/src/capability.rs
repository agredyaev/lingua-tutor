//! Item-level capability projections derived from immutable learning evidence.
//!
//! SQLite/wire text stays at the boundary. The projection pass consumes parallel
//! evidence columns and returns one bounded borrowed state; it does not retain an
//! owned entity graph or a second source of truth.

use serde::{Deserialize, Serialize};

const DAY_SECONDS: f64 = 86_400.0;
use crate::MIN_DISTRIBUTED_SUCCESS_GAP_SECONDS;
// Context breadth is itself retention evidence. Keep a successful context only
// while at least one quarter of its age-weight remains (two capability half-lives).
const MIN_CONTEXT_EVIDENCE_WEIGHT: f64 = 0.25;

fn stable_context_family_id(value: &str) -> bool {
    let bytes = value.as_bytes();
    let Some(first) = bytes.first().copied() else {
        return false;
    };
    let Some(last) = bytes.last().copied() else {
        return false;
    };
    let edge = |byte: u8| byte.is_ascii_lowercase() || byte.is_ascii_digit();
    edge(first)
        && edge(last)
        && bytes.iter().all(|byte| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || matches!(*byte, b'.' | b'_' | b'-')
        })
}

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

/// Maximum recent observations retained per item × capability projection.
pub const MAX_CAPABILITY_OBSERVATIONS: usize = 12;

/// Fixed sorted set of borrowed context strings.
#[derive(Debug, Clone, Copy)]
pub struct ContextSetView<'a> {
    values: [&'a str; MAX_CAPABILITY_OBSERVATIONS],
    len: usize,
}

impl<'a> Default for ContextSetView<'a> {
    fn default() -> Self {
        Self {
            values: [""; MAX_CAPABILITY_OBSERVATIONS],
            len: 0,
        }
    }
}

fn context_set_insert<'a>(set: &mut ContextSetView<'a>, value: Option<&'a str>) {
    let Some(value) = value.filter(|value| !value.trim().is_empty()) else {
        return;
    };
    if set.values[..set.len].contains(&value) || set.len == MAX_CAPABILITY_OBSERVATIONS {
        return;
    }
    let mut position = set.len;
    while position > 0 && set.values[position - 1] > value {
        set.values[position] = set.values[position - 1];
        position -= 1;
    }
    set.values[position] = value;
    set.len += 1;
}

/// Borrow the populated sorted context values.
#[must_use]
fn context_set_slice<'store, 'text>(set: &'store ContextSetView<'text>) -> &'store [&'text str] {
    &set.values[..set.len]
}

impl Serialize for ContextSetView<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        context_set_slice(self).serialize(serializer)
    }
}

/// Allocation-free estimate borrowing context strings from the evidence buffer.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct CapabilityEstimateFixed<'a> {
    /// Number of observations included in the estimate.
    pub attempts: u32,
    /// Number of successful observations.
    pub successes: u32,
    /// Recency-weighted success rate.
    pub weighted_success_rate: Option<f64>,
    /// Success rate among explicitly unaided attempts.
    pub unaided_success_rate: Option<f64>,
    /// Recency-weighted response latency.
    pub latency_weighted_millis: Option<u32>,
    /// Independent qualified episodes that actually measured retrieval latency.
    pub latency_evidence_episodes: u16,
    /// Qualified-success streak from the newest observation backwards.
    pub qualified_success_streak: u8,
    /// Number of distinct successful contexts.
    pub distinct_success_contexts: u16,
    /// Number of distinct successful semantic families.
    pub distinct_success_families: u16,
    /// Successful concrete contexts.
    pub successful_contexts: ContextSetView<'a>,
    /// Successful semantic context families.
    pub successful_context_families: ContextSetView<'a>,
    /// Normalized evidence strength from independent learning episodes.
    pub evidence_strength: f64,
    /// Raw recency/order-weighted independent evidence mass used for decision sensitivity.
    pub effective_evidence_weight: f64,
    /// Raw recency/order-weighted evidence mass from explicitly unaided episodes.
    pub unaided_evidence_weight: f64,
    /// Independent episodes with explicitly unaided measurement.
    pub unaided_evidence_episodes: u16,
    /// Number of independent learning episodes contributing to evidence strength.
    pub independent_evidence_episodes: u16,
    /// Newest observation time.
    pub last_observed_at_unix: Option<i64>,
    /// Newest observation success.
    pub last_observed_success: Option<bool>,
    /// Whether the newest observation is a qualified success.
    pub last_observed_qualified_success: Option<bool>,
}

impl Default for CapabilityEstimateFixed<'_> {
    fn default() -> Self {
        Self {
            attempts: 0,
            successes: 0,
            weighted_success_rate: None,
            unaided_success_rate: None,
            latency_weighted_millis: None,
            latency_evidence_episodes: 0,
            qualified_success_streak: 0,
            distinct_success_contexts: 0,
            distinct_success_families: 0,
            successful_contexts: ContextSetView::default(),
            successful_context_families: ContextSetView::default(),
            evidence_strength: 0.0,
            effective_evidence_weight: 0.0,
            unaided_evidence_weight: 0.0,
            unaided_evidence_episodes: 0,
            independent_evidence_episodes: 0,
            last_observed_at_unix: None,
            last_observed_success: None,
            last_observed_qualified_success: None,
        }
    }
}

/// Allocation-free item capability state borrowing item/evidence text.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct ItemCapabilityStateFixed<'a> {
    /// Stable persisted ID borrowed from the SQLite boundary.
    pub learning_item_id: &'a str,
    /// Target-language text.
    pub target_text: &'a str,
    /// Current lifecycle status.
    pub status: &'a str,
    /// Recognition estimate.
    pub recognition: CapabilityEstimateFixed<'a>,
    /// Listening estimate.
    pub listening: CapabilityEstimateFixed<'a>,
    /// Prompted-production estimate.
    pub prompted_production: CapabilityEstimateFixed<'a>,
    /// Spontaneous-production estimate.
    pub spontaneous_production: CapabilityEstimateFixed<'a>,
    /// Transfer estimate.
    pub transfer: CapabilityEstimateFixed<'a>,
}

/// Project one item directly from parallel evidence columns.
#[allow(clippy::too_many_arguments)]
#[must_use]
pub fn project_item_capabilities_columns<'a>(
    learning_item_id: &'a str,
    target_text: &'a str,
    status: &'a str,
    len: usize,
    capability: &[CapabilityKind],
    success: &[bool],
    assistance_level: &[u8],
    assistance_present: &[bool],
    response_millis: &[u32],
    response_present: &[bool],
    context_key: &[Option<&'a str>],
    context_family: &[Option<&'a str>],
    episode_key: &[Option<&'a str>],
    recorded_at_unix: &[i64],
    now_unix: i64,
) -> ItemCapabilityStateFixed<'a> {
    let estimate = |kind| {
        estimate_columns(
            len,
            kind,
            capability,
            success,
            assistance_level,
            assistance_present,
            response_millis,
            response_present,
            context_key,
            context_family,
            episode_key,
            recorded_at_unix,
            now_unix,
        )
    };
    ItemCapabilityStateFixed {
        learning_item_id,
        target_text,
        status,
        recognition: estimate(CapabilityKind::Recognition),
        listening: estimate(CapabilityKind::Listening),
        prompted_production: estimate(CapabilityKind::PromptedProduction),
        spontaneous_production: estimate(CapabilityKind::SpontaneousProduction),
        transfer: estimate(CapabilityKind::Transfer),
    }
}

#[allow(clippy::too_many_arguments)]
fn estimate_columns<'a>(
    len: usize,
    kind: CapabilityKind,
    capability: &[CapabilityKind],
    success: &[bool],
    assistance_level: &[u8],
    assistance_present: &[bool],
    response_millis: &[u32],
    response_present: &[bool],
    context_key: &[Option<&'a str>],
    context_family: &[Option<&'a str>],
    episode_key: &[Option<&'a str>],
    recorded_at_unix: &[i64],
    now_unix: i64,
) -> CapabilityEstimateFixed<'a> {
    let available = len
        .min(capability.len())
        .min(success.len())
        .min(assistance_level.len())
        .min(assistance_present.len())
        .min(response_millis.len())
        .min(response_present.len())
        .min(context_key.len())
        .min(context_family.len())
        .min(episode_key.len())
        .min(recorded_at_unix.len());

    let mut samples = [0usize; MAX_CAPABILITY_OBSERVATIONS];
    let mut sample_len = 0usize;
    for source in 0..available {
        if capability[source] != kind {
            continue;
        }
        if sample_len == MAX_CAPABILITY_OBSERVATIONS
            && recorded_at_unix[samples[sample_len - 1]] >= recorded_at_unix[source]
        {
            continue;
        }
        let mut position = sample_len.min(MAX_CAPABILITY_OBSERVATIONS - 1);
        while position > 0 && recorded_at_unix[samples[position - 1]] < recorded_at_unix[source] {
            samples[position] = samples[position - 1];
            position -= 1;
        }
        samples[position] = source;
        if sample_len < MAX_CAPABILITY_OBSERVATIONS {
            sample_len += 1;
        }
    }

    let attempts = u32::try_from(sample_len).unwrap_or(u32::MAX);
    let successes = u32::try_from(
        samples[..sample_len]
            .iter()
            .filter(|index| success[**index])
            .count(),
    )
    .unwrap_or(u32::MAX);

    let mut weighted_total = 0.0;
    let mut weighted_success = 0.0;
    let mut independent_weighted_total = 0.0;
    let mut independent_episode_count = 0_u16;
    let mut independent_episode_keys = [None; MAX_CAPABILITY_OBSERVATIONS];
    let mut independent_episode_key_len = 0usize;
    let mut newest_independent_at: Option<i64> = None;
    let mut newest_independent_episode: Option<&'a str> = None;
    let mut weighted_latency_total = 0.0;
    let mut weighted_latency = 0.0;
    let mut latency_evidence_episode_count = 0_u16;
    let mut unaided_weighted_total = 0.0;
    let mut unaided_weighted_success = 0.0;
    let mut unaided_episode_count = 0_u16;
    let mut contexts = ContextSetView::default();
    let mut families = ContextSetView::default();
    let mut observed_contexts = ContextSetView::default();
    let mut observed_families = ContextSetView::default();

    // Spacing stages represent temporally distributed retrieval, not merely
    // distinct lesson records. Episode IDs collapse massed attempts within one
    // lesson; the anti-massing time gap is still required between counted stages.
    let mut streak = 0_u8;
    let mut newest_counted_at: Option<i64> = None;
    let mut newest_episode: Option<&'a str> = None;
    for source in samples[..sample_len].iter().copied() {
        let observed_at = recorded_at_unix[source];
        let episode = non_empty(episode_key[source]);
        if let Some(newer_at) = newest_counted_at {
            if newest_episode.is_some() && newest_episode == episode {
                continue;
            }
            if newer_at.saturating_sub(observed_at) < MIN_DISTRIBUTED_SUCCESS_GAP_SECONDS {
                continue;
            }
        }
        newest_counted_at = Some(observed_at);
        newest_episode = episode;
        if qualified_success(
            kind,
            success[source],
            assistance_present[source].then_some(assistance_level[source]),
            context_family[source],
        ) {
            streak = streak.saturating_add(1);
        } else {
            break;
        }
    }

    for source in samples[..sample_len].iter().copied() {
        let age_seconds = now_unix.saturating_sub(recorded_at_unix[source]).max(0) as f64;
        let age_weight = 0.5_f64.powf((age_seconds / DAY_SECONDS) / half_life_days(kind));

        // Rates and latency describe independent learning episodes, not raw
        // attempts. The newest observation in an explicit episode is its exit
        // observation; legacy evidence falls back to the 12h anti-massing gap.
        let observed_at = recorded_at_unix[source];
        let episode = non_empty(episode_key[source]);
        let repeated_known_episode = episode.is_some_and(|value| {
            independent_episode_keys[..independent_episode_key_len].contains(&Some(value))
        });
        let too_close_to_legacy = newest_independent_at.is_some_and(|newer_at| {
            let both_known = newest_independent_episode.is_some() && episode.is_some();
            !both_known
                && newer_at.saturating_sub(observed_at) < MIN_DISTRIBUTED_SUCCESS_GAP_SECONDS
        });
        let independent =
            assistance_present[source] && !repeated_known_episode && !too_close_to_legacy;
        let qualified = qualified_success(
            kind,
            success[source],
            assistance_present[source].then_some(assistance_level[source]),
            context_family[source],
        );

        if independent {
            let independent_order_weight = 0.90_f64.powi(i32::from(independent_episode_count));
            let independent_weight = independent_order_weight * age_weight;
            independent_weighted_total += independent_weight;
            weighted_total += independent_weight;
            if success[source] {
                weighted_success += independent_weight;
            }
            if assistance_level[source] == 0 {
                unaided_weighted_total += independent_weight;
                unaided_episode_count = unaided_episode_count.saturating_add(1);
                if success[source] {
                    unaided_weighted_success += independent_weight;
                }
            }
            if response_present[source] && qualified {
                weighted_latency_total += independent_weight;
                weighted_latency += independent_weight * f64::from(response_millis[source]);
                latency_evidence_episode_count = latency_evidence_episode_count.saturating_add(1);
            }

            let context = non_empty(context_key[source]);
            let family = context_family[source].filter(|value| stable_context_family_id(value));
            let context_is_latest = context
                .is_some_and(|value| !context_set_slice(&observed_contexts).contains(&value));
            let family_is_latest =
                family.is_some_and(|value| !context_set_slice(&observed_families).contains(&value));
            context_set_insert(&mut observed_contexts, context);
            context_set_insert(&mut observed_families, family);
            if qualified && age_weight >= MIN_CONTEXT_EVIDENCE_WEIGHT {
                if context_is_latest {
                    context_set_insert(&mut contexts, context);
                }
                if family_is_latest {
                    context_set_insert(&mut families, family);
                }
            }

            independent_episode_count = independent_episode_count.saturating_add(1);
            newest_independent_at = Some(observed_at);
            newest_independent_episode = episode;
            if let Some(value) = episode
                && independent_episode_key_len < independent_episode_keys.len()
            {
                independent_episode_keys[independent_episode_key_len] = Some(value);
                independent_episode_key_len += 1;
            }
        }
    }

    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let latency_weighted_millis = (weighted_latency_total > 0.0)
        .then_some((weighted_latency / weighted_latency_total).round() as u32);
    let first = samples.first().copied().filter(|_| sample_len > 0);

    CapabilityEstimateFixed {
        attempts,
        successes,
        weighted_success_rate: (weighted_total > 0.0).then_some(weighted_success / weighted_total),
        unaided_success_rate: (unaided_weighted_total > 0.0)
            .then_some(unaided_weighted_success / unaided_weighted_total),
        latency_weighted_millis,
        latency_evidence_episodes: latency_evidence_episode_count,
        qualified_success_streak: streak,
        distinct_success_contexts: u16::try_from(context_set_slice(&contexts).len())
            .unwrap_or(u16::MAX),
        distinct_success_families: u16::try_from(context_set_slice(&families).len())
            .unwrap_or(u16::MAX),
        successful_contexts: contexts,
        successful_context_families: families,
        evidence_strength: (independent_weighted_total / 4.0).min(1.0),
        effective_evidence_weight: independent_weighted_total,
        unaided_evidence_weight: unaided_weighted_total,
        unaided_evidence_episodes: unaided_episode_count,
        independent_evidence_episodes: independent_episode_count,
        last_observed_at_unix: first.map(|index| recorded_at_unix[index]),
        last_observed_success: first.map(|index| success[index]),
        last_observed_qualified_success: first.map(|index| {
            qualified_success(
                kind,
                success[index],
                assistance_present[index].then_some(assistance_level[index]),
                context_family[index],
            )
        }),
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

fn qualified_success(
    capability: CapabilityKind,
    success: bool,
    assistance: Option<u8>,
    family: Option<&str>,
) -> bool {
    if !success {
        return false;
    }
    match capability {
        CapabilityKind::Recognition | CapabilityKind::Listening => assistance == Some(0),
        CapabilityKind::PromptedProduction => assistance.is_some_and(|level| level > 0),
        CapabilityKind::SpontaneousProduction => assistance == Some(0),
        CapabilityKind::Transfer => {
            assistance == Some(0) && family.is_some_and(stable_context_family_id)
        }
    }
}

/// Borrowed text set used by downstream passes.
pub type TextSetRef<'a> = &'a [&'a str];

/// Borrowed capability estimate consumed by scoring/focus passes.
#[derive(Debug, Clone, Copy)]
pub struct CapabilityEstimateView<'a> {
    /// Number of observations.
    pub attempts: u32,
    /// Successful observations.
    pub successes: u32,
    /// Weighted success rate.
    pub weighted_success_rate: Option<f64>,
    /// Explicitly unaided success rate.
    pub unaided_success_rate: Option<f64>,
    /// Weighted latency.
    pub latency_weighted_millis: Option<u32>,
    /// Independent qualified episodes that measured latency.
    pub latency_evidence_episodes: u16,
    /// Qualified success streak.
    pub qualified_success_streak: u8,
    /// Distinct successful contexts.
    pub distinct_success_contexts: u16,
    /// Distinct successful context families.
    pub distinct_success_families: u16,
    /// Successful contexts.
    pub successful_contexts: TextSetRef<'a>,
    /// Successful context families.
    pub successful_context_families: TextSetRef<'a>,
    /// Evidence strength from independent learning episodes.
    pub evidence_strength: f64,
    /// Raw recency/order-weighted independent evidence mass.
    pub effective_evidence_weight: f64,
    /// Raw recency/order-weighted explicitly unaided evidence mass.
    pub unaided_evidence_weight: f64,
    /// Independent episodes with explicitly unaided measurement.
    pub unaided_evidence_episodes: u16,
    /// Number of independent learning episodes represented by the estimate.
    pub independent_evidence_episodes: u16,
    /// Newest observation time.
    pub last_observed_at_unix: Option<i64>,
    /// Newest observation success.
    pub last_observed_success: Option<bool>,
    /// Newest observation qualification.
    pub last_observed_qualified_success: Option<bool>,
}

pub(crate) fn spontaneous_ready_for_transfer(estimate: CapabilityEstimateView<'_>) -> bool {
    estimate
        .weighted_success_rate
        .is_some_and(|rate| rate >= 0.80)
        && estimate
            .unaided_success_rate
            .is_some_and(|rate| rate >= 0.75)
        && estimate.evidence_strength >= 0.55
        && estimate.qualified_success_streak >= 2
}

/// Project one fixed estimate into its borrowed downstream view.
#[must_use]
fn capability_estimate_view<'a>(
    value: &'a CapabilityEstimateFixed<'a>,
) -> CapabilityEstimateView<'a> {
    CapabilityEstimateView {
        attempts: value.attempts,
        successes: value.successes,
        weighted_success_rate: value.weighted_success_rate,
        unaided_success_rate: value.unaided_success_rate,
        latency_weighted_millis: value.latency_weighted_millis,
        latency_evidence_episodes: value.latency_evidence_episodes,
        qualified_success_streak: value.qualified_success_streak,
        distinct_success_contexts: value.distinct_success_contexts,
        distinct_success_families: value.distinct_success_families,
        successful_contexts: context_set_slice(&value.successful_contexts),
        successful_context_families: context_set_slice(&value.successful_context_families),
        evidence_strength: value.evidence_strength,
        effective_evidence_weight: value.effective_evidence_weight,
        unaided_evidence_weight: value.unaided_evidence_weight,
        unaided_evidence_episodes: value.unaided_evidence_episodes,
        independent_evidence_episodes: value.independent_evidence_episodes,
        last_observed_at_unix: value.last_observed_at_unix,
        last_observed_success: value.last_observed_success,
        last_observed_qualified_success: value.last_observed_qualified_success,
    }
}

/// Borrowed item capability view consumed by adaptation passes.
#[derive(Debug, Clone, Copy)]
pub struct ItemCapabilityView<'a> {
    /// Persisted item ID borrowed from the boundary.
    pub learning_item_id: &'a str,
    /// Target text.
    pub target_text: &'a str,
    /// Lifecycle status.
    pub status: &'a str,
    /// Recognition estimate.
    pub recognition: CapabilityEstimateView<'a>,
    /// Listening estimate.
    pub listening: CapabilityEstimateView<'a>,
    /// Prompted-production estimate.
    pub prompted_production: CapabilityEstimateView<'a>,
    /// Spontaneous-production estimate.
    pub spontaneous_production: CapabilityEstimateView<'a>,
    /// Transfer estimate.
    pub transfer: CapabilityEstimateView<'a>,
}

/// Project one fixed item state into the borrowed view consumed by scoring passes.
#[must_use]
pub fn item_capability_view<'a>(value: &'a ItemCapabilityStateFixed<'a>) -> ItemCapabilityView<'a> {
    ItemCapabilityView {
        learning_item_id: value.learning_item_id,
        target_text: value.target_text,
        status: value.status,
        recognition: capability_estimate_view(&value.recognition),
        listening: capability_estimate_view(&value.listening),
        prompted_production: capability_estimate_view(&value.prompted_production),
        spontaneous_production: capability_estimate_view(&value.spontaneous_production),
        transfer: capability_estimate_view(&value.transfer),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn column_projection_tracks_unaided_transfer_contexts() {
        let capability = [
            CapabilityKind::Transfer,
            CapabilityKind::Transfer,
            CapabilityKind::SpontaneousProduction,
        ];
        let success = [true, true, false];
        let assistance = [0, 0, 0];
        let assistance_present = [true, true, true];
        let millis = [1200, 1400, 2000];
        let millis_present = [true, true, true];
        let contexts = [Some("c2"), Some("c1"), None];
        let families = [Some("family-b"), Some("family-a"), None];
        let at = [172_800, 86_400, 100];

        let state = project_item_capabilities_columns(
            "li-1",
            "hola",
            "learning",
            3,
            &capability,
            &success,
            &assistance,
            &assistance_present,
            &millis,
            &millis_present,
            &contexts,
            &families,
            &[Some("les-2"), Some("les-1"), Some("les-0")],
            &at,
            172_900,
        );
        assert_eq!(state.transfer.attempts, 2);
        assert_eq!(state.transfer.qualified_success_streak, 2);
        assert_eq!(
            context_set_slice(&state.transfer.successful_context_families),
            ["family-a", "family-b"]
        );
    }

    #[test]
    fn assisted_listening_success_is_not_spacing_qualified() {
        let capability = [CapabilityKind::Listening];
        let success = [true];
        let assistance = [2];
        let present = [true];
        let millis = [1_000];
        let contexts = [Some("audio")];
        let families = [None];
        let at = [86_400_i64];

        let state = project_item_capabilities_columns(
            "li",
            "x",
            "learning",
            1,
            &capability,
            &success,
            &assistance,
            &present,
            &millis,
            &present,
            &contexts,
            &families,
            &[Some("les-1")],
            &at,
            86_500,
        );

        assert_eq!(state.listening.weighted_success_rate, Some(1.0));
        assert_eq!(state.listening.unaided_success_rate, None);
        assert_eq!(state.listening.distinct_success_contexts, 0);
        assert!(context_set_slice(&state.listening.successful_contexts).is_empty());
        assert_eq!(state.listening.qualified_success_streak, 0);
        assert_eq!(state.listening.last_observed_qualified_success, Some(false));
    }

    #[test]
    fn same_day_successes_advance_only_one_spacing_stage() {
        let capability = [CapabilityKind::SpontaneousProduction; 3];
        let success = [true; 3];
        let assistance = [0; 3];
        let present = [true; 3];
        let millis = [1_000; 3];
        let contexts = [Some("c3"), Some("c2"), Some("c1")];
        let families = [None; 3];
        let at = [86_500_i64, 86_450_i64, 86_400_i64];

        let state = project_item_capabilities_columns(
            "li",
            "x",
            "learning",
            3,
            &capability,
            &success,
            &assistance,
            &present,
            &millis,
            &present,
            &contexts,
            &families,
            &[Some("les-1"); 3],
            &at,
            86_600,
        );

        assert_eq!(state.spontaneous_production.qualified_success_streak, 1);
        assert_eq!(
            state.spontaneous_production.independent_evidence_episodes,
            1
        );
        assert!(state.spontaneous_production.evidence_strength < 0.30);
    }

    #[test]
    fn massed_attempts_do_not_discount_the_next_independent_episode() {
        let capability = [CapabilityKind::SpontaneousProduction; 4];
        let success = [true; 4];
        let assistance = [0; 4];
        let present = [true; 4];
        let millis = [1_000; 4];
        let contexts = [Some("m3"), Some("m2"), Some("m1"), Some("old")];
        let families = [None; 4];
        let episodes = [Some("les-2"), Some("les-2"), Some("les-2"), Some("les-1")];
        let at = [86_500_i64, 86_490_i64, 86_480_i64, 82_900_i64];

        let state = project_item_capabilities_columns(
            "li",
            "x",
            "learning",
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
            86_800,
        );

        assert_eq!(
            state.spontaneous_production.independent_evidence_episodes,
            2
        );
        assert!(
            state.spontaneous_production.evidence_strength > 0.45,
            "independent episode confidence must not be discounted by massed attempts"
        );
    }

    #[test]
    fn distinct_lessons_inside_anti_massing_gap_do_not_advance_two_spacing_stages() {
        let capability = [CapabilityKind::SpontaneousProduction; 2];
        let success = [true; 2];
        let assistance = [0; 2];
        let present = [true; 2];
        let millis = [1_000; 2];
        let contexts = [Some("c2"), Some("c1")];
        let families = [None; 2];
        let at = [86_500_i64, 82_900_i64];

        let state = project_item_capabilities_columns(
            "li",
            "x",
            "learning",
            2,
            &capability,
            &success,
            &assistance,
            &present,
            &millis,
            &present,
            &contexts,
            &families,
            &[Some("les-2"), Some("les-1")],
            &at,
            86_800,
        );

        assert_eq!(state.spontaneous_production.qualified_success_streak, 1);
        assert_eq!(
            state.spontaneous_production.independent_evidence_episodes,
            2
        );
        assert!(state.spontaneous_production.evidence_strength > 0.45);
    }

    #[test]
    fn distinct_lessons_beyond_anti_massing_gap_advance_spacing() {
        let capability = [CapabilityKind::SpontaneousProduction; 2];
        let success = [true; 2];
        let assistance = [0; 2];
        let present = [true; 2];
        let millis = [1_000; 2];
        let contexts = [Some("c2"), Some("c1")];
        let families = [None; 2];
        let at = [86_500_i64, 40_000_i64];

        let state = project_item_capabilities_columns(
            "li",
            "x",
            "learning",
            2,
            &capability,
            &success,
            &assistance,
            &present,
            &millis,
            &present,
            &contexts,
            &families,
            &[Some("les-2"), Some("les-1")],
            &at,
            86_800,
        );

        assert_eq!(state.spontaneous_production.qualified_success_streak, 2);
        assert_eq!(
            state.spontaneous_production.independent_evidence_episodes,
            2
        );
    }

    #[test]
    fn crossing_midnight_does_not_fake_a_second_spacing_success() {
        let capability = [CapabilityKind::SpontaneousProduction; 2];
        let success = [true; 2];
        let assistance = [0; 2];
        let present = [true; 2];
        let millis = [1_000; 2];
        let contexts = [Some("c2"), Some("c1")];
        let families = [None; 2];
        let at = [86_500_i64, 86_300_i64];

        let state = project_item_capabilities_columns(
            "li",
            "x",
            "learning",
            2,
            &capability,
            &success,
            &assistance,
            &present,
            &millis,
            &present,
            &contexts,
            &families,
            &[None; 2],
            &at,
            86_800,
        );

        assert_eq!(state.spontaneous_production.qualified_success_streak, 1);
        assert_eq!(
            state.spontaneous_production.independent_evidence_episodes,
            1
        );
    }

    #[test]
    fn newest_twelve_are_selected() {
        let capability = [CapabilityKind::Listening; 18];
        let mut success = [false; 18];
        for value in &mut success[6..] {
            *value = true;
        }
        let assistance = [0; 18];
        let present = [false; 18];
        let millis = [0; 18];
        let contexts = [None; 18];
        let families = [None; 18];
        let mut at = [0_i64; 18];
        for (index, value) in at.iter_mut().enumerate() {
            *value = index as i64;
        }
        let state = project_item_capabilities_columns(
            "li",
            "x",
            "learning",
            18,
            &capability,
            &success,
            &assistance,
            &present,
            &millis,
            &present,
            &contexts,
            &families,
            &[None; 18],
            &at,
            20,
        );
        assert_eq!(state.listening.attempts, 12);
        assert_eq!(state.listening.successes, 12);
    }

    #[test]
    fn massed_successes_do_not_dominate_episode_weighted_rate() {
        let capability = [CapabilityKind::SpontaneousProduction; 12];
        let mut success = [true; 12];
        success[10] = false;
        success[11] = false;
        let assistance = [0; 12];
        let present = [true; 12];
        let millis = [1_000; 12];
        let contexts = [None; 12];
        let families = [None; 12];
        let mut episodes = [Some("les-new"); 12];
        episodes[10] = Some("les-old-2");
        episodes[11] = Some("les-old-1");
        let at = [120, 119, 118, 117, 116, 115, 114, 113, 112, 111, 100, 90];

        let state = project_item_capabilities_columns(
            "li",
            "x",
            "learning",
            12,
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
            121,
        );

        assert!(
            state
                .spontaneous_production
                .weighted_success_rate
                .is_some_and(|rate| rate < 0.50),
            "one massed lesson must not outweigh two independent failed lessons"
        );
    }

    #[test]
    fn newer_family_failure_invalidates_older_family_success() {
        let capability = [CapabilityKind::Transfer; 4];
        let success = [true, true, false, true];
        let assistance = [0; 4];
        let present = [true; 4];
        let millis = [1_000; 4];
        let contexts = [None; 4];
        let families = [
            Some("family-a"),
            Some("family-c"),
            Some("family-b"),
            Some("family-b"),
        ];
        let episodes = [Some("les-4"), Some("les-3"), Some("les-2"), Some("les-1")];
        let at = [400_i64, 300, 200, 100];

        let state = project_item_capabilities_columns(
            "li",
            "x",
            "learning",
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
            401,
        );

        assert_eq!(
            state.transfer.distinct_success_families, 2,
            "a family's latest independent failure must invalidate its older success"
        );
        assert_eq!(
            context_set_slice(&state.transfer.successful_context_families),
            ["family-a", "family-c"]
        );
    }

    #[test]
    fn noncanonical_transfer_family_is_not_qualified_or_counted() {
        let capability = [CapabilityKind::Transfer];
        let success = [true];
        let assistance = [0];
        let present = [true];
        let millis = [1_000];
        let contexts = [None];
        let families = [Some("Service.Ask_Help")];
        let episodes = [Some("les-1")];
        let at = [100_i64];

        let state = project_item_capabilities_columns(
            "li",
            "x",
            "learning",
            1,
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
            101,
        );

        assert_eq!(state.transfer.distinct_success_families, 0);
        assert_eq!(state.transfer.last_observed_qualified_success, Some(false));
    }

    #[test]
    fn stale_transfer_families_do_not_count_as_current_diversity() {
        let capability = [CapabilityKind::Transfer; 6];
        let success = [true; 6];
        let assistance = [0; 6];
        let present = [true; 6];
        let millis = [1_000; 6];
        let contexts = [None; 6];
        let families = [
            Some("family-a"),
            Some("family-a"),
            Some("family-a"),
            Some("family-a"),
            Some("family-b"),
            Some("family-c"),
        ];
        let episodes = [
            Some("recent-3"),
            Some("recent-2"),
            Some("recent-1"),
            Some("old-3"),
            Some("old-2"),
            Some("old-1"),
        ];
        let now = 100 * DAY_SECONDS as i64;
        let at = [
            now,
            now - DAY_SECONDS as i64,
            now - 2 * DAY_SECONDS as i64,
            now - 98 * DAY_SECONDS as i64,
            now - 99 * DAY_SECONDS as i64,
            now - 100 * DAY_SECONDS as i64,
        ];

        let state = project_item_capabilities_columns(
            "li",
            "x",
            "learning",
            6,
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
            now,
        );

        assert!(
            state.transfer.evidence_strength >= 0.65,
            "fresh repeated evidence should keep overall transfer confidence high"
        );
        assert_eq!(
            state.transfer.distinct_success_families, 1,
            "context diversity must itself decay; ancient families cannot be kept alive by fresh success in one family"
        );
    }

    #[test]
    fn latency_uses_only_qualified_successes() {
        let capability = [CapabilityKind::SpontaneousProduction; 3];
        let success = [true, false, true];
        let assistance = [0, 0, 2];
        let present = [true; 3];
        let millis = [1_000, 20_000, 30_000];
        let contexts = [None; 3];
        let families = [None; 3];
        let episodes = [Some("les-3"), Some("les-2"), Some("les-1")];
        let at = [300, 200, 100];

        let state = project_item_capabilities_columns(
            "li",
            "x",
            "learning",
            3,
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
            301,
        );

        assert_eq!(
            state.spontaneous_production.latency_weighted_millis,
            Some(1_000)
        );
    }

    #[test]
    fn unaided_success_rate_is_recency_weighted_not_lifetime_raw_count() {
        let capability = [CapabilityKind::SpontaneousProduction; 8];
        let success = [true, true, true, false, false, false, false, false];
        let assistance = [0; 8];
        let present = [true; 8];
        let millis = [1_000; 8];
        let contexts = [None; 8];
        let families = [None; 8];
        let episodes = [
            Some("les-8"),
            Some("les-7"),
            Some("les-6"),
            Some("les-5"),
            Some("les-4"),
            Some("les-3"),
            Some("les-2"),
            Some("les-1"),
        ];
        let now = 100 * DAY_SECONDS as i64;
        let at = [
            now,
            now - DAY_SECONDS as i64,
            now - 2 * DAY_SECONDS as i64,
            now - 95 * DAY_SECONDS as i64,
            now - 96 * DAY_SECONDS as i64,
            now - 97 * DAY_SECONDS as i64,
            now - 98 * DAY_SECONDS as i64,
            now - 99 * DAY_SECONDS as i64,
        ];

        let state = project_item_capabilities_columns(
            "li",
            "x",
            "learning",
            8,
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
            now,
        );

        assert!(
            state
                .spontaneous_production
                .unaided_success_rate
                .is_some_and(|rate| rate > 0.90),
            "ancient unaided failures must decay instead of permanently dominating current independence"
        );
    }

    #[test]
    fn unknown_assistance_does_not_build_independent_evidence_strength() {
        let capability = [CapabilityKind::SpontaneousProduction; 3];
        let success = [true; 3];
        let assistance = [0; 3];
        let assistance_present = [false; 3];
        let millis = [1_000; 3];
        let response_present = [true; 3];
        let contexts = [None; 3];
        let families = [None; 3];
        let episodes = [Some("les-3"), Some("les-2"), Some("les-1")];
        let at = [300_i64, 200, 100];

        let state = project_item_capabilities_columns(
            "li",
            "x",
            "learning",
            3,
            &capability,
            &success,
            &assistance,
            &assistance_present,
            &millis,
            &response_present,
            &contexts,
            &families,
            &episodes,
            &at,
            301,
        );

        assert_eq!(
            state.spontaneous_production.independent_evidence_episodes, 0,
            "unknown assistance cannot create independent learning episodes"
        );
        assert_eq!(state.spontaneous_production.evidence_strength, 0.0);
    }

    #[test]
    fn unknown_assistance_is_not_unaided() {
        let capability = [CapabilityKind::SpontaneousProduction];
        let success = [true];
        let assistance = [0];
        let assistance_present = [false];
        let millis = [0];
        let response_present = [false];
        let contexts = [None];
        let families = [None];
        let at = [1];
        let state = project_item_capabilities_columns(
            "li",
            "x",
            "learning",
            1,
            &capability,
            &success,
            &assistance,
            &assistance_present,
            &millis,
            &response_present,
            &contexts,
            &families,
            &[None],
            &at,
            2,
        );
        assert_eq!(state.spontaneous_production.unaided_success_rate, None);
        assert_eq!(state.spontaneous_production.qualified_success_streak, 0);
    }
}
