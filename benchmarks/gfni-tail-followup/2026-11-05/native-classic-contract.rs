#[path = "../fuzz/codec_contracts.rs"]
mod codec_contracts;

fn main() {
    assert!(std::is_x86_feature_detected!("avx2"));
    assert_eq!(rustfs_erasure_codec::galois_8::active_backend_name(), std::env::var("RSE_EXPECT_BACKEND").unwrap());
    let mut state = 0x9e37_79b9_7f4a_7c15u64;
    for index in 0..20_000usize {
        let mut bytes = [0u8; 80];
        for byte in &mut bytes {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            *byte = state as u8;
        }
        bytes[0] = 0; // Classic GF8.
        bytes[5] = (index % 12) as u8;
        bytes[7] = (index % 6) as u8;
        let parity = usize::from(bytes[4] % 4) + 1;
        bytes[8] = (1 + index % parity) as u8;
        codec_contracts::exercise_codec_contracts(&bytes);
    }
    println!("20,000 deterministic Classic GF8 contract cases passed");
}
