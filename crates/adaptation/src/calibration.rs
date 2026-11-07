//! Deterministic intervention-effect summaries from persisted planner snapshots.
//!
//! This does not auto-tune policy. It exposes delayed, observable effects so
//! thresholds and teaching methods can later be changed from evidence.

use crate::CapabilityKind;
use serde::{Deserialize, Serialize};

/// Maximum planner snapshots admitted into one calibration summary.
pub const MAX_CALIBRATION_SNAPSHOTS: usize = 10;
const MAX_CALIBRATION_ITEMS_PER_SNAPSHOT: usize = 200;
const MAX_CALIBRATION_INTERVENTIONS_PER_SNAPSHOT: usize = 100;
const MAX_CALIBRATION_UNIQUE_ITEMS: usize = MAX_CALIBRATION_SNAPSHOTS
    * (MAX_CALIBRATION_ITEMS_PER_SNAPSHOT + MAX_CALIBRATION_INTERVENTIONS_PER_SNAPSHOT);
const ITEM_KEY_INDEX_CAPACITY: usize = 8192;
const EMPTY_ITEM_KEY: u32 = u32::MAX;

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
    #[serde(default)]
    calibration_eligible: bool,
}
#[derive(Deserialize)]
struct CompactOutcome<'a> {
    #[serde(default)]
    calibration_valid: bool,
    #[serde(default)]
    completed_at_unix: Option<i64>,
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
    intervention_completed_at: Vec<i64>,
}

fn item_key_hash(value: &str) -> usize {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in value.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash as usize
}

