#![allow(clippy::unwrap_used)] // Static source-fixture assertions intentionally fail fast on malformed fixtures.
//! Static regression gates for DOD/PWP migrations.
//! Only audit completed paths; incomplete areas remain visible in docs/dod-preflight.md.

const LESSONS: &str = include_str!("../../../apps/local/src/handlers/lessons.rs");
const ERRORS: &str = include_str!("../../../apps/local/src/handlers/records.rs");
const CARDS: &str = include_str!("../../../apps/local/src/handlers/cards.rs");
const PROFILES: &str = include_str!("../../../apps/local/src/handlers/profiles.rs");
const CORE_CARD: &str = include_str!("../../../crates/learning-core/src/card.rs");
const CORE: &str = include_str!("../../../crates/learning-core/src/lib.rs");
const LOCAL_MANIFEST: &str = include_str!("../../../apps/local/Cargo.toml");
const ROOT_MANIFEST: &str = include_str!("../../../Cargo.toml");
const LESSON_PLAN: &str = include_str!("../../../apps/local/src/operations/lesson_plan.rs");

fn between<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    source
        .split_once(start)
        .unwrap()
        .1
        .split_once(end)
        .unwrap()
        .0
}

#[test]
fn public_read_paths_never_reconstruct_domain_objects() {
    for source in [
        between(
            LESSONS,
            "pub(crate) fn list_lessons_for",
            "pub(crate) fn get_lesson",
        ),
        between(
            LESSONS,
            "pub(crate) fn get_lesson",
            "pub(crate) fn validate_import_text",
        ),
        between(
            ERRORS,
            "pub(crate) fn list_error_targets_for",
            "pub(crate) fn get_error_target",
        ),
        between(
            ERRORS,
            "pub(crate) fn get_error_target",
            "#[derive(serde::Deserialize)]\npub(crate) struct ListeningAttemptParams",
        ),
        between(
            ERRORS,
            "fn active_error_context",
            "fn planner_history_context",
        ),
        between(CARDS, "pub(crate) fn list_for", "pub(crate) fn get"),
        between(CARDS, "pub(crate) fn get", "#[derive(serde::Deserialize)]"),
        between(
            PROFILES,
            "pub(crate) fn list_profiles",
            "#[derive(Deserialize)]\npub(crate) struct PatchProfileParams",
        ),
    ] {
        assert!(
            !source.contains("into_domain"),
            "domain materialization returned to public read path"
        );
        assert!(
            !source.contains("::from_row"),
            "owned SQL row returned to public read path"
        );
    }
}

#[test]
fn adaptation_has_no_owned_compatibility_domain_api() {
    let files = [
        include_str!("../../../crates/adaptation/src/capability.rs"),
        include_str!("../../../crates/adaptation/src/focus.rs"),
        include_str!("../../../crates/adaptation/src/tasks.rs"),
        include_str!("../../../crates/adaptation/src/queue.rs"),
        include_str!("../../../crates/adaptation/src/planner.rs"),
        include_str!("../../../crates/adaptation/src/rules.rs"),
        include_str!("../../../crates/adaptation/src/calibration.rs"),
    ];
    for source in files {
        for obsolete in [
            "pub struct EvidenceObservation",
            "pub struct CapabilityEstimate {",
            "pub struct ItemCapabilityState {",
            "pub struct TutorFocusThread {",
            "pub struct TaskCandidate {",
            "pub struct PlannerPreferences {",
            "pub struct LessonPlannerInput {",
            "pub struct PlanPriority {",
            "pub struct AdaptiveLessonPlan {",
            "pub struct CardSignals {",
            "pub struct Recommendation {",
            "pub struct CalibrationSnapshot {",
            "pub fn project_item_capabilities(",
            "pub fn build_tutor_focus_threads(",
            "pub fn score_task_candidates(",
            "pub fn build_tutor_task_queue(",
            "pub fn plan_next_lesson(",
            "pub fn evaluate_card_rules(",
            "pub fn summarize_intervention_effects(",
        ] {
            assert!(
                !source.contains(obsolete),
                "owned adaptation compatibility API remains: {obsolete}"
            );
        }
    }
}

#[test]
fn adaptation_internal_relationships_use_dense_u32_keys() {
    let calibration = include_str!("../../../crates/adaptation/src/calibration.rs");
    assert!(calibration.contains("item_key: Vec<u32>"));
    assert!(calibration.contains("intervention_item_key: Vec<u32>"));
    assert!(!calibration.contains("intervention_item_id: Vec<"));
    assert!(!calibration.contains("item_id: Vec<std::borrow::Cow"));

    let selection = include_str!("../../../apps/local/src/operations/adaptive_selection.rs");
    assert!(selection.contains("item_key: [u32; MAX_TASKS]"));
    assert!(selection.contains("item_key: [u32; MAX_FOCUS]"));
    assert!(selection.contains("selection.candidates.item_key[ci] == item_key"));
    assert!(selection.contains("selection.focus.item_key[fi] == item_key"));
    assert!(
        !selection
            .contains("text_at(&selection.text, selection.candidates.learning_item_id[ci]) == id")
    );
}

#[test]
fn native_llm_surface_preserves_authoritative_methodology_contract() {
    let records = include_str!("../../../apps/local/src/handlers/records.rs");
    let mcp = include_str!("../../../apps/local/src/mcp_tools.rs");
    let start = include_str!("../../../apps/local/src/operations/start_lesson.rs");
    let selection = include_str!("../../../apps/local/src/operations/adaptive_selection.rs");
    let queue = include_str!("../../../crates/adaptation/src/queue.rs");
    let validation = include_str!("../../../crates/tutor-contract/src/validate.rs");

    assert!(
        mcp.contains("Authoritative adaptive tutor context")
            && mcp.contains("Fetch this before every lesson")
            && mcp.contains("final authoritative snapshot"),
        "MCP tool descriptions no longer make the context-first LLM flow explicit"
    );
    for name in [
        "Partial inspection only: learning items",
        "Partial plan inspection only",
        "Partial inspection only: open error targets",
        "Partial inspection only: review workload summary",
        "Partial inspection only: failing cards",
    ] {
        assert!(
            mcp.contains(name),
            "partial MCP read can again masquerade as sufficient teaching context: {name}"
        );
    }

    assert!(
        records.contains(
            "минимум 1 diagnostic baseline и минимум 2 независимых measured probes overall"
        ) && validation
            .contains("at least 1 diagnostic baseline and 2 independent measured probes overall"),
        "LLM lesson-result instructions drifted from backend measurement validation"
    );
    assert!(
        !records.contains("минимум 2 полностью измеренных diagnostic probes"),
        "stale duplicate-diagnostic rule returned to the live LLM contract"
    );
    assert!(
        records.contains("lesson_result_contract(profile_id, &source_language)")
            && records
                .matches("\"prompt_language\": source_language")
                .count()
                >= 2
            && !records.contains("\"prompt_language\": \"ru\""),
        "live LLM contract reintroduced a compiled source-language default"
    );

    for required in [
        "\"planner_version\": \"planner-v3\"",
        "\"adaptive_lesson_plan\": plan_value",
        "\"tutor_tasks\": tutor_tasks",
        "let plan_json = plan_value.to_string()",
    ] {
        assert!(
            start.contains(required),
            "start_lesson no longer returns the exact persisted final plan snapshot: {required}"
        );
    }

    assert!(
        selection.contains("criterion_for_action(&action, entry.capability)")
            && queue.contains("pub fn criterion_for_action(")
            && queue.contains("\"diagnostic_spontaneous\"")
            && queue.contains("\"listening_probe\"")
            && queue.contains("\"listening_repair\""),
        "LLM task criteria can again collapse diagnostic and intervention semantics"
    );
}

