//! Shared conformance suite vs the es-ES pack (12-testing.md §5).

#[test]
fn es_es_pack_conforms() {
    test_support::assert_conformance(language_pack_es_es::register().as_ref());
}
