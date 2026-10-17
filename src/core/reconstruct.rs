extern crate alloc;

use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;

use smallvec::SmallVec;

use super::{ClassicReconstructPlan, ReconstructionGoal};
use crate::errors::Error;
use crate::{Field, ReconstructShard};

#[cfg(feature = "std")]
use std::sync::atomic::Ordering;

#[cfg(not(feature = "std"))]
use super::ReedSolomon;
#[cfg(feature = "std")]
use super::{ParallelPolicy, ReedSolomon};

/// Validate native decoder buffers before any FFT work or output mutation.
/// An empty output denotes a present or unrequested shard; it remains part of
/// the erasure pattern even when no recovered bytes are requested for it.
pub(super) fn validate_leopard_decode_buffers(
    present: &[bool],
    outputs: &[&mut [u8]],
    input_data: &[Option<&[u8]>],
    data_shards: usize,
    parity_shards: usize,
) -> Result<usize, Error> {
    if data_shards == 0 {
        return Err(Error::TooFewDataShards);
    }
    if parity_shards == 0 {
        return Err(Error::TooFewParityShards);
    }
    let total = data_shards
        .checked_add(parity_shards)
        .ok_or(Error::TooManyShards)?;
    if present.len() != total || outputs.len() != total || input_data.len() != total {
        return Err(Error::IncorrectShardSize);
    }
    let mut shard_size = None;
    let mut number_present = 0;
    for (is_present, input) in present.iter().zip(input_data) {
        if *is_present {
            let data = input.ok_or(Error::TooFewShardsPresent)?;
            if data.is_empty() {
                return Err(Error::EmptyShard);
            }
            if !data.len().is_multiple_of(64) || shard_size.is_some_and(|len| len != data.len()) {
                return Err(Error::IncorrectShardSize);
            }
            shard_size = Some(data.len());
            number_present += 1;
        }
    }
    if number_present < data_shards {
        return Err(Error::TooFewShardsPresent);
    }
    let shard_size = shard_size.ok_or(Error::TooFewShardsPresent)?;
    if outputs
        .iter()
        .any(|output| !output.is_empty() && output.len() != shard_size)
    {
        return Err(Error::IncorrectShardSize);
    }
    Ok(shard_size)
}

impl<F: Field> ReedSolomon<F> {
    #[cfg(feature = "std")]
    pub(crate) fn code_some_slices_with_policy_raw(
        &self,
        matrix_rows: &[&[F::Elem]],
        inputs: &[&[F::Elem]],
        outputs: &mut [&mut [F::Elem]],
        policy: ParallelPolicy,
    ) where
        F::Elem: Send + Sync,
    {
        let shard_len = inputs.first().map(|input| input.len()).unwrap_or(0);
        if shard_len == 0 {
            return;
        }

        let decision = policy.decide_for_elements(
            shard_len,
            core::mem::size_of::<F::Elem>(),
            self.data_shard_count(),
            outputs.len(),
            self.policy_cache.available_parallelism,
        );
        self.runtime_profile_metrics
            .record_parallel_policy(decision);
        if !decision.use_parallel {
            self.code_some_slices_chunked(matrix_rows, inputs, outputs);
            return;
        }
        self.code_some_slices_par_chunked_with_jobs(
            matrix_rows,
            inputs,
            outputs,
            decision.chunk_len,
            decision.jobs,
        );
    }

    #[cfg(feature = "std")]
    pub(crate) fn execute_classic_plan_parallel(
        &self,
        shards: &mut [Option<Vec<F::Elem>>],
        plan: &ClassicReconstructPlan<F>,
        data_policy: ParallelPolicy,
        parity_policy: ParallelPolicy,
    ) -> Result<(), Error>
    where
        F::Elem: Send + Sync,
    {
        self.execute_classic_option_plan_bulk(shards, plan, |data_stage, rows, inputs, outputs| {
            self.code_some_slices_with_policy_raw(
                rows,
                inputs,
                outputs,
                if data_stage {
                    data_policy
                } else {
                    parity_policy
                },
            );
        })
    }

