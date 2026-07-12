//! Manifest contract tests: the 06-language-packs.md §3 JSON shape must parse.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use language_pack_api::{CONTRACT_VERSION, Capability, LanguagePackManifest};

const ES_ES_MANIFEST: &str = r#"{
  "id": "es-ES",
  "package": "language-pack-es-es",
  "version": "1.0.0",
  "contract_version": "1",
  "target_language": "es",
  "target_locale": "es-ES",
  "display_name": "Spanish (Spain)",
  "supported_source_languages": ["ru", "en"],
  "default_source_language": "ru",
  "level_framework": "CEFR",
  "capabilities": [
    "production_cards",
    "listening_cards",
    "contrast_cards",
    "transfer_cards",
    "error_classification",
    "answer_matching"
  ]
}"#;

#[test]
fn spec_manifest_json_parses() {
    let manifest: LanguagePackManifest = serde_json::from_str(ES_ES_MANIFEST).unwrap();
    assert_eq!(manifest.id, "es-ES");
    assert_eq!(manifest.package, "language-pack-es-es");
    assert_eq!(manifest.contract_version, 1);
    assert_eq!(manifest.target_locale, "es-ES");
    assert_eq!(manifest.default_source_language, "ru");
    assert_eq!(manifest.capabilities.len(), 6);
    assert!(manifest.capabilities.contains(&Capability::ProductionCards));
    assert!(manifest.capabilities.contains(&Capability::AnswerMatching));
}

#[test]
fn contract_version_is_one_for_v1() {
    assert_eq!(CONTRACT_VERSION, 1);
}

#[test]
fn numeric_contract_version_also_accepted() {
    let json = ES_ES_MANIFEST.replace("\"contract_version\": \"1\"", "\"contract_version\": 1");
    let manifest: LanguagePackManifest = serde_json::from_str(&json).unwrap();
    assert_eq!(manifest.contract_version, 1);
}

#[test]
fn unknown_capability_is_rejected() {
    let json = ES_ES_MANIFEST.replace("production_cards", "telepathy_cards");
    assert!(serde_json::from_str::<LanguagePackManifest>(&json).is_err());
}
