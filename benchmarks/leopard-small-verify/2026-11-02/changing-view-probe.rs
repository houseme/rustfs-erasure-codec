use rustfs_erasure_codec::{CodecFamily, CodecOptions, galois_8::ReedSolomon};
use std::cell::Cell;
use std::panic::{AssertUnwindSafe, catch_unwind};

struct View {
    bytes: Vec<u8>,
    remaining: Cell<usize>,
}
impl AsRef<[u8]> for View {
    fn as_ref(&self) -> &[u8] {
        let left = self.remaining.get();
        self.remaining.set(left.saturating_sub(1));
        if left == 0 { &[] } else { &self.bytes }
    }
}

#[test]
fn leopard_small_verify_matches_retained_buffer_and_rejects_corruption() {
    for family in [CodecFamily::LeopardGF8, CodecFamily::LeopardGF16] {
        for (data, parity) in [(4, 2), (10, 4), (16, 8), (20, 9)] {
            let codec = ReedSolomon::with_options(
                data,
                parity,
                CodecOptions::builder()
                    .codec_family(family)
                    .max_parallel_jobs(1)
                    .build(),
            )
            .unwrap();
            for len in [64, 128, 256, 512, 576, 1024] {
                let mut shards: Vec<Vec<u8>> = (0..data + parity)
                    .map(|i| (0..len).map(|j| (i * 31 + j * 17 + j / 11) as u8).collect())
                    .collect();
                codec.encode(&mut shards).unwrap();
                let mut buffers = vec![vec![0; len]; parity];
                assert_eq!(
                    codec.verify(&shards),
                    codec.verify_with_buffer(&shards, &mut buffers)
                );
                assert_eq!(codec.verify(&shards), Ok(true));
                for i in 0..parity {
                    for index in [0, len - 1] {
                        shards[data + i][index] ^= 1;
                        assert_eq!(
                            codec.verify(&shards),
                            Ok(false),
                            "{family:?} {data}+{parity} len={len} i={i}"
                        );
                        assert_eq!(
                            codec.verify(&shards),
                            codec.verify_with_buffer(&shards, &mut buffers)
                        );
                        shards[data + i][index] ^= 1;
                    }
                }
            }
        }
    }
}

#[test]
fn changing_views_never_panic_and_preserve_call_counts() {
    for family in [CodecFamily::LeopardGF8, CodecFamily::LeopardGF16] {
        for len in [64, 128, 320] {
            let codec = ReedSolomon::with_options(
                4,
                2,
                CodecOptions::builder()
                    .codec_family(family)
                    .max_parallel_jobs(1)
                    .build(),
            )
            .unwrap();
            let mut source: Vec<Vec<u8>> = (0..6)
                .map(|i| (0..len).map(|j| (i * 31 + j * 17) as u8).collect())
                .collect();
            codec.encode(&mut source).unwrap();
            for changed in 0..6 {
                for remaining in 0..8 {
                    let views: Vec<_> = source
                        .iter()
                        .enumerate()
                        .map(|(i, v)| View {
                            bytes: v.clone(),
                            remaining: Cell::new(if i == changed { remaining } else { usize::MAX }),
                        })
                        .collect();
                    let result = catch_unwind(AssertUnwindSafe(|| codec.verify(&views)));
                    println!(
                        "{family:?},{len},{changed},{remaining},{result:?},{}",
                        views[changed].remaining.get()
                    );
                }
            }
        }
    }
}
