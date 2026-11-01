use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use rustfs_erasure_codec::{CodecFamily, CodecOptions, galois_8::ReedSolomon};
use std::hint::black_box;

fn run(c: &mut Criterion) {
    if std::env::var_os("RSE_GFNI_BENCH_PRESELECT").is_some() {
        assert_eq!(rustfs_erasure_codec::galois_8::active_backend_name(), "rust-gfni-avx2");
    }
    if let Ok(expected) = std::env::var("RSE_EXPECT_BACKEND") {
        assert_eq!(rustfs_erasure_codec::galois_8::active_backend_name(), expected);
    }
    let mut group = c.benchmark_group("leopard_verify");
    for (label, family) in [("gf8", CodecFamily::LeopardGF8), ("gf16", CodecFamily::LeopardGF16)] {
        for (data, parity) in [(10, 4), (16, 8)] {
            let codec = ReedSolomon::with_options(data, parity, CodecOptions::builder().codec_family(family).max_parallel_jobs(1).build()).unwrap();
            for size in [64usize, 128, 4096, 1048576] {
                let mut shards: Vec<Vec<u8>> = (0..data + parity).map(|i| (0..size).map(|j| (i * 31 + j * 17 + j / 11) as u8).collect()).collect();
                codec.encode(&mut shards).unwrap();
                assert_eq!(codec.verify(&shards), Ok(true));
                shards[data][size - 1] ^= 1;
                assert_eq!(codec.verify(&shards), Ok(false));
                shards[data][size - 1] ^= 1;
                let label = format!("{label}/{data}x{parity}/{size}");
                group.bench_function(BenchmarkId::new("ordinary", label), |b| b.iter(|| assert!(codec.verify(black_box(&shards)).unwrap())));
            }
        }
    }
    group.finish();
}
criterion_group!(benches, run);
criterion_main!(benches);
