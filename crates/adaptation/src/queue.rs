//! One tutor-facing task queue built from spaced focus state + item task scores.
use crate::CapabilityKind;
use serde::Serialize;

/// One tutor-facing output record serialized at the boundary.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TutorTask {
    /// Stable identifier of the proposed tutoring or review action.
    pub action: String,
    /// learning item id used by the corresponding adaptation pass.
    pub learning_item_id: String,
    /// Target-language phrase or response to practice.
    pub target_text: String,
    /// capability used by the corresponding adaptation pass.
    pub capability: CapabilityKind,
    /// Relative priority used to rank tutor tasks.
    pub priority: u8,
    /// due now used by the corresponding adaptation pass.
    pub due_now: bool,
    /// Next scheduled practice time as a Unix timestamp in seconds.
    pub due_at_unix: Option<i64>,
    /// stage used by the corresponding adaptation pass.
    pub stage: Option<u8>,
    /// Concrete contexts in which the learner succeeded unaided.
    pub successful_contexts: Vec<String>,
    /// successful context families used by the corresponding adaptation pass.
    pub successful_context_families: Vec<String>,
    /// Rule for selecting the next practice context.
    pub next_context_policy: Option<String>,
    /// rationale used by the corresponding adaptation pass.
    pub rationale: String,
    /// Observable condition for marking this task successful.
    pub success_criterion: String,
    /// Task-local instrumentation contract. This keeps measurement requirements
    /// next to the executable task instead of forcing the tutor to infer them
    /// from the global lesson-result contract.
    pub measurement_requirements: Vec<&'static str>,
    /// Task-local closeout contract. These requirements are evaluated after the
    /// observed outcome and before lesson completion.
    pub completion_requirements: Vec<&'static str>,
}

#[must_use]
/// Decode the expected evidence mode into a capability, rejecting unknown values.
pub fn evidence_capability(value: &str) -> Option<CapabilityKind> {
    match value {
        "recognition" => Some(CapabilityKind::Recognition),
        "listening" => Some(CapabilityKind::Listening),
        "prompted_production" => Some(CapabilityKind::PromptedProduction),
        "spontaneous_production" => Some(CapabilityKind::SpontaneousProduction),
        "transfer" => Some(CapabilityKind::Transfer),
        _ => None,
    }
}

#[must_use]
/// Return the fallback action for a capability without allocating.
pub fn default_action(capability: CapabilityKind) -> &'static str {
    match capability {
        CapabilityKind::Recognition => "recognition_probe",
        CapabilityKind::Listening => "listening_probe",
        CapabilityKind::PromptedProduction => "guided_retrieval",
        CapabilityKind::SpontaneousProduction => "spontaneous_retrieval",
        CapabilityKind::Transfer => "transfer_practice",
    }
}

#[must_use]
/// Return the default ranking score for a capability.
pub fn default_focus_score(capability: CapabilityKind) -> u8 {
    match capability {
        CapabilityKind::Transfer => 82,
        CapabilityKind::SpontaneousProduction => 86,
        CapabilityKind::Listening => 78,
        CapabilityKind::PromptedProduction => 72,
        CapabilityKind::Recognition => 68,
    }
}

#[must_use]
/// Return the observable success criterion for the capability.
pub fn criterion(capability: CapabilityKind) -> &'static str {
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

const PRODUCTION_MEASUREMENTS: &[&str] = &[
    "assistance_level",
    "probe_phase",
    "context",
    "success",
    "response_millis_if_instrumented",
];
const DIAGNOSTIC_PRODUCTION_MEASUREMENTS: &[&str] = &[
    "assistance_level=0_on_initial_probe",
    "probe_phase=diagnostic",
    "context",
    "success",
    "response_millis_if_instrumented",
];
const LISTENING_MEASUREMENTS: &[&str] = &[
    "assistance_level",
    "probe_phase",
    "context_tag",
    "source_reference",
    "success",
    "response_millis_if_instrumented",
];
const DIAGNOSTIC_LISTENING_MEASUREMENTS: &[&str] = &[
    "assistance_level=0_on_initial_probe",
    "probe_phase=diagnostic",
    "context_tag",
    "source_reference",
    "success",
    "response_millis_if_instrumented",
];
const TRANSFER_MEASUREMENTS: &[&str] = &[
    "assistance_level=0",
    "probe_phase=transfer",
    "context",
    "context_family",
    "success",
    "response_millis_if_instrumented",
];
const AUTOMATICITY_MEASUREMENTS: &[&str] = &[
    "assistance_level=0",
    "probe_phase",
    "context",
    "success",
    "response_millis_required",
];

const PROBE_CLOSEOUT: &[&str] = &[
    "persist_task_outcome",
    "evaluate_card_mutation_from_observed_outcome",
    "card_trigger=failed_retrieval|failed_listening|prompt_dependence|recurring_error|new_high_value_item_needing_spaced_retrieval_or_transfer|high_value_expected_situation",
    "no_duplicate_active_card_same_target_modality",
    "no_card_for_mere_exposure_or_stable_independent_retrieval",
];
const INTERVENTION_CLOSEOUT: &[&str] = &[
    "persist_task_outcome",
    "persist_intervention_if_applied",
    "evaluate_card_mutation_from_observed_outcome",
    "card_trigger=failed_retrieval|failed_listening|prompt_dependence|recurring_error|new_high_value_item_needing_spaced_retrieval_or_transfer|high_value_expected_situation",
    "no_duplicate_active_card_same_target_modality",
    "no_card_for_mere_exposure_or_stable_independent_retrieval",
];

