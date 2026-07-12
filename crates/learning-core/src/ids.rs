//! Entity ID newtypes (03-domain-model.md §1): stable opaque strings
//! (UUIDv7/ULID), one type per entity — bare `String` IDs never cross crate
//! boundaries (16-rust-guidelines.md §3).

use serde::{Deserialize, Serialize};

macro_rules! id_newtype {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub String);

        impl $name {
            /// Borrowed ID value.
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl From<String> for $name {
            fn from(value: String) -> Self {
                Self(value)
            }
        }
    };
}

id_newtype!(
    /// Global learner (single-owner app: exactly one row).
    LearnerId
);
id_newtype!(
    /// Per-language learning profile.
    LanguageProfileId
);
id_newtype!(
    /// Stable language pack ID, e.g. `es-ES`.
    LanguagePackId
);
id_newtype!(
    /// Lesson.
    LessonId
);
id_newtype!(
    /// Learning item (phrase/pattern/word/frame).
    LearningItemId
);
id_newtype!(
    /// One evidence row.
    EvidenceId
);
id_newtype!(
    /// Tracked error target.
    ErrorTargetId
);
id_newtype!(
    /// Review card.
    CardId
);
id_newtype!(
    /// Adaptive recommendation.
    RecommendationId
);

/// BCP 47 language or locale tag, e.g. `ru`, `es-ES`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LanguageTag(pub String);

impl LanguageTag {
    /// Borrowed tag value.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
