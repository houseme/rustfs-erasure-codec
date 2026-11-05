#[test]
fn gf16_high_fanout_matches_published_parity_and_cross_recovers() {
    if cfg!(target_endian = "big") {
        return;
    }
    let selected = codec_new::galois_8::active_backend_name();
    if let Ok(expected) = std::env::var("RSE_EXPECT_BACKEND") {
        assert_eq!(selected, expected);
    }
    for (data, parity) in [(64usize, 16usize), (128, 32)] {
        for len in [64usize, 128] {
            let old = codec_old::galois_8::ReedSolomon::with_options(
                data,
                parity,
                codec_old::CodecOptions::builder()
                    .codec_family(codec_old::CodecFamily::LeopardGF16)
                    .max_parallel_jobs(1)
                    .build(),
            )
            .unwrap();
            let new = codec_new::galois_8::ReedSolomon::with_options(
                data,
                parity,
                codec_new::CodecOptions::builder()
                    .codec_family(codec_new::CodecFamily::LeopardGF16)
                    .max_parallel_jobs(1)
                    .build(),
            )
            .unwrap();
            let source: Vec<Vec<u8>> = (0..data + parity)
                .map(|i| (0..len).map(|j| (i * 31 + j * 17 + j / 11) as u8).collect())
                .collect();
            let mut old_codeword = source.clone();
            let mut new_codeword = source;
            old.encode(&mut old_codeword).unwrap();
            new.encode(&mut new_codeword).unwrap();
            assert_eq!(old_codeword, new_codeword);

            let missing = [0, data - 1, data, data + parity - 1];
            let mut old_slots: Vec<_> = new_codeword
                .iter()
                .cloned()
                .map(codec_old::ShardSlot::new_present)
                .collect();
            let mut new_slots: Vec<_> = old_codeword
                .iter()
                .cloned()
                .map(codec_new::ShardSlot::new_present)
                .collect();
            for &index in &missing {
                old_slots[index].mark_missing();
                new_slots[index].mark_missing();
            }
            old.reconstruct(&mut old_slots).unwrap();
            new.reconstruct(&mut new_slots).unwrap();
            for ((old_slot, new_slot), expected) in
                old_slots.iter().zip(&new_slots).zip(&old_codeword)
            {
                assert!(old_slot.is_present() && new_slot.is_present());
                assert_eq!(old_slot.as_inner(), expected);
                assert_eq!(new_slot.as_inner(), expected);
            }
        }
    }
}
