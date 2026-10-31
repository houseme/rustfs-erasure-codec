use rustfs_erasure_codec::{CodecOptions, MatrixMode, galois_8::ReedSolomon};

#[test]
fn generated_encode_matches_incremental_reference_and_reconstructs() {
    for (data_count, parity_count) in [(4, 2), (10, 4), (12, 4)] {
        for mode in [MatrixMode::Vandermonde, MatrixMode::Cauchy, MatrixMode::JerasureLike] {
            let codec = ReedSolomon::with_options(data_count, parity_count, CodecOptions::builder().matrix_mode(mode).max_parallel_jobs(4).build()).unwrap();
            for len in [256, 32767, 32768, 32769] {
                let data: Vec<Vec<u8>> = (0..data_count).map(|row| (0..len).map(|i| (row * 73 + i * 19 + i / 17) as u8).collect()).collect();
                let mut reference = vec![vec![0; len]; parity_count];
                for (index, shard) in data.iter().enumerate() { codec.encode_single_sep(index, shard, &mut reference).unwrap(); }
                let mut serial = vec![vec![0xa5; len]; parity_count];
                codec.encode_sep(&data, &mut serial).unwrap();
                assert_eq!(serial, reference, "serial shape={data_count}+{parity_count} mode={mode:?} len={len}");
                let mut parallel = vec![vec![0x5a; len]; parity_count];
                codec.encode_sep_par(&data, &mut parallel).unwrap();
                assert_eq!(parallel, reference, "parallel shape={data_count}+{parity_count} mode={mode:?} len={len}");
                let mut shards: Vec<Option<Vec<u8>>> = data.iter().chain(&reference).cloned().map(Some).collect();
                shards[data_count / 2] = None;
                codec.reconstruct_data_opt(&mut shards).unwrap();
                assert_eq!(shards[data_count / 2].as_ref().unwrap(), &data[data_count / 2]);
            }
        }
    }
}
