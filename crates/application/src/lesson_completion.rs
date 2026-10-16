//! Lesson-completion planning (07-tutor-contract.md §2, 08-local-rpc.md §4):
//! validated lesson result + current state → deterministic apply plan.
//! The local runtime persists the plan atomically; this module never touches IO.

use learning_core::{
    ErrorTarget, ErrorTargetId, ErrorTargetStatus, LearnerLanguageProfile, LearningItem,
    LearningItemEvidence, LearningItemId, LearningItemStatus, Lesson, LessonId, MasteryAssessment,
    Timestamp, assess_mastery,
};
use tutor_contract::LessonResult;

/// Recheck schedule offsets in days (03-domain-model.md §7).
pub const RECHECK_OFFSETS_DAYS: [u32; 3] = [1, 3, 7];

/// Everything the planner needs, fetched by the local runtime beforehand.
#[derive(Debug, Clone)]
pub struct LessonCompletionContext {
    /// Target profile (already resolved from `language_profile_id`).
    pub profile: LearnerLanguageProfile,
    /// Request instant.
    pub now: Timestamp,
    /// Lesson referenced by `lesson.lesson_id`, when it exists.
    pub existing_lesson: Option<Lesson>,
    /// Prior import with the same `client_operation_id`, if any (idempotency).
    pub duplicate_import: bool,
    /// Items whose `normalized_target_text` matches any proposed new item.
    pub items_by_normalized: Vec<LearningItem>,
    /// Pack-normalized canonical text per `new_learning_items` entry,
    /// same order (local runtime calls the active pack).
    pub normalized_new_items: Vec<String>,
    /// IDs of items that exist in this profile (for reference checks).
    pub known_item_ids: Vec<LearningItemId>,
    /// Active/recheck error targets for occurrence matching.
    pub error_targets: Vec<ErrorTarget>,
    /// Evidence trails for items named in `mastery_changes`.
    pub evidence_for_mastery: Vec<(LearningItemId, Vec<LearningItemEvidence>)>,
}

/// Reference to an item that may not have an ID yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemRef {
    /// Existing row.
    Existing(LearningItemId),
    /// Index into [`LessonCompletionPlan::new_items`].
    New(usize),
}

/// New learning item to insert (ID assigned at persist time).
#[derive(Debug, Clone, PartialEq)]
pub struct PlannedItem {
    /// Index into `result.new_learning_items`.
    pub source_index: usize,
    /// Canonical text from the active pack.
    pub normalized_target_text: String,
}

/// Evidence row to insert.
#[derive(Debug, Clone, PartialEq)]
pub struct PlannedEvidence {
    /// Index into `result.evidence`.
    pub source_index: usize,
    /// Which item it belongs to.
    pub item: ItemRef,
}

/// Listening attempt to insert.
#[derive(Debug, Clone, PartialEq)]
pub struct PlannedListening {
    /// Index into `result.listening_attempts`.
    pub source_index: usize,
    /// Referenced item, when given.
    pub item: Option<ItemRef>,
}

/// What to do with one error occurrence.
#[derive(Debug, Clone, PartialEq)]
pub enum ErrorAction {
    /// Create a new target (+ occurrence + 1/3/7 rechecks).
    NewTarget {
        /// Index into `result.error_occurrences`.
        source_index: usize,
    },
    /// Register a sighting on an existing target.
    Register {
        /// Index into `result.error_occurrences`.
        source_index: usize,
        /// Target to update.
        target_id: ErrorTargetId,
        /// New occurrence count.
        new_count: u32,
        /// Escalation reached (≥ 3): recommend mini-target/contrast card.
        escalate: bool,
        /// Method change required (≥ 5 unresolved).
        method_change: bool,
    },
}

/// Lesson row outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LessonOutcome {
    /// Duplicate `client_operation_id`: return original result, write nothing.
    DuplicateNoop,
    /// Complete the referenced lesson.
    CompleteExisting(LessonId),
    /// Create a completed lesson with this number (import without prior start).
    CreateCompleted {
        /// `profile.current_lesson_number + 1`.
        lesson_number: u32,
    },
}

