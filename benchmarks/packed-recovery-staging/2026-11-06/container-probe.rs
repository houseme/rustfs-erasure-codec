use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use rustfs_erasure_codec::{CodecFamily, CodecOptions, galois_8::ReedSolomon};
use std::hint::black_box;

fn run(c: &mut Criterion) {
    if let Ok(expected) = std::env::var("RSE_EXPECT_BACKEND") {
        assert_eq!(
            rustfs_erasure_codec::galois_8::active_backend_name(),
            expected
        );
    }
    let mut group = c.benchmark_group("packed_public_recovery");
    for (name, family) in [
        ("gf8", CodecFamily::LeopardGF8),
        ("gf16", CodecFamily::LeopardGF16),
    ] {
        for (data, parity, len) in [(10usize, 4usize, 64usize), (16, 8, 128), (10, 4, 1_048_576)] {
            let codec = ReedSolomon::with_options(
                data,
                parity,
                CodecOptions::builder()
                    .codec_family(family)
                    .max_parallel_jobs(1)
                    .build(),
            )
            .unwrap();
            let mut source: Vec<Vec<u8>> = (0..data + parity)
                .map(|i| (0..len).map(|j| (i * 31 + j * 17 + j / 11) as u8).collect())
                .collect();
            codec.encode(&mut source).unwrap();
            let id = format!("{name}/{data}x{parity}/{len}/missing4");

            let mut tuples: Vec<_> = source.iter().cloned().map(|bytes| (bytes, true)).collect();
            for shard in &mut tuples[..4] {
                shard.1 = false;
            }
            codec.reconstruct(&mut tuples).unwrap();
            assert!(
                tuples
                    .iter()
                    .zip(&source)
                    .all(|((bytes, present), expected)| *present && bytes == expected)
            );
            group.bench_function(BenchmarkId::new("tuple", &id), |b| {
                b.iter(|| {
                    for shard in &mut tuples[..4] {
                        shard.1 = false;
                    }
                    codec.reconstruct(black_box(&mut tuples)).unwrap();
                })
            });

            let mut options: Vec<_> = source.iter().cloned().map(Some).collect();
            for shard in &mut options[..4] {
                *shard = None;
            }
            codec.reconstruct(&mut options).unwrap();
            assert!(
                options
                    .iter()
                    .zip(&source)
                    .all(|(actual, expected)| actual.as_ref() == Some(expected))
            );
            group.bench_function(BenchmarkId::new("option_generic", &id), |b| {
                b.iter(|| {
                    for shard in &mut options[..4] {
                        *shard = None;
                    }
                    codec.reconstruct(black_box(&mut options)).unwrap();
                })
            });
        }
    }
    group.finish();
}
criterion_group!(benches, run);
criterion_main!(benches);
