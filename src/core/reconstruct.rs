extern crate alloc;

use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;

use smallvec::SmallVec;

#[cfg(feature = "std")]
use super::recovery_plan::ClassicReconstructRows;
use super::{ClassicReconstructPlan, ReconstructWorkspace, ReconstructionGoal};
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
        matrix_rows: &(impl crate::matrix::MatrixRows<F::Elem> + Sync + ?Sized),
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

    /// After all data outputs succeed, replace parity survivor views with the
    /// complete systematic input order. Surviving data is the sorted prefix of
    /// the selected inputs; merge backwards so unread prefix views stay intact.
    fn use_recovered_data<'a, T: AsRef<[F::Elem]>>(
        inputs: &mut [&'a [F::Elem]],
        data_targets: &[usize],
        recovered_data: &'a [T],
    ) {
        debug_assert_eq!(data_targets.len(), recovered_data.len());
        let mut existing = inputs.len() - data_targets.len();
        let mut recovered = data_targets.len();
        for index in (0..inputs.len()).rev() {
            inputs[index] = if recovered != 0 && data_targets[recovered - 1] == index {
                recovered -= 1;
                recovered_data[recovered].as_ref()
            } else {
                existing -= 1;
                inputs[existing]
            };
        }
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
            plan.data_targets().len(),
            plan.parity_targets().len(),
            plan.shape.invalid_indices.is_empty(),
        );
        if !plan.has_outputs() {
            return Ok(());
        }
        let data_rows = plan.data_rows();
        let data_target_count = plan.data_targets().len();
        let parity_target_count = plan.parity_targets().len();
        let mut data_destinations: SmallVec<[&mut T; 32]> =
            SmallVec::with_capacity(data_target_count);
        let mut parity_destinations: SmallVec<[&mut T; 32]> =
            SmallVec::with_capacity(parity_target_count);
        // Only ordinary mixed recovery reads completed data outputs again.
        let recovered_capacity = if plan.direct_parity || parity_target_count == 0 {
            0
        } else {
            data_target_count
        };
        let mut recovered_data: SmallVec<[&[F::Elem]; 32]> =
            SmallVec::with_capacity(recovered_capacity);
        // Drop the merged input views before the output owners they borrow.
        let mut inputs: SmallVec<[&[F::Elem]; 32]> =
            SmallVec::with_capacity(plan.shape.valid_indices.len());
        let mut survivors = plan.shape.valid_indices.iter().copied().peekable();
        let mut data_targets = plan.data_targets().iter().copied().peekable();
        let mut parity_targets = plan.parity_targets().iter().copied().peekable();
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
        if !plan.direct_parity && !recovered_data.is_empty() {
            Self::use_recovered_data(&mut inputs, plan.data_targets(), &recovered_data);
        }
        for (destination, row) in parity_destinations.iter_mut().zip(plan.parity_rows(self)) {
            self.reconstruct_classic_output(
                &mut **destination,
                row,
                &inputs,
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
        mut compute: impl FnMut(
            bool,
            &ClassicReconstructRows<'_, F>,
            &[&[F::Elem]],
            &mut [&mut [F::Elem]],
        ),
    ) -> Result<(), Error> {
        #[cfg(feature = "std")]
        self.runtime_profile_metrics.record_reconstruct(
            plan.data_only,
            plan.data_targets().len(),
            plan.parity_targets().len(),
            plan.shape.invalid_indices.is_empty(),
        );
        if !plan.has_outputs() {
            return Ok(());
        }
        let rows = plan.row_views(self);
        let data_target_count = plan.data_targets().len();
        let parity_target_count = plan.parity_targets().len();
        let (data_rows, parity_rows) = rows.split_at(data_target_count);
        // Composed rows execute as one data-then-parity stage. Collect those
        // disjoint views directly into its final table instead of extending a
        // second wide table after collection.
        let data_output_count = if plan.direct_parity {
            data_target_count + parity_target_count
        } else {
            data_target_count
        };
        let parity_output_count = if plan.direct_parity {
            0
        } else {
            parity_target_count
        };
        let mut data_outputs: SmallVec<[&mut [F::Elem]; 32]> =
            SmallVec::with_capacity(data_output_count);
        let mut parity_outputs: SmallVec<[&mut [F::Elem]; 32]> =
            SmallVec::with_capacity(parity_output_count);
        let mut inputs: SmallVec<[&[F::Elem]; 32]> =
            SmallVec::with_capacity(plan.shape.valid_indices.len());
        let mut survivors = plan.shape.valid_indices.iter().copied().peekable();
        let mut data_targets = plan.data_targets().iter().copied().peekable();
        let mut parity_targets = plan.parity_targets().iter().copied().peekable();

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
                if plan.direct_parity {
                    data_outputs.push(output);
                } else {
                    parity_outputs.push(output);
                }
                parity_targets.next();
            }
        }
        if plan.direct_parity {
            #[cfg(feature = "std")]
            {
                if data_target_count != 0 {
                    self.runtime_profile_metrics
                        .record_reconstruct_data_stage(plan.shape.shard_len, data_target_count);
                }
                self.runtime_profile_metrics
                    .record_reconstruct_parity_stage(plan.shape.shard_len, parity_target_count);
            }
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
        if !data_outputs.is_empty() {
            // Composed selective parity already returned above. Ordinary mixed
            // recovery can reuse the finished data outputs and survivor views.
            Self::use_recovered_data(&mut inputs, plan.data_targets(), &data_outputs);
        }
        compute(false, &parity_rows, &inputs, &mut parity_outputs);
        Ok(())
    }

    /// Prepare an owned plan for repeated full Classic reconstruction.
    ///
    /// The plan retains the inverse even when the inversion cache is disabled
    /// or evicts its entry. It can be used with any [`ReconstructShard`] having
    /// the same missing pattern and length, including caller-owned
    /// [`crate::ShardSlot`] buffers. It does not borrow the input shards.
    /// Leopard families return [`Error::UnsupportedCodecFamily`].
    pub fn prepare_reconstruct_workspace<T: ReconstructShard<F>>(
        &self,
        shards: &[T],
    ) -> Result<ReconstructWorkspace<F>, Error> {
        self.prepare_reconstruct_workspace_with_goal(shards, ReconstructionGoal::All)
    }

    /// Prepare a Classic plan that restores missing data and leaves parity missing.
    ///
    /// All present lengths and the complete erasure layout are still validated.
    /// Reuse it with [`Self::reconstruct_with_workspace`] for the same layout.
    pub fn prepare_reconstruct_data_workspace<T: ReconstructShard<F>>(
        &self,
        shards: &[T],
    ) -> Result<ReconstructWorkspace<F>, Error> {
        self.prepare_reconstruct_workspace_with_goal(shards, ReconstructionGoal::Data)
    }

    /// Prepare a Classic plan for only the requested missing shards.
    ///
    /// Accepts a data-only or complete `required` mask, with the same validation
    /// order as [`Self::reconstruct_some`]. Requested targets and composed parity
    /// rows are owned by the plan, so the mask can be changed or dropped later.
    /// Unrequested erasures remain part of the validated input layout.
    ///
    /// ```
    /// use rustfs_erasure_codec::galois_8::ReedSolomon;
    /// let codec = ReedSolomon::new(4, 3)?;
    /// let mut shards = vec![vec![2u8; 65]; 7];
    /// codec.encode(&mut shards)?;
    /// let mut shards: Vec<_> = shards.into_iter().map(Some).collect();
    /// for index in [0, 1, 4] { shards[index] = None; }
    /// let mut required = vec![false; 7];
    /// required[4] = true;
    /// let plan = codec.prepare_reconstruct_some_workspace(&shards, &required)?;
    /// required.fill(false);
    /// codec.reconstruct_with_workspace(&mut shards, &plan)?;
    /// assert!(shards[0].is_none() && shards[1].is_none());
    /// assert!(shards[4].is_some());
    /// assert_eq!(plan.parity_targets(), &[4]);
    /// # Ok::<(), rustfs_erasure_codec::Error>(())
    /// ```
    pub fn prepare_reconstruct_some_workspace<T: ReconstructShard<F>>(
        &self,
        shards: &[T],
        required: &[bool],
    ) -> Result<ReconstructWorkspace<F>, Error> {
        if required.len() != self.data_shard_count() && required.len() != self.total_shard_count() {
            return Err(Error::InvalidShardFlags);
        }
        self.prepare_reconstruct_workspace_with_goal(shards, ReconstructionGoal::Required(required))
    }

    fn prepare_reconstruct_workspace_with_goal<T: ReconstructShard<F>>(
        &self,
        shards: &[T],
        goal: ReconstructionGoal<'_>,
    ) -> Result<ReconstructWorkspace<F>, Error> {
        check_piece_count!(all => self, shards);
        if self.is_leopard_family() {
            return Err(Error::UnsupportedCodecFamily);
        }
        let shape = self.classic_reconstruct_shape(shards.iter().map(ReconstructShard::len))?;
        Ok(ReconstructWorkspace {
            plan: self.prepare_classic_reconstruct_plan(shape, goal)?,
            format: self.encoding_format().clone(),
        })
    }

    /// Execute the missing-shard outputs selected by an owned Classic plan.
    ///
    /// Validates shard counts, effective format, every present length and the
    /// complete missing pattern before initializing any destination. Format
    /// mismatches return [`Error::InvalidCustomMatrix`]; a different missing
    /// pattern returns [`Error::InvalidShardFlags`]. Equivalent codecs may reuse
    /// the plan. The existing serial executor and fallible-output publication
    /// order are preserved; no inversion-cache lookup or plan rebuild runs here.
    ///
    /// ```
    /// use rustfs_erasure_codec::{galois_16::ReedSolomon, ShardSlot};
    /// let codec = ReedSolomon::new(4, 2)?;
    /// let mut shards = vec![vec![[3u8, 7]; 65]; 6];
    /// codec.encode(&mut shards)?;
    /// let mut slots: Vec<_> = shards.into_iter().map(ShardSlot::new_present).collect();
    /// slots[0].mark_missing();
    /// let workspace = codec.prepare_reconstruct_workspace(&slots)?;
    /// for _ in 0..3 {
    ///     slots[0].mark_missing();
    ///     codec.reconstruct_with_workspace(&mut slots, &workspace)?;
    /// }
    /// # Ok::<(), rustfs_erasure_codec::Error>(())
    /// ```
    pub fn reconstruct_with_workspace<T: ReconstructShard<F>>(
        &self,
        shards: &mut [T],
        workspace: &ReconstructWorkspace<F>,
    ) -> Result<(), Error> {
        self.validate_reconstruct_workspace(shards, workspace)?;
        self.execute_classic_plan_serial(shards, &workspace.plan)
    }

    pub(crate) fn validate_reconstruct_workspace<T: ReconstructShard<F>>(
        &self,
        shards: &[T],
        workspace: &ReconstructWorkspace<F>,
    ) -> Result<(), Error> {
        check_piece_count!(all => self, shards);
        if self.is_leopard_family() {
            return Err(Error::UnsupportedCodecFamily);
        }
        if workspace.format.data_shard_count() < self.data_shard_count() {
            return Err(Error::TooFewDataShards);
        }
        if workspace.format.data_shard_count() > self.data_shard_count() {
            return Err(Error::TooManyDataShards);
        }
        if workspace.format.parity_shard_count() < self.parity_shard_count() {
            return Err(Error::TooFewParityShards);
        }
        if workspace.format.parity_shard_count() > self.parity_shard_count() {
            return Err(Error::TooManyParityShards);
        }
        if self.encoding_format() != &workspace.format {
            return Err(Error::InvalidCustomMatrix);
        }
        self.validate_classic_plan_layout(
            shards.iter().map(ReconstructShard::len),
            &workspace.plan.shape,
        )
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

    /// Substitute surviving systematic data before solving parity equations.
    /// All Classic constructors, including custom matrices, have identity data
    /// rows. Only the missing-data coefficients need pivot elimination; survivor
    /// order and the complete inverse representation remain unchanged.
    fn build_data_decode_matrix(
        &self,
        valid_indices: &[usize],
    ) -> Result<crate::matrix::Matrix<F>, Error> {
        use crate::matrix::Matrix;

        let data_count = self.data_shard_count();
        debug_assert_eq!(valid_indices.len(), data_count);
        debug_assert!(valid_indices.windows(2).all(|pair| pair[0] < pair[1]));
        let matrix = self.encoding_format().matrix();
        let present_count = valid_indices.partition_point(|&index| index < data_count);
        if present_count + 1 == data_count {
            let missing = valid_indices[..present_count]
                .iter()
                .enumerate()
                .find_map(|(index, &valid)| (index != valid).then_some(index))
                .unwrap_or(present_count);
            let parity_row = matrix.get_row(valid_indices[present_count]);
            let pivot = parity_row[missing];
            if pivot == F::zero() {
                return Err(Error::InvalidCustomMatrix);
            }
            let reciprocal = F::div(F::one(), pivot);
            let mut decode = Matrix::new(data_count, data_count);
            for (column, &index) in valid_indices[..present_count].iter().enumerate() {
                decode.set(index, column, F::one());
                // In characteristic two, subtracting the known data terms
                // from the parity equation uses the field's addition.
                decode.set(missing, column, F::mul(reciprocal, parity_row[index]));
            }
            decode.set(missing, present_count, reciprocal);
            return Ok(decode);
        }

        let missing_count = data_count - present_count;
        if present_count != 0 && missing_count > 1 {
            let mut missing: SmallVec<[usize; 32]> = SmallVec::new();
            let mut known = valid_indices[..present_count].iter().copied().peekable();
            for index in 0..data_count {
                if known.peek() == Some(&index) {
                    known.next();
                } else {
                    missing.push(index);
                }
            }

            // Selected survivor equations are [I 0; B C] after ordering
            // columns as known/unknown data. Eliminate only C in [C | B | I]
            // to obtain [I | C^-1 B | C^-1]. In characteristic two the known
            // terms use addition. The right-hand side keeps original input
            // order: known systematic shards, then selected parity shards.
            let mut equations = Matrix::<F>::new(missing_count, missing_count + data_count);
            for (row, &index) in valid_indices[present_count..].iter().enumerate() {
                let coefficients = matrix.get_row(index);
                for (column, &index) in missing.iter().enumerate() {
                    equations.set(row, column, coefficients[index]);
                }
                for (column, &index) in valid_indices[..present_count].iter().enumerate() {
                    equations.set(row, missing_count + column, coefficients[index]);
                }
                equations.set(row, missing_count + present_count + row, F::one());
            }
            equations
                .gaussian_elim()
                .map_err(|_| Error::InvalidCustomMatrix)?;

            let mut decode = Matrix::new(data_count, data_count);
            for (column, &index) in valid_indices[..present_count].iter().enumerate() {
                decode.set(index, column, F::one());
            }
            for (row, &index) in missing.iter().enumerate() {
                for (column, &value) in equations.get_row(row)[missing_count..].iter().enumerate() {
                    decode.set(index, column, value);
                }
            }
            return Ok(decode);
        }

        // With no surviving systematic data, every equation is unknown: keep
        // the existing dense inverse and its full SIMD row widths. The single
        // equation above retains its direct reciprocal without a work matrix.
        let mut selected = Matrix::new(data_count, data_count);
        for (row, &index) in valid_indices.iter().enumerate() {
            for column in 0..data_count {
                selected.set(row, column, matrix.get(index, column));
            }
        }
        selected.invert().map_err(|_| Error::InvalidCustomMatrix)
    }

    pub(crate) fn get_data_decode_matrix(
        &self,
        valid_indices: &[usize],
        invalid_indices: &[usize],
    ) -> Result<Arc<crate::matrix::Matrix<F>>, Error> {
        if valid_indices.len() != self.data_shard_count() {
            return Err(Error::TooFewShardsPresent);
        }

        // Ordered erasures before the last selected survivor uniquely determine
        // the first k inputs. Later missing parity never participates in this
        // inverse, so keep it out of both the lookup and insertion keys. The
        // common pattern without a trailing erasure keeps its original slice.
        let cache_erasures = match (valid_indices.last(), invalid_indices.last()) {
            (Some(&last_selected), Some(&last_missing))
                if self.options.inversion_cache && last_missing > last_selected =>
            {
                &invalid_indices[..invalid_indices.partition_point(|&index| index < last_selected)]
            }
            _ => invalid_indices,
        };

        if self.options.inversion_cache {
            #[cfg(feature = "std")]
            self.reconstruction_cache_metrics
                .requests
                .fetch_add(1, Ordering::Relaxed);

            let mut cache = self.data_decode_matrix_cache.lock();
            if let Some(entry) = cache.get(cache_erasures) {
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
        let data_decode_matrix = Arc::new(self.build_data_decode_matrix(valid_indices)?);
        if self.options.inversion_cache {
            let data_decode_matrix = data_decode_matrix.clone();
            let mut cache = self.data_decode_matrix_cache.lock();
            #[cfg(feature = "std")]
            let before_len = cache.len();
            #[cfg(feature = "std")]
            let capacity = cache.capacity();
            let previous = cache.insert(Vec::from(cache_erasures), data_decode_matrix);
            #[cfg(feature = "std")]
            let evicted = capacity > 0 && before_len >= capacity && previous.is_none();
            // A concurrent miss may have inserted the same pattern while we
            // inverted the matrix. Replacing that entry does not evict an LRU.
            drop(previous);
            #[cfg(feature = "std")]
            if evicted {
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

#[cfg(test)]
mod tests;
