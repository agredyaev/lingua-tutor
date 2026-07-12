//! Use-case services (02-workspace.md §2). Pure decision logic: handlers in
//! `apps/worker` fetch state via repositories, call these functions, then
//! persist the returned plan in one D1 batch. No IO, no HTTP types here —
//! that split keeps services deterministic and testable against real data
//! structures instead of mocks (16-rust-guidelines.md §6).

mod card_mutations;
mod lesson_completion;
mod profiles;
mod review;

pub use card_mutations::{
    MutationContext, MutationEffect, PlannedMutation, Verdict, plan_card_mutations,
};
pub use lesson_completion::{
    ErrorAction, ItemRef, LessonCompletionContext, LessonCompletionPlan, LessonOutcome, PlanError,
    PlannedEvidence, PlannedItem, PlannedListening, RECHECK_OFFSETS_DAYS, plan_lesson_completion,
};
pub use profiles::{ProfileCreationError, validate_profile_creation};
pub use review::{QueueCandidate, SIGNIFICANT_OVERDUE_SECONDS, build_queue, next_card_status};
