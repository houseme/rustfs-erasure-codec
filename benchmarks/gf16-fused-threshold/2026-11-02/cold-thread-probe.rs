use rustfs_erasure_codec::{CodecFamily, CodecOptions, ShardSlot, galois_8::ReedSolomon};
use std::{hint::black_box, time::Instant};

fn main() {
    assert_eq!(
        rustfs_erasure_codec::galois_8::active_backend_name(),
        "rust-gfni-avx2"
    );
    for (data, parity) in [(10, 4), (16, 8)] {
        for size in [1024usize, 2048] {
            let codec = ReedSolomon::with_options(
                data,
                parity,
                CodecOptions::builder()
                    .codec_family(CodecFamily::LeopardGF16)
                    .max_parallel_jobs(1)
                    .build(),
            )
            .unwrap();
            let mut source: Vec<Vec<u8>> = (0..data + parity)
                .map(|i| {
                    (0..size)
                        .map(|j| (i * 31 + j * 17 + j / 11) as u8)
                        .collect()
                })
                .collect();
            codec.encode(&mut source).unwrap();
            let mut prime: Vec<_> = source.iter().cloned().map(ShardSlot::new_present).collect();
            for slot in &mut prime[..4] {
                slot.mark_missing();
            }
            codec.reconstruct(&mut prime).unwrap();
            for _ in 0..300 {
                let (cold, warm) = std::thread::scope(|scope| {
                    scope
                        .spawn(|| {
                            let mut shards: Vec<_> =
                                source.iter().cloned().map(ShardSlot::new_present).collect();
                            for slot in &mut shards[..4] {
                                slot.mark_missing();
                            }
                            let start = Instant::now();
                            codec.reconstruct(black_box(&mut shards)).unwrap();
                            let cold = start.elapsed().as_nanos();
                            let start = Instant::now();
                            for _ in 0..16 {
                                for slot in &mut shards[..4] {
                                    slot.mark_missing();
                                }
                                codec.reconstruct(black_box(&mut shards)).unwrap();
                            }
                            let warm = start.elapsed().as_nanos();
                            assert!(
                                shards
                                    .iter()
                                    .zip(&source)
                                    .all(|(s, x)| s.is_present() && s.as_inner() == x)
                            );
                            (cold, warm)
                        })
                        .join()
                        .unwrap()
                });
                println!("{data},{parity},{size},{cold},{warm}");
            }
        }
    }
}
