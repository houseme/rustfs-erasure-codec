#[cfg(all(feature = "std", feature = "benchmark-metrics"))]
use std::sync::atomic::AtomicUsize;
#[cfg(feature = "std")]
use std::sync::atomic::Ordering;

#[cfg(feature = "std")]
#[derive(Debug, Default)]
pub(crate) struct MetricCounter {
    #[cfg(feature = "benchmark-metrics")]
    value: AtomicUsize,
}

#[cfg(feature = "std")]
impl MetricCounter {
    pub(crate) const fn new() -> Self {
        Self {
            #[cfg(feature = "benchmark-metrics")]
            value: AtomicUsize::new(0),
        }
    }

    #[inline]
    pub(crate) fn load(&self, ordering: Ordering) -> usize {
        #[cfg(feature = "benchmark-metrics")]
        {
            self.value.load(ordering)
        }
        #[cfg(not(feature = "benchmark-metrics"))]
        {
            let _ = ordering;
            0
        }
    }

    #[inline]
    pub(crate) fn store(&self, value: usize, ordering: Ordering) {
        #[cfg(feature = "benchmark-metrics")]
        {
            self.value.store(value, ordering);
        }
        #[cfg(not(feature = "benchmark-metrics"))]
        {
            let _ = (value, ordering);
        }
    }

    #[inline]
    pub(crate) fn fetch_add(&self, value: usize, ordering: Ordering) -> usize {
        #[cfg(feature = "benchmark-metrics")]
        {
            self.value.fetch_add(value, ordering)
        }
        #[cfg(not(feature = "benchmark-metrics"))]
        {
            let _ = (value, ordering);
            0
        }
    }
}

#[cfg(feature = "std")]
#[derive(Debug, Default)]
pub(crate) struct ReconstructionCacheMetrics {
    pub(crate) requests: MetricCounter,
    pub(crate) hits: MetricCounter,
    pub(crate) misses: MetricCounter,
    pub(crate) inserts: MetricCounter,
    pub(crate) evictions: MetricCounter,
}

#[cfg(feature = "std")]
#[derive(Debug, Default)]
pub(crate) struct RuntimeProfileMetrics {
    code_some_serial_calls: MetricCounter,
    code_some_parallel_calls: MetricCounter,
    code_some_total_bytes: MetricCounter,
    code_some_total_chunks: MetricCounter,
    code_some_small_output_chunk_parallel_calls: MetricCounter,
    code_some_small_output_chunk_parallel_outputs: MetricCounter,
    code_some_small_output_chunk_parallel_chunks: MetricCounter,
    code_single_serial_calls: MetricCounter,
    code_single_parallel_calls: MetricCounter,
    code_single_total_bytes: MetricCounter,
    code_single_total_chunks: MetricCounter,
    parallel_policy_calls: MetricCounter,
    parallel_policy_parallel: MetricCounter,
    parallel_policy_serial: MetricCounter,
    parallel_policy_total_jobs: MetricCounter,
    parallel_policy_total_chunk_len: MetricCounter,
    reconstruct_calls: MetricCounter,
    reconstruct_data_only_calls: MetricCounter,
    reconstruct_entry_parallel_calls: MetricCounter,
    reconstruct_entry_serial_calls: MetricCounter,
    reconstruct_opt_fallback_serial_calls: MetricCounter,
    reconstruct_total_missing_data: MetricCounter,
    reconstruct_total_missing_parity: MetricCounter,
    reconstruct_all_present_fast_path: MetricCounter,
    reconstruct_data_stage_calls: MetricCounter,
    reconstruct_data_stage_bytes: MetricCounter,
    reconstruct_parity_stage_calls: MetricCounter,
    reconstruct_parity_stage_bytes: MetricCounter,
    reconstruct_data_small_output_specialized_calls: MetricCounter,
}

#[cfg(feature = "std")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReconstructionCacheStats {
    pub requests: usize,
    pub hits: usize,
    pub misses: usize,
    pub inserts: usize,
    pub evictions: usize,
}

#[cfg(feature = "std")]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReconstructionCacheAnalysis {
    pub hit_rate: f64,
    pub reuse_ratio: f64,
    pub miss_cost_per_request: f64,
}

