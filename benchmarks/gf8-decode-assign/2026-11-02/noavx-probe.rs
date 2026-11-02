use rustfs_erasure_codec::{CodecFamily, CodecOptions, ShardSlot, galois_8::ReedSolomon};
fn main() {
    assert!(!std::is_x86_feature_detected!("avx2"), "probe requires CPU without AVX2");
    let mut cases = 0usize;
    for (data, parity) in [(4usize, 2usize), (10, 4), (16, 8), (24, 9)] {
        let codec = ReedSolomon::with_options(data, parity, CodecOptions::builder().codec_family(CodecFamily::LeopardGF8).max_parallel_jobs(1).build()).unwrap();
        for len in [64usize, 128, 192, 65_600] {
            let mut source: Vec<Vec<u8>> = (0..data + parity).map(|i| (0..len).map(|j| (i * 31 + j * 17 + j / 11) as u8).collect()).collect();
            codec.encode(&mut source).unwrap();
            for missing in [vec![0], vec![data], vec![0, data]] {
                let mut slots: Vec<_> = source.iter().cloned().map(ShardSlot::new_present).collect();
                for index in missing { slots[index].mark_missing(); }
                codec.reconstruct(&mut slots).unwrap();
                assert!(slots.iter().zip(&source).all(|(s, x)| s.is_present() && s.as_inner() == x));
                cases += 1;
            }
        }
    }
    println!("non-AVX2 GF8 reconstruction: {cases} cases passed");
}
