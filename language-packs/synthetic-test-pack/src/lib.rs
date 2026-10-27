//! Minimal pack with artificial language rules used only by tests.
//! Behavior is expressed as explicit processing passes over flat pack-owned columns.

use language_pack_api::{
    AnswerEvaluation, AnswerEvaluationRequest, AnswerVerdict, Capability, CardIntent,
    CardTemplateView, CoreErrorCategory, LanguageErrorClassification,
    LanguageErrorClassificationRequest, LanguagePackManifest, NormalizationMode, NormalizedText,
    PhraseSeedView, ScenarioView, TutorInstructionBundle,
};

/// Artificial language-pack data used to verify composition and contracts.
pub struct SyntheticTestPack {
    template_id: Vec<String>,
    template_intent: Vec<CardIntent>,
    template_prompt: Vec<String>,
    template_evaluation_mode: Vec<String>,
    scenario_id: Vec<String>,
    scenario_title: Vec<String>,
    scenario_level: Vec<String>,
    seed_target_text: Vec<String>,
    seed_prompt: Vec<String>,
    seed_scenario_key: Vec<u32>,
}

/// Build manifest + pack data for test registration.
#[must_use]
pub fn register() -> (LanguagePackManifest, SyntheticTestPack) {
    (
        LanguagePackManifest {
            id: "zz-ZZ".to_owned(),
            package: "synthetic-test-pack".to_owned(),
            version: "0.1.0".to_owned(),
            contract_version: language_pack_api::CONTRACT_VERSION,
            target_language: "zz".to_owned(),
            target_locale: "zz-ZZ".to_owned(),
            display_name: "Synthetic Test Language".to_owned(),
            supported_source_languages: vec!["ru".to_owned(), "en".to_owned()],
            default_source_language: "ru".to_owned(),
            level_framework: "CEFR".to_owned(),
            capabilities: vec![
                Capability::ProductionCards,
                Capability::ListeningCards,
                Capability::ContrastCards,
                Capability::TransferCards,
                Capability::ErrorClassification,
                Capability::AnswerMatching,
            ],
        },
        SyntheticTestPack {
            template_id: vec![
                "syn-production".to_owned(),
                "syn-listening".to_owned(),
                "syn-contrast".to_owned(),
                "syn-transfer".to_owned(),
            ],
            template_intent: vec![
                CardIntent::Production,
                CardIntent::Listening,
                CardIntent::Contrast,
                CardIntent::Transfer,
            ],
            template_prompt: vec![
                "syn-production: {prompt}".to_owned(),
                "syn-listening: {prompt}".to_owned(),
                "syn-contrast: {prompt}".to_owned(),
                "syn-transfer: {prompt}".to_owned(),
            ],
            template_evaluation_mode: vec![
                "exact".to_owned(),
                "exact".to_owned(),
                "exact".to_owned(),
                "exact".to_owned(),
            ],
            scenario_id: vec!["syn-greetings".to_owned(), "syn-market".to_owned()],
            scenario_title: vec![
                "Synthetic greetings".to_owned(),
                "Synthetic market".to_owned(),
            ],
            scenario_level: vec!["A1".to_owned(), "A2".to_owned()],
            seed_target_text: vec!["zug bla".to_owned()],
            seed_prompt: vec!["say the greeting".to_owned()],
            seed_scenario_key: vec![0],
        },
    )
}

fn canonical(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn comparison(input: &str) -> String {
    canonical(input)
        .to_lowercase()
        .trim_end_matches(['!', '?', '.'])
        .to_owned()
}

/// Normalize synthetic target text deterministically for the requested comparison mode.
#[must_use]
pub fn normalize_target_text(input: &str, mode: NormalizationMode) -> NormalizedText {
    NormalizedText(match mode {
        NormalizationMode::Canonical => canonical(input),
        NormalizationMode::Comparison => comparison(input),
    })
}

/// Evaluate a synthetic learner answer against expected and accepted answers.
#[must_use]
pub fn evaluate_answer(request: &AnswerEvaluationRequest) -> AnswerEvaluation {
    let given = comparison(&request.learner_answer);
    if given == comparison(&request.expected_answer) {
        return AnswerEvaluation {
            verdict: AnswerVerdict::Correct,
            matched_answer: Some(request.expected_answer.clone()),
            feedback: None,
        };
    }
    let variant = request
        .accepted_answers
        .iter()
        .find(|answer| comparison(answer) == given);
    match variant {
        Some(matched) => AnswerEvaluation {
            verdict: AnswerVerdict::AcceptableVariant,
            matched_answer: Some(matched.clone()),
            feedback: None,
        },
        None => AnswerEvaluation {
            verdict: AnswerVerdict::Incorrect,
            matched_answer: None,
            feedback: None,
        },
    }
}

/// Classify a synthetic correction into the shared core error taxonomy.
#[must_use]
pub fn classify_error(request: &LanguageErrorClassificationRequest) -> LanguageErrorClassification {
    let original = comparison(&request.original_production);
    let corrected = comparison(&request.corrected_production);
    let (category, code) = if original == corrected {
        (CoreErrorCategory::Orthography, "SYN-ORTH")
    } else if original.split(' ').count() != corrected.split(' ').count() {
        (CoreErrorCategory::Grammar, "SYN-GRAM")
    } else {
        (CoreErrorCategory::Vocabulary, "SYN-VOC")
    };
    LanguageErrorClassification {
        core_category: category,
        pack_error_code: Some(code.to_owned()),
        explanation: None,
    }
}

/// Number of card-template rows.
#[must_use]
pub fn card_template_count(pack: &SyntheticTestPack) -> usize {
    pack.template_id.len()
}

/// Borrow one card-template row by dense index.
#[must_use]
pub fn card_template_at(pack: &SyntheticTestPack, index: u32) -> Option<CardTemplateView<'_>> {
    let index = usize::try_from(index).ok()?;
    Some(CardTemplateView {
        template_id: pack.template_id.get(index)?,
        intent: *pack.template_intent.get(index)?,
        prompt_template: pack.template_prompt.get(index)?,
        answer_evaluation_mode: pack.template_evaluation_mode.get(index)?,
    })
}