/// Deterministic apply plan; persisting it is one atomic database batch.
#[derive(Debug, Clone, PartialEq)]
pub struct LessonCompletionPlan {
    /// Lesson row action.
    pub lesson: LessonOutcome,
    /// Items to insert.
    pub new_items: Vec<PlannedItem>,
    /// Proposed items that already exist: (source index, existing ID).
    pub reused_items: Vec<(usize, LearningItemId)>,
    /// Evidence rows.
    pub evidence: Vec<PlannedEvidence>,
    /// Listening rows.
    pub listening: Vec<PlannedListening>,
    /// Error target actions.
    pub error_actions: Vec<ErrorAction>,
    /// Item status updates after server-side mastery re-check.
    pub item_status_changes: Vec<(LearningItemId, LearningItemStatus)>,
    /// Human-readable non-fatal notes (e.g. downgraded mastery proposal).
    pub warnings: Vec<String>,
}

/// Why a lesson result cannot be applied.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PlanError {
    /// Result addressed to a different profile.
    #[error("language_profile_id '{0}' does not match the target profile")]
    ProfileMismatch(String),
    /// Result produced under a different pack.
    #[error("language_pack_id '{0}' does not match the profile's pack")]
    PackMismatch(String),
    /// Referenced item does not exist in this profile.
    #[error("unknown learning_item_id '{0}'")]
    UnknownLearningItem(String),
    /// Lesson-local item key does not exist in new_learning_items.
    #[error("unknown client_item_key '{0}'")]
    UnknownClientItemKey(String),
    /// Referenced lesson does not exist.
    #[error("unknown lesson_id '{0}'")]
    UnknownLesson(String),
    /// Context arrays are inconsistent (runtime bug, not user input).
    #[error("invalid context: {0}")]
    InvalidContext(String),
}

/// Builds the apply plan. Pure and deterministic: same inputs → same plan.
///
/// # Errors
///
/// [`PlanError`] on profile/pack mismatch or unknown references; the import
/// is rejected as a whole (atomicity, 07-tutor-contract.md §2).
pub fn plan_lesson_completion(
    result: &LessonResult,
    ctx: &LessonCompletionContext,
) -> Result<LessonCompletionPlan, PlanError> {
    check_addressing(result, ctx)?;
    if ctx.duplicate_import {
        return Ok(LessonCompletionPlan {
            lesson: LessonOutcome::DuplicateNoop,
            new_items: Vec::new(),
            reused_items: Vec::new(),
            evidence: Vec::new(),
            listening: Vec::new(),
            error_actions: Vec::new(),
            item_status_changes: Vec::new(),
            warnings: Vec::new(),
        });
    }
    if ctx.normalized_new_items.len() != result.new_learning_items.len() {
        return Err(PlanError::InvalidContext(
            "normalized_new_items length mismatch".to_owned(),
        ));
    }

    let mut warnings = Vec::new();
    let (new_items, reused_items) = split_new_items(result, ctx);
    let evidence = resolve_evidence(result, ctx, &new_items, &reused_items)?;
    let listening = resolve_listening(result, ctx, &new_items, &reused_items)?;
    let error_actions = plan_error_actions(result, ctx);
    let item_status_changes = recheck_mastery(result, ctx, &mut warnings)?;

    Ok(LessonCompletionPlan {
        lesson: lesson_outcome(result, ctx)?,
        new_items,
        reused_items,
        evidence,
        listening,
        error_actions,
        item_status_changes,
        warnings,
    })
}

fn check_addressing(result: &LessonResult, ctx: &LessonCompletionContext) -> Result<(), PlanError> {
    if result.language_profile_id != ctx.profile.id.0 {
        return Err(PlanError::ProfileMismatch(
            result.language_profile_id.clone(),
        ));
    }
    if result.language_pack_id != ctx.profile.language_pack_id.0 {
        return Err(PlanError::PackMismatch(result.language_pack_id.clone()));
    }
    Ok(())
}

