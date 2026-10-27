//! Deterministic intervention-effect summaries from persisted planner snapshots.
//!
//! This does not auto-tune policy. It exposes delayed, observable effects so
//! thresholds and teaching methods can later be changed from evidence.

use crate::CapabilityKind;
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
struct CompactEstimate {
    #[serde(default)]
    weighted_success_rate: Option<f64>,
    #[serde(default)]
    unaided_success_rate: Option<f64>,
    #[serde(default)]
    last_observed_at_unix: Option<i64>,
    #[serde(default)]
    distinct_success_families: u16,
}
#[derive(Deserialize)]
struct CompactItem<'a> {
    #[serde(borrow)]
    learning_item_id: std::borrow::Cow<'a, str>,
    recognition: CompactEstimate,
    listening: CompactEstimate,
    prompted_production: CompactEstimate,
    spontaneous_production: CompactEstimate,
    transfer: CompactEstimate,
}
#[derive(Deserialize)]
struct CompactIntervention<'a> {
    #[serde(borrow)]
    action: std::borrow::Cow<'a, str>,
    learning_item_id: std::borrow::Cow<'a, str>,
    target_capability: std::borrow::Cow<'a, str>,
}
#[derive(Deserialize)]
struct CompactOutcome<'a> {
    #[serde(default)]
    calibration_valid: bool,
    #[serde(borrow, default)]
    interventions: Vec<CompactIntervention<'a>>,
}
// Request-local Structure of Arrays. Compact JSON is decoded at the boundary;
// the core pass operates only on dense u32 item/candidate IDs and column ranges.
#[derive(Default)]
struct CalibrationColumns<'a> {
    snapshot_at: Vec<i64>,
    item_end: Vec<u32>,
    intervention_end: Vec<u32>,
    item_key_text: Vec<std::borrow::Cow<'a, str>>,
    item_key: Vec<u32>,
    item_rate: Vec<Option<f64>>,
    item_observed_at: Vec<Option<i64>>,
    item_families: Vec<u16>,
    item_order: Vec<u32>,
    intervention_action: Vec<std::borrow::Cow<'a, str>>,
    intervention_item_key: Vec<u32>,
    intervention_capability: Vec<Option<CapabilityKind>>,
}

fn intern_item_key<'a>(columns: &mut CalibrationColumns<'a>, id: std::borrow::Cow<'a, str>) -> u32 {
    if let Some(index) = columns
        .item_key_text
        .iter()
        .position(|existing| existing.as_ref() == id.as_ref())
    {
        return u32::try_from(index).unwrap_or(u32::MAX);
    }
    let key = u32::try_from(columns.item_key_text.len()).unwrap_or(u32::MAX);
    columns.item_key_text.push(id);
    key
}

fn push_item(
    columns: &mut CalibrationColumns<'_>,
    key: u32,
    metrics: [(Option<f64>, Option<i64>, u16); 5],
) {
    columns.item_key.push(key);
    for (rate, observed_at, families) in metrics {
        columns.item_rate.push(rate);
        columns.item_observed_at.push(observed_at);
        columns.item_families.push(families);
    }
}

fn push_intervention<'a>(
    columns: &mut CalibrationColumns<'a>,
    action: std::borrow::Cow<'a, str>,
    item_key: u32,
    capability: &str,
) {
    columns.intervention_action.push(action);
    columns.intervention_item_key.push(item_key);
    columns
        .intervention_capability
        .push(crate::queue::evidence_capability(capability));
}

fn finish_snapshot(columns: &mut CalibrationColumns<'_>, at: i64) {
    let item_start = columns.item_end.last().copied().unwrap_or(0);
    // Bounded local history; exceeding u32 would require billions of decoded items.
    let item_end = u32::try_from(columns.item_key.len()).unwrap_or(u32::MAX);
    columns.item_order.extend(item_start..item_end);
    columns.item_order[item_start as usize..item_end as usize].sort_unstable_by(|&a, &b| {
        columns.item_key[a as usize]
            .cmp(&columns.item_key[b as usize])
            .then_with(|| a.cmp(&b))
    });
    columns.item_end.push(item_end);
    columns
        .intervention_end
        .push(u32::try_from(columns.intervention_action.len()).unwrap_or(u32::MAX));
    columns.snapshot_at.push(at);
}

