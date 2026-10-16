//! Pins adaptation's threshold mirror to learning-core constants
//! (adaptation must not depend on learning-core; this test is the guard).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[test]
fn escalation_thresholds_stay_in_sync() {
    assert_eq!(
        adaptation::error_escalation("t", learning_core::ERROR_ESCALATION_THRESHOLD)[0].action,
        "activate_mini_target"
    );
    assert_eq!(
        adaptation::error_escalation("t", learning_core::ERROR_METHOD_CHANGE_THRESHOLD)[0].action,
        "require_method_change"
    );
    assert!(
        adaptation::error_escalation("t", learning_core::ERROR_ESCALATION_THRESHOLD - 1).is_empty()
    );
}
