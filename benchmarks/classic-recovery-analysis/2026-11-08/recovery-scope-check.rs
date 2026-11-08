use rustfs_erasure_codec::{
    CodecOptions,
    galois_8::{ReedSolomon, active_backend_name},
};
fn main() {
    let expected_backend = std::env::var("RSE_EXPECT_BACKEND").unwrap();
    assert_eq!(active_backend_name(), expected_backend);
    let codec =
        ReedSolomon::with_options(32, 16, CodecOptions::builder().max_parallel_jobs(1).build())
            .unwrap();
    for size in [65536, 131072, 1048576] {
        let mut original: Vec<Vec<u8>> = (0..48)
            .map(|i| {
                (0..size)
                    .map(|j| (i * 31 + j * 17 + j / 251) as u8)
                    .collect()
            })
            .collect();
        codec.encode(&mut original).unwrap();
        let mut parity = vec![vec![0u8; size]; 16];
        for (i, input) in original[..32].iter().enumerate() {
            codec.encode_single_sep(i, input, &mut parity).unwrap();
        }
        assert_eq!(&original[32..], &parity);
        for missing in [2, 3, 4] {
            let mut generic: Vec<Option<Vec<u8>>> = original.iter().cloned().map(Some).collect();
            for i in 0..missing {
                generic[i * 2] = None;
            }
            let mut optimized = generic.clone();
            codec.reconstruct_data(&mut generic).unwrap();
            codec.reconstruct_data_opt(&mut optimized).unwrap();
            assert_eq!(generic, optimized);
            for (actual, expected) in optimized.iter().zip(&original) {
                assert_eq!(actual.as_ref().unwrap(), expected);
            }
        }
    }
    println!("{expected_backend}: 9 recovery fixtures and 3 encoding parity checks passed");
}