#[test]
fn adaptation_processing_has_no_pointer_or_map_object_graphs() {
    let files = [
        include_str!("../../../crates/adaptation/src/capability.rs"),
        include_str!("../../../crates/adaptation/src/focus.rs"),
        include_str!("../../../crates/adaptation/src/tasks.rs"),
        include_str!("../../../crates/adaptation/src/queue.rs"),
        include_str!("../../../crates/adaptation/src/planner.rs"),
        include_str!("../../../crates/adaptation/src/rules.rs"),
        include_str!("../../../crates/adaptation/src/calibration.rs"),
        include_str!("../../../apps/local/src/operations/adaptive_selection.rs"),
        include_str!("../../../apps/local/src/operations/capability_read.rs"),
        include_str!("../../../apps/local/src/operations/planner_read.rs"),
        include_str!("../../../apps/local/src/operations/lesson_plan.rs"),
    ];
    for source in files {
        for forbidden in [
            "HashMap<",
            "HashSet<",
            "BTreeMap<",
            "BTreeSet<",
            "Rc<",
            "Arc<",
            "Weak<",
            "Box<",
            "&dyn ",
            "*const ",
            "*mut ",
            "UnsafeCell<",
        ] {
            assert!(
                !source.contains(forbidden),
                "pointer/map object graph reintroduced into processing path: {forbidden}"
            );
        }
    }
}

#[test]
fn learning_core_contains_only_scalar_contracts_and_pure_rules() {
    for obsolete in [
        "pub struct LearnerProfile",
        "pub struct LearnerLanguageProfile",
        "pub struct Lesson",
        "pub struct LearningItem",
        "pub struct LearningItemEvidence",
        "pub struct ErrorTarget",
        "pub enum RecommendationSubject",
    ] {
        assert!(
            !CORE.contains(obsolete),
            "retained entity record remains: {obsolete}"
        );
    }
    assert!(
        !CORE_CARD.contains("pub struct Card"),
        "retained Card entity remains"
    );
    assert!(
        !CORE.contains("mod ids;"),
        "owned string entity-ID module remains"
    );
    assert!(
        !CORE.contains("mod mastery;"),
        "unused AoS mastery module remains"
    );
}

#[test]
fn card_and_error_rules_are_data_passes_not_entity_methods() {
    assert!(
        !CORE_CARD.contains("impl Card {"),
        "Card behavior reintroduced"
    );
    assert!(
        !CORE.contains("impl ErrorTarget {"),
        "ErrorTarget behavior reintroduced"
    );
    assert!(CORE_CARD.contains("pub fn card_content_hash("));
    assert!(CORE.contains("pub fn error_escalation_due("));
    assert!(CORE.contains("pub fn error_method_change_due("));
}

#[test]
fn record_views_use_scalar_profile_reads_without_domain_reconstruction() {
    let records = include_str!("../../../apps/local/src/handlers/records.rs");
    let live = between(
        records,
        "pub(crate) fn learning_state_for",
        "mod error_projection_tests",
    );
    assert!(
        !live.contains("into_domain()"),
        "profile domain reconstruction reintroduced"
    );
    assert!(
        !live.contains("learning_core::LearnerLanguageProfile"),
        "domain profile reintroduced"
    );
    for obsolete in [
        "struct RecommendationRow",
        "struct RedesignRow",
        "struct DueRecheckRow",
        "struct CardSlim",
        "struct ItemRowSlim",
        "struct CountRow",
        "struct PlannerHistoryRow",
        "struct PlannerHistoryContext",
    ] {
        assert!(
            !live.contains(obsolete),
            "unnecessary read wrapper: {obsolete}"
        );
    }
}

#[test]
fn planner_history_uses_one_sql_pass_and_only_calibration_columns() {
    let path = between(
        ERRORS,
        "fn planner_history_context",
        "pub(crate) fn tutor_context_for",
    );
    assert!(path.contains("ORDER BY created_at DESC LIMIT 10"));
    assert!(path.contains("while let Some(row) = rows.next()?"));
    assert!(path.contains("let mut timestamps = Vec::with_capacity(10);"));
    assert!(path.contains("let mut capability_json = Vec::with_capacity(10);"));
    assert!(path.contains("let mut outcome_json = Vec::with_capacity(10);"));
    assert!(
        !path.contains("task_candidates_json"),
        "planned candidates must not drive calibration"
    );
    assert!(
        !path.contains("query_map("),
        "intermediate SQL rows reintroduced"
    );
    assert!(
        !path.contains("collect::<"),
        "intermediate row collection reintroduced"
    );
    assert!(!path.contains("PlannerHistoryRow"));
    assert!(!path.contains("PlannerHistoryContext"));
}

#[test]
fn calibration_is_explicit_columnar_processing_without_domain_traits() {
    let source = include_str!("../../../crates/adaptation/src/calibration.rs");
    let live = source.split_once("#[cfg(test)]").unwrap().0;
    for obsolete in [
        "trait CalItem",
        "trait CalCandidate",
        "trait CalSnapshot",
        "BTreeMap<",
        "BTreeSet<",
        "struct CompactSnapshot",
    ] {
        assert!(
            !live.contains(obsolete),
            "calibration regression: {obsolete}"
        );
    }
    for required in [
        "item_end: Vec<u32>",
        "intervention_end: Vec<u32>",
        "item_key_text: Vec<",
        "item_key: Vec<u32>",
        "intervention_item_key: Vec<u32>",
        "intervention_action: Vec<",
        "item_rate: Vec<",
        "item_observed_at: Vec<",
    ] {
        assert!(live.contains(required), "missing SoA storage: {required}");
    }
}