#[cfg(feature = "std")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeProfileStats {
    pub code_some_serial_calls: usize,
    pub code_some_parallel_calls: usize,
    pub code_some_total_bytes: usize,
    pub code_some_total_chunks: usize,
    pub code_some_small_output_chunk_parallel_calls: usize,
    pub code_some_small_output_chunk_parallel_outputs: usize,
    pub code_some_small_output_chunk_parallel_chunks: usize,
    pub code_single_serial_calls: usize,
    pub code_single_parallel_calls: usize,
    pub code_single_total_bytes: usize,
    pub code_single_total_chunks: usize,
    pub parallel_policy_calls: usize,
    pub parallel_policy_parallel: usize,
    pub parallel_policy_serial: usize,
    pub parallel_policy_total_jobs: usize,
    pub parallel_policy_total_chunk_len: usize,
    pub reconstruct_calls: usize,
    pub reconstruct_data_only_calls: usize,
    pub reconstruct_entry_parallel_calls: usize,
    pub reconstruct_entry_serial_calls: usize,
    pub reconstruct_opt_fallback_serial_calls: usize,
    pub reconstruct_total_missing_data: usize,
    pub reconstruct_total_missing_parity: usize,
    pub reconstruct_all_present_fast_path: usize,
    pub reconstruct_data_stage_calls: usize,
    pub reconstruct_data_stage_bytes: usize,
    pub reconstruct_parity_stage_calls: usize,
    pub reconstruct_parity_stage_bytes: usize,
    pub reconstruct_data_small_output_specialized_calls: usize,
}

#[cfg(feature = "std")]
impl ReconstructionCacheMetrics {
    pub(crate) fn snapshot(&self) -> ReconstructionCacheStats {
        ReconstructionCacheStats {
            requests: self.requests.load(Ordering::Relaxed),
            hits: self.hits.load(Ordering::Relaxed),
            misses: self.misses.load(Ordering::Relaxed),
            inserts: self.inserts.load(Ordering::Relaxed),
            evictions: self.evictions.load(Ordering::Relaxed),
        }
    }
}

#[cfg(feature = "std")]
impl ReconstructionCacheStats {
    /// Returns the cache hit rate (hits / requests).
    #[inline]
    pub fn hit_rate(&self) -> f64 {
        if self.requests == 0 {
            0.0
        } else {
            self.hits as f64 / self.requests as f64
        }
    }

    /// Returns the cache reuse ratio (hits / inserts).
    #[inline]
    pub fn reuse_ratio(&self) -> f64 {
        if self.inserts == 0 {
            0.0
        } else {
            self.hits as f64 / self.inserts as f64
        }
    }

    /// Returns the miss cost per request (misses / requests).
    #[inline]
    pub fn miss_cost_per_request(&self) -> f64 {
        if self.requests == 0 {
            0.0
        } else {
            self.misses as f64 / self.requests as f64
        }
    }

    /// Returns a combined analysis of all cache metrics.
    #[inline]
    pub fn analysis(&self) -> ReconstructionCacheAnalysis {
        ReconstructionCacheAnalysis {
            hit_rate: self.hit_rate(),
            reuse_ratio: self.reuse_ratio(),
            miss_cost_per_request: self.miss_cost_per_request(),
        }
    }
}

