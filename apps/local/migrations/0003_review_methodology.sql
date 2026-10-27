-- 0003_review_methodology: persist effective daily new-card budget for gradual backlog recovery.
-- Review minutes and recall remain derived from immutable sessions/events.

CREATE TABLE review_daily_budget (
    learner_language_profile_id TEXT NOT NULL REFERENCES learner_language_profiles(id),
    local_day_start             TEXT NOT NULL,
    effective_budget            INTEGER NOT NULL CHECK (effective_budget BETWEEN 0 AND 255),
    updated_at                  TEXT NOT NULL,
    PRIMARY KEY (learner_language_profile_id, local_day_start)
);
CREATE INDEX ix_review_daily_budget_profile_day
    ON review_daily_budget(learner_language_profile_id, local_day_start DESC);