/// Build deterministic tutor instructions from scalar learner profile fields.
#[must_use]
pub fn tutor_instructions(
    source_language: &str,
    estimated_level: Option<&str>,
) -> TutorInstructionBundle {
    let level = estimated_level.unwrap_or("unknown");
    TutorInstructionBundle {
        instructions: format!(
            "Synthetic tutor rules; instruct from {source_language}; level {level}."
        ),
        correction_conventions: "Correct inline, restate once.".to_owned(),
    }
}

/// Number of scenario rows.
#[must_use]
pub fn scenario_count(pack: &SyntheticTestPack) -> usize {
    pack.scenario_id.len()
}

/// Borrow one scenario row by dense index.
#[must_use]
pub fn scenario_at(pack: &SyntheticTestPack, index: u32) -> Option<ScenarioView<'_>> {
    let index = usize::try_from(index).ok()?;
    Some(ScenarioView {
        id: pack.scenario_id.get(index)?,
        title: pack.scenario_title.get(index)?,
        level: pack.scenario_level.get(index)?,
    })
}

/// Number of seed rows.
#[must_use]
pub fn seed_count(pack: &SyntheticTestPack) -> usize {
    pack.seed_target_text.len()
}

/// Borrow one seed row by dense index.
#[must_use]
pub fn seed_at(pack: &SyntheticTestPack, index: u32) -> Option<PhraseSeedView<'_>> {
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

    #[test]
    fn canonical_collapses_whitespace_and_trims() {
        let out = normalize_target_text("  zug   BLA  ", NormalizationMode::Canonical);
        assert_eq!(out.0, "zug BLA");
    }

    #[test]
    fn comparison_lowercases_and_strips_trailing_punctuation() {
        let out = normalize_target_text("Zug bla!?", NormalizationMode::Comparison);
        assert_eq!(out.0, "zug bla");
    }

    #[test]
    fn evaluate_correct_variant_and_incorrect() {
        let request = |answer: &str| AnswerEvaluationRequest {
            expected_answer: "zug bla".to_owned(),
            accepted_answers: vec!["blip zug".to_owned()],
            learner_answer: answer.to_owned(),
            evaluation_mode: "exact".to_owned(),
        };
        assert_eq!(
            evaluate_answer(&request("ZUG bla!")).verdict,
            AnswerVerdict::Correct
        );
        assert_eq!(
            evaluate_answer(&request("Blip zug")).verdict,
            AnswerVerdict::AcceptableVariant
        );
        assert_eq!(
            evaluate_answer(&request("nope")).verdict,
            AnswerVerdict::Incorrect
        );
    }

    #[test]
    fn classification_is_deterministic_and_coded() {
        let request = LanguageErrorClassificationRequest {
            original_production: "zug".to_owned(),
            corrected_production: "zug bla".to_owned(),
            context: None,
        };
        let first = classify_error(&request);
        let second = classify_error(&request);
        assert_eq!(first, second);
        assert_eq!(first.pack_error_code.as_deref(), Some("SYN-GRAM"));
        assert_eq!(first.core_category, CoreErrorCategory::Grammar);
    }

    #[test]
    fn columns_expose_expected_views() {
        let (_, pack) = register();
        assert_eq!(card_template_count(&pack), 4);
        assert_eq!(scenario_count(&pack), 2);
        assert_eq!(seed_count(&pack), 1);
        assert_eq!(
            card_template_at(&pack, 2).unwrap().intent,
            CardIntent::Contrast
        );
        assert_eq!(seed_at(&pack, 0).unwrap().scenario_id, "syn-greetings");
    }
}
