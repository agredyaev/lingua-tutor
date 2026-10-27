//! `cards`, `card_tags`, `card_mutations` repositories (WBS 5.1–5.2).

/// Insert a card.
pub const INSERT_CARD: &str = "INSERT INTO cards
    (id, learner_language_profile_id, language_pack_id, language_pack_version,
     intent, status, prompt, prompt_language, answer, answer_language,
     accepted_answers_json, answer_evaluation_mode, explanation,
     pronunciation_note, source_learning_item_id, source_error_target_id,
     source_lesson_id, creation_reason, priority_score, content_hash,
     content_version, parent_card_id, suspended_reason, created_by,
     created_at, updated_at)
 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15,
         ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?25)";

const CARD_COLUMNS: &str = "id, learner_language_profile_id, language_pack_id,
    language_pack_version, intent, status, prompt, prompt_language, answer,
    answer_language, accepted_answers_json, answer_evaluation_mode, explanation,
    pronunciation_note, source_learning_item_id, source_error_target_id,
    source_lesson_id, creation_reason, priority_score, content_hash,
    content_version, parent_card_id, suspended_reason, created_by";

/// Card by ID.
#[must_use]
pub fn select_card_sql() -> String {
    format!("SELECT {CARD_COLUMNS} FROM cards WHERE id = ?1")
}

/// Non-retired card by duplicate-detection hash (uses `ix_cards_content_hash`).
#[must_use]
pub fn select_card_by_hash_sql() -> String {
    format!(
        "SELECT {CARD_COLUMNS} FROM cards
         WHERE learner_language_profile_id = ?1 AND content_hash = ?2
           AND status != 'retired'"
    )
}

/// Profile cards with optional status/intent filters, bounded.
#[must_use]
pub fn select_cards_sql() -> String {
    format!(
        "SELECT {CARD_COLUMNS} FROM cards
         WHERE learner_language_profile_id = ?1
           AND (?2 IS NULL OR status = ?2)
           AND (?3 IS NULL OR intent = ?3)
         ORDER BY created_at DESC LIMIT ?4"
    )
}

/// Content update, schedule preserved (cosmetic/clarifying edits).
pub const UPDATE_CARD_CONTENT: &str = "UPDATE cards SET
    prompt = ?2, explanation = ?3, pronunciation_note = ?4,
    accepted_answers_json = ?5, content_hash = ?6,
    content_version = content_version + 1, updated_at = ?7
 WHERE id = ?1";

/// Status transition (+ suspended_reason bookkeeping).
pub const UPDATE_CARD_STATUS: &str = "UPDATE cards SET
    status = ?2, suspended_reason = ?3, updated_at = ?4 WHERE id = ?1";

/// Accepted-answers replacement.
pub const UPDATE_ACCEPTED_ANSWERS: &str = "UPDATE cards SET
    accepted_answers_json = ?2, content_hash = ?3, updated_at = ?4 WHERE id = ?1";

/// Cards activated (created non-candidate) today — activation budget input.
pub const COUNT_ACTIVATED_TODAY: &str = "SELECT COUNT(*) AS n FROM cards
 WHERE learner_language_profile_id = ?1 AND status != 'candidate'
   AND created_at >= ?2";

/// Oldest deferred (`candidate`) cards for a profile, for activation-budget
/// promotion: a card that missed the daily mint budget at creation time has
/// no other path back to `new` — nothing else in the codebase ever revisits
/// it — so each queue fetch gives the oldest ones first claim on today's
/// remaining budget.
pub const SELECT_OLDEST_CANDIDATES: &str = "SELECT id FROM cards
 WHERE learner_language_profile_id = ?1 AND status = 'candidate'
 ORDER BY created_at ASC LIMIT ?2";

/// Audit row per attempted mutation (applied or not, 07 §3).
pub const INSERT_CARD_MUTATION: &str = "INSERT INTO card_mutations
    (id, card_id, operation, client_operation_id, before_json, after_json,
     source_lesson_id, actor, reason, language_pack_id, language_pack_version,
     validation_result, created_at)
 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
 ON CONFLICT(client_operation_id) DO NOTHING";

/// Tag insert.
pub const INSERT_CARD_TAG: &str = "INSERT INTO card_tags
    (card_id, tag_type, normalized_tag) VALUES (?1, ?2, ?3)
 ON CONFLICT DO NOTHING";
