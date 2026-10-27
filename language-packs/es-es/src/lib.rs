//! Spanish (Spain) language pack (06-language-packs.md §7): normalization
//! and matching policy, error taxonomy, card templates, tutor instructions,
//! CEFR curriculum — code plus versioned declarative assets under `assets/`.
//! No Spanish logic exists outside this crate.

mod classify;
mod normalize;

use language_pack_api::{
    AnswerEvaluation, AnswerEvaluationRequest, AnswerVerdict, CardTemplateView,
    LanguageErrorClassification, LanguageErrorClassificationRequest, LanguagePackManifest,
    NormalizationMode, NormalizedText, PhraseSeedView, ScenarioView, TutorInstructionBundle,
};
use serde::Deserialize;

const MANIFEST_JSON: &str = include_str!("../manifest.json");
const TEMPLATES_JSON: &str = include_str!("../assets/card-templates/templates.json");
const SCENARIOS_JSON: &str = include_str!("../assets/curriculum/scenarios.json");
const CONTEXT_FAMILIES_JSON: &str = include_str!("../assets/curriculum/context-families.json");
const TAXONOMY_JSON: &str = include_str!("../assets/error-taxonomy/taxonomy.json");
const SEEDS_JSON: &str = include_str!("../assets/phrase-seeds/a1.json");
const INSTRUCTIONS_RU: &str = include_str!("../assets/tutor-instructions/instructions-ru.md");
const CORRECTIONS_RU: &str = include_str!("../assets/tutor-instructions/corrections-ru.md");

