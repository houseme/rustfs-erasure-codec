extern crate alloc;

#[cfg(feature = "std")]
pub(crate) mod cache_detect;
mod codec;
mod encode;
mod format;
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

pub use format::{ENCODING_FORMAT_VERSION, EncodingFormat};
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
pub use workspace::{ReconstructWorkspace, VerifyWorkspace};

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

// Construction-only options live in EncodingFormat or are consumed during
// setup. Keep only the settings consulted by execution and its caches here.
#[derive(Debug, Clone, Copy)]
struct ExecutionOptions {
    fast_one_parity: bool,
    inversion_cache: bool,
    inversion_cache_capacity: usize,
    max_parallel_jobs: usize,
}

impl From<CodecOptions> for ExecutionOptions {
    fn from(options: CodecOptions) -> Self {
        Self {
            fast_one_parity: options.fast_one_parity,
            inversion_cache: options.inversion_cache,
            inversion_cache_capacity: options.inversion_cache_capacity,
            max_parallel_jobs: options.max_parallel_jobs,
        }
    }
}

#[derive(Debug)]
pub struct ReedSolomon<F: Field> {
    format: EncodingFormat<F>,
    options: ExecutionOptions,
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
            format: self.format.clone(),
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
        self.format == rhs.format
    }
}

impl<F: Field> ReedSolomon<F> {
    /// Borrow the canonical effective encoding format.
    ///
    /// Cloning this value shares the immutable matrix. Its optional persistent
    /// identifier is generated only when explicitly requested; it is not stored
    /// in shard payloads or constructed on encode/reconstruct hot paths.
    #[inline]
    pub fn encoding_format(&self) -> &EncodingFormat<F> {
        &self.format
    }

    pub(crate) fn is_leopard_gf8_family(&self) -> bool {
        self.format.codec_family() == CodecFamily::LeopardGF8
    }

    pub(crate) fn is_leopard_gf16_family(&self) -> bool {
        self.format.codec_family() == CodecFamily::LeopardGF16
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