fn lesson_outcome(
    result: &LessonResult,
    ctx: &LessonCompletionContext,
) -> Result<LessonOutcome, PlanError> {
    match (&result.lesson.lesson_id, &ctx.existing_lesson) {
        (Some(_), Some(lesson)) | (None, Some(lesson)) => {
            Ok(LessonOutcome::CompleteExisting(lesson.id.clone()))
        }
        (Some(id), None) => Err(PlanError::UnknownLesson(id.clone())),
        (None, None) => Ok(LessonOutcome::CreateCompleted {
            lesson_number: ctx.profile.current_lesson_number + 1,
        }),
    }
}

fn split_new_items(
    result: &LessonResult,
    ctx: &LessonCompletionContext,
) -> (Vec<PlannedItem>, Vec<(usize, LearningItemId)>) {
    let mut new_items = Vec::new();
    let mut reused = Vec::new();
    for (i, normalized) in ctx.normalized_new_items.iter().enumerate() {
        match ctx
            .items_by_normalized
            .iter()
            .find(|item| &item.normalized_target_text == normalized)
        {
            Some(existing) => reused.push((i, existing.id.clone())),
            None => new_items.push(PlannedItem {
                source_index: i,
                normalized_target_text: normalized.clone(),
            }),
        }
    }
    let _ = result; // lengths already validated by caller
    (new_items, reused)
}

fn resolve_item_id(raw_id: &str, ctx: &LessonCompletionContext) -> Result<ItemRef, PlanError> {
    if ctx.known_item_ids.iter().any(|id| id.0 == raw_id) {
        return Ok(ItemRef::Existing(LearningItemId(raw_id.to_owned())));
    }
    Err(PlanError::UnknownLearningItem(raw_id.to_owned()))
}

fn resolve_client_item_key(
    key: &str,
    result: &LessonResult,
    new_items: &[PlannedItem],
    reused_items: &[(usize, LearningItemId)],
) -> Result<ItemRef, PlanError> {
    let Some(source_index) = result
        .new_learning_items
        .iter()
        .position(|item| item.client_item_key.as_deref() == Some(key))
    else {
        return Err(PlanError::UnknownClientItemKey(key.to_owned()));
    };
    if let Some((_, id)) = reused_items
        .iter()
        .find(|(index, _)| *index == source_index)
    {
        return Ok(ItemRef::Existing(id.clone()));
    }
    if let Some(plan_index) = new_items
        .iter()
        .position(|planned| planned.source_index == source_index)
    {
        return Ok(ItemRef::New(plan_index));
    }
    Err(PlanError::InvalidContext(format!(
        "client_item_key '{key}' did not resolve to a new or reused item"
    )))
}

fn resolve_reference(
    learning_item_id: Option<&str>,
    client_item_key: Option<&str>,
    result: &LessonResult,
    ctx: &LessonCompletionContext,
    new_items: &[PlannedItem],
    reused_items: &[(usize, LearningItemId)],
) -> Result<ItemRef, PlanError> {
    match (learning_item_id, client_item_key) {
        (Some(id), None) => resolve_item_id(id, ctx),
        (None, Some(key)) => resolve_client_item_key(key, result, new_items, reused_items),
        _ => Err(PlanError::InvalidContext(
            "item reference must contain exactly one of learning_item_id or client_item_key"
                .to_owned(),
        )),
    }
}

fn resolve_evidence(
    result: &LessonResult,
    ctx: &LessonCompletionContext,
    new_items: &[PlannedItem],
    reused_items: &[(usize, LearningItemId)],
) -> Result<Vec<PlannedEvidence>, PlanError> {
    result
        .evidence
        .iter()
        .enumerate()
        .map(|(i, e)| {
            Ok(PlannedEvidence {
                source_index: i,
                item: resolve_reference(
                    e.learning_item_id.as_deref(),
                    e.client_item_key.as_deref(),
                    result,
                    ctx,
                    new_items,
                    reused_items,
                )?,
            })
        })
        .collect()
}

