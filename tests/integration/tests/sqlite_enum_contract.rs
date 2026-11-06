#![allow(clippy::cognitive_complexity)]
// Exhaustive table-driven enum contract test intentionally checks every persisted spelling in one fixture.
//! Golden SQLite closed-enum TEXT contract. No trait or object mapping.
#![allow(clippy::unwrap_used)]

use lingua_tutor_local::repositories::*;

#[test]
fn all_closed_enum_spellings_round_trip() {
    for (value, text) in [
        (learning_core::ProfileStatus::Active, "active"),
        (learning_core::ProfileStatus::Paused, "paused"),
        (learning_core::ProfileStatus::Archived, "archived"),
    ] {
        assert_eq!(profile_status_db(&value), text);
        assert_eq!(profile_status_from_db(text), Some(value));
    }
    assert_eq!(profile_status_from_db("invalid-enum-variant"), None);
    for (value, text) in [
        (learning_core::LessonStatus::Planned, "planned"),
        (learning_core::LessonStatus::Active, "active"),
        (learning_core::LessonStatus::Completed, "completed"),
        (learning_core::LessonStatus::Abandoned, "abandoned"),
    ] {
        assert_eq!(lesson_status_db(&value), text);
        assert_eq!(lesson_status_from_db(text), Some(value));
    }
    assert_eq!(lesson_status_from_db("invalid-enum-variant"), None);
    for (value, text) in [
        (learning_core::LessonSource::Pwa, "pwa"),
        (learning_core::LessonSource::Import, "import"),
        (learning_core::LessonSource::Mcp, "mcp"),
    ] {
        assert_eq!(lesson_source_db(&value), text);
        assert_eq!(lesson_source_from_db(text), Some(value));
    }
    assert_eq!(lesson_source_from_db("invalid-enum-variant"), None);
    for (value, text) in [
        (learning_core::LearningItemType::Phrase, "phrase"),
        (learning_core::LearningItemType::Collocation, "collocation"),
        (learning_core::LearningItemType::Pattern, "pattern"),
        (learning_core::LearningItemType::Word, "word"),
        (
            learning_core::LearningItemType::ResponseFrame,
            "response_frame",
        ),
    ] {
        assert_eq!(learning_item_type_db(&value), text);
        assert_eq!(learning_item_type_from_db(text), Some(value));
    }
    assert_eq!(learning_item_type_from_db("invalid-enum-variant"), None);
    for (value, text) in [
        (learning_core::LearningItemStatus::Candidate, "candidate"),
        (learning_core::LearningItemStatus::Introduced, "introduced"),
        (learning_core::LearningItemStatus::Learning, "learning"),
        (
            learning_core::LearningItemStatus::TransferPending,
            "transfer_pending",
        ),
        (
            learning_core::LearningItemStatus::ProvisionallyMastered,
            "provisionally_mastered",
        ),
        (learning_core::LearningItemStatus::Mastered, "mastered"),
        (learning_core::LearningItemStatus::Suspended, "suspended"),
    ] {
        assert_eq!(learning_item_status_db(&value), text);
        assert_eq!(learning_item_status_from_db(text), Some(value));
    }
    assert_eq!(learning_item_status_from_db("invalid-enum-variant"), None);
    for (value, text) in [
        (learning_core::EvidenceKind::Recognition, "recognition"),
        (learning_core::EvidenceKind::Listening, "listening"),
        (
            learning_core::EvidenceKind::PromptedProduction,
            "prompted_production",
        ),
        (
            learning_core::EvidenceKind::SpontaneousProduction,
            "spontaneous_production",
        ),
        (learning_core::EvidenceKind::Transfer, "transfer"),
        (learning_core::EvidenceKind::Correction, "correction"),
    ] {
        assert_eq!(evidence_kind_db(&value), text);
        assert_eq!(evidence_kind_from_db(text), Some(value));
    }
    assert_eq!(evidence_kind_from_db("invalid-enum-variant"), None);
    for (value, text) in [
        (learning_core::ErrorTargetStatus::Active, "active"),
        (learning_core::ErrorTargetStatus::Recheck, "recheck"),
        (learning_core::ErrorTargetStatus::Resolved, "resolved"),
        (learning_core::ErrorTargetStatus::Escalated, "escalated"),
    ] {
        assert_eq!(error_target_status_db(&value), text);
        assert_eq!(error_target_status_from_db(text), Some(value));
    }
    assert_eq!(error_target_status_from_db("invalid-enum-variant"), None);
    for (value, text) in [
        (learning_core::CardStatus::Candidate, "candidate"),
        (learning_core::CardStatus::New, "new"),
        (learning_core::CardStatus::Learning, "learning"),
        (learning_core::CardStatus::Review, "review"),
        (learning_core::CardStatus::Relearning, "relearning"),
        (learning_core::CardStatus::Suspended, "suspended"),
        (learning_core::CardStatus::Retired, "retired"),
    ] {
        assert_eq!(card_status_db(&value), text);
        assert_eq!(card_status_from_db(text), Some(value));
    }
    assert_eq!(card_status_from_db("invalid-enum-variant"), None);
    for (value, text) in [
        (learning_core::CardActor::Tutor, "tutor"),
        (learning_core::CardActor::User, "user"),
        (learning_core::CardActor::System, "system"),
    ] {
        assert_eq!(card_actor_db(&value), text);
        assert_eq!(card_actor_from_db(text), Some(value));
    }
    assert_eq!(card_actor_from_db("invalid-enum-variant"), None);
    for (value, text) in [
        (learning_core::CoreErrorCategory::Grammar, "grammar"),
        (learning_core::CoreErrorCategory::Vocabulary, "vocabulary"),
        (learning_core::CoreErrorCategory::WordOrder, "word_order"),
        (learning_core::CoreErrorCategory::Register, "register"),
        (
            learning_core::CoreErrorCategory::Pronunciation,
            "pronunciation",
        ),
        (learning_core::CoreErrorCategory::Orthography, "orthography"),
        (learning_core::CoreErrorCategory::Other, "other"),
    ] {
        assert_eq!(core_error_category_db(&value), text);
        assert_eq!(core_error_category_from_db(text), Some(value));
    }
    assert_eq!(core_error_category_from_db("invalid-enum-variant"), None);
    for (value, text) in [
        (learning_core::CardIntent::Production, "production"),
        (learning_core::CardIntent::Listening, "listening"),
        (learning_core::CardIntent::Contrast, "contrast"),
        (learning_core::CardIntent::Transfer, "transfer"),
    ] {
        assert_eq!(card_intent_db(&value), text);
        assert_eq!(card_intent_from_db(text), Some(value));
    }
    assert_eq!(card_intent_from_db("invalid-enum-variant"), None);
}
