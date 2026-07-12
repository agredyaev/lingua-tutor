//! Spanish (Spain) language pack. WBS 2.4 stub: manifest + baseline
//! normalization + exact answer matching. Full normalization policy,
//! error taxonomy, templates, tutor instructions, curriculum land in
//! WBS 7 (06-language-packs.md §7). No Spanish logic outside this crate.

use language_pack_api::{
    AnswerEvaluation, AnswerEvaluationRequest, AnswerVerdict, CardTemplate, CoreErrorCategory,
    CurriculumCatalog, LanguageErrorClassification, LanguageErrorClassificationRequest,
    LanguagePack, LanguagePackManifest, LearnerLanguageProfile, NormalizationMode, NormalizedText,
    TutorInstructionBundle,
};

const MANIFEST_JSON: &str = include_str!("../manifest.json");

/// Builds the pack for registration in the composition root.
#[must_use]
pub fn register() -> Box<dyn LanguagePack> {
    Box::new(EsEsPack::new())
}

struct EsEsPack {
    manifest: LanguagePackManifest,
    catalog: CurriculumCatalog,
}

impl EsEsPack {
    #[allow(clippy::expect_used)] // invariant: embedded manifest.json validated by this crate's tests
    fn new() -> Self {
        Self {
            manifest: serde_json::from_str(MANIFEST_JSON)
                .expect("embedded manifest.json matches LanguagePackManifest"),
            catalog: CurriculumCatalog { scenarios: vec![] },
        }
    }

    /// Baseline canonical form: trimmed, single-spaced. Diacritic,
    /// punctuation (¿¡) and contraction policy arrive in WBS 7.1.
    fn canonical(input: &str) -> String {
        input.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    fn comparison(input: &str) -> String {
        Self::canonical(input).to_lowercase()
    }
}

impl LanguagePack for EsEsPack {
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
        match request
            .accepted_answers
            .iter()
            .find(|a| Self::comparison(a) == given)
        {
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
        _request: &LanguageErrorClassificationRequest,
    ) -> LanguageErrorClassification {
        // error_classification capability not declared until WBS 7.2.
        LanguageErrorClassification {
            core_category: CoreErrorCategory::Other,
            pack_error_code: None,
            explanation: None,
        }
    }

    fn card_templates(&self) -> &[CardTemplate] {
        &[] // card capabilities not declared until WBS 7.3
    }

    fn tutor_instructions(&self, _learner: &LearnerLanguageProfile) -> TutorInstructionBundle {
        TutorInstructionBundle {
            instructions: "Enseña español de España.".to_owned(),
            correction_conventions: "Corrige inmediatamente y repite la forma correcta.".to_owned(),
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

    #[test]
    fn embedded_manifest_parses_and_identifies_pack() {
        let pack = register();
        let manifest = pack.manifest();
        assert_eq!(manifest.id, "es-ES");
        assert_eq!(manifest.package, "language-pack-es-es");
        assert_eq!(
            manifest.contract_version,
            language_pack_api::CONTRACT_VERSION
        );
        assert_eq!(manifest.default_source_language, "ru");
    }

    #[test]
    fn baseline_normalization_keeps_diacritics() {
        let pack = register();
        let out = pack.normalize_target_text("  ¿Qué   tal?  ", NormalizationMode::Canonical);
        assert_eq!(out.0, "¿Qué tal?");
        let cmp = pack.normalize_target_text("¿Qué   tal?", NormalizationMode::Comparison);
        assert_eq!(cmp.0, "¿qué tal?");
    }

    #[test]
    fn exact_matching_accepts_case_difference_only() {
        let pack = register();
        let request = AnswerEvaluationRequest {
            expected_answer: "buenos días".to_owned(),
            accepted_answers: vec![],
            learner_answer: "Buenos días".to_owned(),
            evaluation_mode: "exact".to_owned(),
        };
        assert_eq!(
            pack.evaluate_answer(&request).verdict,
            AnswerVerdict::Correct
        );
    }
}
