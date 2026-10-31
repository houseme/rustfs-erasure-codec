#![cfg(feature = "bench-internal-row")]
use rustfs_erasure_codec::{CodecOptions, MatrixMode, diagnostic_row, galois_8::ReedSolomon};

#[test]
fn retained_output_uses_actual_recovery_row_and_survivor_order() {
    for mode in [
        MatrixMode::Vandermonde,
        MatrixMode::Cauchy,
        MatrixMode::JerasureLike,
    ] {
        let codec = ReedSolomon::with_options(
            10,
            4,
            CodecOptions::builder()
                .matrix_mode(mode)
                .max_parallel_jobs(4)
                .build(),
        )
        .unwrap();
        for len in [65_539, 1_048_577] {
            let mut original: Vec<Vec<u8>> = (0..14)
                .map(|r| (0..len).map(|i| (r * 73 + i * 19 + i / 17) as u8).collect())
                .collect();
            codec.encode(&mut original).unwrap();
            for missing in [0, 3, 9] {
                let mut shards: Vec<Option<Vec<u8>>> = original.iter().cloned().map(Some).collect();
                shards[missing] = None;
                let (indices, row) = diagnostic_row::recovery_row(&codec, &mut shards).unwrap();
                let inputs: Vec<&[u8]> = indices.iter().map(|&i| original[i].as_slice()).collect();
                for jobs in [1, 4] {
                    let mut output = vec![0xa5; len];
                    diagnostic_row::run_row(&codec, &row, &inputs, &mut output, 65_536, jobs);
                    assert_eq!(
                        output, original[missing],
                        "mode={mode:?} len={len} missing={missing} jobs={jobs}"
                    );
                }
            }
        }
    }
}
