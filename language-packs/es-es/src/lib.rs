//! Spanish (Spain) language pack (06-language-packs.md §7): normalization
//! and matching policy, error taxonomy, card templates, tutor instructions,
//! CEFR curriculum — code plus versioned declarative assets under `assets/`.
//! No Spanish logic exists outside this crate.

mod classify;
mod normalize;

use language_pack_api::{
    AnswerEvaluation, AnswerEvaluationRequest, AnswerVerdict, CardTemplate, CurriculumCatalog,
    LanguageErrorClassification, LanguageErrorClassificationRequest, LanguagePack,
    LanguagePackManifest, LearnerLanguageProfile, NormalizationMode, NormalizedText, Scenario,
    TutorInstructionBundle,
};
use serde::Deserialize;

const MANIFEST_JSON: &str = include_str!("../manifest.json");
const TEMPLATES_JSON: &str = include_str!("../assets/card-templates/templates.json");
const SCENARIOS_JSON: &str = include_str!("../assets/curriculum/scenarios.json");
const TAXONOMY_JSON: &str = include_str!("../assets/error-taxonomy/taxonomy.json");
const SEEDS_JSON: &str = include_str!("../assets/phrase-seeds/a1.json");
const INSTRUCTIONS_RU: &str = include_str!("../assets/tutor-instructions/instructions-ru.md");
const CORRECTIONS_RU: &str = include_str!("../assets/tutor-instructions/corrections-ru.md");

/// Builds the pack for registration in the composition root.
#[must_use]
pub fn register() -> Box<dyn LanguagePack> {
    Box::new(EsEsPack::new())
}

