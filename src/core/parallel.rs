use crate::Field;

use super::{
    CODE_SLICE_DEFAULT_CHUNK_BYTES, CODE_SLICE_LARGE_CHUNK_BYTES, CODE_SLICE_MIN_CHUNK_BYTES,
    PARALLEL_MIN_SHARD_BYTES, ReedSolomon,
};

#[cfg(feature = "std")]
pub const PARALLEL_POLICY_VERSION: u32 = 3;
#[cfg(feature = "std")]
const RS_PARALLEL_POLICY_MIN_PARALLEL_SHARD_BYTES_ENV: &str =
    "RS_PARALLEL_POLICY_MIN_PARALLEL_SHARD_BYTES";
#[cfg(feature = "std")]
const RS_PARALLEL_POLICY_MIN_BYTES_PER_JOB_ENV: &str = "RS_PARALLEL_POLICY_MIN_BYTES_PER_JOB";
#[cfg(feature = "std")]
const RS_PARALLEL_POLICY_MAX_JOBS_ENV: &str = "RS_PARALLEL_POLICY_MAX_JOBS";
#[cfg(feature = "std")]
const RS_PARALLEL_POLICY_L2_CACHE_BYTES_ENV: &str = "RS_PARALLEL_POLICY_L2_CACHE_BYTES";

#[cfg(feature = "std")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParallelPolicy {
    pub min_parallel_shard_bytes: usize,
    pub min_bytes_per_job: usize,
    pub max_jobs: usize,
    /// Estimated L2 cache size per core in bytes. Used to bound chunk sizes so
    /// each job's working set fits in L2. Set to 0 to disable cache-aware sizing.
    pub l2_cache_bytes: usize,
}

#[cfg(feature = "std")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParallelDecision {
    pub use_parallel: bool,
    pub jobs: usize,
    pub chunk_len: usize,
}

#[cfg(feature = "std")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RuntimeParallelPolicyCache {
    pub available_parallelism: usize,
    pub data: ParallelPolicy,
    pub reconstruct_data: ParallelPolicy,
    pub reconstruct_full_data: ParallelPolicy,
    pub reconstruct_full_parity: ParallelPolicy,
}

#[cfg(feature = "std")]
impl Default for ParallelPolicy {
    fn default() -> Self {
        Self {
            min_parallel_shard_bytes: PARALLEL_MIN_SHARD_BYTES,
            min_bytes_per_job: CODE_SLICE_LARGE_CHUNK_BYTES,
            max_jobs: 0,
            l2_cache_bytes: super::cache_detect::detect_l2_cache_bytes()
                .unwrap_or(super::cache_detect::DEFAULT_L2_CACHE_BYTES),
        }
    }
}

#[cfg(feature = "std")]
impl ParallelPolicy {
    /// Decide whether to use parallel execution and how to chunk the work.
    pub fn decide(
        &self,
        shard_size: usize,
        data_shards: usize,
        output_shards: usize,
        available_parallelism: usize,
    ) -> ParallelDecision {
        if shard_size == 0 || data_shards == 0 || output_shards == 0 {
            return ParallelDecision {
                use_parallel: false,
                jobs: 1,
                chunk_len: shard_size,
            };
        }

        let available_parallelism = available_parallelism.max(1);
        let max_jobs = if self.max_jobs == 0 {
            available_parallelism
        } else {
            core::cmp::min(self.max_jobs, available_parallelism)
        }
        .max(1);

        let min_parallel_shard_bytes = self.min_parallel_shard_bytes.max(1);
        let min_bytes_per_job = self.min_bytes_per_job.max(CODE_SLICE_MIN_CHUNK_BYTES);

        // Bound the actual inner chunk, independently of the number of jobs.
        // More chunks alone do not reduce a job's working set.
        let cache_chunk_len = if self.l2_cache_bytes == 0 {
            usize::MAX
        } else {
            let active_shards = data_shards.saturating_add(output_shards).max(1);
            (self.l2_cache_bytes / active_shards).max(1)
        };
        let chunk_cap = min_bytes_per_job.min(cache_chunk_len).min(shard_size);
        let chunk_count = shard_size.div_ceil(chunk_cap).max(1);
        let max_useful_jobs = if output_shards <= 2 {
            chunk_count
        } else {
            output_shards.saturating_mul(chunk_count)
        }
        .max(1);
        let jobs = core::cmp::min(max_jobs, max_useful_jobs).max(1);

        if shard_size < min_parallel_shard_bytes || jobs < 2 {
            return ParallelDecision {
                use_parallel: false,
                jobs: 1,
                chunk_len: align_chunk_len(Self::serial_chunk_len(shard_size).min(cache_chunk_len)),
            };
        }

        let chunks_per_output = core::cmp::max(1, jobs.div_ceil(output_shards));
        let chunk_len = align_chunk_len(shard_size.div_ceil(chunks_per_output).min(chunk_cap));

        ParallelDecision {
            use_parallel: true,
            jobs,
            chunk_len: core::cmp::min(chunk_len, shard_size),
        }
    }

