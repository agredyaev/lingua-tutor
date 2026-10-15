//! One tutor-facing task queue built from spaced focus state + item task scores.

use crate::{CapabilityKind, TaskCandidate, TutorFocusThread};
use serde::Serialize;
use std::collections::BTreeSet;

/// One concrete tutor action. This is the only item-level queue the tutor needs.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TutorTask {
    /// Stable action code.
    pub action: String,
    /// Learning item targeted by this action.
    pub learning_item_id: String,
    /// Target-language text.
    pub target_text: String,
    /// Capability being trained/probed.
    pub capability: CapabilityKind,
    /// Deterministic priority 0..=100.
    pub priority: u8,
    /// Whether spaced retrieval is due now.
    pub due_now: bool,
    /// Next due timestamp when this task came from a focus thread.
    pub due_at_unix: Option<i64>,
    /// Spacing stage when this task came from a focus thread.
    pub stage: Option<u8>,
    /// Successful concrete contexts already used.
    pub successful_contexts: Vec<String>,
    /// Successful semantic context families already used.
    pub successful_context_families: Vec<String>,
    /// How the next context should differ.
    pub next_context_policy: Option<String>,
    /// Why this action is ranked here.
    pub rationale: String,
    /// Observable result expected from the action.
    pub success_criterion: String,
}

/// Merges internal focus state and candidate scoring into one bounded tutor queue.
#[must_use]
pub fn build_tutor_task_queue(
    focus_threads: &[TutorFocusThread],
    candidates: &[TaskCandidate],
    limit: usize,
) -> Vec<TutorTask> {
    let mut tasks = Vec::new();
    let mut represented: BTreeSet<(String, CapabilityKind)> = BTreeSet::new();

    for focus in focus_threads {
        let capability = focus.capability;
        let best = candidates
            .iter()
            .filter(|candidate| {
                candidate.learning_item_id == focus.learning_item_id
                    && evidence_capability(&candidate.expected_evidence) == Some(capability)
            })
            .max_by_key(|candidate| candidate.score);

        let (action, score, reason) = best.map_or_else(
            || {
                (
                    default_action(capability).to_owned(),
                    default_focus_score(capability),
                    focus.rationale.clone(),
                )
            },
            |candidate| {
                (
                    candidate.task_type.clone(),
                    candidate.score,
                    format!("{}; {}", focus.rationale, candidate.rationale),
                )
            },
        );

        tasks.push(TutorTask {
            action,
            learning_item_id: focus.learning_item_id.clone(),
            target_text: focus.target_text.clone(),
            capability,
            priority: score
                .saturating_add(if focus.due_now { 8 } else { 0 })
                .min(100),
            due_now: focus.due_now,
            due_at_unix: Some(focus.due_at_unix),
            stage: Some(focus.stage),
            successful_contexts: focus.successful_contexts.clone(),
            successful_context_families: focus.successful_context_families.clone(),
            next_context_policy: Some(focus.next_context_policy.clone()),
            rationale: reason,
            success_criterion: criterion(capability).to_owned(),
        });
        represented.insert((focus.learning_item_id.clone(), capability));
    }

    for candidate in candidates {
        let Some(capability) = evidence_capability(&candidate.expected_evidence) else {
            continue;
        };
        if represented.contains(&(candidate.learning_item_id.clone(), capability)) {
            continue;
        }
        tasks.push(TutorTask {
            action: candidate.task_type.clone(),
            learning_item_id: candidate.learning_item_id.clone(),
            target_text: candidate.target_text.clone(),
            capability,
            priority: candidate.score,
            due_now: false,
            due_at_unix: None,
            stage: None,
            successful_contexts: vec![],
            successful_context_families: vec![],
            next_context_policy: None,
            rationale: candidate.rationale.clone(),
            success_criterion: criterion(capability).to_owned(),
        });
    }

    tasks.sort_by(|left, right| {
        right
            .due_now
            .cmp(&left.due_now)
            .then_with(|| right.priority.cmp(&left.priority))
            .then_with(|| left.learning_item_id.cmp(&right.learning_item_id))
            .then_with(|| left.capability.cmp(&right.capability))
    });
    tasks.truncate(limit);
    tasks
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

fn default_action(capability: CapabilityKind) -> &'static str {
    match capability {
        CapabilityKind::Recognition => "recognition_probe",
        CapabilityKind::Listening => "listening_probe",
        CapabilityKind::PromptedProduction => "guided_retrieval",
        CapabilityKind::SpontaneousProduction => "spontaneous_retrieval",
        CapabilityKind::Transfer => "transfer_practice",
    }
}

fn default_focus_score(capability: CapabilityKind) -> u8 {
    match capability {
        CapabilityKind::Transfer => 82,
        CapabilityKind::SpontaneousProduction => 86,
        CapabilityKind::Listening => 78,
        CapabilityKind::PromptedProduction => 72,
        CapabilityKind::Recognition => 68,
    }
}

fn criterion(capability: CapabilityKind) -> &'static str {
    match capability {
        CapabilityKind::Recognition => "correct recognition without revealing the answer first",
        CapabilityKind::Listening => "correct comprehension on a fresh listening variant",
        CapabilityKind::PromptedProduction => "correct production with less assistance than before",
        CapabilityKind::SpontaneousProduction => {
            "correct production with assistance_level=0, then delayed retrieval"
        }
        CapabilityKind::Transfer => {
            "correct unaided use in a new context_family, recorded as transfer evidence"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn focus(capability: CapabilityKind, due: bool) -> TutorFocusThread {
        TutorFocusThread {
            learning_item_id: "li-1".to_owned(),
            target_text: "target".to_owned(),
            capability,
            stage: 0,
            due_at_unix: 0,
            due_now: due,
            successful_contexts: vec![],
            successful_context_families: vec![],
            next_context_policy: "new context".to_owned(),
            rationale: "weak".to_owned(),
        }
    }

    #[test]
    fn due_focus_becomes_single_tutor_task() {
        let candidates = vec![TaskCandidate {
            task_type: "fade_prompts".to_owned(),
            learning_item_id: "li-1".to_owned(),
            target_text: "target".to_owned(),
            score: 90,
            rationale: "prompt gap".to_owned(),
            expected_evidence: "spontaneous_production".to_owned(),
        }];
        let tasks = build_tutor_task_queue(
            &[focus(CapabilityKind::SpontaneousProduction, true)],
            &candidates,
            10,
        );
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].action, "fade_prompts");
        assert!(tasks[0].due_now);
        assert_eq!(tasks[0].priority, 98);
    }
}