/// Asset schemas (7.7 validation; `deny_unknown_fields` keeps them strict).
mod assets {
    use super::Deserialize;

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct TemplatesFile {
        #[allow(dead_code)] // asset version pin, read by humans + tests
        pub version: u32,
        pub templates: Vec<TemplateEntry>,
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct TemplateEntry {
        pub template_id: String,
        pub intent: language_pack_api::CardIntent,
        pub prompt_template: String,
        pub answer_evaluation_mode: String,
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct ScenariosFile {
        #[allow(dead_code)] // asset version pin
        pub version: u32,
        pub scenarios: Vec<ScenarioEntry>,
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct ScenarioEntry {
        pub id: String,
        pub title: String,
        pub level: String,
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct TaxonomyFile {
        #[allow(dead_code)] // asset version pin
        pub version: u32,
        pub codes: Vec<TaxonomyEntry>,
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct TaxonomyEntry {
        pub code: String,
        #[allow(dead_code)] // consumed by conformance/asset tests
        pub core_category: language_pack_api::CoreErrorCategory,
        #[allow(dead_code)] // learner-facing label, surfaced in later UI work
        pub title_ru: String,
        pub guidance: String,
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct SeedsFile {
        #[allow(dead_code)] // asset version pin
        pub version: u32,
        pub seeds: Vec<SeedEntry>,
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct SeedEntry {
        pub target_text: String,
        pub prompt_ru: String,
        pub scenario: String,
    }
}

struct EsEsPack {
    manifest: LanguagePackManifest,
    templates: Vec<CardTemplate>,
    catalog: CurriculumCatalog,
    taxonomy: Vec<assets::TaxonomyEntry>,
}

impl EsEsPack {
    // Invariant: embedded assets are validated by this crate's tests and the
    // shared conformance suite; a parse failure is a build defect.
    #[allow(clippy::expect_used)]
    fn new() -> Self {
        let manifest: LanguagePackManifest =
            serde_json::from_str(MANIFEST_JSON).expect("manifest.json matches schema");
        let templates: assets::TemplatesFile =
            serde_json::from_str(TEMPLATES_JSON).expect("templates.json matches schema");
        let scenarios: assets::ScenariosFile =
            serde_json::from_str(SCENARIOS_JSON).expect("scenarios.json matches schema");
        let taxonomy: assets::TaxonomyFile =
            serde_json::from_str(TAXONOMY_JSON).expect("taxonomy.json matches schema");
        let seeds: assets::SeedsFile =
            serde_json::from_str(SEEDS_JSON).expect("a1.json matches schema");
        Self {
            manifest,
            templates: templates
                .templates
                .into_iter()
                .map(|t| CardTemplate {
                    template_id: t.template_id,
                    intent: t.intent,
                    prompt_template: t.prompt_template,
                    answer_evaluation_mode: t.answer_evaluation_mode,
                })
                .collect(),
            catalog: CurriculumCatalog {
                scenarios: scenarios
                    .scenarios
                    .into_iter()
                    .map(|s| Scenario {
                        id: s.id,
                        title: s.title,
                        level: s.level,
                    })
                    .collect(),
                seeds: seeds
                    .seeds
                    .into_iter()
                    .map(|seed| language_pack_api::PhraseSeed {
                        target_text: seed.target_text,
                        prompt: seed.prompt_ru,
                        scenario_id: seed.scenario,
                    })
                    .collect(),
            },
            taxonomy: taxonomy.codes,
        }
    }
}

impl LanguagePack for EsEsPack {
    fn manifest(&self) -> &LanguagePackManifest {
        &self.manifest
    }

    fn normalize_target_text(&self, input: &str, mode: NormalizationMode) -> NormalizedText {
        NormalizedText(match mode {
            NormalizationMode::Canonical => normalize::canonical(input),
            NormalizationMode::Comparison => normalize::comparison(input),
        })
    }

    fn evaluate_answer(&self, request: &AnswerEvaluationRequest) -> AnswerEvaluation {
        let given = normalize::comparison(&request.learner_answer);
        if given == normalize::comparison(&request.expected_answer) {
            return AnswerEvaluation {
                verdict: AnswerVerdict::Correct,
                matched_answer: Some(request.expected_answer.clone()),
                feedback: None,
            };
        }
        if let Some(matched) = request
            .accepted_answers
            .iter()
            .find(|a| normalize::comparison(a) == given)
        {
            return AnswerEvaluation {
                verdict: AnswerVerdict::AcceptableVariant,
                matched_answer: Some(matched.clone()),
                feedback: None,
            };
        }
        // Accent-only miss: still incorrect, but say why (tildes matter).
        let accent_only = normalize::strip_accents(&given)
            == normalize::strip_accents(&normalize::comparison(&request.expected_answer));
        AnswerEvaluation {
            verdict: AnswerVerdict::Incorrect,
            matched_answer: None,
            feedback: accent_only.then(|| "Casi: revisa las tildes.".to_owned()),
        }
    }

    fn classify_error(
        &self,
        request: &LanguageErrorClassificationRequest,
    ) -> LanguageErrorClassification {
        let mut result =
            classify::classify(&request.original_production, &request.corrected_production);
        // Attach learner-facing guidance from the taxonomy asset.
        if let Some(code) = &result.pack_error_code
            && let Some(entry) = self.taxonomy.iter().find(|e| &e.code == code)
        {
            result.explanation = Some(entry.guidance.clone());
        }
        result
    }

    fn card_templates(&self) -> &[CardTemplate] {
        &self.templates
    }

    fn tutor_instructions(&self, learner: &LearnerLanguageProfile) -> TutorInstructionBundle {
        let level = learner.estimated_level.as_deref().unwrap_or("не определён");
        TutorInstructionBundle {
            instructions: format!(
                "{INSTRUCTIONS_RU}\nУровень ученика: {level}. Язык объяснений: {}.",
                learner.source_language
            ),
            correction_conventions: CORRECTIONS_RU.to_owned(),
        }
    }

    fn curriculum_catalog(&self) -> &CurriculumCatalog {
        &self.catalog
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use language_pack_api::CardIntent;

    #[test]
    fn manifest_declares_full_capability_set() {
        let pack = register();
        assert_eq!(pack.manifest().capabilities.len(), 6);
        assert_eq!(pack.card_templates().len(), 4);
        assert!(pack.curriculum_catalog().scenarios.len() >= 12);
    }

    #[test]
    fn asset_schemas_validate_and_codes_are_unique() {
        let taxonomy: assets::TaxonomyFile = serde_json::from_str(TAXONOMY_JSON).unwrap();
        assert_eq!(taxonomy.version, 1);
        let mut codes: Vec<&str> = taxonomy.codes.iter().map(|c| c.code.as_str()).collect();
        let total = codes.len();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), total, "duplicate taxonomy code");

        let seeds: assets::SeedsFile = serde_json::from_str(SEEDS_JSON).unwrap();
        assert!(seeds.seeds.len() >= 20, "initial curriculum needs seeds");
        let scenarios: assets::ScenariosFile = serde_json::from_str(SCENARIOS_JSON).unwrap();
        let scenario_ids: Vec<&str> = scenarios.scenarios.iter().map(|s| s.id.as_str()).collect();
        for seed in &seeds.seeds {
            assert!(
                scenario_ids.contains(&seed.scenario.as_str()),
                "seed '{}' references unknown scenario '{}'",
                seed.target_text,
                seed.scenario
            );
        }
    }

    #[test]
    fn matching_accepts_regional_variant_and_flags_accent_miss() {
        let pack = register();
        let request = AnswerEvaluationRequest {
            expected_answer: "¿Me pone un café con leche, por favor?".to_owned(),
            accepted_answers: vec!["¿Me da un café con leche, por favor?".to_owned()],
            learner_answer: "me da un café con leche por favor".to_owned(),
            evaluation_mode: "exact".to_owned(),
        };
        assert_eq!(
            pack.evaluate_answer(&request).verdict,
            AnswerVerdict::AcceptableVariant
        );

        let accents = AnswerEvaluationRequest {
            expected_answer: "¿Qué tal?".to_owned(),
            accepted_answers: vec![],
            learner_answer: "que tal".to_owned(),
            evaluation_mode: "exact".to_owned(),
        };
        let evaluation = pack.evaluate_answer(&accents);
        assert_eq!(evaluation.verdict, AnswerVerdict::Incorrect);
        assert!(evaluation.feedback.unwrap().contains("tildes"));
    }

    #[test]
    fn classification_carries_taxonomy_guidance() {
        let pack = register();
        let result = pack.classify_error(&LanguageErrorClassificationRequest {
            original_production: "estoy médico".to_owned(),
            corrected_production: "soy médico".to_owned(),
            context: None,
        });
        assert_eq!(result.pack_error_code.as_deref(), Some("es.verb.ser_estar"));
        assert!(result.explanation.unwrap().contains("ser"));
    }

    #[test]
    fn tutor_instructions_are_russian_and_level_aware() {
        let pack = register();
        let bundle = pack.tutor_instructions(&LearnerLanguageProfile {
            source_language: "ru".to_owned(),
            target_locale: "es-ES".to_owned(),
            estimated_level: Some("A2".to_owned()),
        });
        assert!(bundle.instructions.contains("репетитор"));
        assert!(bundle.instructions.contains("A2"));
        assert!(bundle.correction_conventions.contains("error_target_id"));
    }

    #[test]
    fn contrast_template_exists_for_error_cards() {
        let pack = register();
        assert!(
            pack.card_templates()
                .iter()
                .any(|t| t.intent == CardIntent::Contrast)
        );
    }
}
