#[cfg(feature = "std")]
use core::sync::atomic::{AtomicUsize, Ordering};
#[cfg(feature = "std")]
use std::sync::OnceLock;

#[cfg(feature = "std")]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RustNeonProfileStats {
    pub mul_calls: usize,
    pub mul_xor_calls: usize,
    pub total_bytes: usize,
    pub vector_64b_chunks: usize,
    pub vector_16b_chunks: usize,
    pub tail_bytes: usize,
    pub tail_calls: usize,
    pub table_lookups: usize,
}

#[cfg(feature = "std")]
impl RustNeonProfileStats {
    pub fn saturating_sub(self, baseline: Self) -> Self {
        Self {
            mul_calls: self.mul_calls.saturating_sub(baseline.mul_calls),
            mul_xor_calls: self.mul_xor_calls.saturating_sub(baseline.mul_xor_calls),
            total_bytes: self.total_bytes.saturating_sub(baseline.total_bytes),
            vector_64b_chunks: self
                .vector_64b_chunks
                .saturating_sub(baseline.vector_64b_chunks),
            vector_16b_chunks: self
                .vector_16b_chunks
                .saturating_sub(baseline.vector_16b_chunks),
            tail_bytes: self.tail_bytes.saturating_sub(baseline.tail_bytes),
            tail_calls: self.tail_calls.saturating_sub(baseline.tail_calls),
            table_lookups: self.table_lookups.saturating_sub(baseline.table_lookups),
        }
    }
}

#[cfg(feature = "std")]
#[derive(Debug, Default)]
pub(crate) struct RustNeonProfileMetrics {
    mul_calls: AtomicUsize,
    mul_xor_calls: AtomicUsize,
    total_bytes: AtomicUsize,
    vector_64b_chunks: AtomicUsize,
    vector_16b_chunks: AtomicUsize,
    tail_bytes: AtomicUsize,
    tail_calls: AtomicUsize,
    table_lookups: AtomicUsize,
}

#[cfg(feature = "std")]
impl RustNeonProfileMetrics {
    fn snapshot(&self) -> RustNeonProfileStats {
        RustNeonProfileStats {
            mul_calls: self.mul_calls.load(Ordering::Relaxed),
            mul_xor_calls: self.mul_xor_calls.load(Ordering::Relaxed),
            total_bytes: self.total_bytes.load(Ordering::Relaxed),
            vector_64b_chunks: self.vector_64b_chunks.load(Ordering::Relaxed),
            vector_16b_chunks: self.vector_16b_chunks.load(Ordering::Relaxed),
            tail_bytes: self.tail_bytes.load(Ordering::Relaxed),
            tail_calls: self.tail_calls.load(Ordering::Relaxed),
            table_lookups: self.table_lookups.load(Ordering::Relaxed),
        }
    }

    fn reset(&self) {
        self.mul_calls.store(0, Ordering::Relaxed);
        self.mul_xor_calls.store(0, Ordering::Relaxed);
        self.total_bytes.store(0, Ordering::Relaxed);
        self.vector_64b_chunks.store(0, Ordering::Relaxed);
        self.vector_16b_chunks.store(0, Ordering::Relaxed);
        self.tail_bytes.store(0, Ordering::Relaxed);
        self.tail_calls.store(0, Ordering::Relaxed);
        self.table_lookups.store(0, Ordering::Relaxed);
    }

    #[cfg(all(
        feature = "simd-neon",
        target_arch = "aarch64",
        not(target_env = "msvc"),
        not(any(target_os = "android", target_os = "ios"))
    ))]
    pub(crate) fn record_call(
        &self,
        is_xor: bool,
        input_len: usize,
        vector_64b_chunks: usize,
        vector_16b_chunks: usize,
        tail_bytes: usize,
    ) {
        if is_xor {
            self.mul_xor_calls.fetch_add(1, Ordering::Relaxed);
        } else {
            self.mul_calls.fetch_add(1, Ordering::Relaxed);
        }
        self.total_bytes.fetch_add(input_len, Ordering::Relaxed);
        self.vector_64b_chunks
            .fetch_add(vector_64b_chunks, Ordering::Relaxed);
        self.vector_16b_chunks
            .fetch_add(vector_16b_chunks, Ordering::Relaxed);
        if tail_bytes > 0 {
            self.tail_calls.fetch_add(1, Ordering::Relaxed);
            self.tail_bytes.fetch_add(tail_bytes, Ordering::Relaxed);
        }
        let lookups = vector_64b_chunks
            .saturating_mul(8)
            .saturating_add(vector_16b_chunks.saturating_mul(2));
        self.table_lookups.fetch_add(lookups, Ordering::Relaxed);

        // Test-only per-thread mirror of the increment (compiled out of every
        // non-test build). Lets a test observe exactly its own thread's calls,
        // immune to concurrent tests hitting this shared global aggregate.
        #[cfg(test)]
        capture::record(
            is_xor,
            input_len,
            vector_64b_chunks,
            vector_16b_chunks,
            tail_bytes,
        );
    }
}

