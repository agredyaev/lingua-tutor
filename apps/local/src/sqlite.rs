//! Shared SQLite boundary: runtime-used closed-value codecs, timestamp conversion,
//! and the small set of SQL queries with multiple independent consumers.

use learning_core::Timestamp;

// Explicit, statically dispatched SQLite TEXT passes. Separate encode/decode
// macros avoid generating unused symmetric APIs.
macro_rules! db_enum_from_text {
    ($type:path, $name:ident, { $($variant:ident => $text:literal),+ $(,)? }) => {
        #[must_use]
        pub(crate) fn $name(value: &str) -> Option<$type> {
            use $type as Enum;
            match value {
                $($text => Some(Enum::$variant),)+
                _ => None,
            }
        }
    };
}

macro_rules! db_enum_to_text {
    ($type:path, $name:ident, { $($variant:ident => $text:literal),+ $(,)? }) => {
        #[must_use]
        pub(crate) fn $name(value: &$type) -> &'static str {
            use $type as Enum;
            match value {
                $(Enum::$variant => $text,)+
            }
        }
    };
}

macro_rules! db_enum_passes {
    ($type:path, $parse:ident, $format:ident, { $($variant:ident => $text:literal),+ $(,)? }) => {
        db_enum_from_text!($type, $parse, { $($variant => $text),+ });
        db_enum_to_text!($type, $format, { $($variant => $text),+ });
    };
}

db_enum_to_text!(learning_core::LearningItemType, learning_item_type_db, {
    Phrase => "phrase",
    Collocation => "collocation",
    Pattern => "pattern",
    Word => "word",
    ResponseFrame => "response_frame",
});

db_enum_to_text!(learning_core::LearningItemStatus, learning_item_status_db, {
    Candidate => "candidate",
    Introduced => "introduced",
    Learning => "learning",
    TransferPending => "transfer_pending",
    ProvisionallyMastered => "provisionally_mastered",
    Mastered => "mastered",
    Suspended => "suspended",
});

db_enum_to_text!(learning_core::EvidenceKind, evidence_kind_db, {
    Recognition => "recognition",
    Listening => "listening",
    PromptedProduction => "prompted_production",
    SpontaneousProduction => "spontaneous_production",
    Transfer => "transfer",
    Correction => "correction",
});

db_enum_passes!(learning_core::CardStatus, card_status_from_db, card_status_db, {
    Candidate => "candidate",
    New => "new",
    Learning => "learning",
    Review => "review",
    Relearning => "relearning",
    Suspended => "suspended",
    Retired => "retired",
});

db_enum_to_text!(learning_core::CoreErrorCategory, core_error_category_db, {
    Grammar => "grammar",
    Vocabulary => "vocabulary",
    WordOrder => "word_order",
    Register => "register",
    Pronunciation => "pronunciation",
    Orthography => "orthography",
    Other => "other",
});

db_enum_passes!(learning_core::CardIntent, card_intent_from_db, card_intent_db, {
    Production => "production",
    Listening => "listening",
    Contrast => "contrast",
    Transfer => "transfer",
});

db_enum_to_text!(tutor_contract::MutationOperation, mutation_operation_db, {
    Create => "create",
    Update => "update",
    Suspend => "suspend",
    Restore => "restore",
    Retire => "retire",
    AddAcceptedAnswer => "add_accepted_answer",
    RemoveAcceptedAnswer => "remove_accepted_answer",
});

pub(crate) fn communicative_impact_db(value: &learning_core::CommunicativeImpact) -> &'static str {
    match value {
        learning_core::CommunicativeImpact::Low => "low",
        learning_core::CommunicativeImpact::Medium => "medium",
        learning_core::CommunicativeImpact::High => "high",
    }
}

