#![no_main]

#[path = "../codec_contracts.rs"]
mod codec_contracts;

use libfuzzer_sys::fuzz_target;

fuzz_target!(|input: &[u8]| {
    static CHECK: std::sync::Once = std::sync::Once::new();
    CHECK.call_once(|| {
        let expected = std::env::var("RSE_EXPECT_BACKEND").unwrap();
        assert_eq!(rustfs_erasure_codec::galois_8::active_backend_name(), expected);
    });
    codec_contracts::exercise_codec_contracts(input);
});
