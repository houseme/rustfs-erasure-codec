use rustfs_erasure_codec::{CodecFamily, CodecOptions, ShardSlot, galois_8::ReedSolomon};
use std::sync::Arc;
#[test]
fn stack_child() {
    let Ok(stack) = std::env::var("RSE_STACK_PROBE_BYTES") else {
        return;
    };
    let stack: usize = stack.parse().unwrap();
    let selected = rustfs_erasure_codec::galois_8::active_backend_name();
    if let Ok(expected) = std::env::var("RSE_EXPECT_BACKEND") {
        assert_eq!(selected, expected);
    }
    let family = if std::env::var("RSE_STACK_PROBE_FAMILY").unwrap() == "gf8" {
        CodecFamily::LeopardGF8
    } else {
        CodecFamily::LeopardGF16
    };
    let codec = Arc::new(
        ReedSolomon::with_options(
            16,
            8,
            CodecOptions::builder()
                .codec_family(family)
                .max_parallel_jobs(1)
                .build(),
        )
        .unwrap(),
    );
    let mut original: Vec<Vec<u8>> = (0..24)
        .map(|i| (0..128).map(|j| (i * 31 + j * 17) as u8).collect())
        .collect();
    codec.encode(&mut original).unwrap();
    let mut warm: Vec<_> = original
        .iter()
        .cloned()
        .map(ShardSlot::new_present)
        .collect();
    for slot in &mut warm[..4] {
        slot.mark_missing();
    }
    codec.reconstruct(&mut warm).unwrap();
    let result = std::thread::Builder::new()
        .stack_size(stack)
        .spawn(move || {
            let mut slots: Vec<_> = original
                .iter()
                .cloned()
                .map(ShardSlot::new_present)
                .collect();
            for slot in &mut slots[..4] {
                slot.mark_missing();
            }
            codec.reconstruct(&mut slots).unwrap();
            assert!(
                slots
                    .iter()
                    .zip(&original)
                    .all(|(s, x)| s.is_present() && s.as_inner() == x)
            );
        })
        .unwrap()
        .join();
    assert!(result.is_ok());
}
