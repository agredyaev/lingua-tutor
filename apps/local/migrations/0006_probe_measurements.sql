-- 0006_probe_measurements: persist mandatory listening probe measurements.
ALTER TABLE listening_attempts
    ADD COLUMN assistance_level INTEGER
        CHECK (assistance_level IS NULL OR assistance_level BETWEEN 0 AND 3);
ALTER TABLE listening_attempts
    ADD COLUMN response_millis INTEGER
        CHECK (response_millis IS NULL OR response_millis BETWEEN 0 AND 600000);
ALTER TABLE listening_attempts
    ADD COLUMN probe_phase TEXT
        CHECK (probe_phase IS NULL OR probe_phase IN
            ('diagnostic','practice','delayed_retrieval','transfer'));
