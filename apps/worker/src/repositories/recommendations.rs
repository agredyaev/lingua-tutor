//! `recommendations` repository (WBS 8.2). Append with open-duplicate
//! guard; status transitions accept/dismiss; never silent mutation.

/// Insert one recommendation.
pub const INSERT: &str = "INSERT INTO recommendations
    (id, learner_language_profile_id, subject_type, subject_id, action,
     detail_json, status, created_at)
 VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'open', ?7)";

/// Open duplicate check: same subject + action already pending.
pub const SELECT_OPEN_DUPLICATE: &str = "SELECT id FROM recommendations
 WHERE learner_language_profile_id = ?1 AND subject_type = ?2
   AND (subject_id = ?3 OR (subject_id IS NULL AND ?3 IS NULL))
   AND action = ?4 AND status = 'open' LIMIT 1";

/// Open recommendations, newest first (uses `ix_recommendations_open`).
pub const SELECT_OPEN: &str = "SELECT id, subject_type, subject_id, action,
    detail_json, created_at
 FROM recommendations
 WHERE learner_language_profile_id = ?1 AND status = 'open'
 ORDER BY created_at DESC LIMIT ?2";

/// Accept/dismiss.
pub const UPDATE_STATUS: &str = "UPDATE recommendations SET
    status = ?2, resolved_at = ?3 WHERE id = ?1 AND status = 'open'";

/// Open redesign candidates joined with card fronts (08 §5 endpoint).
pub const SELECT_REDESIGN_CANDIDATES: &str = "SELECT r.id AS recommendation_id,
    r.detail_json, c.id AS card_id, c.prompt, c.answer, c.status AS card_status
 FROM recommendations r
 JOIN cards c ON c.id = r.subject_id
 WHERE r.learner_language_profile_id = ?1 AND r.status = 'open'
   AND r.action = 'redesign_card'
 ORDER BY r.created_at DESC LIMIT 50";
