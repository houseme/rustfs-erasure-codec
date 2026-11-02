use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use rustfs_erasure_codec::{CodecFamily, CodecOptions, galois_8::ReedSolomon};
use std::hint::black_box;

fn run(c: &mut Criterion) {
    if let Ok(expected) = std::env::var("RSE_EXPECT_BACKEND") {
        assert_eq!(
            rustfs_erasure_codec::galois_8::active_backend_name(),
            expected
        );
    } else if std::env::var_os("RSE_GFNI_BENCH_PRESELECT").is_some() {
        assert_eq!(
            rustfs_erasure_codec::galois_8::active_backend_name(),
            "rust-gfni-avx2"
        );
    }
    let mut group = c.benchmark_group("small_gf16_encode");
    for (data, parity) in [(4usize, 2usize), (10, 4), (16, 8), (24, 9)] {
        let codec = ReedSolomon::with_options(
            data,
            parity,
            CodecOptions::builder()
                .codec_family(CodecFamily::LeopardGF16)
                .max_parallel_jobs(1)
                .build(),
        )
        .unwrap();
        for len in [64usize, 128, 4096, 65_600, 1_048_576] {
            let mut shards: Vec<Vec<u8>> = (0..data + parity)
                .map(|i| (0..len).map(|j| (i * 31 + j * 17 + j / 11) as u8).collect())
                .collect();
            codec.encode(&mut shards).unwrap();
            assert_eq!(codec.verify(&shards), Ok(true));
            let id = format!("{data}x{parity}/{len}");
            group.bench_function(BenchmarkId::new("encode", id), |b| {
                b.iter(|| codec.encode(black_box(&mut shards)).unwrap())
            });
        }
    }
    group.finish();
}
criterion_group!(benches, run);
criterion_main!(benches);
