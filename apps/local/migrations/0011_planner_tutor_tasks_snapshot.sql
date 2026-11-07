-- 0011_planner_tutor_tasks_snapshot: persist the exact bounded tutor-task queue chosen at lesson start.
-- Legacy planner rows remain NULL because their exact final queue was never persisted and must not be fabricated.

ALTER TABLE planner_decisions
    ADD COLUMN tutor_tasks_json TEXT;
