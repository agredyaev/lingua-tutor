-- 0009_error_recheck_cycles: preserve recheck history across failed 1/3/7-day cycles.
CREATE TABLE error_rechecks_v9 (
    id              TEXT PRIMARY KEY,
    error_target_id TEXT NOT NULL REFERENCES error_targets(id),
    cycle           INTEGER NOT NULL DEFAULT 0 CHECK (cycle >= 0),
    offset_days     INTEGER NOT NULL CHECK (offset_days IN (1,3,7)),
    due_at          TEXT NOT NULL,
    completed_at    TEXT,
    passed          INTEGER,
    assistance_level INTEGER CHECK (assistance_level BETWEEN 0 AND 3),
    context         TEXT,
    repair_applied  INTEGER CHECK (repair_applied IN (0,1)),
    UNIQUE (error_target_id, cycle, offset_days)
);

INSERT INTO error_rechecks_v9
    (id,error_target_id,cycle,offset_days,due_at,completed_at,passed,
     assistance_level,context,repair_applied)
SELECT r.id,r.error_target_id,0,r.offset_days,r.due_at,r.completed_at,r.passed,
       NULL,NULL,NULL
FROM error_rechecks r
JOIN error_targets t ON t.id = r.error_target_id
WHERE r.completed_at IS NOT NULL
   OR t.occurrence_count >=
      CASE
        WHEN EXISTS (
            SELECT 1 FROM error_occurrences o
            WHERE o.error_target_id=t.id AND o.communicative_impact='high'
        ) THEN 2
        WHEN EXISTS (
            SELECT 1 FROM error_occurrences o
            WHERE o.error_target_id=t.id AND o.communicative_impact='low'
        )
        AND NOT EXISTS (
            SELECT 1 FROM error_occurrences o
            WHERE o.error_target_id=t.id AND o.communicative_impact='medium'
        ) THEN 4
        ELSE 3
      END;

DROP TABLE error_rechecks;
ALTER TABLE error_rechecks_v9 RENAME TO error_rechecks;
CREATE INDEX ix_rechecks_due ON error_rechecks(due_at) WHERE completed_at IS NULL;
