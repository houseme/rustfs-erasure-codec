#![cfg(feature = "std")]
use rustfs_erasure_codec::{CodecFamily, CodecOptions, VerifyWorkspace, galois_8};

#[test]
fn vec_workspace_opt_matches_generic_parallel_for_results_and_scratch() {
    for family in [
        CodecFamily::Classic,
        CodecFamily::LeopardGF8,
        CodecFamily::LeopardGF16,
    ] {
        if family == CodecFamily::LeopardGF16 && cfg!(target_endian = "big") {
            continue;
        }
        for fast in [false, true] {
            if fast && family != CodecFamily::Classic {
                continue;
            }
            let parity = if fast { 1 } else { 2 };
            let codec = galois_8::ReedSolomon::with_options(
                4,
                parity,
                CodecOptions::builder()
                    .codec_family(family)
                    .fast_one_parity(fast)
                    .max_parallel_jobs(2)
                    .build(),
            )
            .unwrap();
            for len in [64, 128, 256, 4096] {
                let mut shards: Vec<Vec<u8>> = (0..4 + parity)
                    .map(|row| (0..len).map(|i| (row * 37 + i * 13) as u8).collect())
                    .collect();
                codec.encode(&mut shards).unwrap();
                for case in 0..7 {
                    let mut input = shards.clone();
                    match case {
                        0 => (),
                        1 => input[4][0] ^= 1,
                        2 => {
                            input.pop();
                        }
                        3 => input.push(vec![0; len]),
                        4 => input[0].clear(),
                        5 => {
                            input.last_mut().unwrap().pop();
                        }
                        6 => {
                            input[4][0] ^= 1;
                            input.last_mut().unwrap().pop();
                        }
                        _ => unreachable!(),
                    }
                    let before = input.clone();
                    let mut expected = VerifyWorkspace::new(&codec, 1);
                    let mut actual = expected.clone();
                    let result_a = codec.verify_with_workspace_par(&input, &mut expected);
                    let result_b = codec.verify_with_workspace_opt(&input, &mut actual);
                    assert_eq!(
                        result_a, result_b,
                        "family={family:?} fast={fast} len={len} case={case}"
                    );
                    assert_eq!(
                        expected, actual,
                        "scratch family={family:?} fast={fast} len={len} case={case}"
                    );
                    assert_eq!(input, before);
                }
            }
        }
    }
}
