-- 0006_language_neutral_storage: remove obsolete language-pack identity.
DROP INDEX IF EXISTS ux_llp_active_pack;

ALTER TABLE learner_language_profiles DROP COLUMN language_pack_id;
CREATE UNIQUE INDEX ux_llp_active_locale
    ON learner_language_profiles(learner_id, target_locale)
    WHERE status = 'active';

ALTER TABLE lessons DROP COLUMN language_pack_id;
ALTER TABLE lessons DROP COLUMN language_pack_version;
ALTER TABLE learning_items DROP COLUMN language_pack_id;
ALTER TABLE error_targets DROP COLUMN language_pack_id;
ALTER TABLE error_targets DROP COLUMN language_pack_version;
ALTER TABLE cards DROP COLUMN language_pack_id;
ALTER TABLE cards DROP COLUMN language_pack_version;
ALTER TABLE card_mutations DROP COLUMN language_pack_id;
ALTER TABLE card_mutations DROP COLUMN language_pack_version;

DROP TABLE language_packs;
