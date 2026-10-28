//! Targeted Leopard recovery cases for controlled baseline/candidate comparison.
//! The caller must record source identity and establish resource isolation.

use criterion::{BatchSize, BenchmarkId, Criterion, criterion_group, criterion_main};
use rustfs_erasure_codec::{CodecFamily, CodecOptions, ShardSlot, galois_8::ReedSolomon};
use std::hint::black_box;

const CHUNK_BYTES_GF16: usize = 64 * 1024;
// Keep in sync with the GF16 decoder's per-thread retained-buffer limit.
const GF16_CACHE_LIMIT_PER_WORKER: usize = 4 * 1024 * 1024;

fn configured_warm_workers() -> Option<usize> {
    match std::env::var("RSE_LEOPARD_WARM_ALL_WORKERS") {
        Err(std::env::VarError::NotPresent) => None,
        Ok(value) if value == "0" => None,
        Ok(value) if value == "1" => {
            let workers = std::env::var("RAYON_NUM_THREADS")
                .expect("all-worker warmup requires explicit RAYON_NUM_THREADS")
                .parse::<usize>()
                .ok()
                .filter(|&count| count > 0)
                .expect("RAYON_NUM_THREADS must be a positive integer");
            let budget = std::env::var("RSE_LEOPARD_WARM_MAX_RETAINED_BYTES")
                .expect("all-worker warmup requires RSE_LEOPARD_WARM_MAX_RETAINED_BYTES")
                .parse::<usize>()
                .expect("RSE_LEOPARD_WARM_MAX_RETAINED_BYTES must be an integer");
            let retained_limit = workers
                .checked_mul(GF16_CACHE_LIMIT_PER_WORKER)
                .expect("worker cache limit overflows usize");
            assert!(
                retained_limit <= budget,
                "worker GF16 cache limit exceeds RSE_LEOPARD_WARM_MAX_RETAINED_BYTES"
            );
            assert_eq!(
                rayon::current_num_threads(),
                workers,
                "actual Rayon pool does not match RAYON_NUM_THREADS"
            );
            Some(workers)
        }
        Ok(_) => panic!("RSE_LEOPARD_WARM_ALL_WORKERS must be 0 or 1"),
        Err(error) => panic!("cannot read RSE_LEOPARD_WARM_ALL_WORKERS: {error}"),
    }
}

fn warm_all_workers(data: usize, parity: usize, missing: usize, workers: usize) {
    let codec = ReedSolomon::with_options(
        data,
        parity,
        CodecOptions::builder()
            .codec_family(CodecFamily::LeopardGF16)
            .max_parallel_jobs(1)
            .build(),
    )
    .expect("construct worker warmup codec");
    let original = fixture(&codec, CHUNK_BYTES_GF16);
    let mut seen = rayon::broadcast(|context| {
        assert_eq!(context.num_threads(), workers);
        let mut shards: Vec<_> = original
            .iter()
            .cloned()
            .map(ShardSlot::new_present)
            .collect();
        for slot in &mut shards[..missing] {
            slot.mark_missing();
            slot.as_inner_mut().fill(0xa5);
        }
        codec.reconstruct(&mut shards).expect("worker warmup");
        assert!(
            shards
                .iter()
                .zip(&original)
                .all(|(slot, expected)| slot.is_present() && slot.as_inner() == expected)
        );
        context.index()
    });
    seen.sort_unstable();
    assert_eq!(seen, (0..workers).collect::<Vec<_>>());
    eprintln!(
        "RSE_GF16_WORKER_WARMUP data={data} parity={parity} missing={missing} workers={workers}"
    );
}

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
    let warm_workers = configured_warm_workers();
    let group_name = if warm_workers.is_some() {
        "leopard_recovery_comparison_all_workers_warm"
    } else {
        "leopard_recovery_comparison"
    };
    let mut group = criterion.benchmark_group(group_name);
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
                    let mut warmed = false;
                    group.bench_function(BenchmarkId::new("retained_slots", &label), |b| {
                        if !warmed {
                            if let Some(workers) = warm_workers
                                && family == CodecFamily::LeopardGF16
                                && size > CHUNK_BYTES_GF16
                            {
                                warm_all_workers(data, parity, missing, workers);
                            }
                            warmed = true;
                        }
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
                        let mut warmed = false;
                        group.bench_function(BenchmarkId::new(name, &label), |b| {
                            if !warmed {
                                if let Some(workers) = warm_workers
                                    && family == CodecFamily::LeopardGF16
                                    && size > CHUNK_BYTES_GF16
                                {
                                    warm_all_workers(data, parity, missing, workers);
                                }
                                warmed = true;
                            }
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
