use rustfs_erasure_codec::{CodecFamily, CodecOptions, ShardSlot, galois_8::ReedSolomon};

#[test]
fn small_staging_boundaries_and_selective_publication_match_original_shards() {
    for family in [CodecFamily::LeopardGF8, CodecFamily::LeopardGF16] {
        if family == CodecFamily::LeopardGF16 && cfg!(target_endian = "big") {
            continue;
        }
        // 2 KiB boundary, a larger fallback, and 32/33 total shards.
        for (data, parity, len) in [
            (4usize, 2usize, 320usize),
            (4, 2, 384),
            (10, 4, 128),
            (16, 8, 128),
            (16, 16, 64),
            (24, 9, 64),
        ] {
            let codec = ReedSolomon::with_options(
                data,
                parity,
                CodecOptions::builder()
                    .codec_family(family)
                    .max_parallel_jobs(1)
                    .build(),
            )
            .unwrap();
            let mut source: Vec<Vec<u8>> = (0..data + parity)
                .map(|i| (0..len).map(|j| (i * 31 + j * 17 + j / 11) as u8).collect())
                .collect();
            codec.encode(&mut source).unwrap();
            for selected in [false, true] {
                let mut slots: Vec<_> =
                    source.iter().cloned().map(ShardSlot::new_present).collect();
                slots[0].mark_missing();
                slots[0].as_inner_mut().fill(0xa5);
                slots[data].mark_missing();
                slots[data].as_inner_mut().fill(0x5a);
                if selected {
                    let mut required = vec![false; data + parity];
                    required[0] = true;
                    codec.reconstruct_some(&mut slots, &required).unwrap();
                    assert!(!slots[data].is_present());
                    assert!(slots[data].as_inner().iter().all(|&b| b == 0x5a));
                } else {
                    codec.reconstruct(&mut slots).unwrap();
                    assert!(slots[data].is_present());
                    assert_eq!(slots[data].as_inner(), &source[data]);
                }
                assert!(slots[0].is_present());
                assert_eq!(slots[0].as_inner(), &source[0]);
            }
        }
    }
}