#[test]
fn review_read_views_stream_sql_into_fixed_counters() {
    let review = include_str!("../../../apps/local/src/handlers/review.rs");
    for (from, to) in [
        ("pub(crate) fn summary_for", "pub(crate) fn forecast_for"),
        ("pub(crate) fn forecast_for", "pub(crate) fn retention_for"),
        ("pub(crate) fn retention_for", "pub(crate) fn evaluate"),
    ] {
        let read = between(review, from, to);
        assert!(
            !read.contains("collect::<rusqlite::Result<Vec<_>>>()"),
            "{from} must not materialize SQL rows"
        );
        assert!(
            read.contains("while let Some(row) = rows.next()?"),
            "streaming pass missing: {from}"
        );
    }
}

#[test]
fn legacy_review_runtime_implementation_is_absent() {
    let review = include_str!("../../../apps/local/src/handlers/review.rs");
    let repository = include_str!("../../../apps/local/src/repositories/review.rs");
    for legacy in [
        "enum Scheduler",
        "fn scheduler_id(name: &str)",
        "struct ReplayEventRow",
        "submit_event_params(",
        "pub(crate) fn undo_last(",
        "pub(crate) fn complete_session(",
        "ScheduleStateRow",
    ] {
        assert!(
            !review.contains(legacy) && !repository.contains(legacy),
            "legacy review implementation remains: {legacy}"
        );
    }
}

#[test]
fn answer_evaluation_uses_only_required_sql_columns() {
    let review = include_str!("../../../apps/local/src/handlers/review.rs");
    let path = review.split_once("pub(crate) fn evaluate(").unwrap().1;
    assert!(!path.contains("CardRow::from_row"));
    assert!(!path.contains("into_domain()"));
    assert!(path.contains("SELECT answer, accepted_answers_json, intent, status, created_by"));
}

#[test]
fn sqlite_enum_conversion_uses_pure_functions_not_trait_dispatch() {
    let repository = include_str!("../../../apps/local/src/repositories/mod.rs");
    assert!(
        !repository.contains("pub trait DbText"),
        "DbText trait reintroduced"
    );
    assert!(
        !repository.contains("impl DbText for"),
        "DbText object adapter reintroduced"
    );
    assert!(
        !repository.contains("pub fn parse_enum<T:"),
        "generic enum conversion reintroduced"
    );
    assert!(
        !repository.contains("pub fn enum_str<T:"),
        "generic enum conversion reintroduced"
    );
    for name in [
        "profile_status_from_db",
        "lesson_status_from_db",
        "lesson_source_from_db",
        "learning_item_type_from_db",
        "learning_item_status_from_db",
        "evidence_kind_from_db",
        "error_target_status_from_db",
        "card_status_from_db",
        "card_actor_from_db",
        "core_error_category_from_db",
        "card_intent_from_db",
        "mutation_operation_from_db",
    ] {
        assert!(
            repository.contains(name),
            "missing explicit enum pass: {name}"
        );
    }
}

#[test]
fn repository_domain_row_adapters_are_absent() {
    for source in [
        include_str!("../../../apps/local/src/repositories/profiles.rs"),
        include_str!("../../../apps/local/src/repositories/lessons.rs"),
        include_str!("../../../apps/local/src/repositories/records.rs"),
        include_str!("../../../apps/local/src/repositories/cards.rs"),
        include_str!("../../../apps/local/src/repositories/review.rs"),
    ] {
        for obsolete in [
            "LanguageProfileRow",
            "LearnerProfileRow",
            "LessonRow",
            "ItemRow",
            "EvidenceRow",
            "ErrorTargetRow",
            "CardRow",
            "ScheduleStateRow",
            "PackRow",
            "into_domain(",
            "into_state(",
            "from_domain(",
            "from_state(",
        ] {
            assert!(
                !source.contains(obsolete),
                "legacy repository adapter remains: {obsolete}"
            );
        }
    }
}

#[test]
fn indexed_queue_and_evidence_stores_have_no_domain_behavior_methods() {
    let queue = include_str!("../../../apps/local/src/operations/review_queue.rs");
    let evidence = include_str!("../../../apps/local/src/operations/capability_read.rs");
    assert!(
        !queue.contains("impl QueueColumns {"),
        "queue store contains behavior"
    );
    assert!(
        !evidence.contains("impl EvidenceColumns {"),
        "evidence store contains behavior"
    );
    for function in [
        "fn empty_queue_columns(",
        "fn shift_queue_column(",
        "fn empty_evidence_columns(",
        "fn clear_evidence_columns(",
        "fn append_evidence_row(",
    ] {
        assert!(
            queue.contains(function) || evidence.contains(function),
            "missing explicit processing pass: {function}"
        );
    }
}

#[test]
fn evidence_pass_initializes_only_observed_slots_not_entire_capacity() {
    let evidence = include_str!("../../../apps/local/src/operations/capability_read.rs");
    let reset = between(
        evidence,
        "fn clear_evidence_columns(",
        "fn append_evidence_row(",
    );
    assert!(
        !reset.contains(".fill(false)"),
        "full-capacity clearing reintroduced"
    );
    let append = between(evidence, "fn append_evidence_row(", "fn emit_one(");
    for present in [
        "data.context_key_present[i] = false",
        "data.context_family_present[i] = false",
        "data.assistance_present[i] = false",
        "data.response_present[i] = false",
    ] {
        assert!(
            append.contains(present),
            "reused evidence slot not initialized: {present}"
        );
    }
}

#[test]
fn local_runtime_has_only_the_new_processing_implementation() {
    let manifest = LOCAL_MANIFEST;
    let lib = include_str!("../../../apps/local/src/lib.rs");
    let handlers = [
        include_str!("../../../apps/local/src/handlers/profiles.rs"),
        include_str!("../../../apps/local/src/handlers/lessons.rs"),
        include_str!("../../../apps/local/src/handlers/records.rs"),
        include_str!("../../../apps/local/src/handlers/cards.rs"),
        include_str!("../../../apps/local/src/handlers/review.rs"),
        include_str!("../../../apps/local/src/mcp_tools.rs"),
    ];

    assert!(
        !manifest.contains("application ="),
        "legacy application crate still linked"
    );
    assert!(
        !ROOT_MANIFEST.contains("crates/application") && !ROOT_MANIFEST.contains("application ="),
        "legacy application crate still belongs to the workspace"
    );
    assert!(
        !lib.contains("mod old"),
        "old implementation module still linked"
    );
    let old_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../apps/local/src/old");
    assert!(
        !old_dir.exists(),
        "old implementation directory still exists"
    );
    for source in handlers {
        assert!(
            !source.contains("legacy_for_test"),
            "legacy helper still present"
        );
        assert!(
            !source.contains("application::"),
            "old application behavior still embedded"
        );
        assert!(
            !source.contains("use application"),
            "old application import still embedded"
        );
    }
}

