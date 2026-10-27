//! Shared conformance suite vs the synthetic explicit processing passes.

#[test]
fn synthetic_pack_conforms() {
    let (manifest, pack) = synthetic_test_pack::register();
    test_support::assert_manifest_conformance(
        &manifest,
        synthetic_test_pack::card_template_count(&pack),
        |index| synthetic_test_pack::card_template_at(&pack, index),
    );
    test_support::assert_normalization_conformance(synthetic_test_pack::normalize_target_text);
    test_support::assert_evaluation_conformance(synthetic_test_pack::evaluate_answer);
    test_support::assert_classification_conformance(synthetic_test_pack::classify_error);
}