#[must_use]
/// Return the task-local measurement contract for one executable tutor action.
///
/// The strings intentionally use the same field names as the lesson-result
/// contract. `response_millis_if_instrumented` means measure prompt-ready to
/// first committed response when a timing instrument is actually available;
/// never synthesize a latency value. `response_millis_required` means the
/// action cannot produce automaticity evidence without an actual timer.
pub fn measurement_requirements_for_action(
    action: &str,
    capability: CapabilityKind,
) -> &'static [&'static str] {
    match action {
        "diagnostic_spontaneous" => DIAGNOSTIC_PRODUCTION_MEASUREMENTS,
        "listening_probe" => DIAGNOSTIC_LISTENING_MEASUREMENTS,
        "automaticity_drill" => AUTOMATICITY_MEASUREMENTS,
        "transfer_practice" => TRANSFER_MEASUREMENTS,
        _ => match capability {
            CapabilityKind::Listening => LISTENING_MEASUREMENTS,
            CapabilityKind::Transfer => TRANSFER_MEASUREMENTS,
            CapabilityKind::Recognition
            | CapabilityKind::PromptedProduction
            | CapabilityKind::SpontaneousProduction => PRODUCTION_MEASUREMENTS,
        },
    }
}
#[must_use]
/// Return the task-local closeout contract for one executable tutor action.
///
/// Card evaluation happens after the observed outcome. A card mutation is
/// warranted only when evidence supports review work: failed retrieval,
/// failed listening, prompt dependence, a recurring error, new high-value
/// material that needs spaced retrieval or transfer, or a high-value expected
/// situation. Mere exposure or stable independent retrieval is not sufficient.
pub fn completion_requirements_for_action(action: &str) -> &'static [&'static str] {
    match action {
        "listening_repair"
        | "fade_prompts"
        | "guided_retrieval"
        | "spontaneous_retrieval"
        | "automaticity_drill" => INTERVENTION_CLOSEOUT,
        _ => PROBE_CLOSEOUT,
    }
}

#[must_use]
/// Return the LLM-facing observable criterion for one concrete tutor action.
pub fn criterion_for_action(action: &str, capability: CapabilityKind) -> &'static str {
    match action {
        "diagnostic_spontaneous" => {
            "measure one fresh production attempt with assistance_level=0 before teaching; record it as diagnostic evidence"
        }
        "listening_probe" => {
            "measure unaided comprehension on a fresh listening variant before teaching; record it as diagnostic evidence"
        }
        "listening_repair" => {
            "after the listening intervention, verify improved comprehension on a fresh variant without revealing the answer"
        }
        "fade_prompts" => {
            "reduce assistance and obtain correct spontaneous production with assistance_level=0"
        }
        "spontaneous_retrieval" => {
            "obtain correct production with assistance_level=0, then check delayed retrieval"
        }
        "transfer_practice" => {
            "obtain correct unaided use in a new semantic context_family and record transfer evidence"
        }
        "automaticity_drill" => {
            "retain unaided accuracy while retrieval latency improves across measured independent attempts"
        }
        _ => criterion(capability),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostic_measurements_are_task_local() {
        let requirements = measurement_requirements_for_action(
            "diagnostic_spontaneous",
            CapabilityKind::SpontaneousProduction,
        );
        assert!(requirements.contains(&"probe_phase=diagnostic"));
        assert!(requirements.contains(&"response_millis_if_instrumented"));
    }

    #[test]
    fn automaticity_requires_real_latency_measurement() {
        let requirements = measurement_requirements_for_action(
            "automaticity_drill",
            CapabilityKind::SpontaneousProduction,
        );
        assert!(requirements.contains(&"response_millis_required"));
        assert!(!requirements.contains(&"response_millis_if_instrumented"));
    }

    #[test]
    fn closeout_requires_outcome_persistence_and_card_evaluation() {
        let probe = completion_requirements_for_action("diagnostic_spontaneous");
        assert!(probe.contains(&"persist_task_outcome"));
        assert!(probe.contains(&"evaluate_card_mutation_from_observed_outcome"));
        assert!(probe.iter().any(|value| {
            value.contains("new_high_value_item_needing_spaced_retrieval_or_transfer")
        }));
        assert!(probe.contains(&"no_duplicate_active_card_same_target_modality"));
        assert!(probe.contains(&"no_card_for_mere_exposure_or_stable_independent_retrieval"));
        assert!(!probe.contains(&"persist_intervention_if_applied"));

        let repair = completion_requirements_for_action("fade_prompts");
        assert!(repair.contains(&"persist_intervention_if_applied"));
        assert!(repair.contains(&"evaluate_card_mutation_from_observed_outcome"));
    }
}