#[test]
fn lesson_planning_uses_pure_passes_over_indexed_storage() {
    let lesson = include_str!("../../../apps/local/src/operations/lesson_plan.rs");
    assert!(
        !lesson.contains("impl LessonPlanState {"),
        "storage must not carry methods"
    );
    assert!(
        !lesson.contains("impl LessonPlan {"),
        "result arrays must not carry methods"
    );
    for operation in [
        "fn new_lesson_plan_state(",
        "fn add_known_lesson_item(",
        "fn add_lesson_mastery_signal(",
        "fn plan_reused_item_id",
        "fn plan_resolved_item_id",
    ] {
        assert!(lesson.contains(operation), "missing pass {operation}");
    }
}

#[test]
fn adaptive_selection_has_no_domain_methods() {
    let s = include_str!("../../../apps/local/src/operations/adaptive_selection.rs");
    assert!(!s.contains("impl Selection {"));
    for n in [
        "fn new_selection(",
        "fn observe_selection(",
        "fn selection_tutor_tasks(",
    ] {
        assert!(s.contains(n), "missing pass: {n}");
    }
}

#[test]
fn candidate_and_focus_columns_use_explicit_passes() {
    let s = include_str!("../../../apps/local/src/operations/adaptive_selection.rs");
    assert!(!s.contains("impl CandidateColumns {"));
    assert!(!s.contains("impl FocusColumns {"));
    for name in [
        "fn new_candidate_columns(",
        "fn copy_candidate_columns(",
        "fn new_focus_columns(",
        "fn copy_focus_columns(",
    ] {
        assert!(s.contains(name), "missing pass: {name}");
    }
}

#[test]
fn review_runtime_uses_persisted_profile_policy() {
    let ops = include_str!("../../../apps/local/src/operations/review_ops.rs");
    assert!(
        !ops.contains("SchedulerSettings::default()"),
        "review runtime must not ignore persisted desired_retention"
    );
    assert!(ops.contains("desired_retention"));
    assert!(ops.contains("scheduler_settings_for_profile"));
}

#[test]
fn adaptation_storage_has_no_processing_methods() {
    let queue = include_str!("../../../crates/adaptation/src/queue.rs");
    let planner = include_str!("../../../crates/adaptation/src/planner.rs");
    let capability = include_str!("../../../crates/adaptation/src/capability.rs");

    for (name, source, forbidden) in [
        (
            "queue",
            queue,
            "impl<'a, const N: usize> TutorTaskQueueFixed",
        ),
        ("planner", planner, "impl<'a> AdaptiveLessonPlanFixed"),
        ("capability-context", capability, "impl<'a> ContextSetView"),
        ("capability-text-set", capability, "impl<'a> TextSetRef"),
    ] {
        assert!(
            !source.contains(forbidden),
            "{name} stores processing behavior on data: {forbidden}"
        );
    }
}

#[test]
fn review_hashing_is_a_pass_not_a_domain_method() {
    let source = include_str!("../../../crates/review-engine/src/lib.rs");
    assert!(!source.contains("impl ScheduledState {"));
    assert!(source.contains("pub fn state_hash(state: &ScheduledState) -> String"));
}

#[test]
fn review_scheduler_uses_explicit_algorithm_passes_not_object_dispatch() {
    let source = include_str!("../../../crates/review-engine/src/lib.rs");
    assert!(
        !source.contains("pub trait ReviewScheduler"),
        "virtual scheduler contract returned"
    );
    assert!(
        !source.contains("&dyn ReviewScheduler"),
        "virtual replay argument returned"
    );
    assert!(source.contains("pub const FSRS_ID: u32"));
    assert!(source.contains("pub const FALLBACK_ID: u32"));
    for function in [
        "pub fn new_scheduler_scratch(",
        "pub fn initial_state(",
        "pub fn apply_review(",
        "pub fn predicted_retrievability(",
        "pub fn replay(",
    ] {
        assert!(
            source.contains(function),
            "missing explicit pass: {function}"
        );
    }
    for file in [
        include_str!("../../../crates/review-engine/src/fsrs_adapter.rs"),
        include_str!("../../../crates/review-engine/src/fallback.rs"),
    ] {
        assert!(!file.contains("impl ReviewScheduler for"));
        assert!(!file.contains("impl FsrsScheduler {"));
        assert!(!file.contains("impl FallbackScheduler {"));
    }
}

#[test]
fn runtime_lesson_contract_has_no_language_pack_dispatch() {
    let source = include_str!("../../../apps/local/src/handlers/records.rs");
    assert!(!source.contains("language_pack_registry"));
    assert!(!source.contains("language_pack_es_es"));
    assert!(!source.contains("dyn LanguagePack"));
}

#[test]
fn card_plan_state_has_no_domain_methods() {
    let source = include_str!("../../../apps/local/src/operations/card_plan.rs");
    assert!(
        !source.contains("impl CardPlanState {"),
        "CardPlanState must be storage only"
    );
    for function in [
        "fn new_card_plan_state(",
        "fn set_card_content_hash(",
        "fn card_hash_at(",
        "fn add_existing_card_json(",
        "fn find_card_index(",
        "fn card_accepted_contains(",
    ] {
        assert!(
            source.contains(function),
            "missing explicit pass {function}"
        );
    }
}

#[test]
fn card_plan_output_uses_pure_functions_not_methods() {
    let cards = include_str!("../../../apps/local/src/operations/card_plan.rs");
    assert!(
        !cards.contains("impl CardPlan {"),
        "result arrays carry behavior"
    );
    for name in [
        "fn empty_card_plan(",
        "fn append_card_plan_entry(",
        "fn card_plan_status(",
        "fn card_plan_note(",
    ] {
        assert!(
            cards.contains(name),
            "missing direct card plan operation: {name}"
        );
    }
}

#[test]
fn lesson_planning_retains_bounded_indexed_layout() {
    assert!(LESSON_PLAN.contains("new_item_source: [u32;"));
    assert!(LESSON_PLAN.contains("evidence_item: [ResolvedItem;"));
    assert!(LESSON_PLAN.contains("Known(u32)"));
    assert!(LESSON_PLAN.contains("Reused(u32)"));
    assert!(LESSON_PLAN.contains("New(u32)"));
    assert!(LESSON_PLAN.contains("status_source: [u32;"));
    let cards = include_str!("../../../apps/local/src/operations/card_plan.rs");
    assert!(cards.contains("source_index: [u32; MAX_MUTATIONS]"));
    assert!(!cards.contains("source_index: [usize; MAX_MUTATIONS]"));
    assert!(!LESSON_PLAN.contains("Arc<"));
    assert!(!LESSON_PLAN.contains("Rc<"));
    assert!(!LESSON_PLAN.contains("HashMap<"));
}

