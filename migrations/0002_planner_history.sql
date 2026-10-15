-- 0002_planner_history: persist pre-lesson planner decisions and post-lesson outcomes.
-- Capability state and tutor-focus threads remain derived from immutable evidence.

CREATE TABLE planner_decisions (
    id                          TEXT PRIMARY KEY,
    learner_language_profile_id TEXT NOT NULL REFERENCES learner_language_profiles(id),
    lesson_id                   TEXT REFERENCES lessons(id),
    planner_version             TEXT NOT NULL,
    input_json                  TEXT NOT NULL,
    plan_json                   TEXT NOT NULL,
    task_candidates_json        TEXT NOT NULL,
    capability_state_json       TEXT NOT NULL,
    focus_threads_json          TEXT NOT NULL,
    outcome_json                TEXT,
    created_at                  TEXT NOT NULL,
    updated_at                  TEXT NOT NULL
);
CREATE UNIQUE INDEX ux_planner_decision_lesson
    ON planner_decisions(lesson_id) WHERE lesson_id IS NOT NULL;
CREATE INDEX ix_planner_decisions_profile_created
    ON planner_decisions(learner_language_profile_id, created_at DESC);
