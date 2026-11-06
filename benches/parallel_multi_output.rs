//! Compare bounded parallel coding with the multi-output feature off and on.
//! Fixtures, reference parity and Rayon pools are prepared outside measurement.

mod common;

use std::hint::black_box;
use std::time::{Duration, Instant};

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use rustfs_erasure_codec::{CodecOptions, galois_8::ReedSolomon};

fn parallel_multi_output(c: &mut Criterion) {
    println!(
        "parallel_multi_output revision={} backend={} features={} experimental={} override={}",
        common::git_revision(),
        common::backend(),
        common::features(),
        cfg!(feature = "experimental-parallel-multi-output"),
        common::backend_override(),
    );
    for jobs in [2, 4] {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(jobs)
            .build()
            .unwrap();
        let mut group = c.benchmark_group(format!("parallel_multi_output/jobs{jobs}"));
        group.sample_size(20);
        group.warm_up_time(Duration::from_secs(1));
        group.measurement_time(Duration::from_secs(3));
        for (data_count, parity_count) in [(4, 2), (6, 3), (8, 3), (8, 4), (10, 4), (12, 4)] {
            for len in [256 * 1024 + 1, 1024 * 1024 + 63] {
                let codec = ReedSolomon::with_options(
                    data_count,
                    parity_count,
                    CodecOptions::builder().max_parallel_jobs(jobs).build(),
                )
                .unwrap();
                println!(
                    "shape={data_count}+{parity_count} len={len} requested_jobs={jobs} policy={:?} decision={:?}",
                    codec.effective_parallel_policy(),
                    codec.parallel_policy(len, parity_count),
                );
                let inputs = common::make_data_shards(common::BASE_SEED, data_count, len);
                let mut expected = common::make_empty_parity_shards(parity_count, len);
                for (index, input) in inputs.iter().enumerate() {
                    codec
                        .encode_single_sep(index, input, &mut expected)
                        .unwrap();
                }
                let mut outputs = common::make_empty_parity_shards(parity_count, len);
                pool.install(|| codec.encode_sep_par(&inputs, &mut outputs).unwrap());
                assert_eq!(outputs, expected);
                group.throughput(Throughput::Bytes((data_count * len) as u64));
                group.bench_function(
                    BenchmarkId::new(format!("{data_count}+{parity_count}"), len),
                    |b| {
                        b.iter_custom(|iterations| {
                            pool.install(|| {
                                let start = Instant::now();
                                for _ in 0..iterations {
                                    codec
                                        .encode_sep_par(black_box(&inputs), black_box(&mut outputs))
                                        .unwrap();
                                }
                                start.elapsed()
                            })
                        });
                    },
                );
                assert_eq!(outputs, expected);
            }
        }
        group.finish();
    }
}

criterion_group!(benches, parallel_multi_output);
criterion_main!(benches);