#[test]
fn runtime_and_workspace_do_not_compile_language_packs() {
    assert!(
        !ROOT_MANIFEST.contains("language-packs/"),
        "workspace still compiles concrete language-pack crates"
    );
    assert!(
        !ROOT_MANIFEST.contains("language-pack-registry"),
        "workspace still carries the obsolete language-pack registry"
    );
    for removed in [
        "language-pack-registry",
        "language-pack-es-es",
        "synthetic-test-pack",
    ] {
        assert!(
            !LOCAL_MANIFEST.contains(removed),
            "local runtime still links removed component {removed}"
        );
    }

    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    assert!(
        !root.join("language-packs").exists(),
        "concrete language-pack source remains in the repository"
    );
}

#[test]
fn obsolete_language_pack_api_is_removed() {
    assert!(
        !ROOT_MANIFEST.contains("language-pack-api"),
        "workspace still carries the transitional language-pack API"
    );
}

#[test]
fn tutor_wire_contract_does_not_expose_language_pack_metadata() {
    let contract = include_str!("../../../crates/tutor-contract/src/lib.rs");
    let validation = include_str!("../../../crates/tutor-contract/src/validate.rs");
    let records = include_str!("../../../apps/local/src/handlers/records.rs");
    for forbidden in ["pub language_pack_id:", "pub language_pack_version:"] {
        assert!(
            !contract.contains(forbidden),
            "obsolete tutor field remains: {forbidden}"
        );
    }
    assert!(!validation.contains("r.language_pack_id"));
    assert!(!validation.contains("r.language_pack_version"));
    assert!(!records.contains("\"language_pack\": manifest"));
}

#[test]
fn current_storage_schema_has_no_language_pack_identity() {
    let init = include_str!("../../../apps/local/migrations/0001_init.sql");
    for forbidden in [
        "CREATE TABLE language_packs",
        "language_pack_id",
        "language_pack_version",
    ] {
        assert!(
            !init.contains(forbidden),
            "obsolete storage identity remains: {forbidden}"
        );
    }
    let production = [
        include_str!("../../../apps/local/src/repositories/profiles.rs"),
        include_str!("../../../apps/local/src/repositories/lessons.rs"),
        include_str!("../../../apps/local/src/repositories/records.rs"),
        include_str!("../../../apps/local/src/repositories/cards.rs"),
    ];
    for source in production {
        assert!(!source.contains("language_pack_id"));
        assert!(!source.contains("language_pack_version"));
    }
}

#[test]
fn current_schema_has_no_pack_semantics_or_dead_language_fields() {
    let init = include_str!("../../../apps/local/migrations/0001_init.sql");
    for forbidden in [
        "pack_error_code",
        "pack_metadata_json",
        "answer_evaluation_mode",
        "language_metadata_json",
    ] {
        assert!(
            !init.contains(forbidden),
            "obsolete semantic field remains in current schema: {forbidden}"
        );
    }

    let contract = include_str!("../../../crates/tutor-contract/src/lib.rs");
    let shared = include_str!("../../../crates/shared-schema/src/lib.rs");
    assert!(!contract.contains("pack_error_code"));
    assert!(!shared.contains("pack_error_code"));
}

#[test]
fn current_runtime_surface_has_no_language_pack_vocabulary() {
    let current = [
        include_str!("../../../apps/local/migrations/0001_init.sql"),
        include_str!("../../../apps/local/src/handlers/profiles.rs"),
        include_str!("../../../apps/local/src/handlers/review.rs"),
        include_str!("../../../crates/shared-schema/src/lib.rs"),
    ];
    for source in current {
        for forbidden in [
            "language_pack",
            "language pack",
            "language-pack",
            "pack_upgrade_warning",
            "pack tags",
            "active pack",
        ] {
            assert!(
                !source.contains(forbidden),
                "obsolete pack vocabulary remains: {forbidden}"
            );
        }
    }
}

#[test]
fn current_architecture_docs_do_not_describe_removed_language_pack_model() {
    let current_specs = [
        include_str!("../../../docs/spec/02-workspace.md"),
        include_str!("../../../docs/spec/03-domain-model.md"),
        include_str!("../../../docs/spec/04-database.md"),
        include_str!("../../../docs/spec/06-language-boundary.md"),
        include_str!("../../../docs/spec/07-tutor-contract.md"),
        include_str!("../../../docs/spec/12-testing.md"),
        include_str!("../../../docs/spec/13-roadmap.md"),
        include_str!("../../../docs/spec/14-traceability.md"),
        include_str!("../../../docs/spec/15-wbs.md"),
        include_str!("../../../docs/spec/17-documentation.md"),
    ];
    for source in current_specs {
        for forbidden in [
            "language-pack-api",
            "language-pack-registry",
            "language-pack-es-es",
            "language_pack_id",
            "language_pack_version",
            "PackUpgradeWarning",
            "language-pack discovery",
            "Language-pack contract",
        ] {
            assert!(
                !source.contains(forbidden),
                "current docs retain removed architecture: {forbidden}"
            );
        }
    }
}

#[test]
fn language_profile_creation_has_no_compiled_default_language() {
    let page = include_str!("../../../apps/web/src/pages/languages.rs");
    assert!(
        page.contains("RwSignal::new(String::new())"),
        "language profile form must start without a compiled target language"
    );
    assert!(
        !page.contains("RwSignal::new(\"es-ES\".to_owned())"),
        "Spanish target locale is still compiled as the UI default"
    );
    assert!(
        !page.contains("RwSignal::new(\"ru\".to_owned())"),
        "Russian source language is still compiled as the UI default"
    );
}

#[test]
fn language_profile_picker_is_searchable_but_not_an_engine_allowlist() {
    let page = include_str!("../../../apps/web/src/pages/languages.rs");
    let write = include_str!("../../../apps/local/src/operations/profile_write.rs");
    let catalog = between(
        page,
        "const LANGUAGE_SUGGESTIONS:",
        "const TARGET_LOCALE_VARIANTS:",
    );

    assert!(
        catalog.matches("(\"").count() >= 180,
        "language search catalog must cover the complete ISO 639-1 base set"
    );
    for required in [
        "(\"nl\", \"Dutch\")",
        "(\"cy\", \"Welsh\")",
        "(\"zu\", \"Zulu\")",
        "(\"nl-NL\", \"Dutch — Netherlands\")",
        "(\"nl-BE\", \"Dutch — Belgium\")",
        "class=\"language-search\"",
        "class=\"language-search-menu\"",
    ] {
        assert!(
            page.contains(required),
            "language picker lost searchable coverage: {required}"
        );
    }
    assert!(
        !page.contains("<datalist"),
        "native datalist search is inconsistent across browsers and must not be the picker"
    );

    assert!(
        !write.contains("SUPPORTED_LANGUAGES")
            && !write.contains("SUPPORTED_LOCALES")
            && !write.contains("match target_locale"),
        "UI suggestions must not become a compiled backend language allowlist"
    );
}

