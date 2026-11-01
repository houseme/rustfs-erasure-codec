use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use rustfs_erasure_codec::{CodecFamily, CodecOptions, ShardSlot, galois_8::ReedSolomon};
use std::hint::black_box;

fn run(c: &mut Criterion) {
    if let Some(expected) = std::env::var_os("RSE_EXPECT_BACKEND") {
        assert_eq!(
            rustfs_erasure_codec::galois_8::active_backend_name(),
            expected.to_str().unwrap()
        );
    }
    let mut group = c.benchmark_group("std_gf16_sparse_boundary_cold");
    for (name, data, parity, missing) in [
        ("work256_terms16", 224usize, 17usize, 1usize),
        ("work256_terms17", 224, 17, 2),
        ("work257_terms16", 225, 17, 1),
        ("work48_terms16", 32, 16, 16),
    ] {
        let codec = ReedSolomon::with_options(
            data,
            parity,
            CodecOptions::builder()
                .codec_family(CodecFamily::LeopardGF16)
                .max_parallel_jobs(1)
                .build(),
        )
        .unwrap();
        let masks: Vec<Vec<usize>> = (0..32)
            .map(|start| (0..missing).map(|j| (start + j) % data).collect())
            .collect();
        for size in [64usize, 4096] {
            let mut source: Vec<Vec<u8>> = (0..data + parity)
                .map(|i| {
                    (0..size)
                        .map(|j| (i * 31 + j * 17 + j / 11) as u8)
                        .collect()
                })
                .collect();
            codec.encode(&mut source).unwrap();
            let mut slots: Vec<_> = source.iter().cloned().map(ShardSlot::new_present).collect();
            for mask in &masks {
                for &i in mask {
                    slots[i].mark_missing();
                }
                codec.reconstruct(&mut slots).unwrap();
                assert!(
                    slots
                        .iter()
                        .zip(&source)
                        .all(|(slot, expected)| slot.is_present() && slot.as_inner() == expected)
                );
            }
            let mut next = 0usize;
            group.bench_function(BenchmarkId::new(name, size), |b| {
                b.iter(|| {
                    let mask = &masks[next];
                    next = (next + 1) % masks.len();
                    for &i in mask {
                        slots[i].mark_missing();
                    }
                    codec.reconstruct(black_box(&mut slots)).unwrap();
                })
            });
            assert!(
                slots
                    .iter()
                    .zip(&source)
                    .all(|(slot, expected)| slot.is_present() && slot.as_inner() == expected)
            );
        }
    }
    group.finish();
}
criterion_group!(benches, run);
criterion_main!(benches);
