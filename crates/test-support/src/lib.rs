//! Shared language-pack conformance suite (06-language-packs.md §8,
//! 12-testing.md §5). Every pack's test suite calls [`assert_conformance`].
//! Used only as a dev-dependency; never ships in production builds.
//!
//! Checks not expressible here: "no secret/network access" is enforced by
//! the trait boundary itself (no IO capability in `language-pack-api`), and
//! historical version attribution is a core-side concern (04-database.md §1).

// Test-support code: panicking on violation is the contract of an assert helper.
#![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]

use language_pack_api::{
    AnswerEvaluationRequest, CONTRACT_VERSION, Capability, CardIntent,
    LanguageErrorClassificationRequest, LanguagePack, NormalizationMode,
};
use proptest::prelude::*;
use proptest::test_runner::{Config, TestRng, TestRunner};

/// Runs the full conformance suite; panics with a precise message on the
/// first violation.
///
/// # Panics
///
/// On any conformance violation — this is the assertion helper for pack
/// test suites.
pub fn assert_conformance(pack: &dyn LanguagePack) {
    manifest_valid(pack);
    contract_compatible(pack);
    normalization_deterministic_and_idempotent(pack);
    evaluation_bounded_and_deterministic(pack);
    error_codes_stable(pack);
    card_templates_valid(pack);
}

fn manifest_valid(pack: &dyn LanguagePack) {
    let m = pack.manifest();
    for (field, value) in [
        ("id", &m.id),
        ("package", &m.package),
        ("display_name", &m.display_name),
        ("target_language", &m.target_language),
        ("target_locale", &m.target_locale),
        ("level_framework", &m.level_framework),
    ] {
        assert!(!value.trim().is_empty(), "manifest.{field} empty");
    }
    let semver_ok = {
        let parts: Vec<&str> = m.version.split('.').collect();
        parts.len() == 3 && parts.iter().all(|p| p.parse::<u32>().is_ok())
    };
    assert!(
        semver_ok,
        "manifest.version '{}' is not MAJOR.MINOR.PATCH",
        m.version
    );
    assert!(
        m.target_locale.starts_with(&m.target_language),
        "target_locale '{}' does not extend target_language '{}'",
        m.target_locale,
        m.target_language
    );
    assert!(
        m.supported_source_languages
            .contains(&m.default_source_language),
        "default_source_language '{}' not in supported_source_languages",
        m.default_source_language
    );
    let mut caps = m.capabilities.clone();
    caps.dedup();
    assert_eq!(caps.len(), m.capabilities.len(), "duplicate capabilities");
}

fn contract_compatible(pack: &dyn LanguagePack) {
    assert_eq!(
        pack.manifest().contract_version,
        CONTRACT_VERSION,
        "pack '{}' contract_version mismatch",
        pack.manifest().id
    );
}

/// Property check: for arbitrary unicode input, normalization is
/// deterministic (same output twice) and idempotent (fixed point after one
/// application). Fixed RNG seed — deterministic CI (16-rust-guidelines.md §6).
fn normalization_deterministic_and_idempotent(pack: &dyn LanguagePack) {
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
                let once = pack.normalize_target_text(&input, mode);
                let again = pack.normalize_target_text(&input, mode);
                prop_assert_eq!(&once, &again, "non-deterministic normalization");
                let twice = pack.normalize_target_text(&once.0, mode);
                prop_assert_eq!(&twice, &once, "non-idempotent normalization");
            }
            Ok(())
        })
        .expect("normalization property violated");
}

fn evaluation_bounded_and_deterministic(pack: &dyn LanguagePack) {
    let long_input = "palabra ".repeat(1_250); // ~10k chars: must return promptly, no panic
    let request = AnswerEvaluationRequest {
        expected_answer: "respuesta".to_owned(),
        accepted_answers: vec!["variante uno".to_owned(), "variante dos".to_owned()],
        learner_answer: long_input,
        evaluation_mode: "exact".to_owned(),
    };
    let first = pack.evaluate_answer(&request);
    let second = pack.evaluate_answer(&request);
    assert_eq!(first, second, "non-deterministic answer evaluation");
}

fn error_codes_stable(pack: &dyn LanguagePack) {
    let request = LanguageErrorClassificationRequest {
        original_production: "forma incorrecta".to_owned(),
        corrected_production: "forma correcta".to_owned(),
        context: None,
    };
    let first = pack.classify_error(&request);
    let second = pack.classify_error(&request);
    assert_eq!(first, second, "unstable error classification");
}

fn card_templates_valid(pack: &dyn LanguagePack) {
    let templates = pack.card_templates();
    let mut ids: Vec<&str> = templates.iter().map(|t| t.template_id.as_str()).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), templates.len(), "duplicate template_id");
    for t in templates {
        assert!(
            !t.prompt_template.trim().is_empty(),
            "{}: empty prompt_template",
            t.template_id
        );
        assert!(
            !t.answer_evaluation_mode.trim().is_empty(),
            "{}: empty answer_evaluation_mode",
            t.template_id
        );
    }
    let caps = &pack.manifest().capabilities;
    for (capability, intent) in [
        (Capability::ProductionCards, CardIntent::Production),
        (Capability::ListeningCards, CardIntent::Listening),
        (Capability::ContrastCards, CardIntent::Contrast),
        (Capability::TransferCards, CardIntent::Transfer),
    ] {
        let has_template = templates.iter().any(|t| t.intent == intent);
        let declared = caps.contains(&capability);
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
