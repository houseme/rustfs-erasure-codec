#![cfg(feature = "std")]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use rustfs_erasure_codec::{CodecOptions, Field, ParallelPolicy, ReedSolomon, galois_8};

static ACTIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

struct ActiveCall;

impl ActiveCall {
    fn enter() -> Self {
        let active = ACTIVE.fetch_add(1, Ordering::SeqCst) + 1;
        PEAK.fetch_max(active, Ordering::SeqCst);
        // Keep calls overlapping long enough to expose a scheduler that starts
        // one task per output despite a smaller configured job limit.
        std::thread::sleep(Duration::from_micros(100));
        Self
    }
}

impl Drop for ActiveCall {
    fn drop(&mut self) {
        ACTIVE.fetch_sub(1, Ordering::SeqCst);
    }
}

// Retain default specialization hooks so every arithmetic callback remains
// observable even in builds with generated SIMD enabled.
#[derive(Debug)]
struct TrackingField;

impl Field for TrackingField {
    const ORDER: usize = 256;
    type Elem = u8;

    fn add(a: u8, b: u8) -> u8 {
        galois_8::Field::add(a, b)
    }
    fn mul(a: u8, b: u8) -> u8 {
        galois_8::Field::mul(a, b)
    }
    fn div(a: u8, b: u8) -> u8 {
        galois_8::Field::div(a, b)
    }
    fn exp(a: u8, n: usize) -> u8 {
        galois_8::Field::exp(a, n)
    }
    fn zero() -> u8 {
        0
    }
    fn one() -> u8 {
        1
    }
    fn nth_internal(n: usize) -> u8 {
        galois_8::Field::nth_internal(n)
    }

    fn mul_slice(elem: u8, input: &[u8], out: &mut [u8]) {
        let _active = ActiveCall::enter();
        galois_8::Field::mul_slice(elem, input, out);
    }

    fn mul_slice_add(elem: u8, input: &[u8], out: &mut [u8]) {
        let _active = ActiveCall::enter();
        galois_8::Field::mul_slice_add(elem, input, out);
    }
}

fn data_shards(count: usize, len: usize) -> Vec<Vec<u8>> {
    (0..count)
        .map(|row| {
            (0..len)
                .map(|offset| (row * 37 + offset * 13) as u8)
                .collect()
        })
        .collect()
}

#[test]
fn configured_job_limit_bounds_full_and_incremental_encoding() {
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(8)
        .build()
        .unwrap();
    let data = data_shards(4, 256 * 1024 + 13);
    for max_jobs in [1, 2, 3] {
        for parity_count in [1, 2, 5, 8] {
            let options = CodecOptions::builder().max_parallel_jobs(max_jobs).build();
            let codec =
                ReedSolomon::<TrackingField>::with_options(4, parity_count, options).unwrap();
            let reference = galois_8::ReedSolomon::new(4, parity_count).unwrap();
            let mut expected = vec![vec![0; data[0].len()]; parity_count];
            reference.encode_sep(&data, &mut expected).unwrap();
            let mut actual = vec![vec![0; data[0].len()]; parity_count];
            PEAK.store(0, Ordering::SeqCst);
            pool.install(|| codec.encode_sep_par(&data, &mut actual))
                .unwrap();
            assert!(
                PEAK.load(Ordering::SeqCst) <= max_jobs,
                "full encode exceeded {max_jobs} jobs with {parity_count} outputs"
            );
            assert_eq!(actual, expected);

            PEAK.store(0, Ordering::SeqCst);
            for (index, shard) in data.iter().enumerate() {
                pool.install(|| codec.encode_single_sep_par(index, shard, &mut actual))
                    .unwrap();
            }
            assert!(
                PEAK.load(Ordering::SeqCst) <= max_jobs,
                "incremental encode exceeded {max_jobs} jobs with {parity_count} outputs"
            );
            assert_eq!(actual, expected);
        }
    }
    assert_eq!(ACTIVE.load(Ordering::SeqCst), 0);
}

#[test]
fn chunk_lengths_respect_cache_budget_and_simd_alignment() {
    let policy = ParallelPolicy {
        min_parallel_shard_bytes: 1,
        min_bytes_per_job: 256 * 1024,
        max_jobs: 3,
        l2_cache_bytes: 64 * 1024,
    };
    for shard_len in [1024, 32 * 1024 + 7, 1024 * 1024] {
        for (data, output) in [(4, 1), (10, 4), (20, 8)] {
            let decision = policy.decide(shard_len, data, output, 16);
            assert!(decision.jobs <= 3);
            assert!(decision.chunk_len > 0);
            assert!(decision.chunk_len <= shard_len);
            assert!(decision.chunk_len * (data + output) <= policy.l2_cache_bytes);
            assert_eq!(decision.chunk_len % 64, 0);
        }
    }
}

#[test]
fn tiny_cache_budgets_and_empty_inputs_make_progress() {
    let policy = ParallelPolicy {
        min_parallel_shard_bytes: 1,
        min_bytes_per_job: 64 * 1024,
        max_jobs: 2,
        l2_cache_bytes: 14,
    };
    assert_eq!(policy.decide(1024, 10, 4, 8).chunk_len, 1);
    assert_eq!(policy.decide(0, 10, 4, 8).chunk_len, 0);
    assert!(!policy.decide(0, 10, 4, 8).use_parallel);
    let extreme = policy.decide(usize::MAX, usize::MAX, usize::MAX, usize::MAX);
    assert_eq!(extreme.chunk_len, 1);
    assert_eq!(extreme.jobs, 2);
}

#[test]
fn disabling_cache_budget_preserves_work_granularity_limit() {
    let policy = ParallelPolicy {
        min_parallel_shard_bytes: 1,
        min_bytes_per_job: 256 * 1024,
        max_jobs: 2,
        l2_cache_bytes: 0,
    };
    assert_eq!(policy.decide(1024 * 1024, 10, 4, 8).chunk_len, 256 * 1024);
}

#[test]
fn serial_parallel_entry_fallback_matches_native_encoding_for_tails() {
    let codec = galois_8::ReedSolomon::with_options(
        10,
        4,
        CodecOptions::builder().max_parallel_jobs(1).build(),
    )
    .unwrap();
    for len in [1, 63, 64, 65, 1023, 32 * 1024 + 1] {
        let data = data_shards(10, len);
        let mut expected = vec![vec![0; len]; 4];
        let mut actual = expected.clone();
        codec.encode_sep(&data, &mut expected).unwrap();
        codec.encode_sep_par(&data, &mut actual).unwrap();
        assert_eq!(actual, expected, "shard length {len}");
    }
}

#[test]
fn incremental_auto_entry_rejects_leopard_families() {
    use rustfs_erasure_codec::{CodecFamily, Error};

    for codec_family in [CodecFamily::LeopardGF8, CodecFamily::LeopardGF16] {
        let codec = galois_8::ReedSolomon::with_options(
            4,
            2,
            CodecOptions::builder().codec_family(codec_family).build(),
        )
        .unwrap();
        let mut shards = data_shards(6, 64);
        assert_eq!(
            codec.encode_single_opt(0, &mut shards),
            Err(Error::UnsupportedCodecFamily)
        );
    }
}
