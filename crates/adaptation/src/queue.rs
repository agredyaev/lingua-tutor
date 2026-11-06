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
