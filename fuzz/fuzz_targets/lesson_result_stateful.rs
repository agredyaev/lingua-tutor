#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    lingua_tutor_local::fuzzing::exercise_lesson_result_stateful(data);
});
