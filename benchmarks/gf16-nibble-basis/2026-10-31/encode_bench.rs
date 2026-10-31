use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use rustfs_erasure_codec::{CodecFamily, CodecOptions, galois_8::ReedSolomon};
use std::hint::black_box;

fn run(c: &mut Criterion) {
    eprintln!("RSE_BENCH_GF8_BACKEND={:?}", rustfs_erasure_codec::galois_8::active_backend_id());
    let mut group = c.benchmark_group("nibble_basis_gf16_encode");
    for (data, parity) in [(10, 4), (16, 8)] {
        for size in [64, 2048, 65536, 1048576] {
            let codec = ReedSolomon::with_options(data, parity, CodecOptions::builder().codec_family(CodecFamily::LeopardGF16).build()).unwrap();
            let mut shards: Vec<Vec<u8>> = (0..data + parity).map(|i| (0..size).map(|j| ((i * 53 + j * 17 + j / 11) % 256) as u8).collect()).collect();
            codec.encode_opt(&mut shards).unwrap();
            assert!(codec.verify(&shards).unwrap());
            group.bench_function(BenchmarkId::new(format!("{data}x{parity}"), size), |b| {
                b.iter(|| codec.encode_opt(black_box(&mut shards)).unwrap());
            });
            assert!(codec.verify(&shards).unwrap());
        }
    }
    group.finish();
}
criterion_group!(benches, run);
criterion_main!(benches);
