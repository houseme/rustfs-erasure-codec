use criterion::{BatchSize, BenchmarkId, Criterion, criterion_group, criterion_main};
use rustfs_erasure_codec::{CodecFamily, CodecOptions, galois_8::ReedSolomon};
use std::hint::black_box;

fn run(c: &mut Criterion) {
    let mut group = c.benchmark_group("nostd_gf16_cold_encode");
    for (data, parity) in [(10, 4), (16, 8)] {
        let codec = ReedSolomon::with_options(data, parity, CodecOptions::builder().codec_family(CodecFamily::LeopardGF16).max_parallel_jobs(1).build()).unwrap();
        for size in [64usize, 4096, 1048576] {
            let mut fixture: Vec<Vec<u8>> = (0..data+parity).map(|i| (0..size).map(|j| (i*31+j*17+j/11) as u8).collect()).collect();
            codec.encode(&mut fixture).unwrap(); assert!(codec.verify(&fixture).unwrap());
            let cold_check = codec.clone(); let mut checked = fixture.clone();
            cold_check.encode(&mut checked).unwrap(); assert_eq!(checked, fixture);
            group.bench_function(BenchmarkId::new(format!("{data}x{parity}"), size), |b| {
                b.iter_batched(|| (codec.clone(),fixture.clone()), |(cold, mut shards)| cold.encode(black_box(&mut shards)).unwrap(), BatchSize::SmallInput)
            });
        }
    }
    group.finish();
}
criterion_group!(benches,run);
criterion_main!(benches);
