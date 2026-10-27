//! Shared language-pack conformance checks for explicit processing passes.
//! Used only as a dev-dependency; never ships in production builds.

#![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]

use language_pack_api::{
    AnswerEvaluation, AnswerEvaluationRequest, CONTRACT_VERSION, Capability, CardIntent,
    CardTemplateView, LanguageErrorClassification, LanguageErrorClassificationRequest,
    LanguagePackManifest, NormalizationMode, NormalizedText,
};
use proptest::prelude::*;
use proptest::test_runner::{Config, TestRng, TestRunner};

/// Validate manifest fields, contract compatibility, and template capabilities.
pub fn assert_manifest_conformance<'a>(
    manifest: &LanguagePackManifest,
    template_count: usize,
    template_at: impl Fn(u32) -> Option<CardTemplateView<'a>>,
) {
    for (field, value) in [
        ("id", &manifest.id),
        ("package", &manifest.package),
        ("display_name", &manifest.display_name),
        ("target_language", &manifest.target_language),
        ("target_locale", &manifest.target_locale),
        ("level_framework", &manifest.level_framework),
    ] {
        assert!(!value.trim().is_empty(), "manifest.{field} empty");
    }

    let parts: Vec<&str> = manifest.version.split('.').collect();
    assert!(
        parts.len() == 3 && parts.iter().all(|part| part.parse::<u32>().is_ok()),
        "manifest.version '{}' is not MAJOR.MINOR.PATCH",
        manifest.version
    );
    assert!(
        manifest
            .target_locale
            .starts_with(&manifest.target_language),
        "target_locale '{}' does not extend target_language '{}'",
        manifest.target_locale,
        manifest.target_language
    );
    assert!(
        manifest
            .supported_source_languages
            .contains(&manifest.default_source_language),
        "default_source_language '{}' not in supported_source_languages",
        manifest.default_source_language
    );

    let mut capabilities = manifest.capabilities.clone();
    capabilities.dedup();
    assert_eq!(
        capabilities.len(),
        manifest.capabilities.len(),
        "duplicate capabilities"
    );
    assert_eq!(
        manifest.contract_version, CONTRACT_VERSION,
        "pack '{}' contract_version mismatch",
        manifest.id
    );

    assert_card_templates_conformance(manifest, template_count, template_at);
}

/// Verify deterministic and idempotent normalization across generated inputs.
pub fn assert_normalization_conformance(
    normalize: impl Fn(&str, NormalizationMode) -> NormalizedText,
) {
    let mut runner = TestRunner::new_with_rng(
        Config {
            cases: 256,
            failure_persistence: None,
            ..Config::default()
        },
        TestRng::deterministic_rng(prop::test_runner::RngAlgorithm::ChaCha),
    );
    runner
        .run(&any::<String>(), |input| {
            for mode in [NormalizationMode::Canonical, NormalizationMode::Comparison] {
                let once = normalize(&input, mode);
                let again = normalize(&input, mode);
                prop_assert_eq!(&once, &again, "non-deterministic normalization");
                let twice = normalize(&once.0, mode);
                prop_assert_eq!(&twice, &once, "non-idempotent normalization");
            }
            Ok(())
        })
        .expect("normalization property violated");
}

/// Verify bounded answer evaluation remains deterministic for a large input.
pub fn assert_evaluation_conformance(
    evaluate: impl Fn(&AnswerEvaluationRequest) -> AnswerEvaluation,
) {
    let request = AnswerEvaluationRequest {
        expected_answer: "respuesta".to_owned(),
        accepted_answers: vec!["variante uno".to_owned(), "variante dos".to_owned()],
        learner_answer: "palabra ".repeat(1_250),
        evaluation_mode: "exact".to_owned(),
    };
    let first = evaluate(&request);
    let second = evaluate(&request);
    assert_eq!(first, second, "non-deterministic answer evaluation");
}

/// Verify error classification remains deterministic for a stable correction.
pub fn assert_classification_conformance(
    classify: impl Fn(&LanguageErrorClassificationRequest) -> LanguageErrorClassification,
) {
    let request = LanguageErrorClassificationRequest {
        original_production: "forma incorrecta".to_owned(),
        corrected_production: "forma correcta".to_owned(),
        context: None,
    };
    let first = classify(&request);
    let second = classify(&request);
    assert_eq!(first, second, "unstable error classification");
}

fn assert_card_templates_conformance<'a>(
    manifest: &LanguagePackManifest,
    template_count: usize,
    template_at: impl Fn(u32) -> Option<CardTemplateView<'a>>,
) {
    let mut ids = Vec::with_capacity(template_count);
    for index in 0..template_count {
        let index = u32::try_from(index).expect("template count fits u32");
        let template = template_at(index).expect("dense template index exists");
        ids.push(template.template_id);
        assert!(
            !template.prompt_template.trim().is_empty(),
            "{}: empty prompt_template",
            template.template_id
        );
        assert!(
            !template.answer_evaluation_mode.trim().is_empty(),
            "{}: empty answer_evaluation_mode",
            template.template_id
        );
    }
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), template_count, "duplicate template_id");

    for (capability, intent) in [
        (Capability::ProductionCards, CardIntent::Production),
        (Capability::ListeningCards, CardIntent::Listening),
        (Capability::ContrastCards, CardIntent::Contrast),
        (Capability::TransferCards, CardIntent::Transfer),
    ] {
        let has_template = (0..template_count).any(|index| {
            let index = u32::try_from(index).expect("template count fits u32");
            template_at(index)
                .map(|template| template.intent == intent)
                .unwrap_or(false)
        });
        let declared = manifest.capabilities.contains(&capability);
        assert!(
            !declared || has_template,
            "capability {capability:?} declared but no {intent:?} template"
        );
        assert!(
            declared || !has_template,
            "{intent:?} template present but capability {capability:?} not declared"
        );
    }
}