fn append_compact_snapshot<'a>(
    columns: &mut CalibrationColumns<'a>,
    at: i64,
    items_json: &'a str,
    outcome_json: &'a str,
) {
    let (Ok(items), Ok(outcome)): (
        Result<Vec<CompactItem<'a>>, _>,
        Result<CompactOutcome<'a>, _>,
    ) = (
        serde_json::from_str(items_json),
        serde_json::from_str(outcome_json),
    ) else {
        return;
    };
    for item in items {
        let estimates = [
            item.recognition,
            item.listening,
            item.prompted_production,
            item.spontaneous_production,
            item.transfer,
        ];
        let metrics = std::array::from_fn(|i| {
            let estimate = &estimates[i];
            (
                if i >= 3 {
                    estimate.unaided_success_rate
                } else {
                    estimate.weighted_success_rate
                },
                estimate.last_observed_at_unix,
                estimate.distinct_success_families,
            )
        });
        let key = intern_item_key(columns, item.learning_item_id);
        push_item(columns, key, metrics);
    }
    if outcome.calibration_valid {
        for intervention in outcome.interventions {
            let key = intern_item_key(columns, intervention.learning_item_id);
            push_intervention(
                columns,
                intervention.action,
                key,
                &intervention.target_capability,
            );
        }
    }
    finish_snapshot(columns, at);
}

fn find_item(columns: &CalibrationColumns<'_>, snapshot: usize, key: u32) -> Option<usize> {
    let start = if snapshot == 0 {
        0
    } else {
        columns.item_end[snapshot - 1] as usize
    };
    let end = columns.item_end[snapshot] as usize;
    let ordered = &columns.item_order[start..end];
    let upper = ordered.partition_point(|&index| columns.item_key[index as usize] <= key);
    let index = *ordered.get(upper.checked_sub(1)?)? as usize;
    (columns.item_key[index] == key).then_some(index)
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

/// Calibrate directly from persisted JSON; malformed snapshots are omitted.
#[must_use]
pub fn summarize_intervention_history<'a>(
    rows: impl IntoIterator<Item = (i64, &'a str, &'a str)>,
    minimum_gap_seconds: i64,
) -> Vec<InterventionEffect> {
    let mut columns = CalibrationColumns::default();
    for (at, items, outcome) in rows {
        append_compact_snapshot(&mut columns, at, items, outcome);
    }
    summarize_columns(&columns, minimum_gap_seconds)
}

