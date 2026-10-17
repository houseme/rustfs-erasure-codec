#![cfg(feature = "std")]

#[path = "../fuzz/codec_contracts.rs"]
mod codec_contracts;

use codec_contracts::exercise_codec_contracts;

#[test]
fn every_family_matrix_fast_mode_and_encode_recovery_api_obeys_contracts() {
    for family in 0..3 {
        for matrix in 0..4 {
            for fast in 0..2 {
                for encode_api in 0..4 {
                    for recovery_api in 0..7 {
                        let mut header = [0u8; 16];
                        header[0] = family;
                        header[1] = matrix;
                        header[2] = fast;
                        header[3] = 3;
                        header[4] = if fast == 1 { 0 } else { 2 };
                        header[5] = (encode_api + recovery_api) % 6;
                        header[6] = encode_api;
                        header[7] = recovery_api;
                        header[8] = header[4] + 1;
                        header[9] = 3;
                        header[10] = 0x55;
                        header[11] = 0x05;
                        header[12] = (encode_api + recovery_api) & 1;
                        header[15] = 37;
                        exercise_codec_contracts(&header);
                        if fast == 1 {
                            // The same flag is intentionally ignored with more
                            // than one parity shard; cover that contract too.
                            header[4] = 2;
                            header[8] = 3;
                            exercise_codec_contracts(&header);
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn malformed_lengths_counts_flags_and_workspace_mismatches_reach_each_api() {
    for family in 0..3 {
        for scenario in 1..10 {
            for recovery_api in 0..7 {
                let mut header = [0u8; 16];
                header[0] = family;
                header[1] = scenario % 4;
                header[2] = scenario & 1;
                header[3] = 3;
                header[4] = if scenario & 1 == 0 { 2 } else { 0 };
                header[5] = 2;
                header[6] = (scenario + recovery_api) % 4;
                header[7] = recovery_api;
                header[8] = header[4] + 1;
                header[9] = 1;
                header[10] = 0xff;
                header[11] = 0xff;
                header[12] = 1;
                header[13] = scenario;
                header[14] = 1;
                header[15] = 19;
                exercise_codec_contracts(&header);
            }
        }
    }
}

#[test]
fn empty_short_zero_and_extreme_inputs_always_exercise_bounded_cases() {
    exercise_codec_contracts(&[]);
    for len in 1..=24 {
        for fill in [0, 1, 127, 255] {
            exercise_codec_contracts(&vec![fill; len]);
        }
    }
    // Explicit all-present and all-zero valid codewords protect against the
    // former invalid assumption that zeroing a shard must fail verification.
    for family in 0..3 {
        for k in [0, 7] {
            for p in [0, 3] {
                for missing in [0, p + 1] {
                    let mut header = [0u8; 16];
                    header[0] = family;
                    header[3] = k;
                    header[4] = p;
                    header[8] = missing;
                    exercise_codec_contracts(&header);
                }
            }
        }
    }
}
