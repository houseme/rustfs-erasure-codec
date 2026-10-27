#[test]
fn published_and_current_leopard_option_entrypoints_cross_decode() {
    for family in 0..if cfg!(target_endian = "little") { 2 } else { 1 } {
        let old_family = if family == 0 {
            codec_old::CodecFamily::LeopardGF8
        } else {
            codec_old::CodecFamily::LeopardGF16
        };
        let new_family = if family == 0 {
            codec_new::CodecFamily::LeopardGF8
        } else {
            codec_new::CodecFamily::LeopardGF16
        };
        for (data, parity) in [(4, 3), (10, 4)] {
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
            // The 9.0.0 GF8 decoder cannot handle a short final chunk; keep
            // that historical limitation separate from this compatibility test.
            for len in [64, 65_536] {
                let source: Vec<Vec<u8>> = (0..data + parity)
                    .map(|i| (0..len).map(|j| (i * 31 + j * 17) as u8).collect())
                    .collect();
                let mut encoded_old = source.clone();
                let mut encoded_new = source.clone();
                old.encode(&mut encoded_old).unwrap();
                new.encode(&mut encoded_new).unwrap();
                assert_eq!(encoded_old, encoded_new);
                for encoded in [&encoded_old, &encoded_new] {
                    for goal in 0..3 {
                        let mut input: Vec<_> = encoded.iter().cloned().map(Some).collect();
                        input[0] = None;
                        input[data] = None;
                        let mut old_output = input.clone();
                        let mut new_output = input;
                        let mut required = vec![false; data + parity];
                        required[0] = true;
                        let old_result = match goal {
                            0 => old.reconstruct_opt(&mut old_output),
                            1 => old.reconstruct_data_opt(&mut old_output),
                            _ => old.reconstruct_some_opt(&mut old_output, &required),
                        };
                        let new_result = match goal {
                            0 => new.reconstruct_opt(&mut new_output),
                            1 => new.reconstruct_data_opt(&mut new_output),
                            _ => new.reconstruct_some_opt(&mut new_output, &required),
                        };
                        assert!(old_result.is_ok());
                        assert!(new_result.is_ok());
                        // Published 9.0.0 recovers every Leopard shard for all
                        // three APIs; the current selective APIs intentionally
                        // leave unrequested parity missing.
                        assert!(
                            old_output
                                .iter()
                                .zip(encoded)
                                .all(|(a, b)| a.as_ref() == Some(b))
                        );
                        if goal == 0 {
                            assert_eq!(old_output, new_output);
                        }
                        assert_eq!(new_output[0].as_ref(), Some(&encoded[0]));
                        assert_eq!(new_output[data].is_some(), goal == 0);
                    }
                }
            }
        }
    }
}