/// Unix seconds → RFC 3339 TEXT.
#[must_use]
pub(crate) fn to_rfc3339(t: Timestamp) -> String {
    chrono::DateTime::from_timestamp(t.0, 0)
        .unwrap_or_default()
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

/// RFC 3339 TEXT → unix seconds.
///
/// # Errors
/// Returns a concise parse error for invalid persisted or request text.
pub(crate) fn from_rfc3339(text: &str) -> Result<Timestamp, String> {
    chrono::DateTime::parse_from_rfc3339(text)
        .map(|dt| Timestamp(dt.timestamp()))
        .map_err(|error| format!("timestamp '{text}': {error}"))
}

// Multi-consumer review queries.
pub(crate) const SELECT_SESSION: &str = "SELECT id, learner_language_profile_id,
    started_at, completed_at, source, card_count, review_seconds
 FROM review_sessions WHERE id = ?1";

pub(crate) const SELECT_EVENTS_SINCE: &str = "SELECT e.grade, e.response_millis
 FROM review_events e
 JOIN cards c ON c.id = e.card_id
 WHERE c.learner_language_profile_id = ?1 AND e.created_at >= ?2
   AND e.compensates_event_id IS NULL";

pub(crate) const COUNT_OVERDUE: &str = "SELECT COUNT(*) AS n
 FROM card_schedule_state s
 JOIN cards c ON c.id = s.card_id
 WHERE c.learner_language_profile_id = ?1
   AND c.status IN ('review','relearning') AND s.due_at < ?2";

// Multi-consumer recommendation queries. Legacy open rows whose referenced
// subject is no longer actionable are filtered fail-closed at read time.
pub(crate) const SELECT_OPEN: &str = "SELECT r.id, r.subject_type, r.subject_id, r.action,
    r.detail_json, r.created_at
 FROM recommendations r
 WHERE r.learner_language_profile_id = ?1 AND r.status = 'open'
   AND (
       r.subject_type NOT IN ('learning_item','error_target','card')
       OR (
           r.subject_type='learning_item'
           AND EXISTS (
               SELECT 1 FROM learning_items i
               WHERE i.id=r.subject_id
                 AND i.learner_language_profile_id=r.learner_language_profile_id
                 AND i.status IN ('introduced','learning','transfer_pending',
                                  'provisionally_mastered','mastered')
           )
       )
       OR (
           r.subject_type='error_target'
           AND EXISTS (
               SELECT 1 FROM error_targets t
               WHERE t.id=r.subject_id
                 AND t.learner_language_profile_id=r.learner_language_profile_id
                 AND t.status IN ('active','escalated')
           )
       )
       OR (
           r.subject_type='card'
           AND EXISTS (
               SELECT 1 FROM cards c
               WHERE c.id=r.subject_id
                 AND c.learner_language_profile_id=r.learner_language_profile_id
                 AND c.status IN ('new','learning','review','relearning')
           )
       )
   )
 ORDER BY r.created_at ASC LIMIT ?2";

#[cfg(test)]
#[allow(clippy::cognitive_complexity)]
mod enum_contract_tests {
    use super::*;

    #[test]
    fn runtime_storage_encoders_preserve_stable_spellings() {
        use learning_core::{
            CoreErrorCategory as E, EvidenceKind as K, LearningItemStatus as S,
            LearningItemType as T,
        };
        use tutor_contract::MutationOperation as M;

        for (value, text) in [
            (T::Phrase, "phrase"),
            (T::Collocation, "collocation"),
            (T::Pattern, "pattern"),
            (T::Word, "word"),
            (T::ResponseFrame, "response_frame"),
        ] {
            assert_eq!(learning_item_type_db(&value), text);
        }

        for (value, text) in [
            (S::Candidate, "candidate"),
            (S::Introduced, "introduced"),
            (S::Learning, "learning"),
            (S::TransferPending, "transfer_pending"),
            (S::ProvisionallyMastered, "provisionally_mastered"),
            (S::Mastered, "mastered"),
            (S::Suspended, "suspended"),
        ] {
            assert_eq!(learning_item_status_db(&value), text);
        }

        for (value, text) in [
            (K::Recognition, "recognition"),
            (K::Listening, "listening"),
            (K::PromptedProduction, "prompted_production"),
            (K::SpontaneousProduction, "spontaneous_production"),
            (K::Transfer, "transfer"),
            (K::Correction, "correction"),
        ] {
            assert_eq!(evidence_kind_db(&value), text);
        }

        for (value, text) in [
            (E::Grammar, "grammar"),
            (E::Vocabulary, "vocabulary"),
            (E::WordOrder, "word_order"),
            (E::Register, "register"),
            (E::Pronunciation, "pronunciation"),
            (E::Orthography, "orthography"),
            (E::Other, "other"),
        ] {
            assert_eq!(core_error_category_db(&value), text);
        }

        for (value, text) in [
            (M::Create, "create"),
            (M::Update, "update"),
            (M::Suspend, "suspend"),
            (M::Restore, "restore"),
            (M::Retire, "retire"),
            (M::AddAcceptedAnswer, "add_accepted_answer"),
            (M::RemoveAcceptedAnswer, "remove_accepted_answer"),
        ] {
            assert_eq!(mutation_operation_db(&value), text);
        }
    }

    #[test]
    fn runtime_storage_decoders_reject_unknown_spellings() {
        use learning_core::{CardIntent as I, CardStatus as S};

        for (value, text) in [
            (S::Candidate, "candidate"),
            (S::New, "new"),
            (S::Learning, "learning"),
            (S::Review, "review"),
            (S::Relearning, "relearning"),
            (S::Suspended, "suspended"),
            (S::Retired, "retired"),
        ] {
            assert_eq!(card_status_db(&value), text);
            assert_eq!(card_status_from_db(text), Some(value));
        }
        assert_eq!(card_status_from_db("invalid-enum-variant"), None);

        for (value, text) in [
            (I::Production, "production"),
            (I::Listening, "listening"),
            (I::Contrast, "contrast"),
            (I::Transfer, "transfer"),
        ] {
            assert_eq!(card_intent_db(&value), text);
            assert_eq!(card_intent_from_db(text), Some(value));
        }
        assert_eq!(card_intent_from_db("invalid-enum-variant"), None);
    }
}
