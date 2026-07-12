//! Use-case services (02-workspace.md §2). Pure decision logic: handlers in
//! `apps/worker` fetch state via repositories, call these functions, then
//! persist the returned plan in one D1 batch. No IO, no HTTP types here —
//! that split keeps services deterministic and testable against real data
//! structures instead of mocks (16-rust-guidelines.md §6).

mod lesson_completion;
mod profiles;

pub use lesson_completion::{
    ErrorAction, ItemRef, LessonCompletionContext, LessonCompletionPlan, LessonOutcome, PlanError,
    PlannedEvidence, PlannedItem, PlannedListening, RECHECK_OFFSETS_DAYS, plan_lesson_completion,
};
pub use profiles::{ProfileCreationError, validate_profile_creation};