// One pure accumulation pass over indexed, columnar calibration data.
fn summarize_columns(
    columns: &CalibrationColumns<'_>,
    minimum_gap_seconds: i64,
) -> Vec<InterventionEffect> {
    let mut ordered: Vec<u32> = (0..columns.snapshot_at.len())
        .map(|index| u32::try_from(index).unwrap_or(u32::MAX))
        .collect();
    ordered.sort_by_key(|&index| columns.snapshot_at[index as usize]);

    // Accumulation columns share dense u32 bucket indices. Only final results
    // allocate owned action strings.
    let mut action: Vec<&str> = Vec::new();
    let mut kind: Vec<CapabilityKind> = Vec::new();
    let mut samples: Vec<u32> = Vec::new();
    let mut newly_observed: Vec<u32> = Vec::new();
    let mut improved: Vec<u32> = Vec::new();
    let mut stable: Vec<u32> = Vec::new();
    let mut declined: Vec<u32> = Vec::new();
    let mut delta_sum: Vec<f64> = Vec::new();
    let mut transfer_family_gains: Vec<u32> = Vec::new();

    for pair in ordered.windows(2) {
        let before = pair[0] as usize;
        let after = pair[1] as usize;
        if columns.snapshot_at[after].saturating_sub(columns.snapshot_at[before])
            < minimum_gap_seconds
        {
            continue;
        }
        let start = if before == 0 {
            0
        } else {
            columns.intervention_end[before - 1] as usize
        };
        let end = columns.intervention_end[before] as usize;

        // Contiguous intervention IDs: O(C log C) deduplication, then replay
        // in original order to preserve deterministic accumulation semantics.
        let mut interventions: Vec<u32> = (start..end)
            .filter(|&index| columns.intervention_capability[index].is_some())
            .map(|index| u32::try_from(index).unwrap_or(u32::MAX))
            .collect();
        interventions.sort_unstable_by(|&a, &b| {
            let a = a as usize;
            let b = b as usize;
            (
                columns.intervention_action[a].as_ref(),
                columns.intervention_item_key[a],
                columns.intervention_capability[a],
                a,
            )
                .cmp(&(
                    columns.intervention_action[b].as_ref(),
                    columns.intervention_item_key[b],
                    columns.intervention_capability[b],
                    b,
                ))
        });
        interventions.dedup_by(|a, b| {
            let a = *a as usize;
            let b = *b as usize;
            columns.intervention_action[a] == columns.intervention_action[b]
                && columns.intervention_item_key[a] == columns.intervention_item_key[b]
                && columns.intervention_capability[a] == columns.intervention_capability[b]
        });
        interventions.sort_unstable();
        for intervention in interventions {
            let intervention = intervention as usize;
            let Some(capability) = columns.intervention_capability[intervention] else {
                continue;
            };
            let intervention_action = columns.intervention_action[intervention].as_ref();
            let intervention_item = columns.intervention_item_key[intervention];
            let (Some(before_item), Some(after_item)) = (
                find_item(columns, before, intervention_item),
                find_item(columns, after, intervention_item),
            ) else {
                continue;
            };
            let capability_index = match capability {
                CapabilityKind::Recognition => 0,
                CapabilityKind::Listening => 1,
                CapabilityKind::PromptedProduction => 2,
                CapabilityKind::SpontaneousProduction => 3,
                CapabilityKind::Transfer => 4,
            };
            let before_metric = before_item * 5 + capability_index;
            let after_metric = after_item * 5 + capability_index;
            let before_at = columns.item_observed_at[before_metric].unwrap_or(i64::MIN);
            let after_at = columns.item_observed_at[after_metric].unwrap_or(i64::MIN);
            if after_at <= before_at {
                continue;
            }

            let bucket = if let Some(index) =
                action.iter().zip(&kind).position(|(name, existing)| {
                    *name == intervention_action && *existing == capability
                }) {
                index
            } else {
                let index = action.len();
                action.push(intervention_action);
                kind.push(capability);
                samples.push(0);
                newly_observed.push(0);
                improved.push(0);
                stable.push(0);
                declined.push(0);
                delta_sum.push(0.0);
                transfer_family_gains.push(0);
                index
            };

            if capability == CapabilityKind::Transfer {
                transfer_family_gains[bucket] =
                    transfer_family_gains[bucket].saturating_add(u32::from(
                        columns.item_families[after_metric] > columns.item_families[before_metric],
                    ));
            }
            match (
                columns.item_rate[before_metric],
                columns.item_rate[after_metric],
            ) {
                (Some(before_rate), Some(after_rate)) => {
                    let delta = after_rate - before_rate;
                    samples[bucket] = samples[bucket].saturating_add(1);
                    delta_sum[bucket] += delta;
                    if delta > 0.08 {
                        improved[bucket] = improved[bucket].saturating_add(1);
                    } else if delta < -0.08 {
                        declined[bucket] = declined[bucket].saturating_add(1);
                    } else {
                        stable[bucket] = stable[bucket].saturating_add(1);
                    }
                }
                (None, Some(_)) => {
                    newly_observed[bucket] = newly_observed[bucket].saturating_add(1);
                }
                _ => {}
            }
        }
    }

    let mut output: Vec<_> = (0..action.len())
        .map(|index| InterventionEffect {
            action: action[index].to_owned(),
            capability: kind[index],
            samples: samples[index],
            newly_observed: newly_observed[index],
            improved: improved[index],
            stable: stable[index],
            declined: declined[index],
            mean_success_delta: (samples[index] > 0)
                .then_some(delta_sum[index] / f64::from(samples[index])),
            transfer_family_gains: transfer_family_gains[index],
        })
        .collect();
    output.sort_by(|left, right| {
        let left_n = left.samples.saturating_add(left.newly_observed);
        let right_n = right.samples.saturating_add(right.newly_observed);
        right_n
            .cmp(&left_n)
            .then_with(|| right.improved.cmp(&left.improved))
            .then_with(|| left.action.cmp(&right.action))
            // The previous BTreeMap ordered otherwise tied buckets by capability.
            .then_with(|| left.capability.cmp(&right.capability))
    });
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    const EMPTY: &str = r#"[
      {
        "learning_item_id":"li-1",
        "recognition":{},
        "listening":{},
        "prompted_production":{},
        "spontaneous_production":{"unaided_success_rate":0.4,"last_observed_at_unix":10},
        "transfer":{}
      }
    ]"#;
    const IMPROVED: &str = r#"[
      {
        "learning_item_id":"li-1",
        "recognition":{},
        "listening":{},
        "prompted_production":{},
        "spontaneous_production":{"unaided_success_rate":0.8,"last_observed_at_unix":30},
        "transfer":{}
      }
    ]"#;
    const OUTCOME: &str = r#"{
      "calibration_valid":true,
      "interventions":[
        {"action":"fade_prompts","learning_item_id":"li-1","target_capability":"spontaneous_production"}
      ]
    }"#;

    #[test]
    fn compact_history_detects_improvement() {
        let rows = [(0, EMPTY, OUTCOME), (30_000, IMPROVED, "{}")];
        let effects = summarize_intervention_history(rows, 21_600);
        assert_eq!(effects.len(), 1);
        assert_eq!(effects[0].action, "fade_prompts");
        assert_eq!(effects[0].capability, CapabilityKind::SpontaneousProduction);
        assert_eq!(effects[0].samples, 1);
        assert_eq!(effects[0].improved, 1);
        assert_eq!(effects[0].mean_success_delta, Some(0.4));
    }

    #[test]
    fn planned_candidates_without_actual_intervention_do_not_calibrate() {
        let no_actual = r#"{"calibration_valid":true,"interventions":[]}"#;
        let rows = [(0, EMPTY, no_actual), (30_000, IMPROVED, "{}")];
        assert!(summarize_intervention_history(rows, 21_600).is_empty());
    }

    #[test]
    fn measurement_invalid_lesson_does_not_calibrate() {
        let invalid = r#"{
          "calibration_valid":false,
          "interventions":[
            {"action":"fade_prompts","learning_item_id":"li-1","target_capability":"spontaneous_production"}
          ]
        }"#;
        let rows = [(0, EMPTY, invalid), (30_000, IMPROVED, "{}")];
        assert!(summarize_intervention_history(rows, 21_600).is_empty());
    }

    #[test]
    fn malformed_or_too_close_snapshots_are_ignored() {
        let malformed = [(0, "{", OUTCOME), (30_000, IMPROVED, "{}")];
        assert!(summarize_intervention_history(malformed, 21_600).is_empty());

        let close = [(0, EMPTY, OUTCOME), (100, IMPROVED, "{}")];
        assert!(summarize_intervention_history(close, 21_600).is_empty());
    }
}
