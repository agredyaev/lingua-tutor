//! Learning records repositories (WBS 4.3): `learning_items`,
//! `learning_item_evidence`, `error_targets`, `error_occurrences`,
//! `error_rechecks`, `listening_attempts`.

/// Insert a learning item.
pub const INSERT_ITEM: &str = "INSERT INTO learning_items
    (id, learner_language_profile_id, language_pack_id, item_type, target_text,
     normalized_target_text, prompt_text, prompt_language, status,
     language_metadata_json, introduced_lesson_id, created_at, updated_at)
 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?12)";

/// Update item status.
pub const UPDATE_ITEM_STATUS: &str =
    "UPDATE learning_items SET status = ?2, updated_at = ?3 WHERE id = ?1";

/// Item by ID.
pub const SELECT_ITEM: &str = "SELECT id, learner_language_profile_id,
    language_pack_id, item_type, target_text, normalized_target_text,
    prompt_text, prompt_language, status, language_metadata_json,
    introduced_lesson_id
 FROM learning_items WHERE id = ?1";

/// Duplicate check by canonical text (uses `ix_items_normalized`).
pub const SELECT_ITEM_BY_NORMALIZED: &str = "SELECT id, learner_language_profile_id,
    language_pack_id, item_type, target_text, normalized_target_text,
    prompt_text, prompt_language, status, language_metadata_json,
    introduced_lesson_id
 FROM learning_items
 WHERE learner_language_profile_id = ?1 AND normalized_target_text = ?2";

/// Profile items filtered by status (uses `ix_items_profile_status`).
pub const SELECT_ITEMS_BY_STATUS: &str = "SELECT id, learner_language_profile_id,
    language_pack_id, item_type, target_text, normalized_target_text,
    prompt_text, prompt_language, status, language_metadata_json,
    introduced_lesson_id
 FROM learning_items
 WHERE learner_language_profile_id = ?1 AND status = ?2 LIMIT ?3";

/// Insert evidence.
pub const INSERT_EVIDENCE: &str = "INSERT INTO learning_item_evidence
    (id, learning_item_id, lesson_id, kind, success, detail, pack_metadata_json,
     recorded_at, observation_order)
 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)";

/// Evidence trail for one item (uses `ix_evidence_item`).
pub const SELECT_EVIDENCE_FOR_ITEM: &str = "SELECT id, learning_item_id, lesson_id,
    kind, success, detail, pack_metadata_json, recorded_at
 FROM learning_item_evidence WHERE learning_item_id = ?1 ORDER BY recorded_at";

/// Insert an error target.
pub const INSERT_ERROR_TARGET: &str = "INSERT INTO error_targets
    (id, learner_language_profile_id, language_pack_id, language_pack_version,
     core_category, pack_error_code, original_production, corrected_production,
     explanation, occurrence_count, method_change_required, status,
     created_at, updated_at)
 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?13)";

/// Update target counters/status after a sighting.
pub const UPDATE_ERROR_TARGET: &str = "UPDATE error_targets SET
    occurrence_count = ?2, method_change_required = ?3, status = ?4, updated_at = ?5
 WHERE id = ?1";

/// Target by ID.
pub const SELECT_ERROR_TARGET: &str = "SELECT id, learner_language_profile_id,
    language_pack_id, language_pack_version, core_category, pack_error_code,
    original_production, corrected_production, explanation, occurrence_count,
    method_change_required, status
 FROM error_targets WHERE id = ?1";

/// Profile targets by status (uses `ix_errors_profile_status`).
pub const SELECT_ERROR_TARGETS: &str = "SELECT id, learner_language_profile_id,
    language_pack_id, language_pack_version, core_category, pack_error_code,
    original_production, corrected_production, explanation, occurrence_count,
    method_change_required, status
 FROM error_targets
 WHERE learner_language_profile_id = ?1 AND (?2 IS NULL OR status = ?2)
 ORDER BY occurrence_count DESC LIMIT ?3";

/// Insert one sighting.
pub const INSERT_ERROR_OCCURRENCE: &str = "INSERT INTO error_occurrences
    (id, error_target_id, lesson_id, context, communicative_impact, recorded_at)
 VALUES (?1, ?2, ?3, ?4, ?5, ?6)";

/// Insert a 1/3/7-day recheck slot.
pub const INSERT_ERROR_RECHECK: &str = "INSERT INTO error_rechecks
    (id, error_target_id, offset_days, due_at)
 VALUES (?1, ?2, ?3, ?4)
 ON CONFLICT(error_target_id, offset_days) DO NOTHING";

/// Complete a recheck with pass/fail.
pub const COMPLETE_ERROR_RECHECK: &str = "UPDATE error_rechecks SET
    completed_at = ?2, passed = ?3 WHERE id = ?1 AND completed_at IS NULL";

/// Due rechecks (uses partial index `ix_rechecks_due`).
pub const SELECT_DUE_RECHECKS: &str = "SELECT id, error_target_id, offset_days, due_at
 FROM error_rechecks WHERE completed_at IS NULL AND due_at <= ?1 ORDER BY due_at";

/// Insert a listening attempt (no audio, references only).
pub const INSERT_LISTENING_ATTEMPT: &str = "INSERT INTO listening_attempts
    (id, learner_language_profile_id, learning_item_id, lesson_id,
     source_reference, source_variant_id, talker_key, context_tag,
     transcript, success, assistance_level, response_millis, probe_phase, recorded_at)
 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)";

/// Profile listening attempts, newest first, bounded.
pub const SELECT_LISTENING_ATTEMPTS: &str = "SELECT id, learner_language_profile_id,
    learning_item_id, lesson_id, source_reference, source_variant_id, talker_key, context_tag,
    transcript, success, assistance_level, response_millis, probe_phase, recorded_at
 FROM listening_attempts WHERE learner_language_profile_id = ?1
 ORDER BY recorded_at DESC LIMIT ?2";
