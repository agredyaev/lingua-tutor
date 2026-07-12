//! Minimal pack with artificial language rules, implementing the full
//! contract for core CI (06-language-packs.md §6). Never registered in
//! production builds; carries no real-language content.

use language_pack_api::{
    AnswerEvaluation, AnswerEvaluationRequest, AnswerVerdict, Capability, CardIntent, CardTemplate,
    CoreErrorCategory, CurriculumCatalog, LanguageErrorClassification,
    LanguageErrorClassificationRequest, LanguagePack, LanguagePackManifest, LearnerLanguageProfile,
    NormalizationMode, NormalizedText, Scenario, TutorInstructionBundle,
};

/// Builds the pack for registration in test compositions.
#[must_use]
pub fn register() -> Box<dyn LanguagePack> {
    Box::new(SyntheticTestPack::new())
}

/// Artificial rules: canonical = trim + collapse whitespace; comparison
/// additionally lowercases and strips trailing `! ? .`.
struct SyntheticTestPack {
    manifest: LanguagePackManifest,
    templates: Vec<CardTemplate>,
    catalog: CurriculumCatalog,
}

impl SyntheticTestPack {
    fn new() -> Self {
        Self {
            manifest: LanguagePackManifest {
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
            templates: [
                (CardIntent::Production, "syn-production"),
                (CardIntent::Listening, "syn-listening"),
                (CardIntent::Contrast, "syn-contrast"),
                (CardIntent::Transfer, "syn-transfer"),
            ]
            .into_iter()
            .map(|(intent, id)| CardTemplate {
                template_id: id.to_owned(),
                intent,
                prompt_template: format!("{id}: {{prompt}}"),
                answer_evaluation_mode: "exact".to_owned(),
            })
            .collect(),
            catalog: CurriculumCatalog {
                scenarios: vec![
                    Scenario {
                        id: "syn-greetings".to_owned(),
                        title: "Synthetic greetings".to_owned(),
                        level: "A1".to_owned(),
                    },
                    Scenario {
                        id: "syn-market".to_owned(),
                        title: "Synthetic market".to_owned(),
                        level: "A2".to_owned(),
                    },
                ],
            },
        }
    }

    fn canonical(input: &str) -> String {
        input.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    fn comparison(input: &str) -> String {
        Self::canonical(input)
            .to_lowercase()
            .trim_end_matches(['!', '?', '.'])
            .to_owned()
    }
}

impl LanguagePack for SyntheticTestPack {
    fn manifest(&self) -> &LanguagePackManifest {
        &self.manifest
    }

    fn normalize_target_text(&self, input: &str, mode: NormalizationMode) -> NormalizedText {
        NormalizedText(match mode {
            NormalizationMode::Canonical => Self::canonical(input),
            NormalizationMode::Comparison => Self::comparison(input),
        })
    }

    fn evaluate_answer(&self, request: &AnswerEvaluationRequest) -> AnswerEvaluation {
        let given = Self::comparison(&request.learner_answer);
        if given == Self::comparison(&request.expected_answer) {
            return AnswerEvaluation {
                verdict: AnswerVerdict::Correct,
                matched_answer: Some(request.expected_answer.clone()),
                feedback: None,
            };
        }
        let variant = request
            .accepted_answers
            .iter()
            .find(|a| Self::comparison(a) == given);
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

    fn classify_error(
        &self,
        request: &LanguageErrorClassificationRequest,
    ) -> LanguageErrorClassification {
        let original = Self::comparison(&request.original_production);
        let corrected = Self::comparison(&request.corrected_production);
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

    fn card_templates(&self) -> &[CardTemplate] {
        &self.templates
    }

    fn tutor_instructions(&self, learner: &LearnerLanguageProfile) -> TutorInstructionBundle {
        TutorInstructionBundle {
            instructions: format!(
                "Synthetic tutor rules; instruct from {}.",
                learner.source_language
            ),
            correction_conventions: "Correct inline, restate once.".to_owned(),
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

    fn pack() -> Box<dyn LanguagePack> {
        register()
    }

    #[test]
    fn canonical_collapses_whitespace_and_trims() {
        let out = pack().normalize_target_text("  zug   BLA  ", NormalizationMode::Canonical);
        assert_eq!(out.0, "zug BLA");
    }

    #[test]
    fn comparison_lowercases_and_strips_trailing_punctuation() {
        let out = pack().normalize_target_text("Zug bla!?", NormalizationMode::Comparison);
        assert_eq!(out.0, "zug bla");
    }

    #[test]
    fn evaluate_correct_variant_and_incorrect() {
        let pack = pack();
        let request = |answer: &str| AnswerEvaluationRequest {
            expected_answer: "zug bla".to_owned(),
            accepted_answers: vec!["blip zug".to_owned()],
            learner_answer: answer.to_owned(),
            evaluation_mode: "exact".to_owned(),
        };
        assert_eq!(
            pack.evaluate_answer(&request("ZUG bla!")).verdict,
            AnswerVerdict::Correct
        );
        assert_eq!(
            pack.evaluate_answer(&request("Blip zug")).verdict,
            AnswerVerdict::AcceptableVariant
        );
        assert_eq!(
            pack.evaluate_answer(&request("nope")).verdict,
            AnswerVerdict::Incorrect
        );
    }

    #[test]
    fn classification_is_deterministic_and_coded() {
        let pack = pack();
        let request = LanguageErrorClassificationRequest {
            original_production: "zug".to_owned(),
            corrected_production: "zug bla".to_owned(),
            context: None,
        };
        let first = pack.classify_error(&request);
        let second = pack.classify_error(&request);
        assert_eq!(first, second);
        assert_eq!(first.pack_error_code.as_deref(), Some("SYN-GRAM"));
        assert_eq!(first.core_category, CoreErrorCategory::Grammar);
    }
}
