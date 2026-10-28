//! `review_sessions`, `review_events`, `card_schedule_state` repositories
//! (WBS 5.4). Event + state write = one batch (04-database.md §1).

/// Insert a review session.
pub const INSERT_SESSION: &str = "INSERT INTO review_sessions
    (id, learner_language_profile_id, started_at, source, card_count, created_at)
 VALUES (?1, ?2, ?3, ?4, 0, ?3)";

/// Session by ID.
pub const SELECT_SESSION: &str = "SELECT id, learner_language_profile_id,
    started_at, completed_at, source, card_count, review_seconds
 FROM review_sessions WHERE id = ?1";

/// Close a session with totals.
pub const COMPLETE_SESSION: &str = "UPDATE review_sessions SET
    completed_at = ?2, card_count = ?3, review_seconds = ?4
 WHERE id = ?1 AND completed_at IS NULL";

/// Queue candidates: reviewable cards + schedule state (uses
/// `ix_cards_profile_status` + PK join; due filter applied in logic layer).
pub const SELECT_QUEUE_CANDIDATES: &str = "SELECT c.id AS card_id, c.status,
    c.intent, c.prompt, c.prompt_language, c.answer, c.accepted_answers_json,
    c.answer_evaluation_mode, c.explanation, c.pronunciation_note,
    s.algorithm, s.algorithm_version, s.parameter_set_id, s.difficulty,
    s.stability, s.last_review_at, s.due_at, s.scheduled_interval_seconds,
    s.lapse_count, s.review_count, s.state_version, s.state_hash
 FROM cards c LEFT JOIN card_schedule_state s ON s.card_id = c.id
 WHERE c.learner_language_profile_id = ?1
   AND c.status IN ('new','learning','review','relearning')
 LIMIT 500";

/// Schedule state by card.
pub const SELECT_SCHEDULE_STATE: &str = "SELECT card_id, algorithm,
    algorithm_version, parameter_set_id, difficulty, stability, last_review_at,
    due_at, scheduled_interval_seconds, lapse_count, review_count,
    state_version, state_hash
 FROM card_schedule_state WHERE card_id = ?1";

/// Upsert schedule state (one row per card).
pub const UPSERT_SCHEDULE_STATE: &str = "INSERT INTO card_schedule_state
    (card_id, algorithm, algorithm_version, parameter_set_id, difficulty,
     stability, last_review_at, due_at, scheduled_interval_seconds,
     lapse_count, review_count, state_version, state_hash, updated_at)
 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
 ON CONFLICT(card_id) DO UPDATE SET
    algorithm = excluded.algorithm,
    algorithm_version = excluded.algorithm_version,
    parameter_set_id = excluded.parameter_set_id,
    difficulty = excluded.difficulty,
    stability = excluded.stability,
    last_review_at = excluded.last_review_at,
    due_at = excluded.due_at,
    scheduled_interval_seconds = excluded.scheduled_interval_seconds,
    lapse_count = excluded.lapse_count,
    review_count = excluded.review_count,
    state_version = excluded.state_version,
    state_hash = excluded.state_hash,
    updated_at = excluded.updated_at";

/// Insert an immutable review event.
pub const INSERT_EVENT: &str = "INSERT INTO review_events
    (id, card_id, review_session_id, client_operation_id, presented_at,
     answered_at, grade, response_millis, answer_revealed, learner_note,
     prior_state_hash, result_state_hash, algorithm_version, source,
     compensates_event_id, created_at)
 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)";

/// Idempotency lookup.
pub const SELECT_EVENT_BY_OP: &str = "SELECT id, card_id, review_session_id,
    client_operation_id, grade, result_state_hash, compensates_event_id, created_at
 FROM review_events WHERE client_operation_id = ?1";

/// Effective event history for one card, excluding the event being undone.
pub const SELECT_EFFECTIVE_EVENTS_FOR_CARD: &str = "SELECT e.answered_at, e.grade
 FROM review_events e
 WHERE e.card_id = ?1
   AND e.id != ?2
   AND e.compensates_event_id IS NULL
   AND NOT EXISTS (
       SELECT 1 FROM review_events x WHERE x.compensates_event_id = e.id)
 ORDER BY e.rowid";

/// Last event in a session (undo target).
pub const SELECT_LAST_SESSION_EVENT: &str = "SELECT id, card_id, review_session_id,
    client_operation_id, presented_at, answered_at, grade, response_millis,
    answer_revealed, prior_state_hash, result_state_hash, algorithm_version,
    source, compensates_event_id, created_at
 FROM review_events WHERE review_session_id = ?1 ORDER BY rowid DESC LIMIT 1";

