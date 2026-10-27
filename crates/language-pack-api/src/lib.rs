//! Data-only boundary types shared by language-pack processing passes.
//! Core owns all request/response types. Pack processing code receives only
//! explicit inputs and must not touch persistence, network, secrets,
//! scheduling state, or review events.

use serde::{Deserialize, Deserializer, Serialize};

/// App-owned integer contract version. A pack whose manifest declares a
/// different value must fail registration at startup (06-language-packs.md §4).
pub const CONTRACT_VERSION: u32 = 1;

/// Pack self-description, loaded from `manifest.json` (06-language-packs.md §3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LanguagePackManifest {
    /// Stable pack ID, e.g. `es-ES`. Never changes across versions.
    pub id: String,
    /// Crate package name, e.g. `language-pack-es-es`.
    pub package: String,
    /// Pack semver.
    pub version: String,
    /// Must equal [`CONTRACT_VERSION`]; JSON accepts `"1"` or `1`.
    #[serde(deserialize_with = "u32_from_string_or_number")]
    pub contract_version: u32,
    /// BCP 47 language, e.g. `es`.
    pub target_language: String,
    /// BCP 47 locale, e.g. `es-ES`.
    pub target_locale: String,
    /// Human-readable name, e.g. `Spanish (Spain)`.
    pub display_name: String,
    /// Source languages the pack can instruct from.
    pub supported_source_languages: Vec<String>,
    /// Must be one of `supported_source_languages`.
    pub default_source_language: String,
    /// Level framework identifier, e.g. `CEFR`.
    pub level_framework: String,
    /// Declared capabilities; conformance checks behavior against them.
    pub capabilities: Vec<Capability>,
}

/// Closed capability set a pack may declare.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    /// Provides production card templates.
    ProductionCards,
    /// Provides listening card templates.
    ListeningCards,
    /// Provides contrast card templates.
    ContrastCards,
    /// Provides transfer card templates.
    TransferCards,
    /// Maps language errors to core categories.
    ErrorClassification,
    /// Matches learner answers against accepted answers.
    AnswerMatching,
}

/// Normalization purpose. Same input + same mode must always yield the same
/// output (determinism requirement, 06-language-packs.md §2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NormalizationMode {
    /// Canonical form stored in `learning_items.normalized_target_text`.
    Canonical,
    /// Looser form used when comparing learner answers.
    Comparison,
}

/// Output of a target-text normalization pass.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NormalizedText(pub String);

/// Input to an answer-evaluation pass.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnswerEvaluationRequest {
    /// Card's primary answer.
    pub expected_answer: String,
    /// Additional accepted answers (`cards.accepted_answers_json`).
    pub accepted_answers: Vec<String>,
    /// What the learner produced.
    pub learner_answer: String,
    /// Pack-defined mode identifier (`cards.answer_evaluation_mode`).
    pub evaluation_mode: String,
}

/// Verdict on a learner answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnswerVerdict {
    /// Matches the expected answer.
    Correct,
    /// Matches an accepted alternative.
    AcceptableVariant,
    /// No match.
    Incorrect,
}

/// Output of an answer-evaluation pass. Evaluation must be bounded:
/// cost proportional to input size, no unbounded computation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnswerEvaluation {
    /// Match verdict.
    pub verdict: AnswerVerdict,
    /// Which accepted answer matched, when `verdict` is not `Incorrect`.
    pub matched_answer: Option<String>,
    /// Optional pack feedback shown after reveal.
    pub feedback: Option<String>,
}

/// Input to an error-classification pass.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LanguageErrorClassificationRequest {
    /// What the learner said/wrote.
    pub original_production: String,
    /// Corrected form.
    pub corrected_production: String,
    /// Optional lesson context.
    pub context: Option<String>,
}

/// Core-owned closed error category set. Packs map their taxonomy into it;
/// codes must stay stable across pack patch versions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoreErrorCategory {
    /// Morphology, agreement, conjugation, syntax rules.
    Grammar,
    /// Wrong or unnatural word/phrase choice.
    Vocabulary,
    /// Constituent order.
    WordOrder,
    /// Formality/politeness mismatch.
    Register,
    /// Spoken-form errors captured as text notes.
    Pronunciation,
    /// Spelling, diacritics, punctuation.
    Orthography,
    /// Anything the pack cannot place more precisely.
    Other,
}

/// Output of an error-classification pass.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LanguageErrorClassification {
    /// Core category (`error_targets.core_category`).
    pub core_category: CoreErrorCategory,
    /// Stable pack taxonomy code (`error_targets.pack_error_code`).
    pub pack_error_code: Option<String>,
    /// Learner-facing explanation.
    pub explanation: Option<String>,
}

/// Card intent, mirrors `cards.intent` (04-database.md).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CardIntent {
    /// Produce the target form from a source-language prompt.
    Production,
    /// Understand the spoken/heard form.
    Listening,
    /// Distinguish two confusable forms.
    Contrast,
    /// Apply a learned pattern in a new context.
    Transfer,
}

/// Borrowed card-template view over pack-owned column storage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct CardTemplateView<'a> {
    /// Stable template ID, unique within the pack.
    pub template_id: &'a str,
    /// Intent this template produces.
    pub intent: CardIntent,
    /// Prompt template text.
    pub prompt_template: &'a str,
    /// Evaluation mode identifier.
    pub answer_evaluation_mode: &'a str,
}

/// Output of a tutor-instructions pass.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TutorInstructionBundle {
    /// Prompt fragments injected into tutor context.
    pub instructions: String,
    /// How the tutor should present corrections.
    pub correction_conventions: String,
}

/// Borrowed communicative-scenario view over pack-owned column storage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ScenarioView<'a> {
    /// Stable scenario ID.
    pub id: &'a str,
    /// Learner-facing title.
    pub title: &'a str,
    /// Level code in the pack framework.
    pub level: &'a str,
}

/// Borrowed seed-phrase view over pack-owned column storage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct PhraseSeedView<'a> {
    /// Target-language phrase.
    pub target_text: &'a str,
    /// Learner-facing prompt.
    pub prompt: &'a str,
    /// Dense scenario relation resolved back to its boundary ID.
    pub scenario_id: &'a str,
}

fn u32_from_string_or_number<'de, D: Deserializer<'de>>(de: D) -> Result<u32, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Raw {
        Number(u32),
        Text(String),
    }
    match Raw::deserialize(de)? {
        Raw::Number(n) => Ok(n),
        Raw::Text(s) => s.parse().map_err(serde::de::Error::custom),
    }
}
