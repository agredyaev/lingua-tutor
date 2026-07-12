//! Shared conformance suite vs the synthetic pack (12-testing.md §5).

#[test]
fn synthetic_pack_conforms() {
    test_support::assert_conformance(synthetic_test_pack::register().as_ref());
}