fn resolve_listening(
    result: &LessonResult,
    ctx: &LessonCompletionContext,
    new_items: &[PlannedItem],
    reused_items: &[(usize, LearningItemId)],
) -> Result<Vec<PlannedListening>, PlanError> {
    result
        .listening_attempts
        .iter()
        .enumerate()
        .map(|(i, a)| {
            let item = if a.learning_item_id.is_none() && a.client_item_key.is_none() {
                None
            } else {
                Some(resolve_reference(
                    a.learning_item_id.as_deref(),
                    a.client_item_key.as_deref(),
                    result,
                    ctx,
                    new_items,
                    reused_items,
                )?)
            };
            Ok(PlannedListening {
                source_index: i,
                item,
            })
        })
        .collect()
}

fn plan_error_actions(result: &LessonResult, ctx: &LessonCompletionContext) -> Vec<ErrorAction> {
    result
        .error_occurrences
        .iter()
        .enumerate()
        .map(|(i, occurrence)| {
            let matched = occurrence
                .error_target_id
                .as_deref()
                .and_then(|id| ctx.error_targets.iter().find(|t| t.id.0 == id));
            match matched {
                Some(target) => {
                    let new_count = target.occurrence_count + 1;
                    ErrorAction::Register {
                        source_index: i,
                        target_id: target.id.clone(),
                        new_count,
                        escalate: new_count >= learning_core::ERROR_ESCALATION_THRESHOLD
                            && target.status != ErrorTargetStatus::Resolved,
                        method_change: new_count >= learning_core::ERROR_METHOD_CHANGE_THRESHOLD
                            && target.status != ErrorTargetStatus::Resolved,
                    }
                }
                None => ErrorAction::NewTarget { source_index: i },
            }
        })
        .collect()
}

fn recheck_mastery(
    result: &LessonResult,
    ctx: &LessonCompletionContext,
    warnings: &mut Vec<String>,
) -> Result<Vec<(LearningItemId, LearningItemStatus)>, PlanError> {
    let mut changes = Vec::new();
    for proposal in &result.mastery_changes {
        let item_ref = resolve_item_id(&proposal.learning_item_id, ctx)?;
        let ItemRef::Existing(item_id) = item_ref else {
            continue;
        };
        let mut trail = ctx
            .evidence_for_mastery
            .iter()
            .find(|(id, _)| id == &item_id)
            .map(|(_, trail)| trail.clone())
            .unwrap_or_default();
        for evidence in result
            .evidence
            .iter()
            .filter(|e| e.learning_item_id.as_deref() == Some(item_id.0.as_str()))
        {
            trail.push(LearningItemEvidence {
                id: learning_core::EvidenceId("pending-current-lesson".to_owned()),
                learning_item_id: item_id.clone(),
                lesson_id: ctx.existing_lesson.as_ref().map(|lesson| lesson.id.clone()),
                kind: evidence.kind,
                success: evidence.success,
                detail: evidence.detail.clone(),
                pack_metadata: Some(serde_json::json!({
                    "context_key": evidence
                        .context
                        .as_deref()
                        .unwrap_or(result.lesson.scenario.as_str()),
                    "context_family": evidence.context_family,
                    "assistance_level": evidence.assistance_level,
                    "probe_phase": evidence.probe_phase,
                })),
                recorded_at: ctx.now,
            });
        }
        let assessed = assess_mastery(&trail);
        let accepted = match proposal.new_status {
            LearningItemStatus::ProvisionallyMastered => assessed != MasteryAssessment::NotMastered,
            LearningItemStatus::Mastered => assessed == MasteryAssessment::Full,
            // Non-mastery statuses are tutor's call.
            _ => true,
        };
        if accepted {
            changes.push((item_id, proposal.new_status));
        } else {
            warnings.push(format!(
                "mastery proposal for '{}' downgraded: evidence assessment is {assessed:?}",
                proposal.learning_item_id
            ));
        }
    }
    Ok(changes)
}
