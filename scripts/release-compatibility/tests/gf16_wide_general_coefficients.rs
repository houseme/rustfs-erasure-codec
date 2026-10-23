//! Exercise non-subfield coefficients through both real codec versions.

#[test]
fn wide_gf16_parity_and_cross_recovery_match_published_900() {
    const DATA: usize = 2;
    const PARITY: usize = 255;
    const TOTAL: usize = DATA + PARITY;
    for (name, old_mode, new_mode) in [
        (
            "vandermonde",
            codec_old::MatrixMode::Vandermonde,
            codec_new::MatrixMode::Vandermonde,
        ),
        (
            "cauchy",
            codec_old::MatrixMode::Cauchy,
            codec_new::MatrixMode::Cauchy,
        ),
    ] {
        let old = codec_old::galois_16::ReedSolomon::with_options(
            DATA,
            PARITY,
            codec_old::CodecOptions::builder()
                .matrix_mode(old_mode)
                .build(),
        )
        .unwrap();
        let new = codec_new::galois_16::ReedSolomon::with_options(
            DATA,
            PARITY,
            codec_new::CodecOptions::builder()
                .matrix_mode(new_mode)
                .build(),
        )
        .unwrap();
        for length in [31, 32, 33, 1023, 1024, 1025] {
            let input: Vec<Vec<[u8; 2]>> = (0..TOTAL)
                .map(|shard| {
                    (0..length)
                        .map(|i| {
                            [
                                (shard * 131 + i * 17 + 7) as u8,
                                (shard * 41 + i * 29 + 13) as u8,
                            ]
                        })
                        .collect()
                })
                .collect();
            let mut before = input.clone();
            let mut after = input;
            old.encode(&mut before).unwrap();
            new.encode(&mut after).unwrap();
            assert_eq!(after, before, "{name}, length {length}");

            // Retain only data 1 and parity 254. The row beyond GF8's order
            // requires a general tower coefficient when restoring data 0.
            let sparse = |encoded: &[Vec<[u8; 2]>]| -> Vec<Option<Vec<[u8; 2]>>> {
                encoded
                    .iter()
                    .enumerate()
                    .map(|(i, bytes)| [1, TOTAL - 1].contains(&i).then(|| bytes.clone()))
                    .collect()
            };
            let mut new_from_old = sparse(&before);
            let mut old_from_new = sparse(&after);
            new.reconstruct_data(&mut new_from_old).unwrap();
            old.reconstruct_data(&mut old_from_new).unwrap();
            for slots in [&new_from_old, &old_from_new] {
                assert_eq!(slots[0].as_ref(), Some(&before[0]));
                assert_eq!(slots[1].as_ref(), Some(&before[1]));
                assert_eq!(slots[TOTAL - 1].as_ref(), Some(&before[TOTAL - 1]));
                assert!(slots[DATA..TOTAL - 1].iter().all(Option::is_none));
            }
            new.reconstruct(&mut new_from_old).unwrap();
            old.reconstruct(&mut old_from_new).unwrap();
            for slots in [&new_from_old, &old_from_new] {
                assert!(
                    slots
                        .iter()
                        .zip(&before)
                        .all(|(got, expected)| got.as_ref() == Some(expected))
                );
            }
        }
    }
    println!(
        "12 wide GF16 configurations: published 9.0.0 exact parity and bidirectional data-only/full recovery passed"
    );
}
