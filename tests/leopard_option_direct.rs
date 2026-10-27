#![cfg(feature = "std")]
use rustfs_erasure_codec::{CodecFamily, CodecOptions, galois_8::ReedSolomon};

#[cfg(target_endian = "little")]
const FAMILIES: &[CodecFamily] = &[CodecFamily::LeopardGF8, CodecFamily::LeopardGF16];
#[cfg(target_endian = "big")]
const FAMILIES: &[CodecFamily] = &[CodecFamily::LeopardGF8];

#[test]
fn option_entrypoints_match_generic_leopard_across_masks() {
    for &family in FAMILIES {
        for len in [64, 128, 65_600] {
            let codec = ReedSolomon::with_options(
                4,
                3,
                CodecOptions::builder()
                    .codec_family(family)
                    .max_parallel_jobs(2)
                    .build(),
            )
            .unwrap();
            let mut original: Vec<Vec<u8>> = (0..7)
                .map(|i| (0..len).map(|j| (i * 31 + j * 17) as u8).collect())
                .collect();
            codec.encode(&mut original).unwrap();
            for missing_mask in 0u8..128 {
                if len > 128 && ![0, 1, 3, 17, 65, 127].contains(&missing_mask) {
                    continue;
                }
                let shards: Vec<_> = original
                    .iter()
                    .enumerate()
                    .map(|(i, v)| {
                        if missing_mask & (1 << i) != 0 {
                            None
                        } else {
                            Some(v.clone())
                        }
                    })
                    .collect();
                for goal in 0..3 {
                    let required = [true, false, true, false, true, false, true];
                    let mut old = shards.clone();
                    let mut new = shards.clone();
                    let a = match goal {
                        0 => codec.reconstruct(&mut old),
                        1 => codec.reconstruct_data(&mut old),
                        _ => codec.reconstruct_some(&mut old, &required),
                    };
                    let b = match goal {
                        0 => codec.reconstruct_opt(&mut new),
                        1 => codec.reconstruct_data_opt(&mut new),
                        _ => codec.reconstruct_some_opt(&mut new, &required),
                    };
                    assert_eq!(a, b, "{family:?} {len} {missing_mask} {goal}");
                    assert_eq!(old, new, "{family:?} {len} {missing_mask} {goal}");
                }
            }
        }
    }
}

#[test]
fn option_entrypoints_preserve_invalid_input_errors_and_state() {
    for &family in FAMILIES {
        let codec =
            ReedSolomon::with_options(4, 3, CodecOptions::builder().codec_family(family).build())
                .unwrap();
        for len in [0, 63, 64, 129, 192] {
            let mut before = vec![Some(vec![0; 128]); 7];
            before[0] = None;
            before[2] = Some(vec![0; len]);
            let mut generic = before.clone();
            let mut optimized = before.clone();
            assert_eq!(
                codec.reconstruct(&mut generic),
                codec.reconstruct_opt(&mut optimized)
            );
            assert_eq!(generic, optimized);
        }
        for count in [0, 6, 8] {
            let mut generic = vec![Some(vec![0; 128]); count];
            let mut optimized = generic.clone();
            assert_eq!(
                codec.reconstruct(&mut generic),
                codec.reconstruct_opt(&mut optimized)
            );
            assert_eq!(generic, optimized);
        }
    }
}

#[test]
fn selective_leopard_recovery_matches_original_for_all_small_masks() {
    for &family in FAMILIES {
        for (len, jobs) in [(64, 1), (65_600, 1), (65_600, 2)] {
            let codec = ReedSolomon::with_options(
                4,
                3,
                CodecOptions::builder()
                    .codec_family(family)
                    .max_parallel_jobs(jobs)
                    .build(),
            )
            .unwrap();
            let mut original: Vec<Vec<u8>> = (0..7)
                .map(|i| (0..len).map(|j| (i * 31 + j * 17 + j / 11) as u8).collect())
                .collect();
            codec.encode(&mut original).unwrap();
            for missing in 0u8..128 {
                if missing.count_ones() > 3 || (len > 64 && ![1, 3, 17, 65, 81].contains(&missing))
                {
                    continue;
                }
                // Each subset of the missing mask is a distinct selection;
                // alternating present flags also checks requests for existing shards.
                let mut selection = missing;
                loop {
                    let request = selection | (!missing & 0x55);
                    let required: Vec<bool> = (0..7).map(|i| request & (1 << i) != 0).collect();
                    for optimized in [false, true] {
                        let mut shards: Vec<_> = original
                            .iter()
                            .enumerate()
                            .map(|(i, data)| (missing & (1 << i) == 0).then(|| data.clone()))
                            .collect();
                        if optimized {
                            codec.reconstruct_some_opt(&mut shards, &required).unwrap();
                        } else {
                            codec.reconstruct_some(&mut shards, &required).unwrap();
                        }
                        for (i, actual) in shards.iter().enumerate() {
                            let expected = if missing & (1 << i) != 0 && request & (1 << i) == 0 {
                                None
                            } else {
                                Some(&original[i])
                            };
                            assert_eq!(
                                actual.as_ref(),
                                expected,
                                "{family:?} len={len} jobs={jobs} missing={missing:#09b} required={request:#09b} optimized={optimized} index={i}"
                            );
                        }
                    }
                    if selection == 0 {
                        break;
                    }
                    selection = (selection - 1) & missing;
                }
            }
        }
    }
}