#[test]
fn learner_navigation_exposes_only_learner_workflows() {
    let app = include_str!("../../../apps/web/src/app.rs");
    let dashboard = include_str!("../../../apps/web/src/pages/dashboard.rs");
    let nav = between(app, "<nav", "</nav>");

    for required in [
        "label=\"Home\"",
        "label=\"Review\"",
        "label=\"Library\"",
        "label=\"Progress\"",
        "label=\"History\"",
        "label=\"Languages\"",
    ] {
        assert!(
            nav.contains(required),
            "learner navigation lost required workflow: {required}"
        );
    }

    for forbidden in [
        "Import",
        "Export",
        "Settings",
        "System",
        "More",
        "/learn",
        "/errors",
        "/learning-items",
    ] {
        assert!(
            !nav.contains(forbidden),
            "operator/manual workflow leaked into learner navigation: {forbidden}"
        );
    }

    for forbidden in [
        r#"href="/learn""#,
        r#"href="/errors""#,
        r#"href="/learning-items""#,
    ] {
        assert!(
            !dashboard.contains(forbidden),
            "dashboard exposes engine/manual workflow to learner: {forbidden}"
        );
    }
}

#[test]
fn learner_home_hides_engine_control_state() {
    let dashboard = include_str!("../../../apps/web/src/pages/dashboard.rs");

    for required in [
        "Due for review",
        "Reviewed today",
        "Current focus areas",
        "Next lesson",
        "Tutor focus",
    ] {
        assert!(
            dashboard.contains(required),
            "learner home lost meaningful state: {required}"
        );
    }

    for forbidden in [
        "new_card_budget",
        "due_error_rechecks",
        "items_by_status",
        "Active error targets",
        "Learning items",
        "Open inventory",
    ] {
        assert!(
            !dashboard.contains(forbidden),
            "engine-control state leaked into learner home: {forbidden}"
        );
    }
}

#[test]
fn web_ui_has_compact_hierarchy_and_structured_learning_surfaces() {
    let css = include_str!("../../../apps/web/style.css");
    let app = include_str!("../../../apps/web/src/app.rs");
    let lessons = include_str!("../../../apps/web/src/pages/lessons.rs");
    let cards = include_str!("../../../apps/web/src/pages/cards.rs");
    let review = include_str!("../../../apps/web/src/pages/review.rs");

    for required in [
        "--font-display:",
        "--sidebar-width:",
        "--sidebar-collapsed-width:",
        "--color-bg: #0f0f11",
        "@media (min-width: 960px)",
        ".page-header .eyebrow",
        ".page {",
        ".page-header",
        ".entity-list",
        ".metric-grid",
        ".lesson-summary",
        ".lesson-next-goal",
        ".review-stage",
        ".field",
        ".language-search-menu",
    ] {
        assert!(
            css.contains(required),
            "shared visual hierarchy is missing: {required}"
        );
    }

    assert!(app.contains("class=\"brand\""));
    assert!(app.contains("sidebar_collapsed"));
    assert!(app.contains("class=\"sidebar-toggle\""));
    assert!(app.contains("class=\"nav-icon\""));
    assert!(css.contains(".topnav.sidebar-collapsed"));
    assert!(css.contains("main.sidebar-collapsed-main"));
    assert!(lessons.contains("class=\"lesson-summary\""));
    assert!(lessons.contains("class=\"lesson-next-goal\""));
    assert!(cards.contains("class=\"entity-list card-list\""));
    assert!(review.contains("class=\"review-stage\""));
    for forbidden in [
        "border: 1px dashed",
        "radial-gradient(",
        "inset 0 1px",
        "font-weight: 430",
        "font-weight: 520",
        "font-weight: 540",
        "font-weight: 560",
        "font-weight: 570",
        "font-weight: 580",
        "font-weight: 590",
        "font-weight: 620",
        "font-weight: 650",
    ] {
        assert!(
            !css.contains(forbidden),
            "premium minimal UI reintroduced visual drift: {forbidden}"
        );
    }

    for source in [app, lessons, cards, review] {
        assert!(
            !source.contains("style="),
            "web UI must keep presentation in the shared stylesheet"
        );
    }
}

#[test]
fn planner_reads_use_fixed_sql_bind_shapes_without_trait_objects() {
    let source = include_str!("../../../apps/local/src/operations/planner_read.rs");
    assert!(
        !source.contains("dyn rusqlite::ToSql"),
        "planner read path reintroduced virtual SQL bind dispatch"
    );
}

#[test]
fn methodology_distinguishes_stabilization_from_retention_and_transfer() {
    let methodology = include_str!("../../../apps/local/src/methodology_instructions_ru.md");

    for required in [
        "exact-repetition попытки только для первичной стабилизации/процедурализации",
        "не трактуй его как доказательство retention или transfer",
        "procedural/semantic variation",
        "не считаются несколькими spaced-success stages",
    ] {
        assert!(
            methodology.contains(required),
            "methodology lost repetition-vs-learning distinction: {required}"
        );
    }
}

#[test]
fn methodology_decomposes_task_failure_before_intervention() {
    let methodology = include_str!("../../../apps/local/src/methodology_instructions_ru.md");

    for required in [
        "Провал communicative task — это outcome, а не диагноз",
        "короткий discriminating probe",
        "Focus-on-form применяй реактивно",
        "Не лечи все причины одновременно",
    ] {
        assert!(
            methodology.contains(required),
            "methodology lost bottleneck-decomposition rule: {required}"
        );
    }
}

#[test]
fn methodology_optimizes_communicative_outcome_not_proxies() {
    let methodology = include_str!("../../../apps/local/src/methodology_instructions_ru.md");

    for required in [
        "минимизировать число учебных взаимодействий и время",
        "Оптимизируй реальный communicative outcome, а не proxy",
        "наблюдение → предполагаемый bottleneck → intervention → измеримый post/delayed outcome",
        "Если evidence недостаточно, сначала уменьши неопределённость коротким probe",
        "Вовлечённость, естественность разговора и чувство прогресса — важные constraints",
        "Объяснение — это intervention, а не доказательство learning",
        "Новый материал — это expansion после контроля текущих ограничений",
        "Если bottleneck остаётся нестабильным, новых элементов должно быть 0",
        "какой latent state пытается оценить",
    ] {
        assert!(
            methodology.contains(required),
            "methodology lost optimization-policy invariant: {required}"
        );
    }
}