fn intern_item_key<'a>(
    columns: &mut CalibrationColumns<'a>,
    index: &mut [u32; ITEM_KEY_INDEX_CAPACITY],
    id: std::borrow::Cow<'a, str>,
) -> Option<u32> {
    if columns.item_key_text.len() >= MAX_CALIBRATION_UNIQUE_ITEMS {
        return None;
    }
    let start = item_key_hash(id.as_ref()) & (ITEM_KEY_INDEX_CAPACITY - 1);
    for probe in 0..ITEM_KEY_INDEX_CAPACITY {
        let slot = (start + probe) & (ITEM_KEY_INDEX_CAPACITY - 1);
        let existing = index[slot];
        if existing == EMPTY_ITEM_KEY {
            let key = u32::try_from(columns.item_key_text.len()).ok()?;
            columns.item_key_text.push(id);
            index[slot] = key;
            return Some(key);
        }
        if columns.item_key_text[existing as usize].as_ref() == id.as_ref() {
            return Some(existing);
        }
    }
    None
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
    completed_at: i64,
) {
    columns.intervention_action.push(action);
    columns.intervention_item_key.push(item_key);
    columns
        .intervention_capability
        .push(crate::queue::evidence_capability(capability));
    columns.intervention_completed_at.push(completed_at);
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
    item_key_index: &mut [u32; ITEM_KEY_INDEX_CAPACITY],
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
    for item in items.into_iter().take(MAX_CALIBRATION_ITEMS_PER_SNAPSHOT) {
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
                if i == 1 || i >= 3 {
                    estimate.unaided_success_rate
                } else {
                    estimate.weighted_success_rate
                },
                estimate.last_observed_at_unix,
                estimate.distinct_success_families,
            )
        });
        let Some(key) = intern_item_key(columns, item_key_index, item.learning_item_id) else {
            continue;
        };
        push_item(columns, key, metrics);
    }
    if outcome.calibration_valid
        && let Some(completed_at) = outcome.completed_at_unix
    {
        for intervention in outcome
            .interventions
            .into_iter()
            .take(MAX_CALIBRATION_INTERVENTIONS_PER_SNAPSHOT)
        {
            if !intervention.calibration_eligible {
                continue;
            }
            let Some(key) = intern_item_key(columns, item_key_index, intervention.learning_item_id)
            else {
                continue;
            };
            push_intervention(
                columns,
                intervention.action,
                key,
                &intervention.target_capability,
                completed_at,
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
    // Keep only the newest bounded history regardless of caller iteration
    // order. MAX_CALIBRATION_SNAPSHOTS is a fixed constant, so admission is
    // O(total_rows) with bounded request-local storage.
    let mut recent: Vec<(i64, &'a str, &'a str)> = Vec::with_capacity(MAX_CALIBRATION_SNAPSHOTS);
    for row in rows {
        let position = recent.partition_point(|existing| existing.0 <= row.0);
        recent.insert(position, row);
        if recent.len() > MAX_CALIBRATION_SNAPSHOTS {
            recent.remove(0);
        }
    }

    let mut columns = CalibrationColumns::default();
    let mut item_key_index = [EMPTY_ITEM_KEY; ITEM_KEY_INDEX_CAPACITY];
    for (at, items, outcome) in recent {
        append_compact_snapshot(&mut columns, &mut item_key_index, at, items, outcome);
    }
    summarize_columns(&columns, minimum_gap_seconds)
}

fn capability_index(capability: CapabilityKind) -> usize {
    match capability {
        CapabilityKind::Recognition => 0,
        CapabilityKind::Listening => 1,
        CapabilityKind::PromptedProduction => 2,
        CapabilityKind::SpontaneousProduction => 3,
        CapabilityKind::Transfer => 4,
    }
}

// One chronological pass over indexed, columnar calibration data. Pending
// intervention state is addressed by dense item_id × capability slots, so
// delayed attribution never rescans future or intermediate snapshots.
fn summarize_columns(
    columns: &CalibrationColumns<'_>,
    minimum_gap_seconds: i64,
) -> Vec<InterventionEffect> {
    let mut ordered: Vec<u32> = (0..columns.snapshot_at.len())
        .map(|index| u32::try_from(index).unwrap_or(u32::MAX))
        .collect();
    ordered.sort_by_key(|&index| columns.snapshot_at[index as usize]);

    let slot_count = columns.item_key_text.len().saturating_mul(5);
    let mut pending_intervention = vec![u32::MAX; slot_count];
    let mut pending_before_metric = vec![u32::MAX; slot_count];

    // Resolved samples remain columnar until the final grouping pass.
    let mut resolved_intervention: Vec<u32> = Vec::new();
    let mut resolved_before_metric: Vec<u32> = Vec::new();
    let mut resolved_after_metric: Vec<u32> = Vec::new();

    for &snapshot_index in &ordered {
        let snapshot = snapshot_index as usize;
        let item_start = if snapshot == 0 {
            0
        } else {
            columns.item_end[snapshot - 1] as usize
        };
        let item_end = columns.item_end[snapshot] as usize;
        let snapshot_items = &columns.item_order[item_start..item_end];

        // Resolve older pending interventions from this pre-lesson snapshot.
        // For duplicate legacy item rows, preserve find_item semantics by
        // considering only the last row for each dense item key.
        for position in 0..snapshot_items.len() {
            let item = snapshot_items[position] as usize;
            let item_key = columns.item_key[item];
            if snapshot_items
                .get(position + 1)
                .is_some_and(|&next| columns.item_key[next as usize] == item_key)
            {
                continue;
            }

            for cap in 0..5 {
                let slot = item_key as usize * 5 + cap;
                let intervention = pending_intervention[slot];
                if intervention == u32::MAX {
                    continue;
                }
                let metric = item * 5 + cap;
                let completed_at = columns.intervention_completed_at[intervention as usize];
                let observed_at = columns.item_observed_at[metric].unwrap_or(i64::MIN);
                if observed_at > completed_at
                    && observed_at <= columns.snapshot_at[snapshot]
                    && observed_at.saturating_sub(completed_at) >= minimum_gap_seconds
                {
                    resolved_intervention.push(intervention);
                    resolved_before_metric.push(pending_before_metric[slot]);
                    resolved_after_metric.push(u32::try_from(metric).unwrap_or(u32::MAX));
                    pending_intervention[slot] = u32::MAX;
                    pending_before_metric[slot] = u32::MAX;
                }
            }
        }

        let intervention_start = if snapshot == 0 {
            0
        } else {
            columns.intervention_end[snapshot - 1] as usize
        };
        let intervention_end = columns.intervention_end[snapshot] as usize;
        let mut interventions: Vec<u32> = (intervention_start..intervention_end)
            .filter(|&index| columns.intervention_capability[index].is_some())
            .map(|index| u32::try_from(index).unwrap_or(u32::MAX))
            .collect();
        interventions.sort_unstable_by(|&a, &b| {
            let a = a as usize;
            let b = b as usize;
            (
                columns.intervention_item_key[a],
                columns.intervention_capability[a],
                columns.intervention_action[a].as_ref(),
                a,
            )
                .cmp(&(
                    columns.intervention_item_key[b],
                    columns.intervention_capability[b],
                    columns.intervention_action[b].as_ref(),
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

        let mut position = 0usize;
        while position < interventions.len() {
            let intervention = interventions[position] as usize;
            let Some(capability) = columns.intervention_capability[intervention] else {
                position += 1;
                continue;
            };
            let item_key = columns.intervention_item_key[intervention];
            let cap = capability_index(capability);
            let slot = item_key as usize * 5 + cap;

            let mut group_end = position + 1;
            while group_end < interventions.len() {
                let other = interventions[group_end] as usize;
                if columns.intervention_item_key[other] != item_key
                    || columns.intervention_capability[other] != Some(capability)
                {
                    break;
                }
                group_end += 1;
            }

            // Any newer intervention supersedes an unresolved older one for the
            // same target. Multiple distinct actions in one lesson are
            // confounded, so no new attribution starts for that target.
            pending_intervention[slot] = u32::MAX;
            pending_before_metric[slot] = u32::MAX;
            if group_end == position + 1
                && let Some(before_item) = find_item(columns, snapshot, item_key)
            {
                pending_intervention[slot] = u32::try_from(intervention).unwrap_or(u32::MAX);
                pending_before_metric[slot] =
                    u32::try_from(before_item * 5 + cap).unwrap_or(u32::MAX);
            }
            position = group_end;
        }
    }

    let mut resolved_order: Vec<u32> = (0..resolved_intervention.len())
        .map(|index| u32::try_from(index).unwrap_or(u32::MAX))
        .collect();
    resolved_order.sort_unstable_by(|&a, &b| {
        let ia = resolved_intervention[a as usize] as usize;
        let ib = resolved_intervention[b as usize] as usize;
        (
            columns.intervention_action[ia].as_ref(),
            columns.intervention_capability[ia],
            a,
        )
            .cmp(&(
                columns.intervention_action[ib].as_ref(),
                columns.intervention_capability[ib],
                b,
            ))
    });

    let mut output = Vec::new();
    let mut position = 0usize;
    while position < resolved_order.len() {
        let first_sample = resolved_order[position] as usize;
        let first_intervention = resolved_intervention[first_sample] as usize;
        let action = columns.intervention_action[first_intervention].as_ref();
        let Some(capability) = columns.intervention_capability[first_intervention] else {
            position += 1;
            continue;
        };

        let mut samples = 0_u32;
        let mut newly_observed = 0_u32;
        let mut improved = 0_u32;
        let mut stable = 0_u32;
        let mut declined = 0_u32;
        let mut delta_sum = 0.0_f64;
        let mut transfer_family_gains = 0_u32;
        let mut group_end = position;

        while group_end < resolved_order.len() {
            let sample = resolved_order[group_end] as usize;
            let intervention = resolved_intervention[sample] as usize;
            if columns.intervention_action[intervention].as_ref() != action
                || columns.intervention_capability[intervention] != Some(capability)
            {
                break;
            }

            let before_metric = resolved_before_metric[sample] as usize;
            let after_metric = resolved_after_metric[sample] as usize;
            if capability == CapabilityKind::Transfer {
                transfer_family_gains = transfer_family_gains.saturating_add(u32::from(
                    columns.item_families[after_metric] > columns.item_families[before_metric],
                ));
            }
            match (
                columns.item_rate[before_metric],
                columns.item_rate[after_metric],
            ) {
                (Some(before_rate), Some(after_rate)) => {
                    let delta = after_rate - before_rate;
                    samples = samples.saturating_add(1);
                    delta_sum += delta;
                    if delta > 0.08 {
                        improved = improved.saturating_add(1);
                    } else if delta < -0.08 {
                        declined = declined.saturating_add(1);
                    } else {
                        stable = stable.saturating_add(1);
                    }
                }
                (None, Some(_)) => {
                    newly_observed = newly_observed.saturating_add(1);
                }
                _ => {}
            }
            group_end += 1;
        }

        output.push(InterventionEffect {
            action: action.to_owned(),
            capability,
            samples,
            newly_observed,
            improved,
            stable,
            declined,
            mean_success_delta: (samples > 0).then_some(delta_sum / f64::from(samples)),
            transfer_family_gains,
        });
        position = group_end;
    }

    output.sort_by(|left, right| {
        let left_n = left.samples.saturating_add(left.newly_observed);
        let right_n = right.samples.saturating_add(right.newly_observed);
        right_n
            .cmp(&left_n)
            .then_with(|| right.improved.cmp(&left.improved))
            .then_with(|| left.action.cmp(&right.action))
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
        "spontaneous_production":{"unaided_success_rate":0.8,"last_observed_at_unix":30000},
        "transfer":{}
      }
    ]"#;
    const OUTCOME: &str = r#"{
      "completed_at_unix":1000,
      "calibration_valid":true,
      "interventions":[
        {"action":"fade_prompts","learning_item_id":"li-1","target_capability":"spontaneous_production","calibration_eligible":true}
      ]
    }"#;

    #[test]
    fn legacy_intervention_without_completion_timestamp_does_not_calibrate() {
        let legacy = r#"{
          "calibration_valid":true,
          "interventions":[
            {"action":"fade_prompts","learning_item_id":"li-1","target_capability":"spontaneous_production","calibration_eligible":true}
          ]
        }"#;
        let rows = [(0, EMPTY, legacy), (30_000, IMPROVED, "{}")];
        assert!(
            summarize_intervention_history(rows, 21_600).is_empty(),
            "legacy outcomes without an intervention completion timestamp must fail closed"
        );
    }

    #[test]
    fn legacy_intervention_without_explicit_eligibility_does_not_calibrate() {
        let legacy = r#"{
          "calibration_valid":true,
          "interventions":[
            {"action":"fade_prompts","learning_item_id":"li-1","target_capability":"spontaneous_production"}
          ]
        }"#;
        let rows = [(0, EMPTY, legacy), (30_000, IMPROVED, "{}")];
        assert!(
            summarize_intervention_history(rows, 21_600).is_empty(),
            "legacy interventions without explicit calibration eligibility must fail closed"
        );
    }

    #[test]
    fn same_lesson_diagnostic_does_not_consume_pending_delayed_calibration() {
        let before = r#"[
          {
            "learning_item_id":"li-1",
            "recognition":{},
            "listening":{},
            "prompted_production":{},
            "spontaneous_production":{"unaided_success_rate":0.4,"last_observed_at_unix":10},
            "transfer":{}
          }
        ]"#;
        let same_lesson_diagnostic = r#"[
          {
            "learning_item_id":"li-1",
            "recognition":{},
            "listening":{},
            "prompted_production":{},
            "spontaneous_production":{"unaided_success_rate":0.5,"last_observed_at_unix":1000},
            "transfer":{}
          }
        ]"#;
        let delayed = r#"[
          {
            "learning_item_id":"li-1",
            "recognition":{},
            "listening":{},
            "prompted_production":{},
            "spontaneous_production":{"unaided_success_rate":0.8,"last_observed_at_unix":30000},
            "transfer":{}
          }
        ]"#;
        let outcome = r#"{
          "completed_at_unix":1000,
          "calibration_valid":true,
          "interventions":[
            {"action":"fade_prompts","learning_item_id":"li-1","target_capability":"spontaneous_production","calibration_eligible":true}
          ]
        }"#;
        let effects = summarize_intervention_history(
            [
                (0, before, outcome),
                (10_000, same_lesson_diagnostic, "{}"),
                (40_000, delayed, "{}"),
            ],
            21_600,
        );
        assert_eq!(effects.len(), 1);
        assert_eq!(effects[0].samples, 1);
        assert_eq!(effects[0].improved, 1);
        assert_eq!(effects[0].mean_success_delta, Some(0.4));
    }

    #[test]
    fn later_same_target_intervention_supersedes_pending_calibration_credit() {
        let before = r#"[
          {
            "learning_item_id":"li-1",
            "recognition":{},
            "listening":{},
            "prompted_production":{},
            "spontaneous_production":{"unaided_success_rate":0.4,"last_observed_at_unix":10},
            "transfer":{}
          }
        ]"#;
        let before_second = r#"[
          {
            "learning_item_id":"li-1",
            "recognition":{},
            "listening":{},
            "prompted_production":{},
            "spontaneous_production":{"unaided_success_rate":0.4,"last_observed_at_unix":1000},
            "transfer":{}
          }
        ]"#;
        let delayed = r#"[
          {
            "learning_item_id":"li-1",
            "recognition":{},
            "listening":{},
            "prompted_production":{},
            "spontaneous_production":{"unaided_success_rate":0.8,"last_observed_at_unix":35000},
            "transfer":{}
          }
        ]"#;
        let first = r#"{
          "completed_at_unix":1000,
          "calibration_valid":true,
          "interventions":[
            {"action":"contrast","learning_item_id":"li-1","target_capability":"spontaneous_production","calibration_eligible":true}
          ]
        }"#;
        let second = r#"{
          "completed_at_unix":11000,
          "calibration_valid":true,
          "interventions":[
            {"action":"semantic_reframe","learning_item_id":"li-1","target_capability":"spontaneous_production","calibration_eligible":true}
          ]
        }"#;

        let effects = summarize_intervention_history(
            [
                (0, before, first),
                (10_000, before_second, second),
                (40_000, delayed, "{}"),
            ],
            21_600,
        );
        assert_eq!(effects.len(), 1);
        assert_eq!(effects[0].action, "semantic_reframe");
        assert_eq!(effects[0].samples, 1);
    }

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
    fn calibration_uses_newest_bounded_snapshots_independent_of_input_order() {
        let mut rows = vec![
            (0, EMPTY, "{}"),
            (1_000, EMPTY, OUTCOME),
            (2_000, EMPTY, "{}"),
            (3_000, EMPTY, "{}"),
            (4_000, EMPTY, "{}"),
            (5_000, EMPTY, "{}"),
            (6_000, EMPTY, "{}"),
            (7_000, EMPTY, "{}"),
            (8_000, EMPTY, "{}"),
            (9_000, EMPTY, "{}"),
            (40_000, IMPROVED, "{}"),
        ];

        let ascending = summarize_intervention_history(rows.iter().copied(), 21_600);
        rows.reverse();
        let descending = summarize_intervention_history(rows.iter().copied(), 21_600);

        assert_eq!(ascending, descending);
        assert_eq!(ascending.len(), 1);
        assert_eq!(ascending[0].action, "fade_prompts");
        assert_eq!(ascending[0].samples, 1);
        assert_eq!(ascending[0].improved, 1);
    }

    #[test]
    fn listening_calibration_uses_unaided_success() {
        let before = r#"[
          {
            "learning_item_id":"li-1",
            "recognition":{},
            "listening":{"weighted_success_rate":0.9,"unaided_success_rate":0.2,"last_observed_at_unix":10},
            "prompted_production":{},
            "spontaneous_production":{},
            "transfer":{}
          }
        ]"#;
        let after = r#"[
          {
            "learning_item_id":"li-1",
            "recognition":{},
            "listening":{"weighted_success_rate":0.9,"unaided_success_rate":0.8,"last_observed_at_unix":30000},
            "prompted_production":{},
            "spontaneous_production":{},
            "transfer":{}
          }
        ]"#;
        let outcome = r#"{
          "completed_at_unix":1000,
          "calibration_valid":true,
          "interventions":[
            {"action":"listening_probe","learning_item_id":"li-1","target_capability":"listening","calibration_eligible":true}
          ]
        }"#;
        let effects =
            summarize_intervention_history([(0, before, outcome), (30_000, after, "{}")], 21_600);
        assert_eq!(effects.len(), 1);
        assert_eq!(effects[0].improved, 1);
        assert_eq!(effects[0].mean_success_delta, Some(0.6000000000000001));
    }

    #[test]
    fn ambiguous_same_item_capability_interventions_do_not_claim_effectiveness() {
        let outcome = r#"{
          "completed_at_unix":1000,
          "calibration_valid":true,
          "interventions":[
            {"action":"fade_prompts","learning_item_id":"li-1","target_capability":"spontaneous_production","calibration_eligible":true},
            {"action":"automaticity_drill","learning_item_id":"li-1","target_capability":"spontaneous_production","calibration_eligible":true}
          ]
        }"#;
        let rows = [(0, EMPTY, outcome), (30_000, IMPROVED, "{}")];
        assert!(summarize_intervention_history(rows, 21_600).is_empty());
    }

    #[test]
    fn intervention_without_same_target_diagnostic_does_not_calibrate() {
        let outcome = r#"{
          "completed_at_unix":1000,
          "calibration_valid":true,
          "interventions":[
            {"action":"fade_prompts","learning_item_id":"li-1","target_capability":"spontaneous_production","calibration_eligible":false}
          ]
        }"#;
        let rows = [(0, EMPTY, outcome), (30_000, IMPROVED, "{}")];
        assert!(summarize_intervention_history(rows, 21_600).is_empty());
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
            {"action":"fade_prompts","learning_item_id":"li-1","target_capability":"spontaneous_production","calibration_eligible":true}
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
