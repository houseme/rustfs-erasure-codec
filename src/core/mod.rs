extern crate alloc;

#[cfg(feature = "std")]
pub(crate) mod cache_detect;
mod codec;
mod encode;
mod leopard;
pub(crate) mod leopard_gf16;
pub(crate) mod leopard_gf8;
mod metrics;
mod options;
#[cfg(feature = "std")]
mod parallel;
mod reconstruct;
mod recovery_plan;
mod shard_by_shard;
#[cfg(feature = "std")]
pub mod stream;
mod verify;
mod workspace;

use alloc::sync::Arc;
use alloc::vec::Vec;

use hashlink::LruCache;
#[cfg(feature = "std")]
use parking_lot::Mutex;
#[cfg(not(feature = "std"))]
use spin::Mutex;

use crate::Field;
use crate::matrix::Matrix;

pub use leopard::{LEOPARD_SHARD_MULTIPLE, leopard_aligned_shard_len};
#[cfg(feature = "std")]
pub use leopard_gf8::LeopardGf8ProfileStats;
#[cfg(feature = "std")]
pub(crate) use leopard_gf8::{leopard_gf8_profile_stats, reset_leopard_gf8_profile_stats};
#[cfg(feature = "std")]
pub use metrics::{ReconstructionCacheAnalysis, ReconstructionCacheStats, RuntimeProfileStats};
pub use options::{CodecFamily, CodecOptions, LeopardMode, MatrixMode};
#[cfg(feature = "std")]
pub(crate) use parallel::RuntimeParallelPolicyCache;
#[cfg(feature = "std")]
pub use parallel::{PARALLEL_POLICY_VERSION, ParallelDecision, ParallelPolicy};
pub(crate) use recovery_plan::{ClassicReconstructPlan, ReconstructionGoal};
pub use shard_by_shard::ShardByShard;
pub use workspace::VerifyWorkspace;

#[cfg(feature = "std")]
use metrics::{ReconstructionCacheMetrics, RuntimeProfileMetrics};

pub(crate) const DATA_DECODE_MATRIX_CACHE_MIN_CAPACITY: usize = 128;
pub(crate) const DATA_DECODE_MATRIX_CACHE_MAX_CAPACITY: usize = 4096;
pub(crate) const CODE_SLICE_MIN_CHUNK_BYTES: usize = 16 * 1024;
pub(crate) const CODE_SLICE_DEFAULT_CHUNK_BYTES: usize = 64 * 1024;
pub(crate) const CODE_SLICE_LARGE_CHUNK_BYTES: usize = 256 * 1024;
pub(crate) const VERIFY_INLINE_SCRATCH_ELEMS: usize = 4 * 1024;
#[cfg(feature = "std")]
pub(crate) const PARALLEL_MIN_SHARD_BYTES: usize = 256 * 1024;

#[derive(Debug)]
pub struct ReedSolomon<F: Field> {
    data_shard_count: usize,
    parity_shard_count: usize,
    total_shard_count: usize,
    codec_family: CodecFamily,
    // Classic uses an immutable shared matrix; FFT families keep an empty sentinel.
    matrix: Arc<Matrix<F>>,
    options: CodecOptions,
    #[cfg(feature = "std")]
    pub(crate) policy_cache: RuntimeParallelPolicyCache,
    data_decode_matrix_cache: Mutex<LruCache<Vec<usize>, Arc<Matrix<F>>>>,
    #[cfg(feature = "std")]
    reconstruction_cache_metrics: ReconstructionCacheMetrics,
    #[cfg(feature = "std")]
    runtime_profile_metrics: RuntimeProfileMetrics,
}

impl<F: Field> Clone for ReedSolomon<F> {
    fn clone(&self) -> ReedSolomon<F> {
        Self {
            data_shard_count: self.data_shard_count,
            parity_shard_count: self.parity_shard_count,
            total_shard_count: self.total_shard_count,
            codec_family: self.codec_family,
            matrix: Arc::clone(&self.matrix),
            options: self.options,
            #[cfg(feature = "std")]
            policy_cache: self.policy_cache,
            data_decode_matrix_cache: Mutex::new(LruCache::new(
                self.options.inversion_cache_capacity,
            )),
            #[cfg(feature = "std")]
            reconstruction_cache_metrics: ReconstructionCacheMetrics::default(),
            #[cfg(feature = "std")]
            runtime_profile_metrics: RuntimeProfileMetrics::default(),
        }
    }
}

impl<F: Field> PartialEq for ReedSolomon<F> {
    fn eq(&self, rhs: &ReedSolomon<F>) -> bool {
        self.data_shard_count == rhs.data_shard_count
            && self.parity_shard_count == rhs.parity_shard_count
            && self.codec_family == rhs.codec_family
            && (Arc::ptr_eq(&self.matrix, &rhs.matrix)
                || (self.matrix.row_count() == rhs.matrix.row_count()
                    && self.matrix.col_count() == rhs.matrix.col_count()
                    && (0..self.matrix.row_count())
                        .all(|row| self.matrix.get_row(row) == rhs.matrix.get_row(row))))
    }
}

impl<F: Field> ReedSolomon<F> {
    /// Shared immutable encoding matrix, used to validate reusable workspaces.
    #[cfg(feature = "std")]
    pub(crate) fn matrix_identity(&self) -> &Arc<Matrix<F>> {
        &self.matrix
    }

    pub(crate) fn is_leopard_gf8_family(&self) -> bool {
        self.codec_family == CodecFamily::LeopardGF8
    }

    pub(crate) fn is_leopard_gf16_family(&self) -> bool {
        self.codec_family == CodecFamily::LeopardGF16
    }

    /// True for any Leopard codec family (GF8 or GF16). Both decode through the
    /// Leopard FFT reconstruct path in [`ReedSolomon::reconstruct`], never the
    /// Classic inversion-matrix machinery — so the `*_opt` entry points must
    /// route either family to that serial dispatch rather than the Classic
    /// parallel plan.
    pub(crate) fn is_leopard_family(&self) -> bool {
        self.is_leopard_gf8_family() || self.is_leopard_gf16_family()
    }
}
