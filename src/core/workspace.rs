extern crate alloc;

use alloc::vec;
use alloc::vec::Vec;

use crate::Field;

use super::{ClassicReconstructPlan, EncodingFormat, ReedSolomon};

/// Reusable parity scratch space for repeated verify calls.
///
/// This helper keeps the parity buffer allocation outside of `verify` so
/// repeated callers can naturally take the `verify_with_buffer` fast path
/// without having to manage `Vec<Vec<_>>` details themselves.
#[derive(PartialEq, Debug, Clone)]
pub struct VerifyWorkspace<F: Field> {
    parity: Vec<Vec<F::Elem>>,
}

impl<F: Field> VerifyWorkspace<F> {
    /// Create a new workspace with parity buffers sized for the given codec and shard length.
    pub fn new(codec: &ReedSolomon<F>, shard_len: usize) -> Self {
        let mut parity = Vec::with_capacity(codec.parity_shard_count());
        for _ in 0..codec.parity_shard_count() {
            parity.push(vec![F::zero(); shard_len]);
        }
        Self { parity }
    }

    /// Returns the number of parity shard buffers.
    pub fn parity_shards(&self) -> usize {
        self.parity.len()
    }

    /// Returns the current shard buffer length, or `None` if there are no parity shards.
    pub fn shard_len(&self) -> Option<usize> {
        self.parity.first().map(Vec::len)
    }

    /// Resize parity buffers to match the given codec and shard length.
    pub fn resize(&mut self, codec: &ReedSolomon<F>, shard_len: usize) {
        if self.parity.len() < codec.parity_shard_count() {
            self.parity
                .reserve(codec.parity_shard_count() - self.parity.len());
            while self.parity.len() < codec.parity_shard_count() {
                self.parity.push(Vec::new());
            }
        } else if self.parity.len() > codec.parity_shard_count() {
            self.parity.truncate(codec.parity_shard_count());
        }

        for shard in &mut self.parity {
            shard.resize(shard_len, F::zero());
        }
    }

    pub(crate) fn prepare(&mut self, codec: &ReedSolomon<F>, shard_len: usize) {
        if self.parity.len() != codec.parity_shard_count()
            || self.shard_len() != Some(shard_len)
            || self.parity.iter().any(|shard| shard.len() != shard_len)
        {
            self.resize(codec, shard_len);
        }
    }

    pub(crate) fn as_mut_shards(&mut self) -> &mut [Vec<F::Elem>] {
        &mut self.parity
    }
}

/// An owned, immutable plan for repeated Classic reconstruction.
///
/// Choose [`ReedSolomon::prepare_reconstruct_workspace`] for every missing shard,
/// [`ReedSolomon::prepare_reconstruct_data_workspace`] for data, or
/// [`ReedSolomon::prepare_reconstruct_some_workspace`] for requested targets.
/// [`ReedSolomon::reconstruct_with_workspace`] executes the prepared targets.
/// The plan owns its format, erasure layout, requested targets, any inverse and
/// composed parity rows independently of the codec's inversion cache. It owns
/// no shard payload or references to caller storage.
/// Reuse requires the same counts, effective matrix, missing pattern and length;
/// these are checked before execution on every call.
///
/// Pair this plan with [`crate::ShardSlot`] to retain destination buffers across
/// calls. Execution can still allocate view metadata for wide layouts, and
/// allocating shard containers retain their normal initialization behavior.
/// Leopard families are unsupported. The serial API is available in `no_std`
/// and adds no `Send` or `Sync` bounds to the field or shard container.
pub struct ReconstructWorkspace<F: Field> {
    pub(crate) plan: ClassicReconstructPlan<F>,
    pub(crate) format: EncodingFormat<F>,
}

impl<F: Field> ReconstructWorkspace<F> {
    /// Missing data shards requested by this plan, sorted by shard index.
    pub fn data_targets(&self) -> &[usize] {
        self.plan.data_targets()
    }

    /// Missing parity shards requested by this plan, sorted by shard index.
    pub fn parity_targets(&self) -> &[usize] {
        self.plan.parity_targets()
    }

    /// Canonical format for which this reconstruction plan was prepared.
    pub fn encoding_format(&self) -> &EncodingFormat<F> {
        &self.format
    }
}
