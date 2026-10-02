extern crate alloc;

use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;

use smallvec::SmallVec;

use crate::errors::Error;
use crate::{Field, ReconstructShard};

#[cfg(feature = "std")]
use rayon::prelude::*;
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
    pub(crate) fn code_some_slices_par_raw(
        &self,
        matrix_rows: &[&[F::Elem]],
        inputs: &[&[F::Elem]],
        outputs: &mut [&mut [F::Elem]],
    ) where
        F::Elem: Send + Sync,
    {
        let shard_len = inputs.first().map(|input| input.len()).unwrap_or(0);
        if shard_len == 0 {
            return;
        }

        if outputs.len() <= 2 {
            self.code_some_slices_one_or_two_outputs_reconstruct_data_par_raw(
                matrix_rows,
                inputs,
                outputs,
            );
            return;
        }

        let decision = self.parallel_policy(shard_len, outputs.len());
        if !decision.use_parallel {
            self.code_some_slices_chunked(matrix_rows, inputs, outputs);
            return;
        }
        self.code_some_slices_par_chunked(matrix_rows, inputs, outputs, decision.chunk_len);
    }

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

        if outputs.len() <= 2 {
            self.code_some_slices_one_or_two_outputs_reconstruct_data_par_raw(
                matrix_rows,
                inputs,
                outputs,
            );
            return;
        }

        let decision = policy.decide(
            shard_len,
            self.data_shard_count,
            outputs.len(),
            self.policy_cache.available_parallelism,
        );
        self.runtime_profile_metrics
            .record_parallel_policy(decision);
        if !decision.use_parallel {
            self.code_some_slices_chunked(matrix_rows, inputs, outputs);
            return;
        }
        self.code_some_slices_par_chunked(matrix_rows, inputs, outputs, decision.chunk_len);
    }

    #[cfg(feature = "std")]
    pub(crate) fn code_some_slices_two_outputs_reconstruct_data_par_raw(
        &self,
        matrix_rows: &[&[F::Elem]],
        inputs: &[&[F::Elem]],
        outputs: &mut [&mut [F::Elem]],
    ) where
        F::Elem: Send + Sync,
    {
        debug_assert_eq!(2, matrix_rows.len());
        debug_assert_eq!(2, outputs.len());

        let shard_len = inputs.first().map(|input| input.len()).unwrap_or(0);
        if shard_len == 0 {
            return;
        }

        let decision = self.parallel_policy(shard_len, outputs.len());
        self.runtime_profile_metrics
            .record_parallel_policy(decision);
        if !decision.use_parallel {
            self.code_some_slices_chunked(matrix_rows, inputs, outputs);
            return;
        }

        let chunk_len = if self.data_shard_count <= 16 {
            core::cmp::min(shard_len, core::cmp::max(decision.chunk_len, 512 * 1024))
        } else {
            decision.chunk_len
        };
        self.runtime_profile_metrics.record_code_some(
            true,
            shard_len,
            inputs.len(),
            outputs.len(),
            chunk_len,
        );

        let data_shard_count = self.data_shard_count;
        outputs
            .par_iter_mut()
            .enumerate()
            .for_each(|(i_row, output)| {
                let matrix_row = matrix_rows[i_row];

                let mut start = 0;
                while start < shard_len {
                    let end = core::cmp::min(start + chunk_len, shard_len);
                    let output_chunk = &mut output[start..end];

                    F::mul_slice(matrix_row[0], &inputs[0][start..end], output_chunk);
                    for i_input in 1..data_shard_count {
                        F::mul_slice_add(
                            matrix_row[i_input],
                            &inputs[i_input][start..end],
                            output_chunk,
                        );
                    }

                    start = end;
                }
            });
    }

    #[cfg(feature = "std")]
    pub(crate) fn code_some_slices_one_or_two_outputs_reconstruct_data_par_raw(
        &self,
        matrix_rows: &[&[F::Elem]],
        inputs: &[&[F::Elem]],
        outputs: &mut [&mut [F::Elem]],
    ) where
        F::Elem: Send + Sync,
    {
        debug_assert!((1..=2).contains(&outputs.len()));

        if outputs.len() == 1 {
            let shard_len = inputs.first().map(|input| input.len()).unwrap_or(0);
            if shard_len == 0 {
                return;
            }

            let decision = self.parallel_policy(shard_len, outputs.len());
            self.runtime_profile_metrics
                .record_parallel_policy(decision);
            if !decision.use_parallel {
                self.code_some_slices_chunked(matrix_rows, inputs, outputs);
                return;
            }

            self.runtime_profile_metrics.record_code_some(
                true,
                shard_len,
                inputs.len(),
                outputs.len(),
                decision.chunk_len,
            );

            let data_shard_count = self.data_shard_count;
            let matrix_row = matrix_rows[0];
            outputs[0]
                .par_chunks_mut(decision.chunk_len)
                .enumerate()
                .for_each(|(chunk_idx, output_chunk)| {
                    let start = chunk_idx * decision.chunk_len;
                    let end = start + output_chunk.len();

                    F::mul_slice(matrix_row[0], &inputs[0][start..end], output_chunk);
                    for i_input in 1..data_shard_count {
                        F::mul_slice_add(
                            matrix_row[i_input],
                            &inputs[i_input][start..end],
                            output_chunk,
                        );
                    }
                });
            return;
        }

        self.code_some_slices_two_outputs_reconstruct_data_par_raw(matrix_rows, inputs, outputs);
    }

    #[cfg(feature = "std")]
    pub(crate) fn reconstruct_internal_option_vec_par(
        &self,
        shards: &mut [Option<Vec<F::Elem>>],
        data_only: bool,
    ) -> Result<(), Error>
    where
        F::Elem: Send + Sync,
    {
        let (data_policy, parity_policy) = self.policy_cache.reconstruct_stage_policies(data_only);
        self.reconstruct_internal_option_vec_par_with_stage_policies(
            shards,
            data_only,
            data_policy,
            parity_policy,
        )
    }

    #[cfg(feature = "std")]
    pub(crate) fn reconstruct_internal_option_vec_par_with_stage_policies(
        &self,
        shards: &mut [Option<Vec<F::Elem>>],
        data_only: bool,
        data_policy: ParallelPolicy,
        parity_policy: ParallelPolicy,
    ) -> Result<(), Error>
    where
        F::Elem: Send + Sync,
    {
        check_piece_count!(all => self, shards);

        let data_shard_count = self.data_shard_count;

        let mut number_present = 0;
        let mut shard_len = None;
        for shard in shards.iter() {
            if let Some(shard) = shard.as_ref() {
                let len = shard.len();
                if len == 0 {
                    return Err(Error::EmptyShard);
                }
                number_present += 1;
                if let Some(old_len) = shard_len
                    && len != old_len
                {
                    return Err(Error::IncorrectShardSize);
                }
                shard_len = Some(len);
            }
        }

        if number_present == self.total_shard_count {
            self.runtime_profile_metrics
                .record_reconstruct(data_only, 0, 0, true);
            return Ok(());
        }
        if number_present < data_shard_count {
            return Err(Error::TooFewShardsPresent);
        }

        // invariant: reached only when number_present >= data_shard_count >= 1
        // (see the TooFewDataShards guard in new()), so at least one present
        // shard has set shard_len to Some.
        let shard_len = shard_len.expect("at least one shard present; qed");

        let mut valid_indices: SmallVec<[usize; 32]> = SmallVec::with_capacity(data_shard_count);
        let mut invalid_indices: SmallVec<[usize; 32]> =
            SmallVec::with_capacity(self.total_shard_count);
        let mut missing_data_indices: SmallVec<[usize; 32]> = SmallVec::new();
        let mut missing_parity_indices: SmallVec<[usize; 32]> = SmallVec::new();

        for (matrix_row, shard) in shards.iter().enumerate() {
            match shard.as_ref() {
                Some(_shard) => {
                    if valid_indices.len() < data_shard_count {
                        valid_indices.push(matrix_row);
                    }
                }
                None => {
                    invalid_indices.push(matrix_row);
                    if matrix_row < data_shard_count {
                        missing_data_indices.push(matrix_row);
                    } else if !data_only {
                        missing_parity_indices.push(matrix_row);
                    }
                }
            }
        }

        self.runtime_profile_metrics.record_reconstruct(
            data_only,
            missing_data_indices.len(),
            missing_parity_indices.len(),
            false,
        );

        let data_decode_matrix = self.get_data_decode_matrix(&valid_indices, &invalid_indices)?;

        if !missing_data_indices.is_empty() {
            #[cfg(feature = "std")]
            self.runtime_profile_metrics
                .record_reconstruct_data_stage(shard_len, missing_data_indices.len());
            let mut matrix_rows: SmallVec<[&[F::Elem]; 32]> =
                SmallVec::with_capacity(missing_data_indices.len());
            for &idx in &missing_data_indices {
                matrix_rows.push(data_decode_matrix.get_row(idx));
            }

            let mut recovered_data: Vec<Vec<F::Elem>> = missing_data_indices
                .iter()
                .map(|_| vec![F::zero(); shard_len])
                .collect();

            {
                let mut sub_shards: SmallVec<[&[F::Elem]; 32]> =
                    SmallVec::with_capacity(data_shard_count);
                for &idx in &valid_indices {
                    let shard = shards[idx].as_deref().ok_or(Error::TooFewShardsPresent)?;
                    sub_shards.push(shard);
                }

                let mut outputs: SmallVec<[&mut [F::Elem]; 32]> = recovered_data
                    .iter_mut()
                    .map(|shard| shard.as_mut_slice())
                    .collect();

                if data_only && outputs.len() <= 2 {
                    self.runtime_profile_metrics
                        .record_reconstruct_data_small_output_specialized();
                    self.code_some_slices_one_or_two_outputs_reconstruct_data_par_raw(
                        &matrix_rows,
                        &sub_shards,
                        &mut outputs,
                    );
                } else {
                    self.code_some_slices_with_policy_raw(
                        &matrix_rows,
                        &sub_shards,
                        &mut outputs,
                        data_policy,
                    );
                }
            }

            for (idx, recovered) in missing_data_indices.into_iter().zip(recovered_data) {
                shards[idx] = Some(recovered);
            }
        }

        if data_only {
            return Ok(());
        }

        if missing_parity_indices.is_empty() {
            return Ok(());
        }

        #[cfg(feature = "std")]
        self.runtime_profile_metrics
            .record_reconstruct_parity_stage(shard_len, missing_parity_indices.len());
        let parity_rows = self.get_parity_rows();
        let mut matrix_rows: SmallVec<[&[F::Elem]; 32]> =
            SmallVec::with_capacity(missing_parity_indices.len());
        for &idx in &missing_parity_indices {
            matrix_rows.push(parity_rows[idx - data_shard_count]);
        }

        let mut recovered_parity: Vec<Vec<F::Elem>> = missing_parity_indices
            .iter()
            .map(|_| vec![F::zero(); shard_len])
            .collect();

        {
            let mut all_data: SmallVec<[&[F::Elem]; 32]> =
                SmallVec::with_capacity(data_shard_count);
            for shard in shards.iter().take(data_shard_count) {
                let shard = shard.as_deref().ok_or(Error::TooFewShardsPresent)?;
                all_data.push(shard);
            }

            let mut outputs: SmallVec<[&mut [F::Elem]; 32]> = recovered_parity
                .iter_mut()
                .map(|shard| shard.as_mut_slice())
                .collect();

            self.code_some_slices_with_policy_raw(
                &matrix_rows,
                &all_data,
                &mut outputs,
                parity_policy,
            );
        }
        for (idx, recovered) in missing_parity_indices.into_iter().zip(recovered_parity) {
            shards[idx] = Some(recovered);
        }
        Ok(())
    }

    #[cfg(feature = "std")]
    pub(crate) fn reconstruct_internal_option_vec_par_with_policy(
        &self,
        shards: &mut [Option<Vec<F::Elem>>],
        data_only: bool,
        policy: ParallelPolicy,
    ) -> Result<(), Error>
    where
        F::Elem: Send + Sync,
    {
        self.reconstruct_internal_option_vec_par_with_stage_policies(
            shards, data_only, policy, policy,
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
        self.ensure_classic_family_execution()?;
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
        self.ensure_classic_family_execution()?;
        self.reconstruct_internal(slices, true)
    }

    /// Reconstruct only shards marked `true` in the `required` mask.
    pub fn reconstruct_some<T: ReconstructShard<F>>(
        &self,
        shards: &mut [T],
        required: &[bool],
    ) -> Result<(), Error> {
        let normalized_required;
        let required = if required.len() == self.total_shard_count {
            required
        } else if required.len() == self.data_shard_count {
            normalized_required = {
                let mut flags = vec![false; self.total_shard_count];
                flags[..self.data_shard_count].copy_from_slice(required);
                flags
            };
            normalized_required.as_slice()
        } else {
            return Err(Error::InvalidShardFlags);
        };

        if self.is_leopard_gf8_family() {
            return self.reconstruct_leopard_impl(
                shards,
                |index| required[index],
                super::leopard_gf8::reconstruct_with_tables,
            );
        }
        if self.is_leopard_gf16_family() {
            return self.reconstruct_leopard_impl(
                shards,
                |index| required[index],
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
        self.ensure_classic_family_execution()?;

        check_piece_count!(all => self, shards);

        let mut number_present = 0;
        let mut shard_len = None;

        for shard in shards.iter_mut() {
            if let Some(len) = shard.len() {
                if len == 0 {
                    return Err(Error::EmptyShard);
                }
                number_present += 1;
                if let Some(old_len) = shard_len
                    && len != old_len
                {
                    return Err(Error::IncorrectShardSize);
                }
                shard_len = Some(len);
            }
        }

        if number_present == self.total_shard_count {
            return Ok(());
        }

        if number_present < self.data_shard_count {
            return Err(Error::TooFewShardsPresent);
        }

        // invariant: reached only when number_present >= data_shard_count >= 1
        // (see the TooFewDataShards guard in new()), so at least one present
        // shard has set shard_len to Some.
        let shard_len = shard_len.expect("at least one shard present; qed");
        let required_data_only = required
            .iter()
            .enumerate()
            .all(|(i, required)| !*required || i < self.data_shard_count);

        let originally_missing: Vec<bool> = shards
            .iter_mut()
            .map(|shard| shard.get().is_none())
            .collect();

        if required_data_only {
            let mut valid_indices: SmallVec<[usize; 32]> =
                SmallVec::with_capacity(self.data_shard_count);
            let mut invalid_indices: SmallVec<[usize; 32]> =
                SmallVec::with_capacity(self.total_shard_count);
            let mut required_missing_data_indices: SmallVec<[usize; 32]> = SmallVec::new();

            for (index, is_missing) in originally_missing.iter().copied().enumerate() {
                if is_missing {
                    invalid_indices.push(index);
                    if index < self.data_shard_count && required[index] {
                        required_missing_data_indices.push(index);
                    }
                } else if valid_indices.len() < self.data_shard_count {
                    valid_indices.push(index);
                }
            }

            if required_missing_data_indices.is_empty() {
                return Ok(());
            }

            let data_decode_matrix =
                self.get_data_decode_matrix(&valid_indices, &invalid_indices)?;
            let mut matrix_rows: SmallVec<[&[F::Elem]; 32]> =
                SmallVec::with_capacity(required_missing_data_indices.len());
            for &idx in &required_missing_data_indices {
                matrix_rows.push(data_decode_matrix.get_row(idx));
            }

            // Borrow present input shards directly and defer output mutation until
            // after compute completes, so the required-data-only path avoids
            // cloning every valid shard into temporary owned buffers.
            let mut sub_shard_ptrs: SmallVec<[(*const F::Elem, usize); 32]> =
                SmallVec::with_capacity(valid_indices.len());
            for &idx in &valid_indices {
                let shard = shards[idx]
                    .get()
                    .expect("valid shard index must be present");
                sub_shard_ptrs.push((shard.as_ptr(), shard.len()));
            }

            let sub_shards: SmallVec<[&[F::Elem]; 32]> = sub_shard_ptrs
                .iter()
                .map(|&(ptr, len)| {
                    // SAFETY: each pointer/length pair was captured from a present
                    // shard in `shards` after full length validation. The compute
                    // phase only reads from these inputs and writes into separate
                    // owned recovery buffers, so the pointed-to shard storage
                    // remains valid and unmodified for the duration of these
                    // borrowed slices.
                    unsafe { core::slice::from_raw_parts(ptr, len) }
                })
                .collect();
            let mut output_ptrs: SmallVec<[(*mut F::Elem, usize); 32]> =
                SmallVec::with_capacity(required_missing_data_indices.len());
            for &idx in &required_missing_data_indices {
                match shards[idx].get_or_initialize(shard_len) {
                    Ok(dst) | Err(Ok(dst)) => output_ptrs.push((dst.as_mut_ptr(), dst.len())),
                    Err(Err(err)) => return Err(err),
                }
            }
            let mut outputs: SmallVec<[&mut [F::Elem]; 32]> = output_ptrs
                .iter()
                .map(|&(ptr, len)| {
                    // SAFETY: each pointer/length pair came from a distinct
                    // `get_or_initialize(shard_len)` call above after all
                    // validation completed. Required missing indices are unique,
                    // and this compute phase mutates only those output buffers.
                    unsafe { core::slice::from_raw_parts_mut(ptr, len) }
                })
                .collect();
            self.code_some_slices(&matrix_rows, &sub_shards, &mut outputs);
            drop(outputs);
        } else {
            let mut working: Vec<Option<Vec<F::Elem>>> = shards
                .iter_mut()
                .map(|shard| shard.get().map(|data| data.to_vec()))
                .collect();
            self.reconstruct(&mut working)?;

            for (i, shard) in shards.iter_mut().enumerate() {
                if !required[i] || !originally_missing[i] {
                    continue;
                }

                let recovered = working[i]
                    .as_ref()
                    .expect("recovered shard must be present");
                match shard.get_or_initialize(shard_len) {
                    Ok(dst) | Err(Ok(dst)) => dst.copy_from_slice(recovered),
                    Err(Err(err)) => return Err(err),
                }
            }
        }

        Ok(())
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
        if number_present < self.data_shard_count {
            return Err(Error::TooFewShardsPresent);
        }
        if !has_required_missing {
            return Ok(());
        }
        let shard_len = shard_len.ok_or(Error::TooFewShardsPresent)?;

        let mut present: SmallVec<[bool; 32]> = SmallVec::with_capacity(slices.len());
        let mut input_data: SmallVec<[Option<&[u8]>; 32]> = SmallVec::with_capacity(slices.len());
        let mut recovered: Vec<Vec<u8>> = Vec::with_capacity(slices.len());
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
            self.data_shard_count,
            self.parity_shard_count,
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
            |index| !data_only || index < self.data_shard_count,
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
            |index| !data_only || index < self.data_shard_count,
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
        if valid_indices.len() != self.data_shard_count {
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
            crate::matrix::Matrix::new(self.data_shard_count, self.data_shard_count);
        for (sub_matrix_row, &valid_index) in valid_indices.iter().enumerate() {
            for c in 0..self.data_shard_count {
                sub_matrix.set(sub_matrix_row, c, self.matrix.get(valid_index, c));
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
        check_piece_count!(all => self, shards);

        let data_shard_count = self.data_shard_count;

        let mut number_present = 0;
        let mut shard_len = None;

        for shard in shards.iter_mut() {
            if let Some(len) = shard.len() {
                if len == 0 {
                    return Err(Error::EmptyShard);
                }
                number_present += 1;
                if let Some(old_len) = shard_len
                    && len != old_len
                {
                    return Err(Error::IncorrectShardSize);
                }
                shard_len = Some(len);
            }
        }

        if number_present == self.total_shard_count {
            #[cfg(feature = "std")]
            self.runtime_profile_metrics
                .record_reconstruct(data_only, 0, 0, true);
            return Ok(());
        }

        if number_present < data_shard_count {
            return Err(Error::TooFewShardsPresent);
        }

        // invariant: reached only when number_present >= data_shard_count >= 1
        // (see the TooFewDataShards guard in new()), so at least one present
        // shard has set shard_len to Some.
        let shard_len = shard_len.expect("at least one shard present; qed");

        let mut sub_shards: SmallVec<[&[F::Elem]; 32]> = SmallVec::with_capacity(data_shard_count);
        let mut missing_data_slices: SmallVec<[&mut [F::Elem]; 32]> =
            SmallVec::with_capacity(self.parity_shard_count);
        let mut missing_parity_slices: SmallVec<[&mut [F::Elem]; 32]> =
            SmallVec::with_capacity(self.parity_shard_count);
        let mut valid_indices: SmallVec<[usize; 32]> = SmallVec::with_capacity(data_shard_count);
        let mut invalid_indices: SmallVec<[usize; 32]> = SmallVec::with_capacity(data_shard_count);

        for (matrix_row, shard) in shards.iter_mut().enumerate() {
            let shard_data = if matrix_row >= data_shard_count && data_only {
                shard.get().ok_or(None)
            } else {
                shard.get_or_initialize(shard_len).map_err(Some)
            };

            match shard_data {
                Ok(shard) => {
                    if sub_shards.len() < data_shard_count {
                        sub_shards.push(shard);
                        valid_indices.push(matrix_row);
                    }
                }
                Err(None) => {
                    invalid_indices.push(matrix_row);
                }
                Err(Some(x)) => {
                    let shard = x?;
                    if matrix_row < data_shard_count {
                        missing_data_slices.push(shard);
                    } else {
                        missing_parity_slices.push(shard);
                    }

                    invalid_indices.push(matrix_row);
                }
            }
        }

        #[cfg(feature = "std")]
        {
            let missing_data_count = invalid_indices
                .iter()
                .filter(|&&i| i < data_shard_count)
                .count();
            let missing_parity_count = if data_only {
                0
            } else {
                invalid_indices
                    .iter()
                    .filter(|&&i| i >= data_shard_count)
                    .count()
            };
            self.runtime_profile_metrics.record_reconstruct(
                data_only,
                missing_data_count,
                missing_parity_count,
                false,
            );
        }

        let data_decode_matrix = self.get_data_decode_matrix(&valid_indices, &invalid_indices)?;

        let mut matrix_rows: SmallVec<[&[F::Elem]; 32]> =
            SmallVec::with_capacity(self.parity_shard_count);

        for i_slice in invalid_indices
            .iter()
            .cloned()
            .take_while(|i| i < &data_shard_count)
        {
            matrix_rows.push(data_decode_matrix.get_row(i_slice));
        }

        #[cfg(feature = "std")]
        self.runtime_profile_metrics
            .record_reconstruct_data_stage(shard_len, matrix_rows.len());
        self.code_some_slices(&matrix_rows, &sub_shards, &mut missing_data_slices);

        if data_only {
            Ok(())
        } else {
            let mut matrix_rows: SmallVec<[&[F::Elem]; 32]> =
                SmallVec::with_capacity(self.parity_shard_count);
            let parity_rows = self.get_parity_rows();

            for i_slice in invalid_indices
                .iter()
                .cloned()
                .skip_while(|i| i < &data_shard_count)
            {
                matrix_rows.push(parity_rows[i_slice - data_shard_count]);
            }
            #[cfg(feature = "std")]
            self.runtime_profile_metrics
                .record_reconstruct_parity_stage(shard_len, matrix_rows.len());
            {
                let mut i_old_data_slice = 0;
                let mut i_new_data_slice = 0;

                let mut all_data_slices: SmallVec<[&[F::Elem]; 32]> =
                    SmallVec::with_capacity(data_shard_count);

                let mut next_maybe_good = 0;
                let mut push_good_up_to = move |data_slices: &mut SmallVec<_>, up_to| {
                    for _ in next_maybe_good..up_to {
                        data_slices.push(sub_shards[i_old_data_slice]);
                        i_old_data_slice += 1;
                    }

                    next_maybe_good = up_to + 1;
                };

                for i_slice in invalid_indices
                    .iter()
                    .cloned()
                    .take_while(|i| i < &data_shard_count)
                {
                    push_good_up_to(&mut all_data_slices, i_slice);
                    all_data_slices.push(missing_data_slices[i_new_data_slice]);
                    i_new_data_slice += 1;
                }
                push_good_up_to(&mut all_data_slices, data_shard_count);

                self.code_some_slices(&matrix_rows, &all_data_slices, &mut missing_parity_slices);
            }

            Ok(())
        }
    }
}
