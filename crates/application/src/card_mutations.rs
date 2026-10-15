//! Card mutation workflow (07-tutor-contract.md §3–4): quality gate,
//! duplicate detection, activation budget, edit classification. Every
//! attempted mutation yields an audit entry, applied or not.

use learning_core::{Card, CardId, CardStatus, Timestamp};
use tutor_contract::{CardMutation, MutationOperation};

/// Context the local runtime fetches before planning mutations.
#[derive(Debug, Clone)]
pub struct MutationContext {
    /// Existing non-retired cards in the profile (for duplicates + lookups).
    pub existing_cards: Vec<Card>,
    /// New-card activations still allowed today (activation budget).
    pub remaining_activation_budget: u8,
    /// Request instant.
    pub now: Timestamp,
}

/// Planned effect of one mutation.
#[derive(Debug, Clone, PartialEq)]
pub enum MutationEffect {
    /// Insert a new card (status per `activate`).
    CreateCard {
        /// Index into the mutation list.
        source_index: usize,
        /// `New` when within budget, else `Candidate`.
        initial_status: CardStatus,
    },
    /// Update card content, schedule preserved (cosmetic/clarifying).
    UpdateContent {
        /// Target card.
        card_id: CardId,
        /// Index into the mutation list.
        source_index: usize,
    },
    /// Status transition (suspend/restore/retire).
    SetStatus {
        /// Target card.
        card_id: CardId,
        /// New status.
        status: CardStatus,
        /// Reason recorded on the card.
        reason: Option<String>,
    },
    /// Add an accepted alternative.
    AddAcceptedAnswer {
        /// Target card.
        card_id: CardId,
        /// Alternative to add.
        answer: String,
    },
    /// Remove an accepted alternative.
    RemoveAcceptedAnswer {
        /// Target card.
        card_id: CardId,
        /// Alternative to remove.
        answer: String,
    },
}

/// Audit verdict per attempted mutation (`card_mutations.validation_result`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    /// Applied.
    Applied,
    /// Rejected; nothing changed.
    Rejected,
    /// Applied with a warning.
    Warned,
}

/// One audit row + optional effect.
#[derive(Debug, Clone, PartialEq)]
pub struct PlannedMutation {
    /// Index into the input list.
    pub source_index: usize,
    /// Audit verdict.
    pub verdict: Verdict,
    /// Why rejected/warned (audit `reason` complement).
    pub note: Option<String>,
    /// Effect to persist when applied.
    pub effect: Option<MutationEffect>,
}

/// Plans a batch of tutor mutations. Deterministic; every input mutation
/// produces exactly one [`PlannedMutation`] (audit completeness).
#[must_use]
pub fn plan_card_mutations(
    mutations: &[CardMutation],
    ctx: &MutationContext,
) -> Vec<PlannedMutation> {
    let mut budget = ctx.remaining_activation_budget;
    let mut seen_hashes: Vec<String> = Vec::new();
    mutations
        .iter()
        .enumerate()
        .map(|(i, m)| plan_one(i, m, ctx, &mut budget, &mut seen_hashes))
        .collect()
}

fn plan_one(
    index: usize,
    mutation: &CardMutation,
    ctx: &MutationContext,
    budget: &mut u8,
    seen_hashes: &mut Vec<String>,
) -> PlannedMutation {
    match mutation.operation {
        MutationOperation::Create => plan_create(index, mutation, ctx, budget, seen_hashes),
        MutationOperation::Update => plan_update(index, mutation, ctx),
        MutationOperation::Suspend => plan_status(index, mutation, ctx, CardStatus::Suspended),
        MutationOperation::Restore => plan_status(index, mutation, ctx, CardStatus::Review),
        MutationOperation::Retire => plan_status(index, mutation, ctx, CardStatus::Retired),
        MutationOperation::AddAcceptedAnswer | MutationOperation::RemoveAcceptedAnswer => {
            plan_accepted_answer(index, mutation, ctx)
        }
        // Split/merge/link/listening-variant: deferred to redesign loop
        // (WBS 8/M4) — audit as rejected, never silently dropped.
        MutationOperation::Split
        | MutationOperation::Merge
        | MutationOperation::Link
        | MutationOperation::AddListeningVariant => reject(
            index,
            format!(
                "{:?} not supported in M2; proposed via redesign loop",
                mutation.operation
            ),
        ),
    }
}

