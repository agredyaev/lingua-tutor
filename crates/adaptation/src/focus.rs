//! Spaced tutor-focus threads derived from item capability evidence.
//!
//! Focus threads apply Anki-like spacing to live tutoring without duplicating
//! the card scheduler. They exist only for introduced/active learning items.

use crate::{CapabilityEstimate, CapabilityKind, ItemCapabilityState};
use serde::Serialize;

const DAY_SECONDS: i64 = 86_400;

/// One derived tutor focus thread.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TutorFocusThread {
    /// Learning item this focus thread belongs to.
    pub learning_item_id: String,
    /// Target-language text for tutor orientation.
    pub target_text: String,
    /// Capability that remains insufficiently consolidated.
    pub capability: CapabilityKind,
    /// Spacing stage 0..=4; larger means the skill survived longer gaps.
    pub stage: u8,
    /// Next time this skill should be probed by the tutor.
    pub due_at_unix: i64,
    /// Whether this thread should be considered in the current lesson.
    pub due_now: bool,
    /// Successful concrete contexts already observed.
    pub successful_contexts: Vec<String>,
    /// Successful semantic context families already observed.
    pub successful_context_families: Vec<String>,
    /// Human-readable continuation rule for the tutor.
    pub next_context_policy: String,
    /// Evidence-based explanation for why the thread remains active.
    pub rationale: String,
}

/// Derives active focus threads from item state. Fully consolidated skills and
/// not-yet-introduced candidates are omitted.
#[must_use]
pub fn build_tutor_focus_threads(
    items: &[ItemCapabilityState],
    now_unix: i64,
    limit: usize,
) -> Vec<TutorFocusThread> {
    let mut threads = Vec::new();
    for item in items.iter().filter(|item| focusable_status(&item.status)) {
        add_if_needed(
            &mut threads,
            item,
            CapabilityKind::Listening,
            &item.listening,
            now_unix,
        );
        add_if_needed(
            &mut threads,
            item,
            CapabilityKind::SpontaneousProduction,
            &item.spontaneous_production,
            now_unix,
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
            add_if_needed(
                &mut threads,
                item,
                CapabilityKind::Transfer,
                &item.transfer,
                now_unix,
            );
        }
    }

    threads.sort_by(|left, right| {
        right
            .due_now
            .cmp(&left.due_now)
            .then_with(|| left.due_at_unix.cmp(&right.due_at_unix))
            .then_with(|| left.learning_item_id.cmp(&right.learning_item_id))
            .then_with(|| left.capability.cmp(&right.capability))
    });
    threads.truncate(limit);
    threads
}

fn focusable_status(status: &str) -> bool {
    matches!(
        status,
        "introduced" | "learning" | "transfer_pending" | "provisionally_mastered" | "mastered"
    )
}

fn add_if_needed(
    threads: &mut Vec<TutorFocusThread>,
    item: &ItemCapabilityState,
    capability: CapabilityKind,
    estimate: &CapabilityEstimate,
    now_unix: i64,
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

    threads.push(TutorFocusThread {
        learning_item_id: item.learning_item_id.clone(),
        target_text: item.target_text.clone(),
        capability,
        stage,
        due_at_unix,
        due_now,
        successful_contexts: estimate.successful_contexts.clone(),
        successful_context_families: estimate.successful_context_families.clone(),
        next_context_policy: context_policy(capability, estimate),
        rationale: focus_rationale(capability, estimate, stage),
    });
}

