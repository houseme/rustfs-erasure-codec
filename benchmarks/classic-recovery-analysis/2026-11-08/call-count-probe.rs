use codec::{CodecOptions, galois_8::ReedSolomon};
use rustfs_erasure_codec as codec;

fn main() {
    for &(data, parity, len) in &[(10, 4, 65_536), (32, 16, 1_048_576)] {
        let codec = ReedSolomon::with_options(
            data,
            parity,
            CodecOptions::builder().max_parallel_jobs(1).build(),
        )
        .unwrap();
        let mut original: Vec<Vec<u8>> = (0..data + parity)
            .map(|i| {
                (0..len)
                    .map(|j| (i * 31 + j * 17 + j / 251) as u8)
                    .collect()
            })
            .collect();
        codec.encode(&mut original).unwrap();
        let mut generic: Vec<_> = original.iter().cloned().map(Some).collect();
        for i in [0, 2] {
            generic[i] = None;
        }
        let mut bulk = generic.clone();
        codec.reconstruct_data(&mut generic).unwrap();
        codec.reconstruct_data_opt(&mut bulk).unwrap();
        for i in [0, 2] {
            generic[i] = None;
            bulk[i] = None;
        }
        codec.reset_runtime_profile_stats();
        codec.reconstruct_data(&mut generic).unwrap();
        let one = codec.runtime_profile_stats();
        codec.reset_runtime_profile_stats();
        codec.reconstruct_data_opt(&mut bulk).unwrap();
        let two = codec.runtime_profile_stats();
        assert_eq!(generic, bulk);
        for i in [0, 2] {
            assert_eq!(generic[i].as_ref().unwrap(), &original[i]);
        }
        println!(
            "{data}+{parity}/{len}: generic serial calls={} bulk serial calls={}",
            one.code_some_serial_calls, two.code_some_serial_calls
        );
        assert_eq!(one.code_some_serial_calls, 2);
        assert_eq!(two.code_some_serial_calls, 1);
    }
}
