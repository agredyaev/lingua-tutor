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
    for source in [LESSONS, ERRORS, CARDS, PROFILES] {
        assert!(
            !source.contains("into_domain") && !source.contains("::from_row"),
            "domain-row reconstruction returned to read projections"
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
fn lesson_result_reference_validation_uses_one_sorted_index() {
    let validation = include_str!("../../../crates/tutor-contract/src/validate.rs");
    assert!(
        validation.contains("let item_keys = collect_item_keys(result, &mut issues);")
            && validation.contains("binary_search_by"),
        "lesson-result references must share one sorted request-local index"
    );
    for forbidden in [
        "keys.contains(&",
        "known_keys.contains(&",
        "seen.contains(",
        ".iter().any(|learning_item_id",
    ] {
        assert!(
            !validation.contains(forbidden),
            "quadratic validation membership scan reintroduced: {forbidden}"
        );
    }
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
            && mcp.contains("ordered item-level execution queue")
            && mcp.contains("final authoritative execution snapshot"),
        "MCP tool descriptions no longer make the context-first executable LLM flow explicit"
    );
    for name in [
        "Partial inspection only: learning items",
        "Partial plan inspection only",
        "Partial inspection only: open error targets",
        "Partial inspection only: review workload summary",
        "Partial inspection only: failing cards",
    ] {
        assert!(
            !mcp.contains(name),
            "removed partial MCP surface reappeared: {name}"
        );
    }

    assert!(
        records.contains(
            "A calibration-eligible outcome requires at least 1 diagnostic baseline and at least 2 measurement-eligible probes overall"
        ) && records.contains("calibration_valid=false")
            && validation.contains("pub fn measurement_coverage_counts"),
        "LLM lesson-result instructions drifted from calibration coverage semantics"
    );
    assert!(
        !records.contains("at least 2 fully measured diagnostic probes"),
        "stale duplicate-diagnostic rule returned to the live LLM contract"
    );
    assert!(
        records.contains("run a closeout pass over executed tutor_tasks")
            && records.contains("Do not leave card_mutations empty by omission")
            && mcp.contains("completion_requirements")
            && include_str!("../../../apps/local/src/methodology_instructions.md")
                .contains("Do not leave `card_mutations` empty by omission"),
        "LLM lesson closeout no longer requires an explicit outcome-driven card decision"
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
            && selection.contains("measurement_requirements_for_action(&action, entry.capability)")
            && selection.contains("completion_requirements_for_action(&action)")
            && queue.contains("pub fn criterion_for_action(")
            && queue.contains("pub fn measurement_requirements_for_action(")
            && queue.contains("pub fn completion_requirements_for_action(")
            && queue.contains("pub measurement_requirements:")
            && queue.contains("pub completion_requirements:")
            && queue.contains("\"response_millis_if_instrumented\"")
            && queue.contains("\"response_millis_required\"")
            && queue.contains("\"evaluate_card_mutation_from_observed_outcome\"")
            && queue.contains("new_high_value_item_needing_spaced_retrieval_or_transfer")
            && queue.contains("\"no_duplicate_active_card_same_target_modality\"")
            && queue.contains("\"no_card_for_mere_exposure_or_stable_independent_retrieval\"")
            && queue.contains("\"persist_intervention_if_applied\"")
            && queue.contains("\"diagnostic_spontaneous\"")
            && queue.contains("\"listening_probe\"")
            && queue.contains("\"listening_repair\""),
        "LLM task execution, measurement, or closeout contract drifted"
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
    assert!(
        path.contains("ORDER BY created_at DESC LIMIT ?2")
            && ERRORS.contains(
                "CALIBRATION_HISTORY_DECISIONS: i64 = adaptation::MAX_CALIBRATION_SNAPSHOTS as i64"
            ),
        "planner history must keep one bounded SQL scan backed by the shared calibration cap"
    );
    assert_eq!(adaptation::MAX_CALIBRATION_SNAPSHOTS, 10);
    assert!(path.contains("while let Some(row) = rows.next()?"));
    assert!(path.contains(
        "let mut timestamps = Vec::with_capacity(adaptation::MAX_CALIBRATION_SNAPSHOTS);"
    ));
    assert!(path.contains(
        "let mut capability_json = Vec::with_capacity(adaptation::MAX_CALIBRATION_SNAPSHOTS);"
    ));
    assert!(path.contains(
        "let mut outcome_json = Vec::with_capacity(adaptation::MAX_CALIBRATION_SNAPSHOTS);"
    ));
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
        "HashMap<",
        "BTreeMap<",
        "BTreeSet<",
        ".position(|existing|",
        "after_match",
        "middle_start",
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
        "ITEM_KEY_INDEX_CAPACITY",
        "pending_intervention",
        "pending_before_metric",
    ] {
        assert!(live.contains(required), "missing SoA storage: {required}");
    }
}

#[test]
fn review_read_views_stream_sql_into_fixed_counters() {
    let review = include_str!("../../../apps/local/src/handlers/review.rs");
    assert!(
        !review.contains("collect::<rusqlite::Result<Vec<_>>>()"),
        "review projections must not materialize SQL rows"
    );
    assert_eq!(
        review.matches("while let Some(row) = rows.next()?").count(),
        3,
        "summary, forecast, and retention must remain streaming passes"
    );
}

#[test]
fn legacy_review_runtime_implementation_is_absent() {
    let review = include_str!("../../../apps/local/src/handlers/review.rs");
    let sqlite = include_str!("../../../apps/local/src/sqlite.rs");
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
            !review.contains(legacy) && !sqlite.contains(legacy),
            "legacy review implementation remains: {legacy}"
        );
    }
}