#[cfg(feature = "std")]
impl RuntimeProfileMetrics {
    pub(crate) fn snapshot(&self) -> RuntimeProfileStats {
        RuntimeProfileStats {
            code_some_serial_calls: self.code_some_serial_calls.load(Ordering::Relaxed),
            code_some_parallel_calls: self.code_some_parallel_calls.load(Ordering::Relaxed),
            code_some_total_bytes: self.code_some_total_bytes.load(Ordering::Relaxed),
            code_some_total_chunks: self.code_some_total_chunks.load(Ordering::Relaxed),
            code_some_small_output_chunk_parallel_calls: self
                .code_some_small_output_chunk_parallel_calls
                .load(Ordering::Relaxed),
            code_some_small_output_chunk_parallel_outputs: self
                .code_some_small_output_chunk_parallel_outputs
                .load(Ordering::Relaxed),
            code_some_small_output_chunk_parallel_chunks: self
                .code_some_small_output_chunk_parallel_chunks
                .load(Ordering::Relaxed),
            code_single_serial_calls: self.code_single_serial_calls.load(Ordering::Relaxed),
            code_single_parallel_calls: self.code_single_parallel_calls.load(Ordering::Relaxed),
            code_single_total_bytes: self.code_single_total_bytes.load(Ordering::Relaxed),
            code_single_total_chunks: self.code_single_total_chunks.load(Ordering::Relaxed),
            parallel_policy_calls: self.parallel_policy_calls.load(Ordering::Relaxed),
            parallel_policy_parallel: self.parallel_policy_parallel.load(Ordering::Relaxed),
            parallel_policy_serial: self.parallel_policy_serial.load(Ordering::Relaxed),
            parallel_policy_total_jobs: self.parallel_policy_total_jobs.load(Ordering::Relaxed),
            parallel_policy_total_chunk_len: self
                .parallel_policy_total_chunk_len
                .load(Ordering::Relaxed),
            reconstruct_calls: self.reconstruct_calls.load(Ordering::Relaxed),
            reconstruct_data_only_calls: self.reconstruct_data_only_calls.load(Ordering::Relaxed),
            reconstruct_entry_parallel_calls: self
                .reconstruct_entry_parallel_calls
                .load(Ordering::Relaxed),
            reconstruct_entry_serial_calls: self
                .reconstruct_entry_serial_calls
                .load(Ordering::Relaxed),
            reconstruct_opt_fallback_serial_calls: self
                .reconstruct_opt_fallback_serial_calls
                .load(Ordering::Relaxed),
            reconstruct_total_missing_data: self
                .reconstruct_total_missing_data
                .load(Ordering::Relaxed),
            reconstruct_total_missing_parity: self
                .reconstruct_total_missing_parity
                .load(Ordering::Relaxed),
            reconstruct_all_present_fast_path: self
                .reconstruct_all_present_fast_path
                .load(Ordering::Relaxed),
            reconstruct_data_stage_calls: self.reconstruct_data_stage_calls.load(Ordering::Relaxed),
            reconstruct_data_stage_bytes: self.reconstruct_data_stage_bytes.load(Ordering::Relaxed),
            reconstruct_parity_stage_calls: self
                .reconstruct_parity_stage_calls
                .load(Ordering::Relaxed),
            reconstruct_parity_stage_bytes: self
                .reconstruct_parity_stage_bytes
                .load(Ordering::Relaxed),
            reconstruct_data_small_output_specialized_calls: self
                .reconstruct_data_small_output_specialized_calls
                .load(Ordering::Relaxed),
        }
    }

    pub(crate) fn reset(&self) {
        self.code_some_serial_calls.store(0, Ordering::Relaxed);
        self.code_some_parallel_calls.store(0, Ordering::Relaxed);
        self.code_some_total_bytes.store(0, Ordering::Relaxed);
        self.code_some_total_chunks.store(0, Ordering::Relaxed);
        self.code_some_small_output_chunk_parallel_calls
            .store(0, Ordering::Relaxed);
        self.code_some_small_output_chunk_parallel_outputs
            .store(0, Ordering::Relaxed);
        self.code_some_small_output_chunk_parallel_chunks
            .store(0, Ordering::Relaxed);
        self.code_single_serial_calls.store(0, Ordering::Relaxed);
        self.code_single_parallel_calls.store(0, Ordering::Relaxed);
        self.code_single_total_bytes.store(0, Ordering::Relaxed);
        self.code_single_total_chunks.store(0, Ordering::Relaxed);
        self.parallel_policy_calls.store(0, Ordering::Relaxed);
        self.parallel_policy_parallel.store(0, Ordering::Relaxed);
        self.parallel_policy_serial.store(0, Ordering::Relaxed);
        self.parallel_policy_total_jobs.store(0, Ordering::Relaxed);
        self.parallel_policy_total_chunk_len
            .store(0, Ordering::Relaxed);
        self.reconstruct_calls.store(0, Ordering::Relaxed);
        self.reconstruct_data_only_calls.store(0, Ordering::Relaxed);
        self.reconstruct_entry_parallel_calls
            .store(0, Ordering::Relaxed);
        self.reconstruct_entry_serial_calls
            .store(0, Ordering::Relaxed);
        self.reconstruct_opt_fallback_serial_calls
            .store(0, Ordering::Relaxed);
        self.reconstruct_total_missing_data
            .store(0, Ordering::Relaxed);
        self.reconstruct_total_missing_parity
            .store(0, Ordering::Relaxed);
        self.reconstruct_all_present_fast_path
            .store(0, Ordering::Relaxed);
        self.reconstruct_data_stage_calls
            .store(0, Ordering::Relaxed);
        self.reconstruct_data_stage_bytes
            .store(0, Ordering::Relaxed);
        self.reconstruct_parity_stage_calls
            .store(0, Ordering::Relaxed);
        self.reconstruct_parity_stage_bytes
            .store(0, Ordering::Relaxed);
        self.reconstruct_data_small_output_specialized_calls
            .store(0, Ordering::Relaxed);
    }

