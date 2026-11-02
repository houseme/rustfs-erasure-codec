use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use rustfs_erasure_codec::{CodecFamily, CodecOptions, ShardSlot, galois_8::ReedSolomon};
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
    let mut group = c.benchmark_group("small_leopard_recovery");
    for (family_name, family) in [
        ("gf8", CodecFamily::LeopardGF8),
        ("gf16", CodecFamily::LeopardGF16),
    ] {
        for (data, parity) in [(4usize, 2usize), (10, 4), (16, 8), (24, 9)] {
            let codec = ReedSolomon::with_options(
                data,
                parity,
                CodecOptions::builder()
                    .codec_family(family)
                    .max_parallel_jobs(1)
                    .build(),
            )
            .unwrap();
            for len in [64usize, 128, 256, 4096, 1_048_576] {
                let mut source: Vec<Vec<u8>> = (0..data + parity)
                    .map(|i| (0..len).map(|j| (i * 31 + j * 17 + j / 11) as u8).collect())
                    .collect();
                codec.encode(&mut source).unwrap();
                if data == 10 && parity == 4 && len == 64 {
                    let mut complete: Vec<_> =
                        source.iter().cloned().map(ShardSlot::new_present).collect();
                    codec.reconstruct(&mut complete).unwrap();
                    let id = format!("{family_name}/{data}x{parity}/{len}");
                    group.bench_function(BenchmarkId::new("intact", id), |b| {
                        b.iter(|| {
                            codec.reconstruct(black_box(&mut complete)).unwrap();
                        })
                    });
                }
                for missing in [1usize, 4] {
                    if missing > parity {
                        continue;
                    }
                    let mut slots: Vec<_> =
                        source.iter().cloned().map(ShardSlot::new_present).collect();
                    for slot in &mut slots[..missing] {
                        slot.mark_missing();
                    }
                    codec.reconstruct(&mut slots).unwrap();
                    assert!(
                        slots
                            .iter()
                            .zip(&source)
                            .all(|(s, x)| s.is_present() && s.as_inner() == x)
                    );
                    let id = format!("{family_name}/{data}x{parity}/{len}/missing{missing}");
                    group.bench_function(BenchmarkId::new("slots", id), |b| {
                        b.iter(|| {
                            for slot in &mut slots[..missing] {
                                slot.mark_missing();
                            }
                            codec.reconstruct(black_box(&mut slots)).unwrap();
                        })
                    });
                    #[cfg(feature = "std")]
                    if data == 10 && parity == 4 && missing == 4 && matches!(len, 64 | 1_048_576) {
                        let mut opt: Vec<_> = source.iter().cloned().map(Some).collect();
                        for shard in &mut opt[..missing] {
                            *shard = None;
                        }
                        codec.reconstruct_opt(&mut opt).unwrap();
                        assert!(opt.iter().zip(&source).all(|(s, x)| s.as_ref() == Some(x)));
                        let id = format!("{family_name}/{data}x{parity}/{len}/missing{missing}");
                        group.bench_function(BenchmarkId::new("option", id), |b| {
                            b.iter(|| {
                                for shard in &mut opt[..missing] {
                                    *shard = None;
                                }
                                codec.reconstruct_opt(black_box(&mut opt)).unwrap();
                            })
                        });
                    }
                }
            }
        }
    }
    group.finish();
}
criterion_group!(benches, run);
criterion_main!(benches);
