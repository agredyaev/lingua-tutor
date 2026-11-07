-- 0008_remove_review_experiments: remove unused review experiment metadata.
ALTER TABLE card_schedule_state DROP COLUMN parameter_set_id;
DROP TABLE scheduler_parameter_sets;
