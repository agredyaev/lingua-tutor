//! Pins adaptation's threshold mirror to learning-core constants
//! (adaptation must not depend on learning-core; this test is the guard).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[test]
fn escalation_thresholds_stay_in_sync() {
    assert_eq!(
        adaptation::error_escalation_action(learning_core::ERROR_ESCALATION_THRESHOLD),
        Some("activate_mini_target")
    );
    assert_eq!(
        adaptation::error_escalation_action(learning_core::ERROR_METHOD_CHANGE_THRESHOLD),
        Some("require_method_change")
    );
    assert!(
        adaptation::error_escalation_action(learning_core::ERROR_ESCALATION_THRESHOLD - 1)
            .is_none()
    );
}
