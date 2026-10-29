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
    assert!(path.contains("SELECT language_pack_id, answer, accepted_answers_json"));
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
        include_str!("../../../apps/local/src/repositories/packs.rs"),
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
fn adaptation_and_pack_storage_have_no_processing_methods() {
    let queue = include_str!("../../../crates/adaptation/src/queue.rs");
    let planner = include_str!("../../../crates/adaptation/src/planner.rs");
    let capability = include_str!("../../../crates/adaptation/src/capability.rs");
    let registry = include_str!("../../../crates/language-pack-registry/src/lib.rs");
    let spanish = include_str!("../../../language-packs/es-es/src/lib.rs");

    for (name, source, forbidden) in [
        (
            "queue",
            queue,
            "impl<'a, const N: usize> TutorTaskQueueFixed",
        ),
        ("planner", planner, "impl<'a> AdaptiveLessonPlanFixed"),
        ("capability-context", capability, "impl<'a> ContextSetView"),
        ("capability-text-set", capability, "impl<'a> TextSetRef"),
        ("registry", registry, "impl<P> LanguagePackRegistry<P>"),
        ("spanish-pack", spanish, "impl EsEsPack"),
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
fn runtime_lesson_contract_has_no_virtual_language_pack_dispatch() {
    let source = include_str!("../../../apps/local/src/handlers/records.rs");
    assert!(!source.contains("&dyn language_pack_api::LanguagePack"));
    assert!(source.contains("manifest: &language_pack_registry::ManifestView<'_>"));
}

#[test]
fn spanish_pack_behavior_uses_direct_passes() {
    let pack = include_str!("../../../language-packs/es-es/src/lib.rs");
    for expected in [
        "pub fn register() -> (LanguagePackManifest, EsEsPack)",
        "pub fn normalize_target_text(",
        "pub fn evaluate_answer(",
        "pub fn classify_error(",
        "pub fn card_template_at(",
        "pub fn tutor_instructions(",
        "pub fn scenario_at(",
        "pub fn seed_at(",
    ] {
        assert!(pack.contains(expected), "missing Spanish pass: {expected}");
    }
    let native = [
        include_str!("../../../apps/local/src/operations/start_lesson.rs"),
        include_str!("../../../apps/local/src/operations/complete_lesson.rs"),
        include_str!("../../../apps/local/src/operations/review_ops.rs"),
    ];
    for source in native {
        assert!(
            !source.contains("use language_pack_api::LanguagePack as _;"),
            "production still imports LanguagePack behavior"
        );
    }
}

#[test]
fn language_pack_contract_is_data_and_explicit_passes_only() {
    let api = include_str!("../../../crates/language-pack-api/src/lib.rs");
    let registry = include_str!("../../../crates/language-pack-registry/src/lib.rs");
    let spanish = include_str!("../../../language-packs/es-es/src/lib.rs");
    let synthetic = include_str!("../../../language-packs/synthetic-test-pack/src/lib.rs");
    let conformance = include_str!("../../../crates/test-support/src/lib.rs");

    for (name, source) in [
        ("api", api),
        ("registry", registry),
        ("spanish", spanish),
        ("synthetic", synthetic),
        ("conformance", conformance),
    ] {
        for forbidden in [
            "trait LanguagePack",
            "impl LanguagePack",
            "dyn LanguagePack",
            "LanguagePack as _",
        ] {
            assert!(
                !source.contains(forbidden),
                "{name} retains object-style language-pack behavior: {forbidden}"
            );
        }
    }

    assert!(!registry.contains("manifests: Vec<LanguagePackManifest>"));
    for column in [
        "manifest_id: Vec<String>",
        "source_language_end: Vec<u32>",
        "capability_end: Vec<u32>",
        "packs: Vec<P>",
    ] {
        assert!(
            registry.contains(column),
            "missing registry SoA/CSR column: {column}"
        );
    }
    assert!(registry.contains("pub type PackIndex = u32;"));
    assert!(
        registry.contains("pub fn index_of<P>(registry: &LanguagePackRegistry<P>, pack_id: &str)")
    );
    assert!(registry.contains("pub fn build_registry<P>("));

    for (name, source) in [("spanish", spanish), ("synthetic", synthetic)] {
        for forbidden in [
            "templates: Vec<CardTemplate>",
            "catalog: CurriculumCatalog",
            "taxonomy: Vec<",
        ] {
            assert!(
                !source.contains(forbidden),
                "{name} retains pack AoS storage: {forbidden}"
            );
        }
    }
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
