//! `learner_profile` + `learner_language_profiles` repositories (WBS 4.2).

/// Upsert for the single learner row.
pub const UPSERT_LEARNER: &str = "INSERT INTO learner_profile
    (id, timezone, daily_session_target, session_minutes_target, max_new_cards,
     desired_retention, max_review_minutes, created_at, updated_at)
 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)
 ON CONFLICT(id) DO UPDATE SET
    timezone = excluded.timezone,
    daily_session_target = excluded.daily_session_target,
    session_minutes_target = excluded.session_minutes_target,
    max_new_cards = excluded.max_new_cards,
    desired_retention = excluded.desired_retention,
    max_review_minutes = excluded.max_review_minutes,
    updated_at = excluded.updated_at";

/// Single learner row lookup.
pub const SELECT_LEARNER: &str = "SELECT id, timezone, daily_session_target,
    session_minutes_target, max_new_cards, desired_retention, max_review_minutes
 FROM learner_profile LIMIT 1";

/// Insert a language profile.
pub const INSERT_LANGUAGE_PROFILE: &str = "INSERT INTO learner_language_profiles
    (id, learner_id, language_pack_id, source_language, target_locale,
     estimated_level, current_lesson_number, status, is_default,
     preferences_json, created_at, updated_at)
 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?11)";

/// All language profiles, default first.
pub const SELECT_LANGUAGE_PROFILES: &str = "SELECT p.id, p.learner_id, p.language_pack_id,
    p.source_language, p.target_locale, p.estimated_level,
    MAX(
        p.current_lesson_number,
        COALESCE(
            (SELECT MAX(l.lesson_number)
             FROM lessons l
             WHERE l.learner_language_profile_id = p.id
               AND l.status = 'completed'),
            0
        )
    ) AS current_lesson_number,
    p.status, p.is_default, p.preferences_json
 FROM learner_language_profiles p ORDER BY p.is_default DESC, p.created_at";

/// One language profile by ID.
pub const SELECT_LANGUAGE_PROFILE: &str = "SELECT p.id, p.learner_id, p.language_pack_id,
    p.source_language, p.target_locale, p.estimated_level,
    MAX(
        p.current_lesson_number,
        COALESCE(
            (SELECT MAX(l.lesson_number)
             FROM lessons l
             WHERE l.learner_language_profile_id = p.id
               AND l.status = 'completed'),
            0
        )
    ) AS current_lesson_number,
    p.status, p.is_default, p.preferences_json
 FROM learner_language_profiles p WHERE p.id = ?1";

/// Patch mutable profile fields.
pub const UPDATE_LANGUAGE_PROFILE: &str = "UPDATE learner_language_profiles SET
    estimated_level = ?2, status = ?3, is_default = ?4, preferences_json = ?5,
    current_lesson_number = ?6, updated_at = ?7
 WHERE id = ?1";
