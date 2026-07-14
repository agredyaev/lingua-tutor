//! Shared conformance suite vs the zh-CN pack (12-testing.md §5; M7 gate).

#[test]
fn zh_cn_pack_conforms() {
    test_support::assert_conformance(language_pack_zh_cn::register().as_ref());
}
