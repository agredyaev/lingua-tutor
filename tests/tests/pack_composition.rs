//! M7 second-pack validation: es-ES and zh-CN register side by side with
//! zero core changes (13-roadmap.md M7). Production composition stays
//! es-ES-only (worker lib.rs test); this proves multi-pack capability.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[test]
fn es_es_and_zh_cn_compose_in_one_registry() {
    let registry = language_pack_registry::LanguagePackRegistry::new([
        language_pack_es_es::register(),
        language_pack_zh_cn::register(),
    ])
    .expect("two real packs must not collide");
    assert!(registry.get("es-ES").is_some());
    assert!(registry.get("zh-CN").is_some());
    assert_eq!(registry.manifests().count(), 2);

    // The same core normalization entry point serves both scripts.
    let zh = registry.get("zh-CN").unwrap();
    let normalized =
        zh.normalize_target_text("你 好", language_pack_api::NormalizationMode::Canonical);
    assert_eq!(normalized.0, "你好");
}
