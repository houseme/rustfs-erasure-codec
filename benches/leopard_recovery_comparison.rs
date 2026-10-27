//! Targeted Leopard recovery cases for controlled baseline/candidate comparison.
//! The caller must record source identity and establish resource isolation.

use criterion::{BatchSize, BenchmarkId, Criterion, criterion_group, criterion_main};
use rustfs_erasure_codec::{CodecFamily, CodecOptions, ShardSlot, galois_8::ReedSolomon};
use std::hint::black_box;

fn fixture(codec: &ReedSolomon, len: usize) -> Vec<Vec<u8>> {
    let mut shards: Vec<Vec<u8>> = (0..codec.total_shard_count())
        .map(|shard| {
            (0..len)
                .map(|offset| ((shard * 53 + offset * 17 + offset / 11) % 256) as u8)
                .collect()
        })
        .collect();
    codec.encode(&mut shards).expect("encode fixture");
    shards
}

fn run(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("leopard_recovery_comparison");
    for (family, name) in [
        (CodecFamily::LeopardGF8, "gf8"),
        (CodecFamily::LeopardGF16, "gf16"),
    ] {
        for (data, parity) in [(10, 4), (16, 8)] {
            let codec = ReedSolomon::with_options(
                data,
                parity,
                CodecOptions::builder().codec_family(family).build(),
            )
            .expect("construct codec");
            for size in [65_536, 1_048_576] {
                let original = fixture(&codec, size);
                for missing in 1..=4 {
                    let label = format!("{name}/{data}x{parity}/{size}/missing{missing}");
                    let mut slots: Vec<_> = original
                        .iter()
                        .cloned()
                        .map(ShardSlot::new_present)
                        .collect();
                    for slot in &mut slots[..missing] {
                        slot.mark_missing();
                        slot.as_inner_mut().fill(0xa5);
                    }
                    codec
                        .reconstruct(&mut slots)
                        .expect("fixed-buffer preflight");
                    assert!(
                        slots.iter().zip(&original).all(
                            |(slot, expected)| slot.is_present() && slot.as_inner() == expected
                        )
                    );
                    group.bench_function(BenchmarkId::new("retained_slots", &label), |b| {
                        b.iter(|| {
                            for slot in &mut slots[..missing] {
                                slot.mark_missing();
                            }
                            codec
                                .reconstruct(black_box(&mut slots))
                                .expect("reconstruct");
                        });
                    });
                    assert!(
                        slots.iter().zip(&original).all(
                            |(slot, expected)| slot.is_present() && slot.as_inner() == expected
                        )
                    );

                    for optimized in [false, true] {
                        let name = if optimized {
                            "option_opt"
                        } else {
                            "option_generic"
                        };
                        let make = || {
                            original
                                .iter()
                                .enumerate()
                                .map(|(i, shard)| {
                                    if i < missing {
                                        None
                                    } else {
                                        Some(shard.clone())
                                    }
                                })
                                .collect::<Vec<_>>()
                        };
                        let mut preflight = make();
                        if optimized {
                            codec.reconstruct_opt(&mut preflight)
                        } else {
                            codec.reconstruct(&mut preflight)
                        }
                        .expect("option preflight");
                        assert!(
                            preflight
                                .iter()
                                .zip(&original)
                                .all(|(actual, expected)| actual.as_ref() == Some(expected))
                        );
                        group.bench_function(BenchmarkId::new(name, &label), |b| {
                            b.iter_batched(
                                &make,
                                |mut shards| {
                                    if optimized {
                                        codec.reconstruct_opt(black_box(&mut shards))
                                    } else {
                                        codec.reconstruct(black_box(&mut shards))
                                    }
                                    .expect("reconstruct");
                                    black_box(shards)
                                },
                                BatchSize::PerIteration,
                            );
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
