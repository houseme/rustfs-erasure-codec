use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use rustfs_erasure_codec::{CodecFamily, CodecOptions, galois_8::ReedSolomon};
use std::hint::black_box;

fn run(c: &mut Criterion) {
    let mut group = c.benchmark_group("nostd_gf16_encode");
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
        for size in [64usize, 448, 512, 576, 4096, 65536, 1048576] {
            let mut shards: Vec<Vec<u8>> = (0..data + parity)
                .map(|i| {
                    (0..size)
                        .map(|j| (i * 31 + j * 17 + j / 11) as u8)
                        .collect()
                })
                .collect();
            codec.encode(&mut shards).unwrap();
            assert!(codec.verify(&shards).unwrap());
            group.bench_function(BenchmarkId::new(format!("{data}x{parity}"), size), |b| {
                b.iter(|| codec.encode(black_box(&mut shards)).unwrap())
            });
            assert!(codec.verify(&shards).unwrap());
        }
    }
    group.finish();
}
criterion_group!(benches, run);
criterion_main!(benches);