#[test]
fn methodology_treats_intervention_effects_as_observational() {
    let methodology = include_str!("../../../apps/local/src/methodology_instructions_ru.md");
    let tutor_spec = include_str!("../../../docs/spec/07-tutor-contract.md");

    for required in [
        "один delayed sample — только preliminary signal",
        "не менее 2 независимых delayed samples",
    ] {
        assert!(
            methodology.contains(required),
            "methodology lost intervention-effect uncertainty guard: {required}"
        );
    }
    for required in [
        "One delayed sample is preliminary observational evidence only",
        "at least two independent delayed samples",
    ] {
        assert!(
            tutor_spec.contains(required),
            "tutor contract lost intervention-effect uncertainty guard: {required}"
        );
    }
}

#[test]
fn tutor_owns_curriculum_direction_and_engine_persists_observed_context() {
    let records = include_str!("../../../apps/local/src/handlers/records.rs");
    let start_lesson = include_str!("../../../apps/local/src/operations/start_lesson.rs");
    let mcp_tools = include_str!("../../../apps/local/src/mcp_tools.rs");
    let complete_lesson = include_str!("../../../apps/local/src/operations/complete_lesson.rs");
    let planner = include_str!("../../../crates/adaptation/src/planner.rs");
    let methodology = include_str!("../../../apps/local/src/methodology_instructions_ru.md");
    let tutor_spec = include_str!("../../../docs/spec/07-tutor-contract.md");
    let init = include_str!("../../../apps/local/migrations/0001_init.sql");
    let planner_history_migration =
        include_str!("../../../apps/local/migrations/0002_planner_history.sql");

    for forbidden in [
        "priority_scenarios",
        "curriculum_scenarios",
        "suggested_new_phrases",
        "canonical_context_families",
    ] {
        assert!(
            !records.contains(forbidden),
            "runtime still publishes compiled curriculum direction: {forbidden}"
        );
        assert!(
            !start_lesson.contains(forbidden),
            "lesson start still reads compiled/profile curriculum direction: {forbidden}"
        );
        assert!(
            !mcp_tools.contains(forbidden),
            "MCP surface still advertises compiled curriculum direction: {forbidden}"
        );
        assert!(
            !planner.contains(forbidden),
            "adaptation still owns curriculum direction: {forbidden}"
        );
        assert!(
            !methodology.contains(forbidden),
            "methodology still depends on compiled curriculum direction: {forbidden}"
        );
        assert!(
            !tutor_spec.contains(forbidden),
            "tutor contract still documents engine-owned curriculum direction: {forbidden}"
        );
    }

    for persisted in [
        "scenario",
        "next_goal",
        "CREATE TABLE learning_item_evidence",
        "CREATE TABLE error_occurrences",
    ] {
        assert!(
            init.contains(persisted),
            "current schema lost persisted learner/tutor state: {persisted}"
        );
    }
    assert!(
        planner_history_migration.contains("CREATE TABLE planner_decisions"),
        "planner history storage is no longer persisted"
    );
    for persisted_write in [
        "\"scenario\": result.lesson.scenario",
        "\"next_goal\": result.lesson.next_goal",
        "SET outcome_json = ?2",
    ] {
        assert!(
            complete_lesson.contains(persisted_write),
            "lesson completion stopped persisting tutor-owned context: {persisted_write}"
        );
    }
    for exposed in [
        "\"planner_history\"",
        "\"previous_next_goal_hypothesis\"",
        "\"learner_preferences\"",
    ] {
        assert!(
            records.contains(exposed),
            "tutor context lost persisted/adaptive state: {exposed}"
        );
    }
}

#[test]
fn methodology_requires_distributed_evidence_and_fresh_mastery() {
    let methodology = include_str!("../../../apps/local/src/methodology_instructions_ru.md");

    for required in [
        "Сила evidence и decision-rate для capability растут по независимым learning episodes",
        "Recall/task decision-rates усредняй по урокам",
        "старше 28 дней не используй как authority",
        "Переход в provisionally_mastered требует свежего unaided productive success",
        "как минимум 2 независимых предыдущих lesson episodes",
        "непустым `context_family`",
        "Свежий unresolved spontaneous-production failure блокирует full mastery",
        "84-дневном окне revalidation",
        "diagnostic baseline для того же learning item и той же target_capability",
        "`probe_phase=practice` фиксирует acquisition/repair performance",
        "не используй его для повышения operational capability state, profile-level unaided recall или mastery",
        "Успех по другой capability не закрывает этот failure",
        "assisted/unqualified success той же capability тоже не закрывает independent failure",
        "legacy-записи без этого флага fail closed",
        "использует exit-observation каждого prior episode",
        "Item-level weighted_success_rate и unaided_success_rate должны быть recency/episode-weighted",
        "Evidence с неизвестным assistance_level не создаёт independent episode",
        "до минимум 3 независимых episodes предпочитай discriminating diagnostic probe",
        "successful_contexts/successful_context_families — не пожизненные достижения",
        "учитывай последний независимый outcome",
        "самой давно не подтверждённой из listening/spontaneous_production/transfer",
        "Recognition/prompted-only failure не должен занимать дефицитный focus-revalidation reserve",
        "Bounded top-K не должен менять смысл решения",
        "сохраняй только лучшую action для каждой пары item+capability",
        "baseline расположен строго раньше explicit immediate outcome",
        "Numeric effectiveness delta считай между pre-lesson capability snapshot и delayed future snapshot",
        "Если один следующий независимый success/failure способен перевести решение через operational threshold",
        "оба исхода ведут к одному и тому же action, не трать отдельный diagnostic slot",
        "Prompt dependence считай установленной только когда достаточно evidence есть и на prompted, и на spontaneous стороне",
        "устойчиво подтверждённый gap (`listening_repair`)",
        "latency измерена как минимум в 2 независимых qualified episodes",
        "минимум 1 diagnostic baseline и минимум 2 независимых measured probes overall",
        "не увеличивай допустимый порог автоматически из-за числа слов в target_text",
    ] {
        assert!(
            methodology.contains(required),
            "methodology lost distributed-evidence/mastery invariant: {required}"
        );
    }
}

#[test]
fn planner_recall_state_comes_from_persisted_evidence_not_tutor_counters() {
    let source = include_str!("../../../apps/local/src/operations/planner_read.rs");

    assert!(
        source.contains("FROM learning_item_evidence e"),
        "planner recall state must be derived from persisted evidence"
    );
    assert!(
        source.contains("GROUP BY e.lesson_id"),
        "planner recall state must retain lesson-level independence"
    );
    assert!(
        source.contains("RECENT_SIGNAL_WINDOW_SECONDS"),
        "planner current-state evidence must have an explicit freshness window"
    );
    assert!(
        !source.contains("m.unaided_recall_count"),
        "planner reintroduced tutor-reported unaided counters as operational state"
    );
    assert!(
        !source.contains("m.aided_recall_count"),
        "planner reintroduced tutor-reported aided counters as operational state"
    );
}

