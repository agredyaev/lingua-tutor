-- 0010_error_recheck_repair_action: retain the named corrective method for audit/history.
ALTER TABLE error_rechecks ADD COLUMN repair_action TEXT;
