-- 0004_methodology_calibration: objective task outcome + listening variability metadata.
ALTER TABLE lesson_metrics ADD COLUMN task_attempt_count INTEGER NOT NULL DEFAULT 0;
ALTER TABLE lesson_metrics ADD COLUMN task_success_count INTEGER NOT NULL DEFAULT 0;
ALTER TABLE listening_attempts ADD COLUMN source_variant_id TEXT;
ALTER TABLE listening_attempts ADD COLUMN talker_key TEXT;
ALTER TABLE listening_attempts ADD COLUMN context_tag TEXT;
ALTER TABLE error_occurrences ADD COLUMN communicative_impact TEXT NOT NULL DEFAULT 'medium'
    CHECK (communicative_impact IN ('low','medium','high'));
