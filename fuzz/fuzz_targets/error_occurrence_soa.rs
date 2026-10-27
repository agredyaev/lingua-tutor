//! Fuzz fixed-capacity SoA occurrence buffering + SQLite transaction semantics.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    lingua_tutor_local::operations_v2::error::fuzz_error_occurrence_sequence(data);
});