/// Test-only per-thread capture of the NEON profile increments.
///
/// The production metrics ([`RUST_NEON_PROFILE_METRICS`]) are a single global
/// aggregate — correct for benchmarking, but exact-delta assertions on it are
/// racy because every NEON `mul_slice` on any thread (e.g. concurrent codec
/// tests) increments it. In test builds `record_call` additionally feeds this
/// thread-local accumulator, so a test reads only its own thread's calls.
#[cfg(all(
    test,
    feature = "std",
    feature = "simd-neon",
    target_arch = "aarch64",
    not(target_env = "msvc"),
    not(any(target_os = "android", target_os = "ios"))
))]
pub(crate) mod capture {
    use super::RustNeonProfileStats;
    use core::cell::Cell;

    const ZERO: RustNeonProfileStats = RustNeonProfileStats {
        mul_calls: 0,
        mul_xor_calls: 0,
        total_bytes: 0,
        vector_64b_chunks: 0,
        vector_16b_chunks: 0,
        tail_bytes: 0,
        tail_calls: 0,
        table_lookups: 0,
    };

    std::thread_local! {
        static CAPTURE: Cell<RustNeonProfileStats> = const { Cell::new(ZERO) };
    }

    pub(crate) fn record(
        is_xor: bool,
        input_len: usize,
        vector_64b_chunks: usize,
        vector_16b_chunks: usize,
        tail_bytes: usize,
    ) {
        CAPTURE.with(|cell| {
            let mut s = cell.get();
            if is_xor {
                s.mul_xor_calls += 1;
            } else {
                s.mul_calls += 1;
            }
            s.total_bytes += input_len;
            s.vector_64b_chunks += vector_64b_chunks;
            s.vector_16b_chunks += vector_16b_chunks;
            if tail_bytes > 0 {
                s.tail_calls += 1;
                s.tail_bytes += tail_bytes;
            }
            s.table_lookups += vector_64b_chunks
                .saturating_mul(8)
                .saturating_add(vector_16b_chunks.saturating_mul(2));
            cell.set(s);
        });
    }

    /// Reset this thread's accumulator to zero.
    pub(crate) fn reset() {
        CAPTURE.with(|cell| cell.set(ZERO));
    }

    /// This thread's accumulated increments since the last [`reset`].
    pub(crate) fn snapshot() -> RustNeonProfileStats {
        CAPTURE.with(|cell| cell.get())
    }
}

#[cfg(feature = "std")]
pub(crate) static RUST_NEON_PROFILE_METRICS: RustNeonProfileMetrics = RustNeonProfileMetrics {
    mul_calls: AtomicUsize::new(0),
    mul_xor_calls: AtomicUsize::new(0),
    total_bytes: AtomicUsize::new(0),
    vector_64b_chunks: AtomicUsize::new(0),
    vector_16b_chunks: AtomicUsize::new(0),
    tail_bytes: AtomicUsize::new(0),
    tail_calls: AtomicUsize::new(0),
    table_lookups: AtomicUsize::new(0),
};

#[cfg(feature = "std")]
const RS_NEON_MUL_SLICE_XOR_UNROLL_ENV: &str = "RS_NEON_MUL_SLICE_XOR_UNROLL";
#[cfg(feature = "std")]
pub(crate) const RS_NEON_MUL_SLICE_XOR_SCHEDULE_ENV: &str = "RS_NEON_MUL_SLICE_XOR_SCHEDULE";

#[cfg(feature = "std")]
pub(crate) fn parse_rust_neon_xor_unroll(value: &str) -> Option<usize> {
    match value {
        "2" => Some(2),
        "4" => Some(4),
        _ => None,
    }
}

#[cfg(feature = "std")]
pub(crate) fn rust_neon_mul_slice_xor_unroll() -> usize {
    static UNROLL: OnceLock<usize> = OnceLock::new();
    *UNROLL.get_or_init(|| {
        std::env::var(RS_NEON_MUL_SLICE_XOR_UNROLL_ENV)
            .ok()
            .as_deref()
            .and_then(parse_rust_neon_xor_unroll)
            .unwrap_or(4)
    })
}

#[cfg(feature = "std")]
pub(crate) fn rust_neon_mul_slice_xor_schedule_split() -> bool {
    static SPLIT: OnceLock<bool> = OnceLock::new();
    *SPLIT.get_or_init(|| {
        std::env::var(RS_NEON_MUL_SLICE_XOR_SCHEDULE_ENV)
            .ok()
            .is_some_and(|value| value == "split")
    })
}

#[cfg(feature = "std")]
pub fn rust_neon_profile_stats() -> RustNeonProfileStats {
    RUST_NEON_PROFILE_METRICS.snapshot()
}

#[cfg(feature = "std")]
pub fn reset_rust_neon_profile_stats() {
    RUST_NEON_PROFILE_METRICS.reset();
}
