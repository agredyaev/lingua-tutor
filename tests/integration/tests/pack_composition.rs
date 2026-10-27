//! Composition validation with static, indexable concrete-pack storage.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use language_pack_api::NormalizationMode;

// Test-only closed data variants. Dispatch stays explicit at the call site.
enum TestPackData {
    Spanish(language_pack_es_es::EsEsPack),
    Synthetic(synthetic_test_pack::SyntheticTestPack),
}

#[test]
fn production_and_synthetic_packs_compose() {
    let (spanish_manifest, spanish_pack) = language_pack_es_es::register();
    let (synthetic_manifest, synthetic_pack) = synthetic_test_pack::register();
    let registry = language_pack_registry::build_registry([
        (spanish_manifest, TestPackData::Spanish(spanish_pack)),
        (synthetic_manifest, TestPackData::Synthetic(synthetic_pack)),
    ])
    .expect("production and synthetic packs must not collide");

    assert_eq!(
        language_pack_registry::index_of(&registry, "es-ES"),
        Some(0)
    );
    assert_eq!(
        language_pack_registry::index_of(&registry, "zz-ZZ"),
        Some(1)
    );
    assert_eq!(language_pack_registry::manifests(&registry).count(), 2);

    let synthetic_index = language_pack_registry::index_of(&registry, "zz-ZZ").unwrap();
    let normalized =
        match language_pack_registry::pack_by_index(&registry, synthetic_index).unwrap() {
            TestPackData::Synthetic(pack) => {
                assert_eq!(synthetic_test_pack::card_template_count(pack), 4);
                synthetic_test_pack::normalize_target_text(
                    "  ZUG   bla  ",
                    NormalizationMode::Canonical,
                )
            }
            TestPackData::Spanish(pack) => {
                assert_eq!(language_pack_es_es::card_template_count(pack), 4);
                language_pack_es_es::normalize_target_text(
                    "  ZUG   bla  ",
                    NormalizationMode::Canonical,
                )
            }
        };
    assert_eq!(normalized.0, "ZUG bla");
}