/// Builds the pack for registration in the composition root.
#[must_use]
pub fn register() -> (LanguagePackManifest, EsEsPack) {
    build_pack()
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
    pub struct ContextFamiliesFile {
        #[allow(dead_code)]
        pub version: u32,
        pub families: Vec<ContextFamilyEntry>,
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct ContextFamilyEntry {
        pub id: String,
        pub title_ru: String,
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

/// Built-in Spanish pack retained as parallel immutable asset columns.
pub struct EsEsPack {
    template_id: Vec<String>,
    template_intent: Vec<language_pack_api::CardIntent>,
    template_prompt: Vec<String>,
    template_evaluation_mode: Vec<String>,
    scenario_id: Vec<String>,
    scenario_title: Vec<String>,
    scenario_level: Vec<String>,
    context_family_id: Vec<String>,
    context_family_title_ru: Vec<String>,
    seed_target_text: Vec<String>,
    seed_prompt: Vec<String>,
    seed_scenario_key: Vec<u32>,
    taxonomy_code: Vec<String>,
    taxonomy_guidance: Vec<String>,
}

#[allow(clippy::expect_used)]
fn build_pack() -> (LanguagePackManifest, EsEsPack) {
    let manifest: LanguagePackManifest =
        serde_json::from_str(MANIFEST_JSON).expect("manifest.json matches schema");
    let templates: assets::TemplatesFile =
        serde_json::from_str(TEMPLATES_JSON).expect("templates.json matches schema");
    let scenarios: assets::ScenariosFile =
        serde_json::from_str(SCENARIOS_JSON).expect("scenarios.json matches schema");
    let context_families: assets::ContextFamiliesFile =
        serde_json::from_str(CONTEXT_FAMILIES_JSON).expect("context-families.json matches schema");
    let taxonomy: assets::TaxonomyFile =
        serde_json::from_str(TAXONOMY_JSON).expect("taxonomy.json matches schema");
    let seeds: assets::SeedsFile =
        serde_json::from_str(SEEDS_JSON).expect("a1.json matches schema");

    let mut pack = EsEsPack {
        template_id: Vec::with_capacity(templates.templates.len()),
        template_intent: Vec::with_capacity(templates.templates.len()),
        template_prompt: Vec::with_capacity(templates.templates.len()),
        template_evaluation_mode: Vec::with_capacity(templates.templates.len()),
        scenario_id: Vec::with_capacity(scenarios.scenarios.len()),
        scenario_title: Vec::with_capacity(scenarios.scenarios.len()),
        scenario_level: Vec::with_capacity(scenarios.scenarios.len()),
        context_family_id: Vec::with_capacity(context_families.families.len()),
        context_family_title_ru: Vec::with_capacity(context_families.families.len()),
        seed_target_text: Vec::with_capacity(seeds.seeds.len()),
        seed_prompt: Vec::with_capacity(seeds.seeds.len()),
        seed_scenario_key: Vec::with_capacity(seeds.seeds.len()),
        taxonomy_code: Vec::with_capacity(taxonomy.codes.len()),
        taxonomy_guidance: Vec::with_capacity(taxonomy.codes.len()),
    };

    for template in templates.templates {
        pack.template_id.push(template.template_id);
        pack.template_intent.push(template.intent);
        pack.template_prompt.push(template.prompt_template);
        pack.template_evaluation_mode
            .push(template.answer_evaluation_mode);
    }
    for scenario in scenarios.scenarios {
        pack.scenario_id.push(scenario.id);
        pack.scenario_title.push(scenario.title);
        pack.scenario_level.push(scenario.level);
    }
    for family in context_families.families {
        pack.context_family_id.push(family.id);
        pack.context_family_title_ru.push(family.title_ru);
    }
    for seed in seeds.seeds {
        let scenario_key = pack
            .scenario_id
            .iter()
            .position(|id| id == &seed.scenario)
            .and_then(|index| u32::try_from(index).ok())
            .expect("seed scenario validated by pack assets");
        pack.seed_target_text.push(seed.target_text);
        pack.seed_prompt.push(seed.prompt_ru);
        pack.seed_scenario_key.push(scenario_key);
    }
    for entry in taxonomy.codes {
        pack.taxonomy_code.push(entry.code);
        pack.taxonomy_guidance.push(entry.guidance);
    }

    (manifest, pack)
}

/// Execute Spanish normalize target text as an explicit processing pass.
pub fn normalize_target_text(input: &str, mode: NormalizationMode) -> NormalizedText {
    NormalizedText(match mode {
        NormalizationMode::Canonical => normalize::canonical(input),
        NormalizationMode::Comparison => normalize::comparison(input),
    })
}

/// Execute Spanish evaluate answer as an explicit processing pass.
pub fn evaluate_answer(request: &AnswerEvaluationRequest) -> AnswerEvaluation {
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

/// Execute Spanish classify error as an explicit processing pass.
pub fn classify_error(
    pack: &EsEsPack,
    request: &LanguageErrorClassificationRequest,
) -> LanguageErrorClassification {
    let mut result =
        classify::classify(&request.original_production, &request.corrected_production);
    // Attach learner-facing guidance from the taxonomy asset.
    if let Some(code) = &result.pack_error_code
        && let Some(index) = pack
            .taxonomy_code
            .iter()
            .position(|candidate| candidate == code)
    {
        result.explanation = pack.taxonomy_guidance.get(index).cloned();
    }
    result
}

/// Number of card-template rows.
#[must_use]
pub fn card_template_count(pack: &EsEsPack) -> usize {
    pack.template_id.len()
}

/// Borrow one card-template row by dense index.
#[must_use]
pub fn card_template_at(pack: &EsEsPack, index: u32) -> Option<CardTemplateView<'_>> {
    let index = usize::try_from(index).ok()?;
    Some(CardTemplateView {
        template_id: pack.template_id.get(index)?,
        intent: *pack.template_intent.get(index)?,
        prompt_template: pack.template_prompt.get(index)?,
        answer_evaluation_mode: pack.template_evaluation_mode.get(index)?,
    })
}

/// Build Spanish tutor instructions from scalar profile fields.
pub fn tutor_instructions(
    source_language: &str,
    estimated_level: Option<&str>,
) -> TutorInstructionBundle {
    let level = estimated_level.unwrap_or("не определён");
    TutorInstructionBundle {
        instructions: format!(
            "{INSTRUCTIONS_RU}\nУровень ученика: {level}. Язык объяснений: {source_language}."
        ),
        correction_conventions: CORRECTIONS_RU.to_owned(),
    }
}

/// Number of curriculum scenario rows.
#[must_use]
pub fn scenario_count(pack: &EsEsPack) -> usize {
    pack.scenario_id.len()
}

/// Borrow one scenario by dense index.
#[must_use]
pub fn scenario_at(pack: &EsEsPack, index: u32) -> Option<ScenarioView<'_>> {
    let index = usize::try_from(index).ok()?;
    Some(ScenarioView {
        id: pack.scenario_id.get(index)?,
        title: pack.scenario_title.get(index)?,
        level: pack.scenario_level.get(index)?,
    })
}

/// Number of canonical semantic transfer families.
#[must_use]
pub fn context_family_count(pack: &EsEsPack) -> usize {
    pack.context_family_id.len()
}

/// Canonical semantic transfer family ID by dense index.
#[must_use]
pub fn context_family_at(pack: &EsEsPack, index: u32) -> Option<&str> {
    usize::try_from(index)
        .ok()
        .and_then(|index| pack.context_family_id.get(index))
        .map(String::as_str)
}

/// Whether an ID is a canonical semantic transfer family in this pack.
#[must_use]
pub fn is_context_family(pack: &EsEsPack, id: &str) -> bool {
    pack.context_family_id
        .iter()
        .any(|candidate| candidate == id)
}

/// Number of seed phrase rows.
#[must_use]
pub fn seed_count(pack: &EsEsPack) -> usize {
    pack.seed_target_text.len()
}

/// Borrow one seed phrase by dense index.
#[must_use]
pub fn seed_at(pack: &EsEsPack, index: u32) -> Option<PhraseSeedView<'_>> {
    let index = usize::try_from(index).ok()?;
    let scenario_key = usize::try_from(*pack.seed_scenario_key.get(index)?).ok()?;
    Some(PhraseSeedView {
        target_text: pack.seed_target_text.get(index)?,
        prompt: pack.seed_prompt.get(index)?,
        scenario_id: pack.scenario_id.get(scenario_key)?,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use language_pack_api::CardIntent;

    #[test]
    fn manifest_declares_full_capability_set() {
        let (manifest, pack) = register();
        assert_eq!(manifest.capabilities.len(), 6);
        assert_eq!(card_template_count(&pack), 4);
        assert!(scenario_count(&pack) >= 12);
        assert!(context_family_count(&pack) >= 12);
        assert!(is_context_family(&pack, "service.request_bill"));
        assert!(!is_context_family(&pack, "cafe.request_bill"));
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
        let request = AnswerEvaluationRequest {
            expected_answer: "¿Me pone un café con leche, por favor?".to_owned(),
            accepted_answers: vec!["¿Me da un café con leche, por favor?".to_owned()],
            learner_answer: "me da un café con leche por favor".to_owned(),
            evaluation_mode: "exact".to_owned(),
        };
        assert_eq!(
            evaluate_answer(&request).verdict,
            AnswerVerdict::AcceptableVariant
        );

        let accents = AnswerEvaluationRequest {
            expected_answer: "¿Qué tal?".to_owned(),
            accepted_answers: vec![],
            learner_answer: "que tal".to_owned(),
            evaluation_mode: "exact".to_owned(),
        };
        let evaluation = evaluate_answer(&accents);
        assert_eq!(evaluation.verdict, AnswerVerdict::Incorrect);
        assert!(evaluation.feedback.unwrap().contains("tildes"));
    }

    #[test]
    fn classification_carries_taxonomy_guidance() {
        let (_, pack) = register();
        let result = classify_error(
            &pack,
            &LanguageErrorClassificationRequest {
                original_production: "estoy médico".to_owned(),
                corrected_production: "soy médico".to_owned(),
                context: None,
            },
        );
        assert_eq!(result.pack_error_code.as_deref(), Some("es.verb.ser_estar"));
        assert!(result.explanation.unwrap().contains("ser"));
    }

    #[test]
    fn tutor_instructions_are_russian_and_level_aware() {
        let bundle = tutor_instructions("ru", Some("A2"));
        assert!(bundle.instructions.contains("репетитор"));
        assert!(bundle.instructions.contains("A2"));
        assert!(bundle.correction_conventions.contains("error_target_id"));
    }

    #[test]
    fn contrast_template_exists_for_error_cards() {
        let (_, pack) = register();
        assert!((0..card_template_count(&pack)).any(|index| {
            let index = u32::try_from(index).unwrap();
            card_template_at(&pack, index)
                .map(|template| template.intent == CardIntent::Contrast)
                .unwrap_or(false)
        }));
    }
}
