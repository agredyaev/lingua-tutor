//! Mastery rules: review success and tutor confidence never establish mastery.
//! Full mastery requires explicitly unaided transfer across semantic context families.

use crate::{EvidenceKind, LearningItemEvidence};
use std::collections::BTreeSet;

/// Distinct semantic transfer families required for full mastery.
pub const FULL_MASTERY_CONTEXT_FAMILIES: usize = 3;

/// Outcome of evaluating an item's evidence trail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MasteryAssessment {
    /// No explicitly unaided productive evidence yet.
    NotMastered,
    /// At least one explicitly unaided productive success.
    Provisional,
    /// Explicitly unaided transfer in at least three semantic context families.
    Full,
}

/// Assesses mastery from persisted evidence.
///
/// `assistance_level` must be explicitly `0`; missing assistance is unknown and
/// cannot prove independence. Full mastery uses `context_family`, not arbitrary
/// concrete context labels or lesson IDs.
#[must_use]
pub fn assess_mastery(evidence: &[LearningItemEvidence]) -> MasteryAssessment {
    let mut any_unaided_productive = false;
    let mut transfer_families = BTreeSet::new();

    for entry in evidence {
        if !entry.success
            || !matches!(
                entry.kind,
                EvidenceKind::SpontaneousProduction | EvidenceKind::Transfer
            )
            || assistance_level(entry) != Some(0)
        {
            continue;
        }

        any_unaided_productive = true;

        if entry.kind == EvidenceKind::Transfer
            && let Some(family) = metadata_text(entry, "context_family")
        {
            transfer_families.insert(family.to_owned());
        }
    }

    if !any_unaided_productive {
        MasteryAssessment::NotMastered
    } else if transfer_families.len() >= FULL_MASTERY_CONTEXT_FAMILIES {
        MasteryAssessment::Full
    } else {
        MasteryAssessment::Provisional
    }
}

fn assistance_level(entry: &LearningItemEvidence) -> Option<u8> {
    entry
        .pack_metadata
        .as_ref()
        .and_then(|metadata| metadata.get("assistance_level"))
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| u8::try_from(value).ok())
}

fn metadata_text<'a>(entry: &'a LearningItemEvidence, key: &str) -> Option<&'a str> {
    entry
        .pack_metadata
        .as_ref()
        .and_then(|metadata| metadata.get(key))
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EvidenceId, LearningItemId, LessonId, Timestamp};

    fn evidence(
        kind: EvidenceKind,
        success: bool,
        assistance: Option<u8>,
        context: Option<&str>,
        family: Option<&str>,
    ) -> LearningItemEvidence {
        LearningItemEvidence {
            id: EvidenceId("ev".to_owned()),
            learning_item_id: LearningItemId("li-1".to_owned()),
            lesson_id: Some(LessonId("lesson".to_owned())),
            kind,
            success,
            detail: None,
            pack_metadata: Some(serde_json::json!({
                "context_key": context,
                "context_family": family,
                "assistance_level": assistance,
            })),
            recorded_at: Timestamp(0),
        }
    }

    #[test]
    fn review_or_prompted_success_never_masters() {
        let trail = vec![
            evidence(EvidenceKind::Recognition, true, None, None, None),
            evidence(EvidenceKind::PromptedProduction, true, Some(2), None, None),
            evidence(EvidenceKind::Listening, true, None, None, None),
        ];
        assert_eq!(assess_mastery(&trail), MasteryAssessment::NotMastered);
    }

    #[test]
    fn missing_assistance_is_not_proof_of_independence() {
        let trail = vec![evidence(
            EvidenceKind::SpontaneousProduction,
            true,
            None,
            Some("cafe:order"),
            Some("commerce.request"),
        )];
        assert_eq!(assess_mastery(&trail), MasteryAssessment::NotMastered);
    }

    #[test]
    fn one_explicitly_unaided_production_is_provisional() {
        let trail = vec![evidence(
            EvidenceKind::SpontaneousProduction,
            true,
            Some(0),
            Some("cafe:order"),
            Some("commerce.request"),
        )];
        assert_eq!(assess_mastery(&trail), MasteryAssessment::Provisional);
    }

    #[test]
    fn three_unaided_transfer_families_is_full() {
        let trail = vec![
            evidence(
                EvidenceKind::Transfer,
                true,
                Some(0),
                Some("bus:route1"),
                Some("transport.ask_stop"),
            ),
            evidence(
                EvidenceKind::Transfer,
                true,
                Some(0),
                Some("bakery:item"),
                Some("commerce.request_item"),
            ),
            evidence(
                EvidenceKind::Transfer,
                true,
                Some(0),
                Some("pharmacy:help"),
                Some("service.ask_help"),
            ),
        ];
        assert_eq!(assess_mastery(&trail), MasteryAssessment::Full);
    }

    #[test]
    fn different_concrete_contexts_same_family_count_once() {
        let trail = vec![
            evidence(
                EvidenceKind::Transfer,
                true,
                Some(0),
                Some("bus:route1"),
                Some("transport.ask_stop"),
            ),
            evidence(
                EvidenceKind::Transfer,
                true,
                Some(0),
                Some("bus:route2"),
                Some("transport.ask_stop"),
            ),
            evidence(
                EvidenceKind::Transfer,
                true,
                Some(0),
                Some("metro:route3"),
                Some("transport.ask_stop"),
            ),
        ];
        assert_eq!(assess_mastery(&trail), MasteryAssessment::Provisional);
    }

    #[test]
    fn assisted_transfer_does_not_count() {
        let trail = vec![evidence(
            EvidenceKind::Transfer,
            true,
            Some(2),
            Some("bus:route1"),
            Some("transport.ask_stop"),
        )];
        assert_eq!(assess_mastery(&trail), MasteryAssessment::NotMastered);
    }
}
