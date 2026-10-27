//! Shared conformance suite vs the es-ES explicit processing passes.

#[test]
fn es_es_pack_conforms() {
    let (manifest, pack) = language_pack_es_es::register();
    test_support::assert_manifest_conformance(
        &manifest,
        language_pack_es_es::card_template_count(&pack),
        |index| language_pack_es_es::card_template_at(&pack, index),
    );
    test_support::assert_normalization_conformance(language_pack_es_es::normalize_target_text);
    test_support::assert_evaluation_conformance(language_pack_es_es::evaluate_answer);
    test_support::assert_classification_conformance(|request| {
        language_pack_es_es::classify_error(&pack, request)
    });
}