#[test]
fn sqlite_enum_conversion_uses_pure_functions_not_trait_dispatch() {
    let sqlite = include_str!("../../../apps/local/src/sqlite.rs");
    assert!(
        !sqlite.contains("pub trait DbText"),
        "DbText trait reintroduced"
    );
    assert!(
        !sqlite.contains("impl DbText for"),
        "DbText object adapter reintroduced"
    );
    assert!(
        !sqlite.contains("pub fn parse_enum<T:"),
        "generic enum conversion reintroduced"
    );
    assert!(
        !sqlite.contains("pub fn enum_str<T:"),
        "generic enum conversion reintroduced"
    );
    for name in [
        "learning_item_type_db",
        "learning_item_status_db",
        "evidence_kind_db",
        "card_status_from_db",
        "card_status_db",
        "core_error_category_db",
        "card_intent_from_db",
        "card_intent_db",
        "mutation_operation_db",
    ] {
        assert!(
            sqlite.contains(name),
            "missing production-used SQLite enum pass: {name}"
        );
    }
    for dead in [
        "profile_status_from_db",
        "profile_status_db",
        "lesson_status_from_db",
        "lesson_status_db",
        "lesson_source_from_db",
        "lesson_source_db",
        "learning_item_type_from_db",
        "learning_item_status_from_db",
        "evidence_kind_from_db",
        "error_target_status_from_db",
        "error_target_status_db",
        "card_actor_db",
        "core_error_category_from_db",
        "mutation_operation_from_db",
    ] {
        assert!(
            !sqlite.contains(dead),
            "test-only or unused SQLite enum pass reintroduced: {dead}"
        );
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
    assert!(source.contains("pub const ALGORITHM: &str = \"fsrs\""));
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
    let file = include_str!("../../../crates/review-engine/src/fsrs_adapter.rs");
    assert!(!file.contains("impl ReviewScheduler for"));
    assert!(!file.contains("impl FsrsScheduler {"));
    assert!(!file.contains("impl FallbackScheduler {"));
}

#[test]
fn lesson_completion_has_no_implicit_latest_open_lesson_fallback() {
    let complete = include_str!("../../../apps/local/src/operations/complete_lesson.rs");
    let tools = include_str!("../../../apps/local/src/mcp_tools.rs");

    assert!(
        !complete.contains("ORDER BY started_at DESC LIMIT 1"),
        "lesson completion still silently binds missing lesson_id to database state"
    );
    assert!(
        tools.contains(r#"required: &["lesson_id"]"#),
        "MCP complete_lesson must require the explicit start_lesson id"
    );
}

#[test]
fn review_runtime_has_one_scheduler_and_no_speculative_parameter_subsystem() {
    let engine = include_str!("../../../crates/review-engine/src/lib.rs");
    let schema = include_str!("../../../apps/local/migrations/0001_init.sql");
    let review_repo = include_str!("../../../apps/local/src/sqlite.rs");

    for forbidden in [
        "FALLBACK_ID",
        "mod fallback;",
        r#"fallback" =>"#,
        "parameter_set_id",
        "scheduler_parameter_sets",
    ] {
        assert!(
            !engine.contains(forbidden)
                && !schema.contains(forbidden)
                && !review_repo.contains(forbidden),
            "obsolete review subsystem remains: {forbidden}"
        );
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
    let methodology = include_str!("../../../apps/local/src/methodology_instructions.md");

    for required in [
        "one or two short exact-repetition attempts only for initial stabilization and proceduralization",
        "do not treat it as retention or transfer evidence",
        "procedural or semantic variation",
        "do not count as multiple spaced-success stages",
    ] {
        assert!(
            methodology.contains(required),
            "methodology lost repetition-vs-learning distinction: {required}"
        );
    }
}

#[test]
fn methodology_decomposes_task_failure_before_intervention() {
    let methodology = include_str!("../../../apps/local/src/methodology_instructions.md");

    for required in [
        "A communicative task failure is an outcome, not a diagnosis",
        "short discriminating probe",
        "Apply focus on form reactively",
        "Do not treat all causes at once",
    ] {
        assert!(
            methodology.contains(required),
            "methodology lost bottleneck-decomposition rule: {required}"
        );
    }
}

#[test]
fn methodology_optimizes_communicative_outcome_not_proxies() {
    let methodology = include_str!("../../../apps/local/src/methodology_instructions.md");

    for required in [
        "Minimize the number of learning interactions and elapsed time",
        "Do not optimize curriculum completion",
        "observation -> bottleneck hypothesis -> intervention -> measurable immediate or delayed outcome",
        "If evidence is insufficient, reduce uncertainty with a short discriminating probe",
        "Engagement, conversational naturalness, and sustainable cognitive load are constraints",
        "An explanation is an intervention, not learning evidence",
        "New material is optional expansion",
        "Optional expansion must be zero while the main bottleneck remains unstable",
        "the latent state being estimated",
    ] {
        assert!(
            methodology.contains(required),
            "methodology lost optimization-policy invariant: {required}"
        );
    }
}

#[test]
fn methodology_ab_changes_require_predeclared_acceptance_gate() {
    let methodology = include_str!("../../../apps/local/src/methodology_instructions.md");
    let tutor_spec = include_str!("../../../docs/spec/07-tutor-contract.md");

    for required in [
        "primary outcome",
        "control or baseline",
        "unit of analysis",
        "minimally important effect size",
        "sample and power plan",
        "stopping rule",
        "uncertainty/CI",
        "statistical significance",
        "practical significance",
        "optional stopping",
        "result remains UNVERIFIED",
    ] {
        assert!(
            methodology.contains(required),
            "methodology lost A/B acceptance guard: {required}"
        );
    }
    for required in [
        "predeclared primary outcome",
        "control/baseline",
        "unit of analysis",
        "minimally important effect",
        "sample/power plan",
        "stopping rule",
        "confidence interval",
        "statistical significance",
        "practical significance",
    ] {
        assert!(
            tutor_spec.contains(required),
            "tutor spec lost policy-level A/B acceptance guard: {required}"
        );
    }
}

#[test]
fn communicative_task_success_requires_target_language_solution() {
    let methodology = include_str!("../../../apps/local/src/methodology_instructions.md");
    let records = include_str!("../../../apps/local/src/handlers/records.rs");
    let tutor_spec = include_str!("../../../docs/spec/07-tutor-contract.md");

    assert!(
        methodology.contains("Source-language, gesture, or another non-linguistic action")
            && methodology.contains("must not replace the target-language material"),
        "Tutor methodology must not count source-language/non-linguistic substitution as L2 task success"
    );
    assert!(
        records.contains(
            "Source-language or gesture-only workaround does not count as task success when it replaces required target-language material"
        ),
        "LLM-facing lesson-result contract lost the strong v2 communicative-task rubric"
    );
    assert!(
        tutor_spec.contains("Source-language or gesture-only workarounds do not count as v2"),
        "Tutor spec lost the target-language requirement for communicative-task success"
    );
    assert!(
        methodology.contains("A form-only exercise is practice/evidence, not a communicative task"),
        "Tutor methodology must not count form-only drills as communicative tasks"
    );
    assert!(
        records.contains("A form-only drill such as say/repeat X is not a communicative task"),
        "LLM-facing contract must not count form-only drills as communicative tasks"
    );
    assert!(
        tutor_spec.contains(
            "form-only exercises are practice/evidence but do not increment `task_attempt_count`"
        ),
        "Tutor spec lost meaning-focused task-attempt semantics"
    );
}

#[test]
fn methodology_treats_intervention_effects_as_observational() {
    let methodology = include_str!("../../../apps/local/src/methodology_instructions.md");
    let tutor_spec = include_str!("../../../docs/spec/07-tutor-contract.md");

    for required in [
        "One delayed sample is preliminary observational evidence only",
        "at least two independent delayed samples",
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
    let methodology = include_str!("../../../apps/local/src/methodology_instructions.md");
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
fn error_recheck_lifecycle_is_closed_through_canonical_lesson_completion() {
    let contract = include_str!("../../../crates/tutor-contract/src/lib.rs");
    let complete = include_str!("../../../apps/local/src/operations/complete_lesson.rs");
    let errors = include_str!("../../../apps/local/src/operations/error.rs");
    let methodology = include_str!("../../../apps/local/src/methodology_instructions.md");
    let tutor_spec = include_str!("../../../docs/spec/07-tutor-contract.md");
    let records = include_str!("../../../apps/local/src/handlers/records.rs");
    let mcp = include_str!("../../../apps/local/src/mcp_tools.rs");

    assert!(
        contract.contains("pub error_rechecks: Vec<ErrorRecheckResult>"),
        "lesson-result contract lost due error-recheck outcomes"
    );
    assert!(
        complete.contains("apply_error_recheck(")
            && complete.contains("Some(&result.language_profile_id)"),
        "canonical complete_lesson no longer applies rechecks in the lesson transaction"
    );
    for required in [
        "recheck_target_mismatch",
        "recheck_profile_mismatch",
        "recheck_not_due",
        "recheck_not_unaided",
        "recheck_context_missing",
        "recheck_repair_missing",
        "status='resolved',method_change_required=0",
        "schedule_error_rechecks(database, target_id, now)",
        "r.cycle",
    ] {
        assert!(
            errors.contains(required),
            "error recheck lifecycle lost invariant: {required}"
        );
    }
    assert!(
        methodology.contains(
            "If a recheck fails, the target returns logically to active practice for corrective repair"
        ) && methodology
            .contains("Only a fully successful spaced series resolves the target"),
        "Tutor methodology drifted from deterministic error-recheck lifecycle"
    );
    assert!(
        methodology.contains(
            "sufficiently sampled independent communicative-task failure -> required method change -> due error rechecks"
        ) && methodology.contains(
            "recurring errors -> evidence-based recommendations -> sparse spontaneous-production diagnostic"
        ) && methodology.contains(
            "FSRS card backlog is not a standalone Tutor-lesson objective"
        ) && tutor_spec.contains(
            "sufficiently sampled independent communicative-task failure → required method change → due error rechecks"
        ) && tutor_spec.contains(
            "FSRS card backlog is not a standalone Tutor-lesson objective"
        ),
        "Tutor/spec profile-level priority order drifted from deterministic planner"
    );
    assert!(
        records.contains("\"error_rechecks\"") && records.contains("outcome in error_rechecks"),
        "LLM-facing lesson-result contract stopped exposing recheck persistence"
    );
    assert!(
        !mcp.contains("name: \"complete_error_recheck\""),
        "maintenance recheck write leaked back into the canonical MCP surface"
    );
}

#[test]
fn recommendation_lifecycle_is_closed_through_canonical_lesson_completion() {
    let contract = include_str!("../../../crates/tutor-contract/src/lib.rs");
    let complete = include_str!("../../../apps/local/src/operations/complete_lesson.rs");
    let methodology = include_str!("../../../apps/local/src/methodology_instructions.md");
    let records = include_str!("../../../apps/local/src/handlers/records.rs");
    let mcp = include_str!("../../../apps/local/src/mcp_tools.rs");
    let server = include_str!("../../../apps/local/src/server.rs");

    assert!(
        contract.contains("pub recommendation_resolutions: Vec<RecommendationResolution>")
            && contract.contains("RecommendationResolutionStatus"),
        "lesson-result contract lost recommendation disposition"
    );
    assert!(
        complete.contains("WHERE id=?1 AND learner_language_profile_id=?2 AND status='open'")
            && complete.contains("recommendation_resolution_status_id(resolution.status)"),
        "canonical completion no longer owns recommendation state transition"
    );
    assert!(
        methodology.contains("An open recommendation is an unresolved decision")
            && methodology.contains("status=`accepted`")
            && methodology.contains("status=`dismissed`"),
        "Tutor methodology lost recommendation lifecycle rules"
    );
    assert!(
        records.contains("\"recommendation_resolutions\"")
            && records.contains("\"recommendation_resolution_status\""),
        "LLM-facing contract stopped exposing recommendation resolution"
    );
    assert!(
        !mcp.contains("name: \"resolve_recommendation\"")
            && !server.contains("\"resolve_recommendation\" =>"),
        "recommendation lifecycle regressed to a separate maintenance write surface"
    );
}

#[test]
fn sticky_method_change_is_active_repair_pressure_not_recheck_interference() {
    let planner_read = include_str!("../../../apps/local/src/operations/planner_read.rs");
    let records = include_str!("../../../apps/local/src/handlers/records.rs");
    let methodology = include_str!("../../../apps/local/src/methodology_instructions.md");
    let tutor_spec = include_str!("../../../docs/spec/07-tutor-contract.md");

    assert!(
        planner_read.contains("method_change_required=1 AND status IN ('active','escalated')"),
        "planner must preserve method-change pressure for active repair states without leaking it into recheck"
    );
    assert!(
        planner_read.contains("status IN ('active','escalated')")
            && !planner_read.contains("status IN ('active','recheck','escalated')"),
        "recheck state must not create generic recurring-error repair pressure"
    );
    assert!(
        records.contains("method_change_required = 1 AND status IN ('active','escalated')"),
        "tutor-context method-change pressure must use the same active-repair state"
    );
    assert!(
        methodology.contains("`method_change_required` is a constraint on the next repair"),
        "Tutor methodology must distinguish sticky repair constraint from repeated method switching"
    );
    assert!(
        tutor_spec.contains(
            "sticky repair constraint, not a command to reteach before each successful recheck"
        ),
        "Tutor spec lost method-change/recheck state semantics"
    );
}

#[test]
fn methodology_requires_distributed_evidence_and_fresh_mastery() {
    let methodology = include_str!("../../../apps/local/src/methodology_instructions.md");

    for required in [
        "Evidence strength and decision rates increase across independent learning episodes",
        "Average profile decision rates by lesson episode, not by raw attempt count",
        "Recent profile signals older than 28 days must not be treated as current authority",
        "Promotion to `provisionally_mastered` requires the latest mastery-eligible non-practice productive exit observation",
        "at least two distinct successful transfer `context_family` values from at least two temporally separated previous lesson episodes",
        "non-empty `context_family`",
        "A fresh unresolved spontaneous-production failure blocks full mastery",
        "inside the current 84-day revalidation window",
        "a qualified diagnostic baseline for the same learning item and `target_capability`",
        "`probe_phase=practice` records acquisition or repair performance",
        "Do not use practice evidence to raise operational capability state, profile-level unaided recall, or mastery",
        "Success in another capability does not close that failure",
        "Assisted or otherwise unqualified success in the same capability also does not close an independent failure",
        "Legacy records without `calibration_eligible=true` fail closed",
        "uses the exit observation from each prior episode",
        "Item-level `weighted_success_rate` and `unaided_success_rate` must be recency- and episode-weighted",
        "Evidence with unknown `assistance_level` does not create an independent episode",
        "Before at least three independent episodes, prefer a discriminating diagnostic probe",
        "`successful_contexts` and `successful_context_families` are not lifetime achievements",
        "Use the latest independent outcome for each context or family",
        "oldest unconfirmed focus capability among listening, spontaneous production, and transfer",
        "Recognition- or prompted-only failure must not occupy the scarce focus-revalidation reserve",
        "Bounded top-K must not change task semantics",
        "keep only the best action for each item-capability pair",
        "baseline strictly before the explicit immediate outcome",
        "Compute numeric effectiveness delta between the pre-lesson capability snapshot and the first genuinely delayed non-practice observation",
        "If one next independent success or failure can cross an operational threshold and change the next action",
        "If both outcomes lead to the same action, do not spend a diagnostic slot",
        "Prompt dependence is established only at item level for the same learning item",
        "established gap `listening_repair`",
        "measured latency in at least two independent qualified episodes",
        "at least one diagnostic baseline and at least two measurement-eligible probes overall",
        "Do not increase a retrieval-latency threshold automatically because `target_text` contains more words",
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
    let planner = include_str!("../../../crates/adaptation/src/planner.rs");

    assert!(
        source.contains("FROM learning_item_evidence e"),
        "planner recall state must be derived from persisted evidence"
    );
    assert!(
        source.contains("GROUP BY e.lesson_id"),
        "planner spontaneous-production signal must retain lesson-level independence"
    );
    assert!(
        source.contains("WHERE e.kind='spontaneous_production'"),
        "profile production weakness must use spontaneous production only; same-item prompt dependence belongs to tutor_tasks"
    );
    assert!(
        source.contains("RECENT_SIGNAL_WINDOW_SECONDS"),
        "planner current-state evidence must have an explicit freshness window"
    );
    assert!(
        source.contains("MIN_DISTRIBUTED_SUCCESS_GAP_SECONDS")
            && source.contains("recall_temporally_distributed")
            && source.contains("communicative_task_temporally_distributed")
            && planner.contains("input.recent.recall_temporally_distributed")
            && planner.contains("input.recent.communicative_task_temporally_distributed"),
        "massed lesson IDs must not regain positive stretch authority"
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
                && source.contains("adaptive_selection::DEFAULT_FOCUS_LIMIT")
                && source.contains("adaptive_selection::effective_new_item_cap"),
            "lesson-start snapshot and live tutor context must share one selection-capacity and post-selection load contract"
        );
    }
    let selection = include_str!("../../../apps/local/src/operations/adaptive_selection.rs");
    assert!(
        selection.contains("task.action == \"fade_prompts\"")
            && selection.contains("pub(crate) fn effective_new_item_cap"),
        "same-item prompt dependence must deterministically suppress optional expansion"
    );
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
            && lesson_plan.contains("stable_context_family_id"),
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
        contract_validate.contains("pub fn measurement_coverage_counts")
            && complete_lesson.contains(
                "let calibration_valid = diagnostic_probe_count >= 1 && measurement_eligible_probe_count >= 2"
            ),
        "lesson calibration coverage must stay explicit without turning sparse measurement into persistence failure"
    );
    assert!(
        !include_str!("../../../apps/local/src/methodology_instructions.md")
            .contains("at least 2 diagnostic probes"),
        "Tutor methodology reintroduced mandatory duplicate diagnostic probing"
    );
    assert!(
        calibration.contains("if !intervention.calibration_eligible")
            && calibration.contains("#[serde(default)]\n    calibration_eligible: bool")
            && !calibration.contains("default_true"),
        "calibration eligibility no longer fails closed for legacy outcomes"
    );
}

#[test]
fn native_release_platform_paths_are_not_macos_only() {
    let main = include_str!("../../../apps/local/src/main.rs");
    let tunnel = include_str!("../../../apps/local/src/tunnel.rs");

    for forbidden in [
        r#"secure terminal input is currently supported on Unix only"#,
        r#".args(["-f", "/dev/tty", "-echo"])"#,
        r#".args(["-f", "/dev/tty", "echo"])"#,
    ] {
        assert!(
            !main.contains(forbidden),
            "platform-specific credential prompt leaked into release runtime: {forbidden}"
        );
    }
    assert!(
        !tunnel.contains(r#"const CURL_PATH: &str = "/usr/bin/curl";"#),
        "secure tunnel is still hard-wired to a Unix curl path"
    );
    assert!(
        main.contains("prompt_secret_windows"),
        "Windows release lacks a hidden credential-input implementation"
    );
    assert!(
        tunnel.contains("system_curl_path"),
        "tunnel release lacks platform-aware system curl resolution"
    );
}

#[test]
fn release_build_requires_complete_generated_web_bundle() {
    let build = include_str!("../../../apps/local/build.rs");
    assert!(
        build.contains(r#"env::var("PROFILE")"#),
        "native build script does not distinguish release packaging from test compilation"
    );
    assert!(
        build.contains(r#"env::var_os("CARGO_CFG_FUZZING")"#),
        "release web-bundle enforcement must exempt only Cargo fuzz instrumentation builds"
    );
    assert!(
        build.contains(r#"profile == "release" && !fuzzing"#),
        "ordinary release builds must still require the generated web bundle"
    );
    assert!(
        build.contains("release web bundle is incomplete"),
        "release packaging can still silently embed an incomplete generated web bundle"
    );
    for extension in [
        r#""index.html""#,
        r#"Some("wasm")"#,
        r#"Some("js")"#,
        r#"Some("css")"#,
    ] {
        assert!(
            build.contains(extension),
            "release web-bundle validation is missing required asset check: {extension}"
        );
    }
}

#[test]
fn platform_specific_release_helpers_are_cfg_scoped() {
    let tunnel = include_str!("../../../apps/local/src/tunnel.rs");
    let main = include_str!("../../../apps/local/src/main.rs");
    assert!(
        tunnel.contains("#[cfg(any(windows, target_os = \"macos\"))]\nfn require_system_curl"),
        "curl existence helper must not compile on Linux where the Unix search path does not use it"
    );
    let unix_import = main
        .split_once("#[cfg(unix)]")
        .map_or("", |(_, tail)| tail)
        .split_once("use std::{\n    path::PathBuf")
        .map_or("", |(head, _)| head);
    assert!(
        unix_import.contains("io::{Read, Write}"),
        "terminal Write import must stay Unix-scoped"
    );
}

#[test]
fn browser_e2e_uses_only_browser_rpc_surface() {
    let e2e = include_str!("../../e2e/specs/core-loop.spec.ts");
    for mcp_only in [r#""start_lesson""#, r#""complete_lesson""#] {
        assert!(
            !e2e.contains(mcp_only),
            "browser E2E calls MCP-only method through /rpc: {mcp_only}"
        );
    }
}

#[test]
fn github_actions_are_pinned_to_immutable_commits() {
    for path in [
        "../../.github/workflows/ci.yml",
        "../../.github/workflows/e2e.yml",
        "../../.github/workflows/fuzz.yml",
        "../../.github/workflows/release.yml",
        "../../.github/workflows/release-please.yml",
    ] {
        let workflow =
            std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(path))
                .unwrap();
        for line in workflow.lines().map(str::trim) {
            let Some(action) = line.strip_prefix("- uses: ") else {
                continue;
            };
            if action.starts_with("./") {
                continue;
            }
            let action = action.split_once(" #").map_or(action, |(value, _)| value);
            let revision = action.rsplit_once('@').map_or("", |(_, revision)| revision);
            assert!(
                !revision.is_empty(),
                "external action is missing a revision in {path}: {line}"
            );
            assert!(
                revision.len() == 40 && revision.bytes().all(|byte| byte.is_ascii_hexdigit()),
                "external action is not pinned to an immutable commit in {path}: {line}"
            );
        }
    }
}

#[test]
fn fsrs_dependency_is_exactly_pinned_to_persisted_algorithm_version() {
    let manifest = include_str!("../../../crates/review-engine/Cargo.toml");
    let engine = include_str!("../../../crates/review-engine/src/lib.rs");
    assert!(
        manifest.contains(r#"fsrs = "=6.6.1""#),
        "FSRS must be exact-pinned because its implementation version is persisted"
    );
    assert!(
        engine.contains(r#"pub const ALGORITHM_VERSION: &str = "fsrs-rs-6.6.1";"#),
        "persisted scheduler version must match the exact FSRS dependency"
    );
}

#[test]
fn web_has_no_placeholder_or_behaviorless_routes() {
    let app = include_str!("../../../apps/web/src/app.rs");
    let misc = include_str!("../../../apps/web/src/pages/misc.rs");
    let spec = include_str!("../../../docs/spec/09-frontend.md");
    assert!(
        !misc.contains("fn SettingsPage"),
        "hardcoded settings placeholder component remains in the web binary"
    );
    for route in [
        "/settings",
        "/languages/:profile_id",
        "/lessons/:id",
        "/learning-items/:id",
        "/errors/:id",
        "/listening",
        "/cards/redesign",
    ] {
        assert!(
            !app.contains(&format!("path!(\"{route}\")")),
            "route has no route-specific behavior: {route}"
        );
        assert!(
            !spec.lines().any(|line| line.trim() == route),
            "frontend spec still advertises behaviorless route: {route}"
        );
    }
}

#[test]
fn fuzz_ci_has_locked_compile_preflight_before_long_fuzz_runs() {
    let workflow = include_str!("../../../.github/workflows/fuzz.yml");
    let preflight = "cargo +nightly check --locked --manifest-path fuzz/Cargo.toml --bins";
    let first_fuzz = "cargo +nightly fuzz run lesson_result_parse";
    let preflight_at = workflow.find(preflight).unwrap_or(usize::MAX);
    let fuzz_at = workflow.find(first_fuzz).unwrap_or(usize::MAX);
    assert!(
        preflight_at != usize::MAX,
        "fuzz workflow must compile its separate workspace with the committed lockfile"
    );
    assert!(
        fuzz_at != usize::MAX,
        "fuzz workflow must retain parser fuzzing"
    );
    assert!(
        preflight_at < fuzz_at,
        "locked fuzz compile preflight must run before expensive fuzz targets"
    );
}

#[test]
fn release_version_contract_covers_all_release_bearing_files() {
    let config = include_str!("../../../release-please-config.json");
    let ci = include_str!("../../../.github/workflows/ci.yml");
    let app_manifest = include_str!("../../../apps/local/Cargo.toml");
    let version = include_str!("../../../VERSION").trim();
    let release_manifest = include_str!("../../../.release-please-manifest.json");

    assert!(
        config.contains(r#""version-file": "VERSION""#),
        "release-please must own the canonical VERSION file"
    );
    assert!(
        config.contains(r#""path": "apps/local/Cargo.toml""#),
        "release-please does not update the shipped application manifest"
    );
    assert!(
        app_manifest
            .lines()
            .any(|line| line.starts_with("version = ")
                && line.contains("# x-release-please-version")),
        "shipped application manifest lost the generic release-please marker"
    );
    let release_manifest_version = release_manifest
        .lines()
        .find(|line| line.trim_start().starts_with(r#""."#))
        .and_then(|line| line.split('"').nth(3));
    assert_eq!(
        release_manifest_version,
        Some(version),
        "release-please manifest must match the canonical VERSION"
    );
    assert!(
        ci.contains(
            r#"grep -Fq "version = \"$version\" # x-release-please-version" apps/local/Cargo.toml"#
        ),
        "CI must verify the application manifest release marker before Cargo commands"
    );
    assert!(
        ci.contains("release_manifest_version=")
            && ci.contains(r#"test "$version" = "$release_manifest_version""#),
        "CI must enforce VERSION equality with .release-please-manifest.json"
    );

    for lock in ["Cargo.lock", "fuzz/Cargo.lock"] {
        assert!(
            config.contains(&format!(r#""path": "{lock}""#)),
            "release-please does not update {lock}"
        );
        assert!(
            ci.contains(&format!(
                r#"grep -Fq "version = \"$version\" # x-release-please-version" {lock}"#
            )),
            "CI must verify the generic release marker in {lock} before Cargo normalizes comments"
        );
    }
    let release_check = ci
        .find("- name: Release version consistency")
        .unwrap_or(usize::MAX);
    let first_cargo = ci.find("- name: Format").unwrap_or(usize::MAX);
    assert!(
        release_check < first_cargo,
        "release marker consistency must run before Cargo commands can normalize Cargo.lock comments"
    );
    assert!(
        ci.contains("lock_version=")
            && ci.contains("fuzz_lock_version=")
            && ci.contains(r#"test "$version" = "$lock_version""#)
            && ci.contains(r#"test "$version" = "$fuzz_lock_version""#),
        "CI does not enforce semantic version equality for both lockfiles"
    );
}

#[test]
fn card_mutation_contract_exposes_only_runtime_supported_operations() {
    let contract = include_str!("../../../crates/tutor-contract/src/lib.rs");
    let planner = include_str!("../../../apps/local/src/operations/card_plan.rs");
    let schema = include_str!("../../../apps/local/migrations/0001_init.sql");
    let spec = include_str!("../../../docs/spec/07-tutor-contract.md");

    for variant in ["Split,", "Merge,", "AddListeningVariant,", "Link,"] {
        assert!(
            !contract.contains(variant),
            "Tutor contract advertises an unsupported card mutation: {variant}"
        );
    }
    assert!(
        !planner.contains("not supported in M2; proposed via redesign loop"),
        "runtime still carries always-rejected mutation branches"
    );

    let card_mutations_tail = schema
        .split_once("CREATE TABLE card_mutations")
        .map_or("", |(_, tail)| tail);
    assert!(
        !card_mutations_tail.is_empty(),
        "card_mutations table must exist"
    );
    let card_mutations = card_mutations_tail
        .split_once(");")
        .map_or("", |(table, _)| table);
    assert!(
        !card_mutations.is_empty(),
        "card_mutations table terminator must exist"
    );
    for operation in ["'split'", "'merge'", "'add_listening_variant'", "'link'"] {
        assert!(
            !card_mutations.contains(operation),
            "fresh schema permits unsupported card mutation {operation}"
        );
    }

    let operations_line = spec
        .lines()
        .find(|line| line.starts_with("Operations:"))
        .unwrap_or("");
    assert!(
        !operations_line.is_empty(),
        "card mutation operations line must exist"
    );
    for operation in ["`split`", "`merge`", "`add_listening_variant`", "`link`"] {
        assert!(
            !operations_line.contains(operation),
            "spec advertises unsupported card mutation {operation}"
        );
    }
}

#[test]
fn fuzz_write_surface_has_no_self_justifying_targets() {
    let manifest = include_str!("../../../fuzz/Cargo.toml");
    let workflow = include_str!("../../../.github/workflows/fuzz.yml");
    for target in ["error_writes", "auxiliary_writes"] {
        assert!(
            !manifest.contains(&format!("name = \"{target}\"")),
            "dedicated fuzz binary reintroduced: {target}"
        );
        assert!(
            !workflow.contains(&format!("fuzz run {target}")),
            "fuzz workflow invokes removed write target: {target}"
        );
    }

    let system_state = include_str!("../../../fuzz/fuzz_targets/system_state.rs");
    let fuzzing = include_str!("../../../apps/local/src/fuzzing.rs");
    let error = include_str!("../../../apps/local/src/operations/error.rs");
    assert!(!system_state.contains("exercise_error_writes"));
    assert!(!fuzzing.contains("exercise_error_writes"));
    assert!(!error.contains("fuzz_error_occurrence_sequence"));
    assert!(
        system_state.contains("exercise_auxiliary_writes"),
        "real auxiliary write coverage must stay in canonical system_state fuzzing"
    );
    assert!(
        !std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fuzz/fuzz_targets/auxiliary_writes.rs")
            .exists(),
        "dedicated auxiliary_writes target duplicates system_state coverage"
    );
}

#[test]
fn lean_runtime_surface_stays_minimal() {
    let lib = include_str!("../../../apps/local/src/lib.rs");
    let sqlite = include_str!("../../../apps/local/src/sqlite.rs");
    let operations = include_str!("../../../apps/local/src/operations/mod.rs");
    let tools = include_str!("../../../apps/local/src/mcp_tools.rs");
    let server = include_str!("../../../apps/local/src/server.rs");
    let card_direct = include_str!("../../../apps/local/src/operations/card_direct.rs");

    assert!(!lib.contains("repositories"));
    assert!(lib.contains("pub(crate) mod sqlite;"));
    assert_eq!(
        sqlite.matches("pub(crate) const ").count(),
        4,
        "sqlite boundary must contain only multi-consumer SQL"
    );
    assert!(!card_direct.contains("handlers::"));

    for removed in [
        "recommendation_ops",
        "lesson_correction",
        "records_write",
        "upsert_cards",
        "suspend_cards",
        "log_error",
    ] {
        assert!(!operations.contains(&format!("mod {removed};")));
    }

    assert_eq!(tools.matches("        name: \"").count(), 4);
    for required in [
        "get_active_language_profile",
        "get_tutor_context",
        "start_lesson",
        "complete_lesson",
    ] {
        assert!(tools.contains(&format!("name: \"{required}\"")));
    }

    for removed in [
        "update_profile",
        "get_lesson",
        "get_error_target",
        "patch_card",
        "list_recommendations",
        "resolve_recommendation",
        "get_card_redesign_candidates",
        "start_lesson",
        "complete_lesson",
        "create_listening_attempt",
        "evaluate_review_answer",
    ] {
        assert!(!server.contains(&format!("\"{removed}\" =>")));
    }
}

#[test]
fn repository_text_has_no_cyrillic() {
    fn is_cyrillic(ch: char) -> bool {
        matches!(
            ch as u32,
            0x0400..=0x052F | 0x1C80..=0x1C8F | 0x2DE0..=0x2DFF | 0xA640..=0xA69F
        )
    }

    fn scan(path: &std::path::Path, violations: &mut Vec<String>) {
        if path.is_dir() {
            let name = path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("");
            if matches!(
                name,
                ".git"
                    | ".cargo-home"
                    | ".rivet-cargo-home"
                    | "target"
                    | "node_modules"
                    | "dist"
                    | "artifacts"
                    | "corpus"
            ) || name.starts_with(".rivet-")
            {
                return;
            }
            let Ok(entries) = std::fs::read_dir(path) else {
                return;
            };
            for entry in entries.flatten() {
                scan(&entry.path(), violations);
            }
            return;
        }

        let Ok(bytes) = std::fs::read(path) else {
            return;
        };
        let Ok(text) = std::str::from_utf8(&bytes) else {
            return;
        };
        if let Some((offset, ch)) = text.char_indices().find(|(_, ch)| is_cyrillic(*ch)) {
            violations.push(format!("{}:{offset}: U+{:04X}", path.display(), ch as u32));
        }
    }

    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut violations = Vec::new();
    scan(&root, &mut violations);
    assert!(
        violations.is_empty(),
        "Cyrillic text is not allowed in the repository: {}",
        violations.join(", ")
    );

    for removed in [
        "apps/local/src/methodology_instructions_ru.md",
        "apps/local/src/methodology_corrections_ru.md",
    ] {
        assert!(
            !root.join(removed).exists(),
            "language-specific methodology file returned: {removed}"
        );
    }
}
