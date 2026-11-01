use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use rustfs_erasure_codec::{CodecFamily, CodecOptions, ShardSlot, galois_8::ReedSolomon};
use std::hint::black_box;

fn patterns(total: usize, missing: usize, limit: usize) -> Vec<Vec<usize>> {
    let mut values: Vec<_> = (0..missing).collect();
    let mut result = Vec::new();
    loop {
        result.push(values.clone());
        if result.len() == limit {
            break;
        }
        let Some(i) = (0..missing)
            .rev()
            .find(|&i| values[i] < total - missing + i)
        else {
            break;
        };
        values[i] += 1;
        for j in i + 1..missing {
            values[j] = values[j - 1] + 1;
        }
    }
    result
}
fn run(c: &mut Criterion) {
    if std::env::var_os("RSE_GFNI_BENCH_PRESELECT").is_some() {
        assert_eq!(
            rustfs_erasure_codec::galois_8::active_backend_name(),
            "rust-gfni-avx2"
        );
    }
    let mut group = c.benchmark_group("std_gf16_reconstruct");
    for (data, parity, missing) in [(10, 4, 4), (16, 8, 1), (16, 8, 4)] {
        let codec = ReedSolomon::with_options(
            data,
            parity,
            CodecOptions::builder()
                .codec_family(CodecFamily::LeopardGF16)
                .max_parallel_jobs(1)
                .build(),
        )
        .unwrap();
        let erasures = patterns(data + parity, missing, 32);
        assert!(erasures.len() > 16);
        for size in [64usize, 1024, 2048, 4096, 1048576] {
            let mut source: Vec<Vec<u8>> = (0..data + parity)
                .map(|i| (0..size).map(|j| (i * 31 + j * 17) as u8).collect())
                .collect();
            codec.encode(&mut source).unwrap();
            for cold in [true, false] {
                let mut slots: Vec<_> =
                    source.iter().cloned().map(ShardSlot::new_present).collect();
                for mask in &erasures {
                    for &i in mask {
                        slots[i].mark_missing();
                    }
                    codec.reconstruct(&mut slots).unwrap();
                    assert!(
                        slots
                            .iter()
                            .zip(&source)
                            .all(|(s, x)| s.is_present() && s.as_inner() == x)
                    );
                }
                let mut next = 0;
                let label = format!(
                    "{data}x{parity}/{size}/missing{missing}/{}",
                    if cold { "cold" } else { "warm" }
                );
                group.bench_function(BenchmarkId::new("slots", label), |b| {
                    b.iter(|| {
                        let mask = if cold {
                            let m = &erasures[next];
                            next = (next + 1) % erasures.len();
                            m
                        } else {
                            &erasures[0]
                        };
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
                        .all(|(s, x)| s.is_present() && s.as_inner() == x)
                );
            }
        }
    }
    group.finish();
    let mut group = c.benchmark_group("std_gf16_encode");
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
        for size in [64usize, 65536, 1048576] {
            let mut shards: Vec<Vec<u8>> = (0..data + parity)
                .map(|i| (0..size).map(|j| (i * 31 + j * 17) as u8).collect())
                .collect();
            codec.encode(&mut shards).unwrap();
            assert!(codec.verify(&shards).unwrap());
            let label = format!("{data}x{parity}/{size}");
            group.bench_function(BenchmarkId::new("encode", label), |b| {
                b.iter(|| codec.encode(black_box(&mut shards)).unwrap())
            });
            assert!(codec.verify(&shards).unwrap());
        }
    }
    group.finish();
}
criterion_group!(benches, run);
criterion_main!(benches);
