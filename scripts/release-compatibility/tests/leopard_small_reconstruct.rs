#[test]
fn leopard_small_slot_recovery_matches_published_version_across_boundaries() {
    for (old_family, new_family) in [
        (
            codec_old::CodecFamily::LeopardGF8,
            codec_new::CodecFamily::LeopardGF8,
        ),
        (
            codec_old::CodecFamily::LeopardGF16,
            codec_new::CodecFamily::LeopardGF16,
        ),
    ] {
        if new_family == codec_new::CodecFamily::LeopardGF16 && cfg!(target_endian = "big") {
            continue;
        }
        for (data, parity, len) in [
            (4usize, 2usize, 320usize),
            (4, 2, 384),
            (10, 4, 64),
            (10, 4, 128),
            (16, 8, 64),
            (16, 8, 128),
            (16, 16, 64),
            (24, 9, 64),
        ] {
            let old = codec_old::galois_8::ReedSolomon::with_options(
                data,
                parity,
                codec_old::CodecOptions::builder()
                    .codec_family(old_family)
                    .max_parallel_jobs(1)
                    .build(),
            )
            .unwrap();
            let new = codec_new::galois_8::ReedSolomon::with_options(
                data,
                parity,
                codec_new::CodecOptions::builder()
                    .codec_family(new_family)
                    .max_parallel_jobs(1)
                    .build(),
            )
            .unwrap();
            let source: Vec<Vec<u8>> = (0..data + parity)
                .map(|i| (0..len).map(|j| (i * 31 + j * 17 + j / 11) as u8).collect())
                .collect();
            let mut a = source.clone();
            let mut b = source;
            old.encode(&mut a).unwrap();
            new.encode(&mut b).unwrap();
            assert_eq!(a, b);
            for encoded in [&a, &b] {
                for missing in [
                    vec![0],
                    vec![data],
                    vec![0, 1],
                    vec![0, data],
                    vec![data, data + 1],
                ] {
                    let mut old_slots: Vec<_> = encoded
                        .iter()
                        .cloned()
                        .map(codec_old::ShardSlot::new_present)
                        .collect();
                    let mut new_slots: Vec<_> = encoded
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
                    for i in 0..data + parity {
                        assert!(old_slots[i].is_present() && new_slots[i].is_present());
                        assert_eq!(old_slots[i].as_inner(), &encoded[i]);
                        assert_eq!(new_slots[i].as_inner(), &encoded[i]);
                    }
                }
            }
        }
    }
}