/// Events graded today (summary + adaptation signals).
pub const SELECT_EVENTS_SINCE: &str = "SELECT e.grade, e.response_millis
 FROM review_events e
 JOIN cards c ON c.id = e.card_id
 WHERE c.learner_language_profile_id = ?1 AND e.created_at >= ?2
   AND e.compensates_event_id IS NULL";

/// Effective (never-undone) review events since an instant, for retention
/// stats: grade + creation instant, compensation lineage excluded both ways.
pub const SELECT_EVENTS_FOR_RETENTION: &str = "SELECT e.grade, e.created_at
 FROM review_events e
 JOIN cards c ON c.id = e.card_id
 WHERE c.learner_language_profile_id = ?1 AND e.created_at >= ?2
   AND e.compensates_event_id IS NULL
   AND NOT EXISTS (
       SELECT 1 FROM review_events x WHERE x.compensates_event_id = e.id)
 ORDER BY e.created_at";

/// Due counts per day for the forecast (uses `ix_schedule_due`).
pub const SELECT_DUE_BETWEEN: &str = "SELECT s.due_at
 FROM card_schedule_state s
 JOIN cards c ON c.id = s.card_id
 WHERE c.learner_language_profile_id = ?1
   AND c.status IN ('learning','review','relearning')
   AND s.due_at IS NOT NULL AND s.due_at < ?2";

/// Overdue count (backlog protection input).
pub const COUNT_OVERDUE: &str = "SELECT COUNT(*) AS n
 FROM card_schedule_state s
 JOIN cards c ON c.id = s.card_id
 WHERE c.learner_language_profile_id = ?1
   AND c.status IN ('review','relearning') AND s.due_at < ?2";

/// New cards introduced (graded for the first time) today — the *session*
/// admission budget. Deliberately distinct from `cards::COUNT_ACTIVATED_TODAY`
/// (which counts by creation date): a card minted `New` today hasn't been
/// shown in a queue yet, so charging it against creation date would
/// double-spend the same budget and exclude it from the very session meant
/// to introduce it. `review_count = 1` identifies a card graded exactly
/// once ever (all cards start at `New` with `review_count = 0` and any
/// grade moves them off `New` for good), so this counts first-ever grades
/// whose event landed today, regardless of when the card was created.
pub const COUNT_NEW_CARDS_INTRODUCED_TODAY: &str = "SELECT COUNT(*) AS n
 FROM review_events e
 JOIN cards c ON c.id = e.card_id
 JOIN card_schedule_state s ON s.card_id = e.card_id
 WHERE c.learner_language_profile_id = ?1 AND e.created_at >= ?2
   AND e.compensates_event_id IS NULL AND s.review_count = 1";

/// `card_schedule_state` row, column-shaped.
/// Last-window effective grades per card touched in a session (window
/// function — one query for all cards, 16-rust-guidelines.md §5).
pub const SELECT_SESSION_CARD_SIGNALS: &str = "SELECT w.card_id, w.grade, w.response_millis
 FROM (
   SELECT e.card_id, e.grade, e.response_millis,
          ROW_NUMBER() OVER (PARTITION BY e.card_id
                             ORDER BY e.created_at DESC, e.id DESC) AS rn
   FROM review_events e
   WHERE e.compensates_event_id IS NULL
     AND e.id NOT IN (SELECT compensates_event_id FROM review_events
                      WHERE compensates_event_id IS NOT NULL)
     AND e.card_id IN (SELECT DISTINCT card_id FROM review_events
                       WHERE review_session_id = ?1)
 ) w WHERE w.rn <= 10 ORDER BY w.card_id, w.rn";

/// Evidence + volume aggregates per session card (one grouped query).
pub const SELECT_SESSION_CARD_EVIDENCE: &str = "SELECT c.id AS card_id,
    c.source_learning_item_id,
    COALESCE(SUM(CASE WHEN ev.kind = 'spontaneous_production' AND ev.success = 0
                      THEN 1 ELSE 0 END), 0) AS spont_fail,
    COALESCE(SUM(CASE WHEN ev.kind = 'spontaneous_production' AND ev.success = 1
                      THEN 1 ELSE 0 END), 0) AS spont_ok,
    COALESCE(SUM(CASE WHEN ev.kind = 'prompted_production' AND ev.success = 1
                      THEN 1 ELSE 0 END), 0) AS prompted_ok,
    COALESCE(SUM(CASE WHEN ev.kind = 'listening' AND ev.success = 0
                      THEN 1 ELSE 0 END), 0) AS listen_fail,
    (SELECT COUNT(*) FROM review_events re
      WHERE re.card_id = c.id AND re.compensates_event_id IS NULL) AS review_count
 FROM cards c
 LEFT JOIN learning_item_evidence ev ON ev.learning_item_id = c.source_learning_item_id
 WHERE c.id IN (SELECT DISTINCT card_id FROM review_events WHERE review_session_id = ?1)
 GROUP BY c.id";
