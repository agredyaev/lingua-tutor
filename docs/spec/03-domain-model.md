# 03 — Domain Model

Scope: core domain types and rules in `learning-core`. Language-neutral. Column-level persistence: [04-database.md](04-database.md).
Depends on: [00-overview.md](00-overview.md) naming rules.

## 1. Conventions

- Public IDs: stable opaque strings, UUIDv7 or ULID. Newtypes per entity (`LessonId`, `CardId`, …).
- Timestamps: UTC. Local dates derived from stored timezone (default `Europe/Madrid`).
- All enums below are closed sets; extension requires a schema migration, not a stringly value.

## 2. Learner profile (global)

```rust
pub struct LearnerProfile {
    pub id: LearnerId,
    pub timezone: Tz,
    pub daily_session_target: u8,      // default 2
    pub session_minutes_target: u16,   // default 20
    pub max_new_cards: u8,             // default 8
    pub desired_retention: f64,        // default 0.90
    pub max_review_minutes: u16,       // default 12
}
```

## 3. Learner language profile (per language)

All progress, lessons, errors, learning items, cards, review queues scoped to one `LearnerLanguageProfile`.

```rust
pub struct LearnerLanguageProfile {
    pub id: LanguageProfileId,
    pub learner_id: LearnerId,
    pub language_pack_id: LanguagePackId,   // e.g. "es-ES"
    pub source_language: LanguageTag,       // BCP 47, e.g. "ru"
    pub target_locale: LanguageTag,         // BCP 47, e.g. "es-ES"
    pub estimated_level: Option<String>,    // CEFR label from pack framework
    pub current_lesson_number: u32,
    pub status: ProfileStatus,              // Active | Paused | Archived
    pub is_default: bool,
    pub preferences: serde_json::Value,     // validated at boundary
}
```

One active profile per (learner, pack) unless documented otherwise.

## 4. Lesson

```rust
pub enum LessonStatus { Planned, Active, Completed, Abandoned }
pub enum LessonSource { Pwa, Import, Mcp }

pub struct Lesson {
    pub id: LessonId,
    pub language_profile_id: LanguageProfileId,
    pub language_pack_id: LanguagePackId,
    pub language_pack_version: semver::Version,
    pub lesson_number: u32,                 // unique within profile
    pub status: LessonStatus,
    pub scenario: String,
    pub planned_goal: String,
    pub source: LessonSource,
    pub started_at: Option<Timestamp>,
    pub completed_at: Option<Timestamp>,
    pub duration_seconds: Option<u32>,
    pub tutor_summary: Option<String>,
    pub next_goal: Option<String>,
    pub idempotency_key: Option<String>,
}
```

Lesson metrics (per lesson): speaking seconds, recall counts, listening score, answer duration, pause counts, confidence.

## 5. Learning item

Primary unit = useful phrase / sentence frame / pattern. Isolated word only when: independently useful, repeatedly missing in phrases, high practical value, phrase card would add complexity.

```rust
pub enum LearningItemType { Phrase, Collocation, Pattern, Word, ResponseFrame }

pub enum LearningItemStatus {
    Candidate, Introduced, Learning, TransferPending,
    ProvisionallyMastered, Mastered, Suspended,
}

pub struct LearningItem {
    pub id: LearningItemId,
    pub language_profile_id: LanguageProfileId,
    pub language_pack_id: LanguagePackId,
    pub item_type: LearningItemType,
    pub target_text: String,
    pub normalized_target_text: String,     // via active pack
    pub prompt_text: Option<String>,
    pub prompt_language: Option<LanguageTag>,
    pub status: LearningItemStatus,
    pub language_metadata: serde_json::Value, // pack-owned schema, pack-versioned
    pub introduced_lesson_id: Option<LessonId>,
}
```

## 6. Evidence

Core evidence kinds fixed; pack may attach bounded metadata.

```rust
pub enum EvidenceKind {
    Recognition,          // understood in text
    Listening,            // understood in audio
    PromptedProduction,   // produced with hint/prompt
    SpontaneousProduction,// produced unaided in conversation
    Transfer,             // adapted to new context
    Correction,           // corrected form accepted
}

pub struct LearningItemEvidence {
    pub id: EvidenceId,
    pub learning_item_id: LearningItemId,
    pub lesson_id: Option<LessonId>,
    pub kind: EvidenceKind,
    pub success: bool,
    pub detail: Option<String>,
    pub pack_metadata: Option<serde_json::Value>,
    pub recorded_at: Timestamp,
}
```

### Mastery rules

- Card review success alone never yields mastery.
- Provisional mastery: unaided production (`SpontaneousProduction` success).
- Full mastery: correct use in ≥ 3 distinct contexts (or equivalent configured rule).
- Failed 1/3/7-day recheck returns target to active practice.

## 7. Errors

```rust
pub struct ErrorTarget {
    pub id: ErrorTargetId,
    pub language_profile_id: LanguageProfileId,
    pub language_pack_id: LanguagePackId,
    pub language_pack_version: semver::Version,
    pub core_category: CoreErrorCategory,     // core-owned closed set
    pub pack_error_code: Option<String>,      // pack taxonomy code
    pub original_production: String,
    pub corrected_production: String,
    pub explanation: Option<String>,
    pub occurrence_count: u32,
    pub method_change_required: bool,         // set at 5 unresolved occurrences
    pub status: ErrorTargetStatus,            // Active | Recheck | Resolved | Escalated
}
```

- `ErrorOccurrence`: one sighting; links lesson + evidence.
- `ErrorRecheck`: scheduled at +1, +3, +7 days; result pass/fail.
- Escalation: 3 occurrences → mini-target + contrast card when appropriate; 5 unresolved → method change required.

## 8. Recommendations

Deterministic rules ([07-tutor-contract.md](07-tutor-contract.md) §7) emit recommendations; they never silently mutate content.

```rust
pub enum RecommendationSubject {
    LearningItem(LearningItemId),
    ErrorTarget(ErrorTargetId),
    Card(CardId),
    ListeningItem(LearningItemId),
    ReviewLoad(LanguageProfileId),
    LanguageProfile(LanguageProfileId),
    PackUpgradeWarning(LanguagePackId),
}
```

## 9. Analytics: tracked indicators

Minimum tracked set: lessons completed; active speaking minutes; new phrases introduced; cards created/modified/split/merged/suspended/retired by tutor; reviews completed; due + overdue counts; review time; grade distribution; estimated retention; items recalled unaided; items transferred; listening score; response latency + long pauses; repeated errors by category; error recovery at 1/3/7 days; review backlog; weekly consistency.

Dashboard queries answerable at any time: next lesson; due cards today; unaided-producible items; recognition-only items; review-pass-but-conversation-fail items; recurring errors; pending tutor card actions; workload excess; listening trend; speaking-time trend; targets needing method change.