    #[cfg(feature = "std")]
    pub(crate) fn execute_classic_option_plan_serial(
        &self,
        shards: &mut [Option<Vec<F::Elem>>],
        plan: &ClassicReconstructPlan<F>,
    ) -> Result<(), Error> {
        self.execute_classic_option_plan_bulk(shards, plan, |_, rows, inputs, outputs| {
            self.code_some_slices(rows, inputs, outputs);
        })
    }

    fn reconstruct_classic_output<'a, T: ReconstructShard<F>>(
        &self,
        destination: &'a mut T,
        row: &[F::Elem],
        inputs: &[&[F::Elem]],
        shard_len: usize,
    ) -> Result<&'a [F::Elem], Error> {
        let output = match destination.get_or_initialize(shard_len) {
            Ok(output) | Err(Ok(output)) => output,
            Err(Err(error)) => return Err(error),
        };
        if output.len() != shard_len {
            return Err(Error::IncorrectShardSize);
        }
        self.code_some_slices(&[row], inputs, &mut [&mut *output]);
        Ok(output)
    }

    pub(crate) fn execute_classic_plan_serial<T: ReconstructShard<F>>(
        &self,
        shards: &mut [T],
        plan: &ClassicReconstructPlan<F>,
    ) -> Result<(), Error> {
        #[cfg(feature = "std")]
        self.runtime_profile_metrics.record_reconstruct(
            plan.data_only,
            plan.data_targets.len(),
            plan.parity_targets.len(),
            plan.shape.invalid_indices.is_empty(),
        );
        if !plan.has_outputs() {
            return Ok(());
        }
        let data_rows = plan.data_rows();
        let mut inputs: SmallVec<[&[F::Elem]; 32]> = SmallVec::new();
        let mut data_destinations: SmallVec<[&mut T; 32]> = SmallVec::new();
        let mut parity_destinations: SmallVec<[&mut T; 32]> = SmallVec::new();
        let mut survivors = plan.shape.valid_indices.iter().copied().peekable();
        let mut data_targets = plan.data_targets.iter().copied().peekable();
        let mut parity_targets = plan.parity_targets.iter().copied().peekable();
        for (index, shard) in shards.iter_mut().enumerate() {
            if survivors.peek() == Some(&index) {
                let input = shard.get().ok_or(Error::TooFewShardsPresent)?;
                if input.len() != plan.shape.shard_len {
                    return Err(Error::IncorrectShardSize);
                }
                inputs.push(input);
                survivors.next();
            } else if data_targets.peek() == Some(&index) {
                data_destinations.push(shard);
                data_targets.next();
            } else if parity_targets.peek() == Some(&index) {
                parity_destinations.push(shard);
                parity_targets.next();
            }
        }
        // Generic destinations may reject their backing-buffer length. Finish
        // each initialized output before touching the next, so a later error
        // never leaves an earlier destination marked present with stale bytes.
        let mut recovered_data: SmallVec<[&[F::Elem]; 32]> = SmallVec::new();
        #[cfg(feature = "std")]
        if !data_destinations.is_empty() {
            self.runtime_profile_metrics
                .record_reconstruct_data_stage(plan.shape.shard_len, data_destinations.len());
        }
        let mut data_outputs =
            data_destinations
                .iter_mut()
                .zip(data_rows)
                .map(|(destination, row)| {
                    self.reconstruct_classic_output(
                        &mut **destination,
                        row,
                        &inputs,
                        plan.shape.shard_len,
                    )
                });
        if plan.direct_parity || parity_destinations.is_empty() {
            // Data-only and composed parity never read the recovered data again.
            // Consume the same checked executor without building a view list.
            data_outputs.try_for_each(|output| output.map(|_| ()))?;
        } else {
            // Ordinary mixed recovery uses restored data to encode parity.
            for output in data_outputs {
                recovered_data.push(output?);
            }
        }
        if parity_destinations.is_empty() {
            return Ok(());
        }
        #[cfg(feature = "std")]
        self.runtime_profile_metrics
            .record_reconstruct_parity_stage(plan.shape.shard_len, parity_destinations.len());
        let mut all_data: SmallVec<[&[F::Elem]; 32]> = SmallVec::new();
        let parity_inputs = if plan.direct_parity || recovered_data.is_empty() {
            inputs.as_slice()
        } else {
            let mut existing = 0;
            let mut recovered = 0;
            for index in 0..self.data_shard_count() {
                if plan.data_targets.get(recovered) == Some(&index) {
                    all_data.push(recovered_data[recovered]);
                    recovered += 1;
                } else {
                    all_data.push(inputs[existing]);
                    existing += 1;
                }
            }
            all_data.as_slice()
        };
        for (destination, row) in parity_destinations.iter_mut().zip(plan.parity_rows(self)) {
            self.reconstruct_classic_output(
                &mut **destination,
                row,
                parity_inputs,
                plan.shape.shard_len,
            )?;
        }
        Ok(())
    }

    #[cfg(feature = "std")]
    fn execute_classic_option_plan_bulk(
        &self,
        shards: &mut [Option<Vec<F::Elem>>],
        plan: &ClassicReconstructPlan<F>,
        mut compute: impl FnMut(bool, &[&[F::Elem]], &[&[F::Elem]], &mut [&mut [F::Elem]]),
    ) -> Result<(), Error> {
        #[cfg(feature = "std")]
        self.runtime_profile_metrics.record_reconstruct(
            plan.data_only,
            plan.data_targets.len(),
            plan.parity_targets.len(),
            plan.shape.invalid_indices.is_empty(),
        );
        if !plan.has_outputs() {
            return Ok(());
        }
        let data_rows: SmallVec<[&[F::Elem]; 32]> = plan.data_rows().collect();
        let parity_rows: SmallVec<[&[F::Elem]; 32]> = plan.parity_rows(self).collect();
        let mut inputs: SmallVec<[&[F::Elem]; 32]> = SmallVec::new();
        let mut data_outputs: SmallVec<[&mut [F::Elem]; 32]> = SmallVec::new();
        let mut parity_outputs: SmallVec<[&mut [F::Elem]; 32]> = SmallVec::new();
        let mut survivors = plan.shape.valid_indices.iter().copied().peekable();
        let mut data_targets = plan.data_targets.iter().copied().peekable();
        let mut parity_targets = plan.parity_targets.iter().copied().peekable();

        // Concrete Option<Vec<_>> destinations cannot reject initialization:
        // their new buffers have exactly the validated shard length. Keep the
        // fused bulk kernels here while generic fallible containers use the
        // per-output adapter above. Both borrow disjoint views without copying
        // present payloads or constructing raw pointers.
        for (index, shard) in shards.iter_mut().enumerate() {
            if survivors.peek() == Some(&index) {
                let input = shard.as_deref().ok_or(Error::TooFewShardsPresent)?;
                debug_assert_eq!(input.len(), plan.shape.shard_len);
                inputs.push(input);
                survivors.next();
            } else if data_targets.peek() == Some(&index) {
                let output = shard
                    .get_or_insert_with(|| vec![F::zero(); plan.shape.shard_len])
                    .as_mut_slice();
                debug_assert_eq!(output.len(), plan.shape.shard_len);
                data_outputs.push(output);
                data_targets.next();
            } else if parity_targets.peek() == Some(&index) {
                let output = shard
                    .get_or_insert_with(|| vec![F::zero(); plan.shape.shard_len])
                    .as_mut_slice();
                debug_assert_eq!(output.len(), plan.shape.shard_len);
                parity_outputs.push(output);
                parity_targets.next();
            }
        }
        if plan.direct_parity {
            #[cfg(feature = "std")]
            {
                if !data_outputs.is_empty() {
                    self.runtime_profile_metrics
                        .record_reconstruct_data_stage(plan.shape.shard_len, data_outputs.len());
                }
                self.runtime_profile_metrics
                    .record_reconstruct_parity_stage(plan.shape.shard_len, parity_outputs.len());
            }
            let mut rows = data_rows;
            rows.extend(parity_rows);
            data_outputs.extend(parity_outputs);
            compute(true, &rows, &inputs, &mut data_outputs);
            return Ok(());
        }
        if !data_outputs.is_empty() {
            #[cfg(feature = "std")]
            self.runtime_profile_metrics
                .record_reconstruct_data_stage(plan.shape.shard_len, data_outputs.len());
            compute(true, &data_rows, &inputs, &mut data_outputs);
        }
        if parity_outputs.is_empty() {
            return Ok(());
        }
        #[cfg(feature = "std")]
        self.runtime_profile_metrics
            .record_reconstruct_parity_stage(plan.shape.shard_len, parity_outputs.len());
        if data_outputs.is_empty() {
            compute(false, &parity_rows, &inputs, &mut parity_outputs);
        } else {
            // Full recovery first restores missing data, then encodes parity.
            // Selective parity uses composed rows instead, so no unrequested
            // missing data is initialized solely as a dependency.
            let mut all_data: SmallVec<[&[F::Elem]; 32]> = SmallVec::new();
            let mut existing = 0;
            let mut recovered = 0;
            for index in 0..self.data_shard_count() {
                if plan.data_targets.get(recovered) == Some(&index) {
                    all_data.push(data_outputs[recovered]);
                    recovered += 1;
                } else {
                    all_data.push(inputs[existing]);
                    existing += 1;
                }
            }
            compute(false, &parity_rows, &all_data, &mut parity_outputs);
        }
        Ok(())
    }

    /// Reconstruct all missing shards (data and parity).
    ///
    /// Missing shards should be zero-length; present shards must have valid data.
    pub fn reconstruct<T: ReconstructShard<F>>(&self, slices: &mut [T]) -> Result<(), Error> {
        if self.is_leopard_gf8_family() {
            return self.reconstruct_leopard_gf8(slices, false);
        }
        if self.is_leopard_gf16_family() {
            return self.reconstruct_leopard_gf16(slices, false);
        }
        self.reconstruct_internal(slices, false)
    }

    /// Reconstruct only missing data shards (parity shards are not recovered).
    pub fn reconstruct_data<T: ReconstructShard<F>>(&self, slices: &mut [T]) -> Result<(), Error> {
        if self.is_leopard_gf8_family() {
            return self.reconstruct_leopard_gf8(slices, true);
        }
        if self.is_leopard_gf16_family() {
            return self.reconstruct_leopard_gf16(slices, true);
        }
        self.reconstruct_internal(slices, true)
    }

    /// Reconstruct only shards marked `true` in the `required` mask.
    pub fn reconstruct_some<T: ReconstructShard<F>>(
        &self,
        shards: &mut [T],
        required: &[bool],
    ) -> Result<(), Error> {
        if required.len() != self.data_shard_count() && required.len() != self.total_shard_count() {
            return Err(Error::InvalidShardFlags);
        }

        if self.is_leopard_gf8_family() {
            return self.reconstruct_leopard_impl(
                shards,
                |index| required.get(index).copied().unwrap_or(false),
                super::leopard_gf8::reconstruct_with_tables,
            );
        }
        if self.is_leopard_gf16_family() {
            return self.reconstruct_leopard_impl(
                shards,
                |index| required.get(index).copied().unwrap_or(false),
                |present, outputs, inputs, data, parity| {
                    super::leopard::leopard_gf16_reconstruct(
                        present,
                        outputs,
                        inputs,
                        data,
                        parity,
                        self.options.max_parallel_jobs,
                    )
                },
            );
        }
        self.with_classic_reconstruct_plan(
            shards,
            ReconstructionGoal::Required(required),
            |shards, plan| self.execute_classic_plan_serial(shards, plan),
        )
    }

    /// Validate every present shard before initializing recovery destinations.
    /// Missing, unrequested shards remain erasures in the FFT decoder but have
    /// no output buffer, so only requested shards are allocated or published.
    fn reconstruct_leopard_impl<T: ReconstructShard<F>>(
        &self,
        slices: &mut [T],
        required: impl Fn(usize) -> bool,
        reconstruct_fn: impl FnOnce(
            &[bool],
            &mut [&mut [u8]],
            &[Option<&[u8]>],
            usize,
            usize,
        ) -> Result<(), Error>,
    ) -> Result<(), Error> {
        check_piece_count!(all => self, slices);

        let mut shard_len = None;
        let mut number_present = 0;
        let mut has_required_missing = false;
        for (index, shard) in slices.iter_mut().enumerate() {
            let declared_len = shard.len();
            if let Some(data) = shard.get() {
                let len = data.len();
                if len == 0 {
                    return Err(Error::EmptyShard);
                }
                if declared_len != Some(len)
                    || shard_len.is_some_and(|previous| previous != len)
                    || !len.is_multiple_of(64)
                {
                    return Err(Error::IncorrectShardSize);
                }
                let bytes = F::as_gf8_slice(data).ok_or(Error::UnsupportedCodecFamily)?;
                if bytes.len() != len {
                    return Err(Error::IncorrectShardSize);
                }
                shard_len = Some(len);
                number_present += 1;
            } else {
                if declared_len.is_some() {
                    return Err(Error::IncorrectShardSize);
                }
                has_required_missing |= required(index);
            }
        }
        if number_present < self.data_shard_count() {
            return Err(Error::TooFewShardsPresent);
        }
        if !has_required_missing {
            return Ok(());
        }
        let shard_len = shard_len.ok_or(Error::TooFewShardsPresent)?;

        let mut present: SmallVec<[bool; 32]> = SmallVec::with_capacity(slices.len());
        let mut input_data: SmallVec<[Option<&[u8]>; 32]> = SmallVec::with_capacity(slices.len());
        let mut recovered: SmallVec<[Vec<u8>; 32]> = SmallVec::with_capacity(slices.len());
        for (index, shard) in slices.iter_mut().enumerate() {
            let data = shard.get();
            let is_present = data.is_some();
            present.push(is_present);
            if let Some(data) = data {
                let bytes = F::as_gf8_slice(data).ok_or(Error::UnsupportedCodecFamily)?;
                if data.len() != shard_len || bytes.len() != shard_len {
                    return Err(Error::IncorrectShardSize);
                }
                input_data.push(Some(bytes));
                recovered.push(Vec::new());
            } else {
                input_data.push(None);
                recovered.push(if required(index) {
                    vec![0; shard_len]
                } else {
                    Vec::new()
                });
            }
        }
        let mut outputs: SmallVec<[&mut [u8]; 32]> =
            recovered.iter_mut().map(Vec::as_mut_slice).collect();
        reconstruct_fn(
            &present,
            &mut outputs,
            &input_data,
            self.data_shard_count(),
            self.parity_shard_count(),
        )?;
        drop(outputs);
        drop(input_data);

        // A destination can reject initialization (for example a short fixed
        // buffer). Decode first so that every slot marked present during
        // publication immediately receives valid bytes, even if a later slot
        // rejects its buffer. Unrequested shards are never initialized.
        for (shard, bytes) in slices.iter_mut().zip(recovered) {
            if bytes.is_empty() {
                continue;
            }
            let data = match shard.get_or_initialize(shard_len) {
                Ok(data) | Err(Ok(data)) => data,
                Err(Err(error)) => return Err(error),
            };
            if data.len() != shard_len {
                return Err(Error::IncorrectShardSize);
            }
            let destination = F::as_gf8_slice_mut(data).ok_or(Error::UnsupportedCodecFamily)?;
            if destination.len() != shard_len {
                return Err(Error::IncorrectShardSize);
            }
            destination.copy_from_slice(&bytes);
        }
        Ok(())
    }

    fn reconstruct_leopard_gf8<T: ReconstructShard<F>>(
        &self,
        slices: &mut [T],
        data_only: bool,
    ) -> Result<(), Error> {
        self.reconstruct_leopard_impl(
            slices,
            |index| !data_only || index < self.data_shard_count(),
            super::leopard_gf8::reconstruct_with_tables,
        )
    }

    fn reconstruct_leopard_gf16<T: ReconstructShard<F>>(
        &self,
        slices: &mut [T],
        data_only: bool,
    ) -> Result<(), Error> {
        self.reconstruct_leopard_impl(
            slices,
            |index| !data_only || index < self.data_shard_count(),
            |present, outputs, inputs, data, parity| {
                super::leopard::leopard_gf16_reconstruct(
                    present,
                    outputs,
                    inputs,
                    data,
                    parity,
                    self.options.max_parallel_jobs,
                )
            },
        )
    }

    pub(crate) fn get_data_decode_matrix(
        &self,
        valid_indices: &[usize],
        invalid_indices: &[usize],
    ) -> Result<Arc<crate::matrix::Matrix<F>>, Error> {
        if valid_indices.len() != self.data_shard_count() {
            return Err(Error::TooFewShardsPresent);
        }

        if self.options.inversion_cache {
            #[cfg(feature = "std")]
            self.reconstruction_cache_metrics
                .requests
                .fetch_add(1, Ordering::Relaxed);

            let mut cache = self.data_decode_matrix_cache.lock();
            if let Some(entry) = cache.get(invalid_indices) {
                #[cfg(feature = "std")]
                self.reconstruction_cache_metrics
                    .hits
                    .fetch_add(1, Ordering::Relaxed);
                return Ok(entry.clone());
            }

            #[cfg(feature = "std")]
            self.reconstruction_cache_metrics
                .misses
                .fetch_add(1, Ordering::Relaxed);
        }
        let mut sub_matrix =
            crate::matrix::Matrix::new(self.data_shard_count(), self.data_shard_count());
        for (sub_matrix_row, &valid_index) in valid_indices.iter().enumerate() {
            for c in 0..self.data_shard_count() {
                sub_matrix.set(
                    sub_matrix_row,
                    c,
                    self.encoding_format().matrix().get(valid_index, c),
                );
            }
        }
        let data_decode_matrix = Arc::new(
            sub_matrix
                .invert()
                .map_err(|_| Error::InvalidCustomMatrix)?,
        );
        if self.options.inversion_cache {
            let data_decode_matrix = data_decode_matrix.clone();
            let mut cache = self.data_decode_matrix_cache.lock();
            #[cfg(feature = "std")]
            let before_len = cache.len();
            #[cfg(feature = "std")]
            let capacity = cache.capacity();
            cache.insert(Vec::from(invalid_indices), data_decode_matrix);
            #[cfg(feature = "std")]
            if capacity > 0 && before_len >= capacity {
                self.reconstruction_cache_metrics
                    .evictions
                    .fetch_add(1, Ordering::Relaxed);
            }
            #[cfg(feature = "std")]
            self.reconstruction_cache_metrics
                .inserts
                .fetch_add(1, Ordering::Relaxed);
        }
        Ok(data_decode_matrix)
    }

    fn reconstruct_internal<T: ReconstructShard<F>>(
        &self,
        shards: &mut [T],
        data_only: bool,
    ) -> Result<(), Error> {
        let goal = if data_only {
            ReconstructionGoal::Data
        } else {
            ReconstructionGoal::All
        };
        self.with_classic_reconstruct_plan(shards, goal, |shards, plan| {
            self.execute_classic_plan_serial(shards, plan)
        })
    }
}
