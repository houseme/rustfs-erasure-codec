use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use rustfs_erasure_codec::{CodecFamily, CodecOptions, ShardSlot, galois_8::ReedSolomon};
use std::hint::black_box;

fn run(c: &mut Criterion) {
    let mut group = c.benchmark_group("nostd_gf16_reconstruct");
    for (data, parity) in [(10, 4), (16, 8)] {
        let codec = ReedSolomon::with_options(
            data,
            parity,
            CodecOptions::builder()
                .codec_family(CodecFamily::LeopardGF16)
                .max_parallel_jobs(1)
                .build(),
        )
        .unwrap();
        for size in [64usize, 4096, 65536, 1048576] {
            let mut source: Vec<Vec<u8>> = (0..data + parity)
                .map(|i| (0..size).map(|j| (i * 31 + j * 17) as u8).collect())
                .collect();
            codec.encode(&mut source).unwrap();
            for missing in [1, 4] {
                let mut slots: Vec<_> =
                    source.iter().cloned().map(ShardSlot::new_present).collect();
                let recover = |slots: &mut Vec<ShardSlot<Vec<u8>>>| {
                    for slot in &mut slots[..missing] {
                        slot.mark_missing();
                    }
                    codec.reconstruct(black_box(slots)).unwrap();
                };
                recover(&mut slots);
                assert!(
                    slots
                        .iter()
                        .zip(&source)
                        .all(|(s, x)| s.is_present() && s.as_inner() == x)
                );
                group.bench_function(
                    BenchmarkId::new(
                        format!("{data}x{parity}"),
                        format!("{size}/missing{missing}"),
                    ),
                    |b| b.iter(|| recover(&mut slots)),
                );
                assert!(
                    slots
                        .iter()
                        .zip(&source)
                        .all(|(s, x)| s.is_present() && s.as_inner() == x)
                );
            }
        }
    }
    group.finish();
}
criterion_group!(benches, run);
criterion_main!(benches);
