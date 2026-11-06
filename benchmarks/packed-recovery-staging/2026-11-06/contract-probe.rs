#[path = "../fuzz/codec_contracts.rs"]
mod codec_contracts;

#[test]
fn deterministic_packed_contract_inputs() {
    let selected = rustfs_erasure_codec::galois_8::active_backend_name();
    if let Ok(expected) = std::env::var("RSE_EXPECT_BACKEND") {
        assert_eq!(selected, expected);
    }
    let mut state = 0x5eeda11ce1234567u64;
    for index in 0..20_000u32 {
        let mut bytes = [0u8; 80];
        for value in &mut bytes {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            *value = state as u8;
        }
        bytes[0] = 1 + (index % 2) as u8;
        bytes[3] = 7;
        bytes[4] = 3;
        bytes[5] = if (index / 2) % 2 == 0 { 1 } else { 2 };
        bytes[6] = ((index / 4) % 4) as u8;
        bytes[7] = ((index / 16) % 3) as u8;
        bytes[8] = ((index / 48) % 5) as u8;
        bytes[13] = ((index / 240) % 10) as u8;
        codec_contracts::exercise_codec_contracts(&bytes);
    }
}