    /// Apply byte-based policy limits to fields with wider elements, returning
    /// the inner chunk length in elements for slice indexing.
    pub(crate) fn decide_for_elements(
        &self,
        shard_len: usize,
        element_size: usize,
        data_shards: usize,
        output_shards: usize,
        available_parallelism: usize,
    ) -> ParallelDecision {
        let element_size = element_size.max(1);
        let mut decision = self.decide(
            shard_len.saturating_mul(element_size),
            data_shards,
            output_shards,
            available_parallelism,
        );
        decision.chunk_len = if shard_len == 0 {
            0
        } else {
            (decision.chunk_len / element_size).max(1).min(shard_len)
        };
        decision
    }

    fn serial_chunk_len(shard_size: usize) -> usize {
        if shard_size <= CODE_SLICE_MIN_CHUNK_BYTES {
            shard_size
        } else if shard_size <= CODE_SLICE_DEFAULT_CHUNK_BYTES {
            CODE_SLICE_MIN_CHUNK_BYTES
        } else if shard_size <= 4 * 1024 * 1024 {
            CODE_SLICE_DEFAULT_CHUNK_BYTES
        } else {
            CODE_SLICE_LARGE_CHUNK_BYTES
        }
    }

    /// Builder-style setter for L2 cache size.
    pub fn with_l2_cache_bytes(mut self, bytes: usize) -> Self {
        self.l2_cache_bytes = bytes;
        self
    }

    /// Apply environment variable overrides (`RS_PARALLEL_POLICY_*`) to this policy.
    pub fn with_env_overrides(self) -> Self {
        let mut policy = self;
        if let Some(value) = parse_env_usize(RS_PARALLEL_POLICY_MIN_PARALLEL_SHARD_BYTES_ENV)
            && value > 0
        {
            policy.min_parallel_shard_bytes = value;
        }
        if let Some(value) = parse_env_usize(RS_PARALLEL_POLICY_MIN_BYTES_PER_JOB_ENV)
            && value > 0
        {
            policy.min_bytes_per_job = value;
        }
        if let Some(value) = parse_env_usize(RS_PARALLEL_POLICY_MAX_JOBS_ENV) {
            policy.max_jobs = value;
        }
        if let Some(value) = parse_env_usize(RS_PARALLEL_POLICY_L2_CACHE_BYTES_ENV) {
            policy.l2_cache_bytes = value;
        }
        policy
    }
}

// Preserve cache-line and SIMD boundaries without exceeding the budget.
#[cfg(feature = "std")]
fn align_chunk_len(chunk_len: usize) -> usize {
    const ALIGNMENT: usize = 64;
    if chunk_len >= ALIGNMENT {
        chunk_len / ALIGNMENT * ALIGNMENT
    } else {
        chunk_len
    }
}

