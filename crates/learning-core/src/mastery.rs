//! Mastery rules (03-domain-model.md §6): card review success alone never
//! yields mastery; production evidence in distinct contexts does.

use crate::{EvidenceKind, LearningItemEvidence, LessonId};

/// Distinct successful production contexts required for full mastery.
pub const FULL_MASTERY_CONTEXTS: usize = 3;

/// Outcome of evaluating an item's evidence trail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MasteryAssessment {
    /// No unaided production yet.
    NotMastered,
    /// At least one successful unaided production.
    Provisional,
    /// Successful production in ≥ [`FULL_MASTERY_CONTEXTS`] distinct lessons.
    Full,
}

/// Assesses mastery from evidence. Only `SpontaneousProduction` and
/// `Transfer` successes count; a context = one distinct lesson (evidence
/// without a lesson counts as one extra context at most).
#[must_use]
pub fn assess_mastery(evidence: &[LearningItemEvidence]) -> MasteryAssessment {
    let productive = evidence.iter().filter(|e| {
        e.success
            && matches!(
                e.kind,
                EvidenceKind::SpontaneousProduction | EvidenceKind::Transfer
            )
    });

    let mut lesson_contexts: Vec<&LessonId> = Vec::new();
    let mut lessonless_context = false;
    let mut any = false;
    for item in productive {
        any = true;
        match &item.lesson_id {
            Some(lesson) if !lesson_contexts.contains(&lesson) => lesson_contexts.push(lesson),
            Some(_) => {}
            None => lessonless_context = true,
        }
    }

    if !any {
        return MasteryAssessment::NotMastered;
    }
    let contexts = lesson_contexts.len() + usize::from(lessonless_context);
    if contexts >= FULL_MASTERY_CONTEXTS {
        MasteryAssessment::Full
    } else {
        MasteryAssessment::Provisional
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::{EvidenceId, LearningItemId, Timestamp};

    fn evidence(kind: EvidenceKind, success: bool, lesson: Option<&str>) -> LearningItemEvidence {
        LearningItemEvidence {
            id: EvidenceId("ev".to_owned()),
            learning_item_id: LearningItemId("li-1".to_owned()),
            lesson_id: lesson.map(|l| LessonId(l.to_owned())),
            kind,
            success,
            detail: None,
            pack_metadata: None,
            recorded_at: Timestamp(0),
        }
    }

    #[test]
    fn review_success_alone_never_masters() {
        let trail = vec![
            evidence(EvidenceKind::Recognition, true, Some("l1")),
            evidence(EvidenceKind::PromptedProduction, true, Some("l2")),
            evidence(EvidenceKind::Listening, true, Some("l3")),
        ];
        assert_eq!(assess_mastery(&trail), MasteryAssessment::NotMastered);
    }

    #[test]
    fn one_unaided_production_is_provisional() {
        let trail = vec![evidence(
            EvidenceKind::SpontaneousProduction,
            true,
            Some("l1"),
        )];
        assert_eq!(assess_mastery(&trail), MasteryAssessment::Provisional);
    }

    #[test]
    fn three_distinct_contexts_is_full() {
        let trail = vec![
            evidence(EvidenceKind::SpontaneousProduction, true, Some("l1")),
            evidence(EvidenceKind::Transfer, true, Some("l2")),
            evidence(EvidenceKind::SpontaneousProduction, true, Some("l3")),
        ];
        assert_eq!(assess_mastery(&trail), MasteryAssessment::Full);
    }

    #[test]
    fn repeated_same_lesson_counts_once() {
        let trail = vec![
            evidence(EvidenceKind::SpontaneousProduction, true, Some("l1")),
            evidence(EvidenceKind::SpontaneousProduction, true, Some("l1")),
            evidence(EvidenceKind::Transfer, true, Some("l1")),
        ];
        assert_eq!(assess_mastery(&trail), MasteryAssessment::Provisional);
    }

    #[test]
    fn failed_production_does_not_count() {
        let trail = vec![
            evidence(EvidenceKind::SpontaneousProduction, false, Some("l1")),
            evidence(EvidenceKind::Transfer, false, Some("l2")),
        ];
        assert_eq!(assess_mastery(&trail), MasteryAssessment::NotMastered);
    }
}
