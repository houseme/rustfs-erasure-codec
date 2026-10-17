#![no_main]

#[path = "../codec_contracts.rs"]
mod codec_contracts;

use libfuzzer_sys::fuzz_target;

fuzz_target!(|input: &[u8]| codec_contracts::exercise_codec_contracts(input));
