-- 0005_methodology_semantics: deterministic transfer ordering + auditable task rubric.
ALTER TABLE learning_item_evidence
    ADD COLUMN observation_order INTEGER NOT NULL DEFAULT 0;
ALTER TABLE lesson_metrics
    ADD COLUMN task_success_rubric TEXT;
CREATE INDEX ix_evidence_item_order
    ON learning_item_evidence(learning_item_id, recorded_at DESC, observation_order DESC);
