//! `LanguagePack` trait and every boundary type of the plugin contract
//! (06-language-packs.md §2). Core owns all request/response types; packs
//! never expose implementation types across this boundary. Packs must not
//! touch persistence, network, secrets, scheduling state, or review events.

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

/// Output of [`LanguagePack::normalize_target_text`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NormalizedText(pub String);

/// Input to [`LanguagePack::evaluate_answer`].
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

/// Output of [`LanguagePack::evaluate_answer`]. Evaluation must be bounded:
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

/// Input to [`LanguagePack::classify_error`].
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

/// Output of [`LanguagePack::classify_error`].
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

/// One pack-provided card template (06-language-packs.md §7).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CardTemplate {
    /// Stable template ID, unique within the pack.
    pub template_id: String,
    /// Intent this template produces.
    pub intent: CardIntent,
    /// Prompt template text; placeholders are pack-defined.
    pub prompt_template: String,
    /// Evaluation mode identifier written to `cards.answer_evaluation_mode`.
    pub answer_evaluation_mode: String,
}

/// Learner profile view the pack receives. Boundary type owned here —
/// distinct from the `learning-core` domain type; `application` maps between
/// them (02-workspace.md §3 dependency direction).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LearnerLanguageProfile {
    /// Source (instruction) language, BCP 47.
    pub source_language: String,
    /// Target locale, BCP 47.
    pub target_locale: String,
    /// Estimated level in the pack's framework, e.g. `A2`.
    pub estimated_level: Option<String>,
}

/// Output of [`LanguagePack::tutor_instructions`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TutorInstructionBundle {
    /// Prompt fragments injected into tutor context.
    pub instructions: String,
    /// How the tutor should present corrections.
    pub correction_conventions: String,
}

/// One communicative scenario in the pack curriculum.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Scenario {
    /// Stable scenario ID.
    pub id: String,
    /// Learner-facing title.
    pub title: String,
    /// Level code in the pack's framework.
    pub level: String,
}

/// One seed phrase for the initial curriculum (06-language-packs.md §7).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PhraseSeed {
    /// Target-language phrase.
    pub target_text: String,
    /// Learner-facing prompt in the source language.
    pub prompt: String,
    /// Scenario this seed belongs to.
    pub scenario_id: String,
}

/// Pack curriculum (06-language-packs.md §7).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CurriculumCatalog {
    /// Scenarios in recommended order.
    pub scenarios: Vec<Scenario>,
    /// Seed phrases feeding early lessons via the tutor context.
    #[serde(default)]
    pub seeds: Vec<PhraseSeed>,
}

/// The plugin contract (06-language-packs.md §2). All target-language
/// behavior enters the system through this trait.
pub trait LanguagePack: Send + Sync {
    /// Pack self-description; `id` and `contract_version` drive registration.
    fn manifest(&self) -> &LanguagePackManifest;

    /// Deterministic normalization of target-language text.
    fn normalize_target_text(&self, input: &str, mode: NormalizationMode) -> NormalizedText;

    /// Bounded matching of a learner answer against expected/accepted answers.
    fn evaluate_answer(&self, request: &AnswerEvaluationRequest) -> AnswerEvaluation;

    /// Maps a correction to the core error category set with a stable pack code.
    fn classify_error(
        &self,
        request: &LanguageErrorClassificationRequest,
    ) -> LanguageErrorClassification;

    /// Card templates this pack provides; must cover declared card capabilities.
    fn card_templates(&self) -> &[CardTemplate];

    /// Tutor prompt fragments for the given learner profile.
    fn tutor_instructions(&self, learner: &LearnerLanguageProfile) -> TutorInstructionBundle;

    /// Scenario/level catalog.
    fn curriculum_catalog(&self) -> &CurriculumCatalog;
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
