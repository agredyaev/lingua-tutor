#![no_main]
#[path = "support/pure.rs"]
mod pure;
use libfuzzer_sys::fuzz_target;
use lingua_tutor_local::fuzzing::*;
fuzz_target!(|data: &[u8]| {
    let (mode, rest) = data.split_first().unwrap_or((&0, &[]));
    match mode % 15 {
        0 => pure::exercise(rest),
        1 => exercise_contract(rest),
        2 => exercise_card_model(rest),
        3 => exercise_adaptation_model(rest),
        4 => exercise_lesson_lifecycle(rest),
        5 => exercise_lesson_result_stateful(rest),
        6 => exercise_card_lifecycle(rest),
        7 => exercise_review_lifecycle(rest),
        8 => exercise_maintenance_writes(rest),
        9 => exercise_boundaries(rest),
        10 => exercise_storage(rest),
        11 => exercise_tunnel_wire(rest),
        12 => exercise_error_writes(rest),
        13 => exercise_tunnel_transport(rest),
        _ => exercise_review_states(rest),
    }
});