/// Visit disjoint output chunks with at most `jobs` active callbacks.
///
/// Cache chunks run sequentially within a bounded number of Rayon tasks. When
/// there are fewer outputs than jobs, apportion the budget across outputs;
/// otherwise group outputs. Neither path creates a pool or allocates tasks.
#[cfg(feature = "std")]
pub(crate) fn for_each_output_chunk<E, U, C>(
    outputs: &mut [U],
    shard_len: usize,
    chunk_len: usize,
    jobs: usize,
    process: C,
) where
    E: Send,
    U: AsMut<[E]> + Send,
    C: Fn(usize, usize, &mut [E]) + Send + Sync,
{
    use rayon::prelude::*;

    if outputs.is_empty() || shard_len == 0 {
        return;
    }
    let chunk_len = chunk_len.max(1).min(shard_len);
    let jobs = jobs.max(1);
    let output_count = outputs.len();
    if output_count >= jobs {
        let outputs_per_job = output_count.div_ceil(jobs);
        outputs
            .par_chunks_mut(outputs_per_job)
            .enumerate()
            .for_each(|(group_index, group)| {
                for (offset, output) in group.iter_mut().enumerate() {
                    let row = group_index * outputs_per_job + offset;
                    for (chunk_index, chunk) in output.as_mut().chunks_mut(chunk_len).enumerate() {
                        process(row, chunk_index * chunk_len, chunk);
                    }
                }
            });
    } else {
        let chunk_count = shard_len.div_ceil(chunk_len);
        outputs
            .par_iter_mut()
            .enumerate()
            .for_each(|(row, output)| {
                let row_jobs = jobs / output_count + usize::from(row < jobs % output_count);
                let chunks_per_job = chunk_count.div_ceil(row_jobs);
                let group_len = chunks_per_job.saturating_mul(chunk_len).min(shard_len);
                output
                    .as_mut()
                    .par_chunks_mut(group_len)
                    .enumerate()
                    .for_each(|(group_index, group)| {
                        for (chunk_index, chunk) in group.chunks_mut(chunk_len).enumerate() {
                            let start = group_index * group_len + chunk_index * chunk_len;
                            process(row, start, chunk);
                        }
                    });
            });
    }
}

#[cfg(feature = "std")]
impl RuntimeParallelPolicyCache {
    pub(crate) fn new(data: ParallelPolicy) -> Self {
        Self {
            available_parallelism: detect_available_parallelism(),
            data,
            reconstruct_data: data,
            reconstruct_full_data: data,
            reconstruct_full_parity: data,
        }
    }

    pub(crate) fn reconstruct_policy(&self, data_only: bool) -> ParallelPolicy {
        if data_only {
            self.reconstruct_data
        } else {
            self.reconstruct_full_data
        }
    }

    pub(crate) fn reconstruct_stage_policies(
        &self,
        data_only: bool,
    ) -> (ParallelPolicy, ParallelPolicy) {
        if data_only {
            (self.reconstruct_data, self.reconstruct_data)
        } else {
            (self.reconstruct_full_data, self.reconstruct_full_parity)
        }
    }
}

#[cfg(feature = "std")]
fn parse_env_usize(name: &str) -> Option<usize> {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
}

#[cfg(feature = "std")]
fn detect_available_parallelism() -> usize {
    std::thread::available_parallelism()
        .map(|parallelism| parallelism.get())
        .unwrap_or(1)
}

impl<F: Field> ReedSolomon<F> {
    /// Compute the parallel execution decision for the current codec and shard size.
    #[cfg(feature = "std")]
    pub fn parallel_policy(&self, shard_len: usize, output_shards: usize) -> ParallelDecision {
        let decision = self.parallel_policy_with(
            shard_len,
            output_shards,
            self.policy_cache.available_parallelism,
        );

        #[cfg(debug_assertions)]
        if std::env::var_os("RS_PARALLEL_POLICY_DEBUG").is_some() {
            eprintln!(
                "rs-parallel-policy v{} shard_len={} outputs={} -> use_parallel={} jobs={} chunk_len={}",
                PARALLEL_POLICY_VERSION,
                shard_len,
                output_shards,
                decision.use_parallel,
                decision.jobs,
                decision.chunk_len
            );
        }

        self.runtime_profile_metrics
            .record_parallel_policy(decision);
        decision
    }

    #[cfg(feature = "std")]
    pub(crate) fn parallel_policy_with(
        &self,
        shard_len: usize,
        output_shards: usize,
        available_parallelism: usize,
    ) -> ParallelDecision {
        self.effective_parallel_policy().decide_for_elements(
            shard_len,
            core::mem::size_of::<F::Elem>(),
            self.data_shard_count(),
            output_shards,
            available_parallelism,
        )
    }

    /// Returns the parallel policy version number.
    #[cfg(feature = "std")]
    pub fn parallel_policy_version(&self) -> u32 {
        PARALLEL_POLICY_VERSION
    }

    /// Returns the effective parallel policy (including cache-aware adjustments).
    #[cfg(feature = "std")]
    pub fn effective_parallel_policy(&self) -> ParallelPolicy {
        self.policy_cache.data
    }