#[test]
fn lesson_snapshot_and_live_tutor_use_same_selection_capacity() {
    let start = include_str!("../../../apps/local/src/operations/start_lesson.rs");
    let records = include_str!("../../../apps/local/src/handlers/records.rs");

    for source in [start, records] {
        assert!(
            source.contains("adaptive_selection::DEFAULT_CANDIDATE_LIMIT")
                && source.contains("adaptive_selection::DEFAULT_FOCUS_LIMIT"),
            "lesson-start snapshot and live tutor context must share one selection-capacity contract"
        );
    }
}

#[test]
fn adaptive_state_regressions_keep_episode_freshness_and_revalidation() {
    let capability_read = include_str!("../../../apps/local/src/operations/capability_read.rs");
    let planner_read = include_str!("../../../apps/local/src/operations/planner_read.rs");
    let lesson_plan = include_str!("../../../apps/local/src/operations/lesson_plan.rs");
    let complete_lesson = include_str!("../../../apps/local/src/operations/complete_lesson.rs");
    let selection = include_str!("../../../apps/local/src/operations/adaptive_selection.rs");
    let contract_validate = include_str!("../../../crates/tutor-contract/src/validate.rs");
    let capability = include_str!("../../../crates/adaptation/src/capability.rs");
    let tasks = include_str!("../../../crates/adaptation/src/tasks.rs");
    let calibration = include_str!("../../../crates/adaptation/src/calibration.rs");

    for required in [
        "episode_rn=1",
        "MASTERED_REVALIDATION_SLOTS",
        "status='mastered' AND class_ord<=?3",
        "COALESCE(json_extract(e.metadata_json,'$.probe_phase'),'legacy') != 'practice'",
        "PARTITION BY e.learning_item_id,e.kind",
        "qualified_success",
        "revalidation_activity_at",
        "revalidation_capability_count",
        "operational_missing_focus",
        "kind IN ('listening','spontaneous_production','transfer')",
        "operational_unresolved_failure=1",
        "unresolved_failure",
    ] {
        assert!(
            capability_read.contains(required),
            "capability sampling lost anti-massing/revalidation invariant: {required}"
        );
    }
    for required in [
        "GROUP BY e.lesson_id",
        "RECENT_SIGNAL_WINDOW_SECONDS",
        "completed_at>=?2",
        "COALESCE(json_extract(e.metadata_json,'$.probe_phase'),'legacy') != 'practice'",
    ] {
        assert!(
            planner_read.contains(required),
            "planner lost lesson-balanced/fresh-state invariant: {required}"
        );
    }
    assert!(
        lesson_plan.contains("prior_episode_count >= 2"),
        "full mastery no longer requires distributed prior transfer episodes"
    );
    assert!(
        lesson_plan.contains("evidence.probe_phase != Some(tutor_contract::ProbePhase::Practice)"),
        "guided practice can again grant mastery"
    );
    assert!(
        lesson_plan.contains("evidence.probe_phase == Some(tutor_contract::ProbePhase::Transfer)"),
        "full mastery no longer requires a fresh transfer-phase success"
    );
    assert!(
        lesson_plan.contains("latest_fresh_spontaneous_qualified != Some(false)")
            && lesson_plan.contains(".context_family")
            && lesson_plan.contains("!family.trim().is_empty()"),
        "full mastery can again ignore unresolved spontaneous failure or empty transfer family"
    );
    for required in [
        "MIN_CONTEXT_EVIDENCE_WEIGHT",
        "let mut observed_families = ContextSetView::default()",
        "let mut unaided_weighted_total = 0.0",
        "effective_evidence_weight",
        "unaided_evidence_weight",
        "unaided_evidence_episodes",
        "latency_evidence_episodes",
        "assistance_present[source] && !repeated_known_episode",
    ] {
        assert!(
            capability.contains(required),
            "capability estimator lost uncertainty/decay invariant: {required}"
        );
    }
    assert!(
        tasks.contains("let spontaneous_uncertain = needs_diagnostic(spontaneous)")
            && tasks.contains("if !spontaneous_uncertain")
            && tasks.contains("estimate.independent_evidence_episodes < 3")
            && tasks.contains(".weighted_success_rate")
            && tasks.contains("probe_prompt_independence")
            && tasks.contains("decision_fragile")
            && tasks.contains("listening_repair")
            && tasks.contains("latency_evidence_episodes >= 2")
            && tasks.contains("rate >= 0.80"),
        "task scoring can again skip diagnostic value-of-information or automaticity accuracy gates"
    );
    assert!(
        selection.contains("columns.item_key[index] == item_key")
            && selection
                .contains("columns.expected_evidence[index] == candidate.expected_evidence")
            && selection.contains("diagnostic_spontaneous")
            && selection.contains("independent_evidence_episodes[fi] < 3"),
        "bounded candidate selection can again lose diagnostic semantics or waste slots on alternatives"
    );
    assert!(
        complete_lesson.contains("MASTERY_HISTORY_WINDOW_SECONDS")
            && complete_lesson
                .contains("PARTITION BY CASE WHEN lesson_id IS NULL THEN id ELSE lesson_id END")
            && complete_lesson.contains("FROM ranked\n         WHERE episode_rn=1"),
        "mastery history lost its bounded non-practice episode-exit semantics"
    );
    assert!(
        complete_lesson.contains("\"calibration_eligible\": intervention_calibration_eligible")
            && complete_lesson.contains("result.evidence[..outcome_index]")
            && complete_lesson.contains("result.listening_attempts[..outcome_index]")
            && complete_lesson.contains("baseline_qualified")
            && complete_lesson.contains("ProbePhase::Diagnostic"),
        "lesson completion lost ordered qualified causal-baseline eligibility"
    );
    assert!(
        contract_validate
            .contains("intervention immediate outcome must be post-intervention, not diagnostic"),
        "contract again permits a diagnostic baseline to masquerade as intervention outcome"
    );
    assert!(
        contract_validate
            .contains("at least 1 diagnostic baseline and 2 independent measured probes overall"),
        "lesson measurement coverage drifted away from baseline plus independent delayed/transfer evidence"
    );
    assert!(
        !include_str!("../../../apps/local/src/methodology_instructions_ru.md")
            .contains("минимум 2 diagnostic probes"),
        "Tutor methodology reintroduced mandatory duplicate diagnostic probing"
    );
    assert!(
        calibration.contains("if !intervention.calibration_eligible")
            && calibration.contains("#[serde(default)]\n    calibration_eligible: bool")
            && !calibration.contains("default_true"),
        "calibration eligibility no longer fails closed for legacy outcomes"
    );
}
