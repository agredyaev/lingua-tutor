//! Composition validation: production es-ES and the independent synthetic
//! test pack register side by side without changing generic core code.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[test]
fn production_and_synthetic_packs_compose() {
    let registry = language_pack_registry::LanguagePackRegistry::new([
        language_pack_es_es::register(),
        synthetic_test_pack::register(),
    ])
    .expect("production and synthetic packs must not collide");

    assert!(registry.get("es-ES").is_some());
    assert!(registry.get("zz-ZZ").is_some());
    assert_eq!(registry.manifests().count(), 2);

    let synthetic = registry.get("zz-ZZ").unwrap();
    let normalized = synthetic.normalize_target_text(
        "  ZUG   bla  ",
        language_pack_api::NormalizationMode::Canonical,
    );
    assert_eq!(normalized.0, "ZUG bla");
}
