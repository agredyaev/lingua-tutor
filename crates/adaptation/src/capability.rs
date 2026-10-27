//! Item-level capability projections derived from immutable learning evidence.
//!
//! SQLite/wire text stays at the boundary. The projection pass consumes parallel
//! evidence columns and returns one bounded borrowed state; it does not retain an
//! owned entity graph or a second source of truth.

use serde::{Deserialize, Serialize};

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
pub fn context_set_slice<'store, 'text>(
    set: &'store ContextSetView<'text>,
) -> &'store [&'text str] {
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
    /// Normalized evidence strength.
    pub evidence_strength: f64,
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
            qualified_success_streak: 0,
            distinct_success_contexts: 0,
            distinct_success_families: 0,
            successful_contexts: ContextSetView::default(),
            successful_context_families: ContextSetView::default(),
            evidence_strength: 0.0,
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
    let mut weighted_latency_total = 0.0;
    let mut weighted_latency = 0.0;
    let mut unaided_attempts = 0_u32;
    let mut unaided_successes = 0_u32;
    let mut contexts = ContextSetView::default();
    let mut families = ContextSetView::default();
    let mut streak = 0_u8;
    let mut streak_open = true;

    for (order, source) in samples[..sample_len].iter().copied().enumerate() {
        let qualified = qualified_success(
            kind,
            success[source],
            assistance_present[source].then_some(assistance_level[source]),
            context_family[source],
        );
        if streak_open {
            if qualified {
                streak = streak.saturating_add(1);
            } else {
                streak_open = false;
            }
        }

        let order_weight = 0.90_f64.powi(i32::try_from(order).unwrap_or(i32::MAX));
        let age_seconds = now_unix.saturating_sub(recorded_at_unix[source]).max(0) as f64;
        let age_weight = 0.5_f64.powf((age_seconds / DAY_SECONDS) / half_life_days(kind));
        let weight = order_weight * age_weight;

        weighted_total += weight;
        if success[source] {
            weighted_success += weight;
            context_set_insert(&mut contexts, context_key[source]);
            context_set_insert(&mut families, context_family[source]);
        }
        if assistance_present[source] && assistance_level[source] == 0 {
            unaided_attempts = unaided_attempts.saturating_add(1);
            if success[source] {
                unaided_successes = unaided_successes.saturating_add(1);
            }
        }
        if response_present[source] {
            weighted_latency_total += weight;
            weighted_latency += weight * f64::from(response_millis[source]);
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
        unaided_success_rate: (unaided_attempts > 0)
            .then_some(f64::from(unaided_successes) / f64::from(unaided_attempts)),
        latency_weighted_millis,
        qualified_success_streak: streak,
        distinct_success_contexts: u16::try_from(context_set_slice(&contexts).len())
            .unwrap_or(u16::MAX),
        distinct_success_families: u16::try_from(context_set_slice(&families).len())
            .unwrap_or(u16::MAX),
        successful_contexts: contexts,
        successful_context_families: families,
        evidence_strength: (weighted_total / 4.0).min(1.0),
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
        CapabilityKind::Recognition | CapabilityKind::Listening => true,
        CapabilityKind::PromptedProduction => assistance.is_some_and(|level| level > 0),
        CapabilityKind::SpontaneousProduction => assistance == Some(0),
        CapabilityKind::Transfer => assistance == Some(0) && non_empty(family).is_some(),
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
    /// Evidence strength.
    pub evidence_strength: f64,
    /// Newest observation time.
    pub last_observed_at_unix: Option<i64>,
    /// Newest observation success.
    pub last_observed_success: Option<bool>,
    /// Newest observation qualification.
    pub last_observed_qualified_success: Option<bool>,
}

/// Project one fixed estimate into its borrowed downstream view.
#[must_use]
pub fn capability_estimate_view<'a>(
    value: &'a CapabilityEstimateFixed<'a>,
) -> CapabilityEstimateView<'a> {
    CapabilityEstimateView {
        attempts: value.attempts,
        successes: value.successes,
        weighted_success_rate: value.weighted_success_rate,
        unaided_success_rate: value.unaided_success_rate,
        latency_weighted_millis: value.latency_weighted_millis,
        qualified_success_streak: value.qualified_success_streak,
        distinct_success_contexts: value.distinct_success_contexts,
        distinct_success_families: value.distinct_success_families,
        successful_contexts: context_set_slice(&value.successful_contexts),
        successful_context_families: context_set_slice(&value.successful_context_families),
        evidence_strength: value.evidence_strength,
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
        let at = [300, 200, 100];

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
            &at,
            400,
        );
        assert_eq!(state.transfer.attempts, 2);
        assert_eq!(state.transfer.qualified_success_streak, 2);
        assert_eq!(
            context_set_slice(&state.transfer.successful_context_families),
            ["family-a", "family-b"]
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
            &at,
            20,
        );
        assert_eq!(state.listening.attempts, 12);
        assert_eq!(state.listening.successes, 12);
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
            &at,
            2,
        );
        assert_eq!(state.spontaneous_production.unaided_success_rate, None);
        assert_eq!(state.spontaneous_production.qualified_success_streak, 0);
    }
}
