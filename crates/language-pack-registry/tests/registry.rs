//! Registry contract: lookup, contract-version gate, duplicate rejection.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use language_pack_api::{
    AnswerEvaluation, AnswerEvaluationRequest, AnswerVerdict, CardTemplate, CoreErrorCategory,
    CurriculumCatalog, LanguageErrorClassification, LanguageErrorClassificationRequest,
    LanguagePack, LanguagePackManifest, LearnerLanguageProfile, NormalizationMode, NormalizedText,
    TutorInstructionBundle,
};
use language_pack_registry::{LanguagePackRegistry, RegistryError};

struct FakePack {
    manifest: LanguagePackManifest,
    catalog: CurriculumCatalog,
}

impl FakePack {
    fn boxed(id: &str, contract_version: u32) -> Box<dyn LanguagePack> {
        Box::new(Self {
            manifest: LanguagePackManifest {
                id: id.to_owned(),
                package: format!("language-pack-{id}"),
                version: "0.1.0".to_owned(),
                contract_version,
                target_language: "zz".to_owned(),
                target_locale: "zz-ZZ".to_owned(),
                display_name: "Fake".to_owned(),
                supported_source_languages: vec!["ru".to_owned()],
                default_source_language: "ru".to_owned(),
                level_framework: "CEFR".to_owned(),
                capabilities: vec![],
            },
            catalog: CurriculumCatalog {
                scenarios: vec![],
                seeds: vec![],
            },
        })
    }
}

impl LanguagePack for FakePack {
    fn manifest(&self) -> &LanguagePackManifest {
        &self.manifest
    }
    fn normalize_target_text(&self, input: &str, _mode: NormalizationMode) -> NormalizedText {
        NormalizedText(input.to_owned())
    }
    fn evaluate_answer(&self, _request: &AnswerEvaluationRequest) -> AnswerEvaluation {
        AnswerEvaluation {
            verdict: AnswerVerdict::Incorrect,
            matched_answer: None,
            feedback: None,
        }
    }
    fn classify_error(
        &self,
        _request: &LanguageErrorClassificationRequest,
    ) -> LanguageErrorClassification {
        LanguageErrorClassification {
            core_category: CoreErrorCategory::Other,
            pack_error_code: None,
            explanation: None,
        }
    }
    fn card_templates(&self) -> &[CardTemplate] {
        &[]
    }
    fn tutor_instructions(&self, _learner: &LearnerLanguageProfile) -> TutorInstructionBundle {
        TutorInstructionBundle {
            instructions: String::new(),
            correction_conventions: String::new(),
        }
    }
    fn curriculum_catalog(&self) -> &CurriculumCatalog {
        &self.catalog
    }
}

#[test]
fn lookup_by_id_returns_registered_pack() {
    let registry =
        LanguagePackRegistry::new([FakePack::boxed("aa-AA", 1), FakePack::boxed("bb-BB", 1)])
            .unwrap();
    assert_eq!(registry.get("bb-BB").unwrap().manifest().id, "bb-BB");
    assert!(registry.get("cc-CC").is_none());
}

#[test]
fn incompatible_contract_version_fails_with_pack_named() {
    let err = LanguagePackRegistry::new([FakePack::boxed("aa-AA", 999)]).unwrap_err();
    assert_eq!(
        err,
        RegistryError::IncompatibleContract {
            pack_id: "aa-AA".to_owned(),
            pack_contract: 999,
            core_contract: 1,
        }
    );
    assert!(
        err.to_string().contains("aa-AA"),
        "startup error must name the pack"
    );
}

#[test]
fn duplicate_pack_id_rejected() {
    let err = LanguagePackRegistry::new([FakePack::boxed("aa-AA", 1), FakePack::boxed("aa-AA", 1)])
        .unwrap_err();
    assert_eq!(err, RegistryError::DuplicateId("aa-AA".to_owned()));
}

#[test]
fn manifests_preserve_registration_order() {
    let registry =
        LanguagePackRegistry::new([FakePack::boxed("aa-AA", 1), FakePack::boxed("bb-BB", 1)])
            .unwrap();
    let ids: Vec<_> = registry.manifests().map(|m| m.id.as_str()).collect();
    assert_eq!(ids, ["aa-AA", "bb-BB"]);
}