    #[cfg(feature = "std")]
    pub(crate) fn resolve_policy_cache() -> RuntimeParallelPolicyCache {
        Self::resolve_policy_cache_with_options(super::CodecOptions::default())
    }

    #[cfg(feature = "std")]
    pub(crate) fn resolve_policy_cache_with_options(
        options: super::CodecOptions,
    ) -> RuntimeParallelPolicyCache {
        let mut data = ParallelPolicy::default().with_env_overrides();
        if options.max_parallel_jobs > 0 {
            data.max_jobs = options.max_parallel_jobs;
        }
        if core::any::type_name::<F>() == core::any::type_name::<crate::galois_8::Field>() {
            crate::galois_8::resolve_runtime_parallel_policy_cache(data)
        } else {
            RuntimeParallelPolicyCache::new(data)
        }
    }
}

#[cfg(all(test, feature = "std"))]
mod tests {
    use super::*;

    #[test]
    fn output_chunk_scheduler_covers_each_byte_once() {
        for output_count in [1, 2, 4, 7] {
            for shard_len in [0, 1, 63, 65, 4097] {
                for chunk_len in [1, 64, 1024, 8192] {
                    for jobs in [0, 1, 2, 4, 8] {
                        let mut outputs = vec![vec![0u8; shard_len]; output_count];
                        for_each_output_chunk(
                            &mut outputs,
                            shard_len,
                            chunk_len,
                            jobs,
                            |row, start, chunk| {
                                for (offset, byte) in chunk.iter_mut().enumerate() {
                                    assert_eq!(*byte, 0, "output byte visited twice");
                                    *byte = ((row * 13 + start + offset) % 254 + 1) as u8;
                                }
                            },
                        );
                        for (row, output) in outputs.iter().enumerate() {
                            for (offset, &byte) in output.iter().enumerate() {
                                assert_eq!(
                                    byte,
                                    ((row * 13 + offset) % 254 + 1) as u8,
                                    "outputs={output_count}, len={shard_len}, chunk={chunk_len}, jobs={jobs}, row={row}, offset={offset}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn cache_budget_uses_bytes_for_wider_field_elements() {
        let policy = ParallelPolicy {
            min_parallel_shard_bytes: 1,
            min_bytes_per_job: 256 * 1024,
            max_jobs: 2,
            l2_cache_bytes: 64 * 1024,
        };
        let decision = policy.decide_for_elements(1024 * 1024, 2, 10, 4, 8);
        assert!(decision.use_parallel);
        assert!(decision.chunk_len * 2 * 14 <= policy.l2_cache_bytes);
        assert_eq!(decision.chunk_len * 2 % 64, 0);
    }

    #[test]
    fn test_default_policy_has_l2_cache() {
        let policy = ParallelPolicy::default();
        assert!(
            policy.l2_cache_bytes > 0,
            "L2 cache should be detected or defaulted"
        );
    }

    #[test]
    fn test_cache_aware_increases_chunk_count() {
        // Small L2 cache forces more chunks
        let policy = ParallelPolicy {
            l2_cache_bytes: 64 * 1024, // 64 KiB
            ..Default::default()
        };
        let decision = policy.decide(1024 * 1024, 10, 4, 8);
        // With 64 KiB L2 / 14 active shards, each chunk is about 4.5 KiB.
        // 1 MiB / 4.5 KiB is roughly 227 chunks, yielding many parallel jobs.
        assert!(decision.jobs > 1, "should parallelize with small cache");
    }

    #[test]
    fn test_l2_cache_zero_disables_cache_aware() {
        let policy = ParallelPolicy {
            l2_cache_bytes: 0,
            ..Default::default()
        };
        let decision = policy.decide(1024 * 1024, 10, 4, 8);
        // Should still work, just without cache-aware sizing
        assert!(decision.jobs >= 1);
    }

    #[test]
    fn test_with_l2_cache_bytes_builder() {
        let policy = ParallelPolicy::default().with_l2_cache_bytes(512 * 1024);
        assert_eq!(policy.l2_cache_bytes, 512 * 1024);
    }

    #[test]
    fn test_cache_aware_small_shard_no_effect() {
        // Shard smaller than min_parallel_shard_bytes → serial regardless
        let policy = ParallelPolicy {
            l2_cache_bytes: 256 * 1024,
            ..Default::default()
        };
        let decision = policy.decide(1024, 10, 4, 8);
        assert!(!decision.use_parallel);
    }
}