    pub(crate) fn record_code_some(
        &self,
        parallel: bool,
        shard_len: usize,
        inputs: usize,
        outputs: usize,
        chunk_len: usize,
    ) {
        if parallel {
            self.code_some_parallel_calls
                .fetch_add(1, Ordering::Relaxed);
        } else {
            self.code_some_serial_calls.fetch_add(1, Ordering::Relaxed);
        }
        self.code_some_total_bytes.fetch_add(
            shard_len.saturating_mul(inputs).saturating_mul(outputs),
            Ordering::Relaxed,
        );
        let chunks = if chunk_len == 0 {
            0
        } else {
            shard_len.div_ceil(chunk_len)
        };
        self.code_some_total_chunks
            .fetch_add(chunks, Ordering::Relaxed);
    }

    pub(crate) fn record_code_some_small_output_chunk_parallel(
        &self,
        outputs: usize,
        chunks: usize,
    ) {
        self.code_some_small_output_chunk_parallel_calls
            .fetch_add(1, Ordering::Relaxed);
        self.code_some_small_output_chunk_parallel_outputs
            .fetch_add(outputs, Ordering::Relaxed);
        self.code_some_small_output_chunk_parallel_chunks
            .fetch_add(chunks, Ordering::Relaxed);
    }

    pub(crate) fn record_code_single(
        &self,
        parallel: bool,
        shard_len: usize,
        outputs: usize,
        chunk_len: usize,
    ) {
        if parallel {
            self.code_single_parallel_calls
                .fetch_add(1, Ordering::Relaxed);
        } else {
            self.code_single_serial_calls
                .fetch_add(1, Ordering::Relaxed);
        }
        self.code_single_total_bytes
            .fetch_add(shard_len.saturating_mul(outputs), Ordering::Relaxed);
        let chunks = if chunk_len == 0 {
            0
        } else {
            shard_len.div_ceil(chunk_len)
        };
        self.code_single_total_chunks
            .fetch_add(chunks, Ordering::Relaxed);
    }

    pub(crate) fn record_parallel_policy(&self, decision: crate::ParallelDecision) {
        self.parallel_policy_calls.fetch_add(1, Ordering::Relaxed);
        if decision.use_parallel {
            self.parallel_policy_parallel
                .fetch_add(1, Ordering::Relaxed);
        } else {
            self.parallel_policy_serial.fetch_add(1, Ordering::Relaxed);
        }
        self.parallel_policy_total_jobs
            .fetch_add(decision.jobs, Ordering::Relaxed);
        self.parallel_policy_total_chunk_len
            .fetch_add(decision.chunk_len, Ordering::Relaxed);
    }

    pub(crate) fn record_reconstruct(
        &self,
        data_only: bool,
        missing_data_count: usize,
        missing_parity_count: usize,
        all_present_fast_path: bool,
    ) {
        self.reconstruct_calls.fetch_add(1, Ordering::Relaxed);
        if data_only {
            self.reconstruct_data_only_calls
                .fetch_add(1, Ordering::Relaxed);
        }
        if all_present_fast_path {
            self.reconstruct_all_present_fast_path
                .fetch_add(1, Ordering::Relaxed);
        }
        self.reconstruct_total_missing_data
            .fetch_add(missing_data_count, Ordering::Relaxed);
        self.reconstruct_total_missing_parity
            .fetch_add(missing_parity_count, Ordering::Relaxed);
    }

    pub(crate) fn record_reconstruct_entry(&self, parallel: bool) {
        if parallel {
            self.reconstruct_entry_parallel_calls
                .fetch_add(1, Ordering::Relaxed);
        } else {
            self.reconstruct_entry_serial_calls
                .fetch_add(1, Ordering::Relaxed);
        }
    }

    pub(crate) fn record_reconstruct_opt_fallback_serial(&self) {
        self.reconstruct_opt_fallback_serial_calls
            .fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn record_reconstruct_data_stage(&self, shard_len: usize, output_count: usize) {
        self.reconstruct_data_stage_calls
            .fetch_add(1, Ordering::Relaxed);
        self.reconstruct_data_stage_bytes
            .fetch_add(shard_len.saturating_mul(output_count), Ordering::Relaxed);
    }

    pub(crate) fn record_reconstruct_parity_stage(&self, shard_len: usize, output_count: usize) {
        self.reconstruct_parity_stage_calls
            .fetch_add(1, Ordering::Relaxed);
        self.reconstruct_parity_stage_bytes
            .fetch_add(shard_len.saturating_mul(output_count), Ordering::Relaxed);
    }

    pub(crate) fn record_reconstruct_data_small_output_specialized(&self) {
        self.reconstruct_data_small_output_specialized_calls
            .fetch_add(1, Ordering::Relaxed);
    }
}
