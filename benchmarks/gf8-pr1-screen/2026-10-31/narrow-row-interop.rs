use rustfs_erasure_codec::{CodecOptions, MatrixMode, galois_8::ReedSolomon};

#[test]
fn single_missing_data_roundtrips_at_narrow_boundaries() {
    for mode in [MatrixMode::Vandermonde, MatrixMode::Cauchy, MatrixMode::JerasureLike] {
        let codec = ReedSolomon::with_options(10, 4, CodecOptions::builder().matrix_mode(mode).build()).unwrap();
        for len in [1_048_575, 1_048_576, 1_048_577] {
            let mut original: Vec<Vec<u8>> = (0..14).map(|row| {
                (0..len).map(|i| (row * 73 + i * 19 + i / 17) as u8).collect()
            }).collect();
            codec.encode(&mut original).unwrap();
            for missing in [0, 4, 9] {
                let mut shards: Vec<Option<Vec<u8>>> = original.iter().cloned().map(Some).collect();
                shards[missing] = None;
                codec.reconstruct_data_opt(&mut shards).unwrap();
                for (actual, expected) in shards.iter().zip(&original) {
                    assert_eq!(actual.as_ref().unwrap(), expected, "mode={mode:?} len={len} missing={missing}");
                }
            }
        }
    }
}
