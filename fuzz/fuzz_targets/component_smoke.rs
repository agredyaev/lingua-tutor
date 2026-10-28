#![no_main]
#[path = "support/pure.rs"]
mod pure;
use libfuzzer_sys::fuzz_target;
fuzz_target!(|data: &[u8]| {
    pure::exercise(data);
});