fn consolidated(capability: CapabilityKind, estimate: &CapabilityEstimate) -> bool {
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

fn spacing_stage(estimate: &CapabilityEstimate) -> u8 {
    if estimate.attempts == 0 || estimate.last_observed_qualified_success != Some(true) {
        return 0;
    }
    estimate.qualified_success_streak.min(4)
}

fn context_policy(capability: CapabilityKind, estimate: &CapabilityEstimate) -> String {
    match capability {
        CapabilityKind::Transfer => {
            if estimate.successful_context_families.is_empty() {
                "Use a meaningfully different situation and record both context and context_family."
                    .to_owned()
            } else {
                format!(
                    "Use a new semantic context family; avoid already successful families: {}",
                    estimate.successful_context_families.join(", ")
                )
            }
        }
        CapabilityKind::SpontaneousProduction => {
            if estimate.successful_contexts.is_empty() {
                "Use a real communicative context and record assistance_level=0 only if truly unaided."
                    .to_owned()
            } else {
                format!(
                    "Vary the concrete situation; avoid only repeating: {}",
                    estimate.successful_contexts.join(", ")
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

fn focus_rationale(capability: CapabilityKind, estimate: &CapabilityEstimate, stage: u8) -> String {
    let rate = estimate.weighted_success_rate.map_or_else(
        || "unknown".to_owned(),
        |value| format!("{:.0}%", value * 100.0),
    );
    format!(
        "{capability:?}: success={rate}, evidence_strength={:.2}, spacing_stage={stage}, contexts={}, families={}",
        estimate.evidence_strength,
        estimate.distinct_success_contexts,
        estimate.distinct_success_families
    )
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    fn estimate() -> CapabilityEstimate {
        CapabilityEstimate {
            attempts: 1,
            successes: 1,
            weighted_success_rate: Some(0.9),
            unaided_success_rate: Some(1.0),
            latency_weighted_millis: Some(1500),
            qualified_success_streak: 1,
            distinct_success_contexts: 1,
            distinct_success_families: 1,
            successful_contexts: vec!["bus:route1".to_owned()],
            successful_context_families: vec!["transport.ask_stop".to_owned()],
            evidence_strength: 0.4,
            last_observed_at_unix: Some(0),
            last_observed_success: Some(true),
            last_observed_qualified_success: Some(true),
        }
    }

    fn item(status: &str) -> ItemCapabilityState {
        ItemCapabilityState {
            learning_item_id: "li-1".to_owned(),
            target_text: "target".to_owned(),
            status: status.to_owned(),
            recognition: CapabilityEstimate::default(),
            listening: estimate(),
            prompted_production: CapabilityEstimate::default(),
            spontaneous_production: estimate(),
            transfer: estimate(),
        }
    }

    #[test]
    fn candidate_items_never_enter_tutor_focus() {
        assert!(build_tutor_focus_threads(&[item("candidate")], 100, 10).is_empty());
    }

    #[test]
    fn unknown_or_unqualified_latest_attempt_is_due_immediately() {
        let mut state = item("learning");
        state.spontaneous_production.last_observed_qualified_success = Some(false);
        state.spontaneous_production.qualified_success_streak = 0;
        let threads = build_tutor_focus_threads(&[state], 100, 10);
        let spontaneous = threads
            .iter()
            .find(|thread| thread.capability == CapabilityKind::SpontaneousProduction)
            .expect("spontaneous thread");
        assert_eq!(spontaneous.stage, 0);
        assert!(spontaneous.due_now);
    }

    #[test]
    fn one_qualified_success_moves_only_to_one_day() {
        let threads = build_tutor_focus_threads(&[item("learning")], DAY_SECONDS / 2, 10);
        let spontaneous = threads
            .iter()
            .find(|thread| thread.capability == CapabilityKind::SpontaneousProduction)
            .expect("spontaneous thread");
        assert_eq!(spontaneous.stage, 1);
        assert_eq!(spontaneous.due_at_unix, DAY_SECONDS);
        assert!(!spontaneous.due_now);
    }

    #[test]
    fn transfer_needs_three_semantic_families_to_close() {
        let mut state = item("transfer_pending");
        state.spontaneous_production.evidence_strength = 0.9;
        state.spontaneous_production.qualified_success_streak = 3;
        state.transfer.evidence_strength = 0.9;
        state.transfer.weighted_success_rate = Some(0.95);
        state.transfer.qualified_success_streak = 3;
        state.transfer.distinct_success_families = 2;
        let threads = build_tutor_focus_threads(&[state], 100, 10);
        assert!(
            threads
                .iter()
                .any(|thread| thread.capability == CapabilityKind::Transfer)
        );
    }
}
