//! Fuzz target: lesson-result parsing + validation must never panic on any
//! byte sequence — this is the pasted-ChatGPT-output boundary
//! (07-tutor-contract.md §2, 10-security.md). Run: cargo +nightly fuzz run
//! lesson_result_parse (separate CI job, not a PR gate).

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(text) = std::str::from_utf8(data) {
        if let Ok(result) = tutor_contract::parse_lesson_result(text) {
            // Validation must also hold on anything that parses.
            let _ = tutor_contract::validate(&result);
        }
    }
});