fn plan_create(
    index: usize,
    mutation: &CardMutation,
    ctx: &MutationContext,
    budget: &mut u8,
    seen_hashes: &mut Vec<String>,
) -> PlannedMutation {
    let Some(card) = &mutation.card else {
        return reject(index, "create without card payload".to_owned());
    };
    let hash = Card::content_hash_of(
        card.intent,
        &card.prompt,
        &card.answer,
        &card.accepted_answers,
    );
    let duplicate = ctx
        .existing_cards
        .iter()
        .find(|c| c.content_hash == hash && c.status != CardStatus::Retired);
    if let Some(existing) = duplicate {
        return reject(
            index,
            format!(
                "duplicate of card '{}' — never silently created",
                existing.id.0
            ),
        );
    }
    if seen_hashes.contains(&hash) {
        return reject(
            index,
            "duplicate of earlier create in this batch".to_owned(),
        );
    }
    seen_hashes.push(hash);

    let initial_status = if *budget > 0 {
        *budget -= 1;
        CardStatus::New
    } else {
        CardStatus::Candidate // over budget: stays candidate (07 §4)
    };
    let verdict_note = (initial_status == CardStatus::Candidate)
        .then(|| "activation budget exhausted; card stays candidate".to_owned());
    PlannedMutation {
        source_index: index,
        verdict: if verdict_note.is_some() {
            Verdict::Warned
        } else {
            Verdict::Applied
        },
        note: verdict_note,
        effect: Some(MutationEffect::CreateCard {
            source_index: index,
            initial_status,
        }),
    }
}

fn plan_update(index: usize, mutation: &CardMutation, ctx: &MutationContext) -> PlannedMutation {
    let Some(target) = find_target(mutation, ctx) else {
        return reject(index, "unknown card_id".to_owned());
    };
    let Some(payload) = &mutation.card else {
        return reject(index, "update without card payload".to_owned());
    };
    // Edit classification (07 §4): tested answer changed → semantic → not
    // auto-applied from a lesson result; needs explicit user confirmation
    // (PATCH /cards/{id} with edit_class=semantic).
    let answer_changed = payload.answer.trim().to_lowercase()
        != target.answer.trim().to_lowercase()
        || payload.intent != target.intent;
    if answer_changed {
        return reject(
            index,
            "semantic edit (answer/intent changed) requires explicit confirmation".to_owned(),
        );
    }
    PlannedMutation {
        source_index: index,
        verdict: Verdict::Applied,
        note: None,
        effect: Some(MutationEffect::UpdateContent {
            card_id: target.id.clone(),
            source_index: index,
        }),
    }
}

fn plan_status(
    index: usize,
    mutation: &CardMutation,
    ctx: &MutationContext,
    status: CardStatus,
) -> PlannedMutation {
    let Some(target) = find_target(mutation, ctx) else {
        return reject(index, "unknown card_id".to_owned());
    };
    let valid = match status {
        CardStatus::Suspended => target.status != CardStatus::Retired,
        CardStatus::Review => target.status == CardStatus::Suspended, // restore
        CardStatus::Retired => true,
        _ => false,
    };
    if !valid {
        return reject(
            index,
            format!("cannot move card from {:?} to {status:?}", target.status),
        );
    }
    PlannedMutation {
        source_index: index,
        verdict: Verdict::Applied,
        note: None,
        effect: Some(MutationEffect::SetStatus {
            card_id: target.id.clone(),
            status,
            reason: mutation.reason.clone(),
        }),
    }
}

fn plan_accepted_answer(
    index: usize,
    mutation: &CardMutation,
    ctx: &MutationContext,
) -> PlannedMutation {
    let Some(target) = find_target(mutation, ctx) else {
        return reject(index, "unknown card_id".to_owned());
    };
    let Some(answer) = mutation
        .accepted_answer
        .clone()
        .filter(|a| !a.trim().is_empty())
    else {
        return reject(index, "missing accepted_answer".to_owned());
    };
    let exists = target
        .accepted_answers
        .iter()
        .any(|a| a.trim().to_lowercase() == answer.trim().to_lowercase());
    let effect = match mutation.operation {
        MutationOperation::AddAcceptedAnswer if !exists => {
            Some(MutationEffect::AddAcceptedAnswer {
                card_id: target.id.clone(),
                answer,
            })
        }
        MutationOperation::RemoveAcceptedAnswer if exists => {
            Some(MutationEffect::RemoveAcceptedAnswer {
                card_id: target.id.clone(),
                answer,
            })
        }
        _ => None,
    };
    match effect {
        Some(effect) => PlannedMutation {
            source_index: index,
            verdict: Verdict::Applied,
            note: None,
            effect: Some(effect),
        },
        None => reject(
            index,
            "accepted answer already in requested state".to_owned(),
        ),
    }
}

fn find_target<'a>(mutation: &CardMutation, ctx: &'a MutationContext) -> Option<&'a Card> {
    let id = mutation.card_id.as_deref()?;
    ctx.existing_cards.iter().find(|c| c.id.0 == id)
}

fn reject(index: usize, note: String) -> PlannedMutation {
    PlannedMutation {
        source_index: index,
        verdict: Verdict::Rejected,
        note: Some(note),
        effect: None,
    }
}
