-- 0007_semantic_metadata: neutral semantic names and removal of dead fields.
ALTER TABLE error_targets RENAME COLUMN pack_error_code TO error_code;
ALTER TABLE learning_item_evidence RENAME COLUMN pack_metadata_json TO metadata_json;
ALTER TABLE cards DROP COLUMN answer_evaluation_mode;
ALTER TABLE learning_items DROP COLUMN language_metadata_json;
