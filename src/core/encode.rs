extern crate alloc;

use alloc::vec;
use smallvec::SmallVec;

use crate::galois_8::codegen::SelectedEncodeKernel;

use crate::Field;
use crate::errors::Error;

use super::{
    CODE_SLICE_DEFAULT_CHUNK_BYTES, CODE_SLICE_LARGE_CHUNK_BYTES, CODE_SLICE_MIN_CHUNK_BYTES,
    ReedSolomon, leopard,
};

/// Checked native byte views shared by Classic generated kernels and Leopard.
/// Failed preparation never modifies an output shard.
struct ByteShardViews<'a> {
    data: SmallVec<[&'a [u8]; 32]>,
    parity: SmallVec<[&'a mut [u8]; 32]>,
}

impl<'a> ByteShardViews<'a> {
    #[inline]
    fn prepare<F, T, U>(data: &'a [T], parity: &'a mut [U], shard_len: usize) -> Result<Self, Error>
    where
        F: Field,
        F::Elem: 'a,
        T: AsRef<[F::Elem]>,
        U: AsMut<[F::Elem]>,
    {
        let mut data_bytes = SmallVec::with_capacity(data.len());
        for input in data {
            let slice = input.as_ref();
            if slice.len() != shard_len {
                return Err(Error::IncorrectShardSize);
            }
            let bytes = F::as_gf8_slice(slice).ok_or(Error::UnsupportedCodecFamily)?;
            if bytes.len() != shard_len {
                return Err(Error::IncorrectShardSize);
            }
            data_bytes.push(bytes);
        }
        let mut parity_bytes = SmallVec::with_capacity(parity.len());
        for output in parity {
            let slice = output.as_mut();
            if slice.len() != shard_len {
                return Err(Error::IncorrectShardSize);
            }
            let bytes = F::as_gf8_slice_mut(slice).ok_or(Error::UnsupportedCodecFamily)?;
            if bytes.len() != shard_len {
                return Err(Error::IncorrectShardSize);
            }
            parity_bytes.push(bytes);
        }
        Ok(Self {
            data: data_bytes,
            parity: parity_bytes,
        })
    }
}

#[derive(Clone, Copy)]
enum NativeEncodeExecution {
    Serial,
    #[cfg(all(feature = "std", rse_aarch64_neon))]
    Parallel(super::ParallelDecision),
}

impl NativeEncodeExecution {
    #[cfg(feature = "std")]
    fn for_parallel_decision(
        data_shards: usize,
        parity_shards: usize,
        shard_len: usize,
        decision: super::ParallelDecision,
    ) -> Option<Self> {
        if !decision.use_parallel {
            return Some(Self::Serial);
        }
        // Preserve the measured activation boundary. Other shapes, sizes,
        // budgets, and architectures continue through the generic scheduler.
        #[cfg(rse_aarch64_neon)]
        if (data_shards, parity_shards) == (10, 4)
            && shard_len >= 1024 * 1024
            && (2..=4).contains(&decision.jobs)
        {
            return Some(Self::Parallel(decision));
        }
        let _ = (data_shards, parity_shards, shard_len);
        None
    }
}

struct PreparedNativeEncode<'a> {
    kernel: SelectedEncodeKernel,
    execution: NativeEncodeExecution,
    rows: SmallVec<[&'a [u8]; 4]>,
    shards: ByteShardViews<'a>,
    shard_len: usize,
}

impl PreparedNativeEncode<'_> {
    /// Execute in place so the inline shard views are not copied into another
    /// stack frame. Errors never switch algorithms after parity has been written.
    fn execute(&mut self) -> Result<usize, Error> {
        let Self {
            kernel,
            execution,
            rows,
            shards,
            shard_len,
        } = self;
        let ByteShardViews { data, parity } = shards;
        match *execution {
            NativeEncodeExecution::Serial => {
                kernel.execute(rows, data, parity, *shard_len)?;
                Ok(*shard_len)
            }
            #[cfg(all(feature = "std", rse_aarch64_neon))]
            NativeEncodeExecution::Parallel(decision) => {
                use rayon::prelude::*;

                let mut jobs = split_neon_encode_jobs(parity, *shard_len, decision.jobs);
                let chunk_len = decision.chunk_len.max(1);
                let effective_chunk_len = chunk_len.min(jobs[0].outputs[0].len());
                jobs.par_iter_mut().try_for_each(|job| {
                    let job_len = job.outputs[0].len();
                    for offset in (0..job_len).step_by(chunk_len) {
                        let end = offset.saturating_add(chunk_len).min(job_len);
                        let inputs: SmallVec<[&[u8]; 12]> = data
                            .iter()
                            .map(|input| &input[job.start + offset..job.start + end])
                            .collect();
                        let mut outputs: SmallVec<[&mut [u8]; 4]> = job
                            .outputs
                            .iter_mut()
                            .map(|output| &mut output[offset..end])
                            .collect();
                        kernel.execute(rows, &inputs, &mut outputs, end - offset)?;
                    }
                    Ok::<_, Error>(())
                })?;
                Ok(effective_chunk_len)
            }
        }
    }
}

#[cfg(all(feature = "std", rse_aarch64_neon))]
struct NeonEncodeJob<'a> {
    start: usize,
    outputs: SmallVec<[&'a mut [u8]; 4]>,
}

// Native parallel eligibility permits at most four jobs and four outputs;
// both dimensions stay on the stack without reserving unused 32-job storage.
#[cfg(all(feature = "std", rse_aarch64_neon))]
fn split_neon_encode_jobs<'a>(
    parity: &'a mut [&mut [u8]],
    shard_len: usize,
    job_limit: usize,
) -> SmallVec<[NeonEncodeJob<'a>; 4]> {
    let group_len = shard_len.div_ceil(job_limit).div_ceil(64) * 64;
    let job_count = shard_len.div_ceil(group_len);
    let mut jobs: SmallVec<[NeonEncodeJob<'a>; 4]> = (0..job_count)
        .map(|index| NeonEncodeJob {
            start: index * group_len,
            outputs: SmallVec::new(),
        })
        .collect();
    for output in parity {
        for (job, chunk) in jobs.iter_mut().zip(output.chunks_mut(group_len)) {
            job.outputs.push(chunk);
        }
    }
    jobs
}

impl<F: Field> ReedSolomon<F> {
    pub(crate) fn code_some_slices<T: AsRef<[F::Elem]>, U: AsMut<[F::Elem]>>(
        &self,
        matrix_rows: &[&[F::Elem]],
        inputs: &[T],
        outputs: &mut [U],
    ) {
        self.code_some_slices_chunked(matrix_rows, inputs, outputs);
    }

    pub(crate) fn code_some_slices_chunked<T: AsRef<[F::Elem]>, U: AsMut<[F::Elem]>>(
        &self,
        matrix_rows: &[&[F::Elem]],
        inputs: &[T],
        outputs: &mut [U],
    ) {
        let shard_len = inputs
            .first()
            .map(|input| input.as_ref().len())
            .unwrap_or(0);
        if shard_len == 0 {
            return;
        }

        let chunk_len = self.code_chunk_len(shard_len);
        #[cfg(feature = "std")]
        self.runtime_profile_metrics.record_code_some(
            false,
            shard_len,
            inputs.len(),
            outputs.len(),
            chunk_len,
        );
        let mut start = 0;
        while start < shard_len {
            let end = core::cmp::min(start + chunk_len, shard_len);
            for (i_input, input) in inputs.iter().enumerate().take(self.data_shard_count()) {
                self.code_single_slice_range(
                    matrix_rows,
                    i_input,
                    input.as_ref(),
                    outputs,
                    start,
                    end,
                );
            }
            start = end;
        }
    }

    pub(crate) fn code_chunk_len(&self, shard_len: usize) -> usize {
        let chunk = Self::serial_code_chunk_len(shard_len);

        core::cmp::min(chunk, shard_len)
    }

    fn serial_code_chunk_len(shard_len: usize) -> usize {
        if shard_len <= CODE_SLICE_MIN_CHUNK_BYTES {
            shard_len
        } else if shard_len <= CODE_SLICE_DEFAULT_CHUNK_BYTES {
            CODE_SLICE_MIN_CHUNK_BYTES
        } else if shard_len <= 4 * 1024 * 1024 {
            CODE_SLICE_DEFAULT_CHUNK_BYTES
        } else {
            CODE_SLICE_LARGE_CHUNK_BYTES
        }
    }

    #[cfg(feature = "std")]
    pub(crate) fn code_some_slices_par_chunked_with_jobs<T, U>(
        &self,
        matrix_rows: &[&[F::Elem]],
        inputs: &[T],
        outputs: &mut [U],
        chunk_len: usize,
        jobs: usize,
    ) where
        F::Elem: Send + Sync,
        T: AsRef<[F::Elem]> + Sync,
        U: AsMut<[F::Elem]> + Send,
    {
        let shard_len = inputs
            .first()
            .map(|input| input.as_ref().len())
            .unwrap_or(0);
        if shard_len == 0 {
            return;
        }
        self.runtime_profile_metrics.record_code_some(
            true,
            shard_len,
            inputs.len(),
            outputs.len(),
            chunk_len,
        );
        let chunk_count = shard_len.div_ceil(chunk_len);
        if outputs.len() <= 2 && chunk_count > 1 {
            self.runtime_profile_metrics
                .record_code_some_small_output_chunk_parallel(outputs.len(), chunk_count);
        }
        let data_shard_count = self.data_shard_count();
        super::parallel::for_each_output_chunk(
            outputs,
            shard_len,
            chunk_len,
            jobs,
            |row, start, output_chunk| {
                let matrix_row = matrix_rows[row];
                let end = start + output_chunk.len();
                F::mul_slice(matrix_row[0], &inputs[0].as_ref()[start..end], output_chunk);
                for i_input in 1..data_shard_count {
                    F::mul_slice_add(
                        matrix_row[i_input],
                        &inputs[i_input].as_ref()[start..end],
                        output_chunk,
                    );
                }
            },
        );
    }

    #[cfg(feature = "std")]
    fn code_single_slice_par_chunked<U: AsMut<[F::Elem]> + Send>(
        &self,
        matrix_rows: &[&[F::Elem]],
        i_input: usize,
        input: &[F::Elem],
        outputs: &mut [U],
        chunk_len: usize,
        jobs: usize,
    ) where
        F::Elem: Send + Sync,
    {
        let shard_len = input.len();
        if shard_len == 0 {
            return;
        }
        self.runtime_profile_metrics
            .record_code_single(true, shard_len, outputs.len(), chunk_len);
        super::parallel::for_each_output_chunk(
            outputs,
            shard_len,
            chunk_len,
            jobs,
            |row, start, output_chunk| {
                let coefficient = matrix_rows[row][i_input];
                let input_chunk = &input[start..start + output_chunk.len()];
                if i_input == 0 {
                    F::mul_slice(coefficient, input_chunk, output_chunk);
                } else {
                    F::mul_slice_add(coefficient, input_chunk, output_chunk);
                }
            },
        );
    }

    pub(crate) fn code_single_slice_range<U: AsMut<[F::Elem]>>(
        &self,
        matrix_rows: &[&[F::Elem]],
        i_input: usize,
        input: &[F::Elem],
        outputs: &mut [U],
        start: usize,
        end: usize,
    ) {
        let input = &input[start..end];
        outputs.iter_mut().enumerate().for_each(|(i_row, output)| {
            let matrix_row_to_use = matrix_rows[i_row][i_input];
            let output = &mut output.as_mut()[start..end];

            if i_input == 0 {
                F::mul_slice(matrix_row_to_use, input, output);
            } else {
                F::mul_slice_add(matrix_row_to_use, input, output);
            }
        })
    }

    pub(crate) fn code_single_slice<U: AsMut<[F::Elem]>>(
        &self,
        matrix_rows: &[&[F::Elem]],
        i_input: usize,
        input: &[F::Elem],
        outputs: &mut [U],
    ) {
        #[cfg(feature = "std")]
        self.runtime_profile_metrics.record_code_single(
            false,
            input.len(),
            outputs.len(),
            input.len(),
        );
        self.code_single_slice_range(matrix_rows, i_input, input, outputs, 0, input.len());
    }

    fn update_parity_with_delta<U: AsRef<[F::Elem]> + AsMut<[F::Elem]>>(
        &self,
        matrix_rows: &[&[F::Elem]],
        i_input: usize,
        delta: &[F::Elem],
        outputs: &mut [U],
    ) {
        outputs.iter_mut().enumerate().for_each(|(i_row, output)| {
            let coefficient = matrix_rows[i_row][i_input];
            F::mul_slice_add(coefficient, delta, output.as_mut());
        });
    }

    pub(crate) fn fast_one_parity_enabled(&self) -> bool {
        self.options.fast_one_parity && self.parity_shard_count() == 1
    }

    /// Select a supported ISA/shape and validate all native views exactly once
    /// before execution. Unsupported or inconsistent Field views use the generic
    /// Classic path, preserving the public safe-Field fallback contract.
    #[inline]
    fn prepare_native_encode<'a, T, U>(
        &'a self,
        data: &'a [T],
        parity: &'a mut [U],
        shard_len: usize,
        execution: NativeEncodeExecution,
    ) -> Option<PreparedNativeEncode<'a>>
    where
        T: AsRef<[F::Elem]>,
        U: AsMut<[F::Elem]>,
    {
        if !leopard::is_byte_field::<F>() || !crate::galois_8::generated_encode_allowed() {
            return None;
        }
        let kernel =
            SelectedEncodeKernel::select(self.data_shard_count(), self.parity_shard_count())?;
        let shards = ByteShardViews::prepare::<F, _, _>(data, parity, shard_len).ok()?;
        let mut rows = SmallVec::new();
        for row in self.get_parity_rows() {
            let bytes = F::as_gf8_slice(row)?;
            if bytes.len() != self.data_shard_count() {
                return None;
            }
            rows.push(bytes);
        }
        Some(PreparedNativeEncode {
            kernel,
            execution,
            rows,
            shards,
            shard_len,
        })
    }

    pub(crate) fn encode_fast_one_parity<
        T: AsRef<[F::Elem]>,
        U: AsRef<[F::Elem]> + AsMut<[F::Elem]>,
    >(
        &self,
        data: &[T],
        parity: &mut [U],
    ) {
        let output = parity[0].as_mut();
        output.copy_from_slice(data[0].as_ref());
        for input in &data[1..] {
            for (out, value) in output.iter_mut().zip(input.as_ref().iter()) {
                *out = F::add(*out, *value);
            }
        }
    }

    /// Encode one data shard into all parity shards.
    ///
    /// Not supported for Leopard codec families.
    pub fn encode_single<T, U>(&self, i_data: usize, mut shards: T) -> Result<(), Error>
    where
        T: AsRef<[U]> + AsMut<[U]>,
        U: AsRef<[F::Elem]> + AsMut<[F::Elem]>,
    {
        if self.is_leopard_gf8_family() || self.is_leopard_gf16_family() {
            return Err(Error::UnsupportedCodecFamily);
        }
        let slices = shards.as_mut();

        check_slice_index!(data => self, i_data);
        check_piece_count!(all=> self, slices);
        check_slices!(multi => slices);

        let (mut_input, output) = slices.split_at_mut(self.data_shard_count());
        let input = mut_input[i_data].as_ref();

        self.encode_single_sep(i_data, input, output)
    }

    /// Encode one data shard into separate parity slices.
    pub fn encode_single_sep<U: AsRef<[F::Elem]> + AsMut<[F::Elem]>>(
        &self,
        i_data: usize,
        single_data: &[F::Elem],
        parity: &mut [U],
    ) -> Result<(), Error> {
        if self.is_leopard_gf8_family() || self.is_leopard_gf16_family() {
            return Err(Error::UnsupportedCodecFamily);
        }
        check_slice_index!(data => self, i_data);
        check_piece_count!(parity => self, parity);
        check_slices!(multi => parity, single => single_data);

        let parity_rows = self.get_parity_rows();
        self.code_single_slice(&parity_rows, i_data, single_data, parity);

        Ok(())
    }

    /// Encode data shards in-place, filling parity shards.
    ///
    /// The first `data_shard_count` slices are data (read-only), the rest are parity (written).
    pub fn encode<T, U>(&self, mut shards: T) -> Result<(), Error>
    where
        T: AsRef<[U]> + AsMut<[U]>,
        U: AsRef<[F::Elem]> + AsMut<[F::Elem]>,
    {
        let slices: &mut [U] = shards.as_mut();

        check_piece_count!(all => self, slices);
        check_slices!(multi => slices);

        let (input, output) = slices.split_at_mut(self.data_shard_count());
        self.encode_sep(&*input, output)
    }

    /// Encode from separate data and parity slices.
    pub fn encode_sep<T: AsRef<[F::Elem]>, U: AsRef<[F::Elem]> + AsMut<[F::Elem]>>(
        &self,
        data: &[T],
        parity: &mut [U],
    ) -> Result<(), Error> {
        check_piece_count!(data => self, data);
        check_piece_count!(parity => self, parity);
        check_slices!(multi => data, multi => parity);

        if self.is_leopard_gf8_family() {
            return self.encode_leopard_gf8_sep(data, parity);
        }
        if self.is_leopard_gf16_family() {
            return self.encode_leopard_gf16_sep(data, parity);
        }

        if self.fast_one_parity_enabled() {
            self.encode_fast_one_parity(data, parity);
            return Ok(());
        }

        // Try SIMD codegen path for GF(2^8) with common configurations.
        let shard_len = data[0].as_ref().len();
        if let Some(mut prepared) =
            self.prepare_native_encode(data, parity, shard_len, NativeEncodeExecution::Serial)
        {
            return prepared.execute().map(|_| ());
        }

        let parity_rows = self.get_parity_rows();
        self.code_some_slices(&parity_rows, data, parity);

        Ok(())
    }

    pub(crate) fn encode_leopard_gf8_sep<
        T: AsRef<[F::Elem]>,
        U: AsRef<[F::Elem]> + AsMut<[F::Elem]>,
    >(
        &self,
        data: &[T],
        parity: &mut [U],
    ) -> Result<(), Error> {
        self.encode_leopard_sep_inner(data, parity, leopard::leopard_gf8_encode)
    }

    pub(crate) fn encode_leopard_gf16_sep<
        T: AsRef<[F::Elem]>,
        U: AsRef<[F::Elem]> + AsMut<[F::Elem]>,
    >(
        &self,
        data: &[T],
        parity: &mut [U],
    ) -> Result<(), Error> {
        self.encode_leopard_sep_inner(data, parity, leopard::leopard_gf16_encode)
    }

    #[allow(clippy::type_complexity)]
    fn encode_leopard_sep_inner<T: AsRef<[F::Elem]>, U: AsRef<[F::Elem]> + AsMut<[F::Elem]>>(
        &self,
        data: &[T],
        parity: &mut [U],
        encode_fn: fn(usize, usize, &[&[u8]], &mut [&mut [u8]]) -> Result<(), Error>,
    ) -> Result<(), Error> {
        let shard_len = data.first().ok_or(Error::TooFewDataShards)?.as_ref().len();
        let mut shards = ByteShardViews::prepare::<F, _, _>(data, parity, shard_len)?;
        encode_fn(
            self.data_shard_count(),
            self.parity_shard_count(),
            &shards.data,
            &mut shards.parity,
        )
    }

    /// Incrementally update parity shards when some data shards change.
    ///
    /// `old_data` contains the previous data shards; `new_data` contains `Some(new)` for
    /// changed shards and `None` for unchanged ones. Not supported for Leopard families.
    pub fn update<T, U>(
        &self,
        old_data: &[T],
        new_data: &[Option<T>],
        parity: &mut [U],
    ) -> Result<(), Error>
    where
        T: AsRef<[F::Elem]>,
        U: AsRef<[F::Elem]> + AsMut<[F::Elem]>,
    {
        if self.is_leopard_gf8_family() || self.is_leopard_gf16_family() {
            return Err(Error::UnsupportedCodecFamily);
        }
        check_piece_count!(data => self, old_data);
        check_piece_count!(parity => self, parity);

        if new_data.len() != self.data_shard_count() {
            return Err(Error::TooFewDataShards);
        }

        check_slices!(multi => old_data, multi => parity);

        let shard_len = old_data
            .first()
            .map(|shard| shard.as_ref().len())
            .ok_or(Error::TooFewDataShards)?;
        if shard_len == 0 {
            return Err(Error::EmptyShard);
        }

        for new_shard in new_data.iter().flatten() {
            if new_shard.as_ref().len() != shard_len {
                return Err(Error::IncorrectShardSize);
            }
        }

        if self.fast_one_parity_enabled() {
            let parity = parity[0].as_mut();
            for (old, new) in old_data.iter().zip(new_data.iter()) {
                let Some(new) = new.as_ref() else {
                    continue;
                };

                for ((dst, old_byte), new_byte) in parity
                    .iter_mut()
                    .zip(old.as_ref().iter())
                    .zip(new.as_ref().iter())
                {
                    *dst = F::add(*dst, F::add(*old_byte, *new_byte));
                }
            }
            return Ok(());
        }

        let parity_rows = self.get_parity_rows();
        let mut delta = vec![F::zero(); shard_len];

        for (i_data, (old, new)) in old_data.iter().zip(new_data.iter()).enumerate() {
            let Some(new) = new.as_ref() else {
                continue;
            };

            let old = old.as_ref();
            let new = new.as_ref();
            for (slot, (old_elem, new_elem)) in delta.iter_mut().zip(old.iter().zip(new.iter())) {
                *slot = F::add(*old_elem, *new_elem);
            }
            self.update_parity_with_delta(&parity_rows, i_data, &delta, parity);
        }

        Ok(())
    }

    /// Parallel version of [`encode_sep`](Self::encode_sep).
    #[cfg(feature = "std")]
    pub fn encode_sep_par<T, U>(&self, data: &[T], parity: &mut [U]) -> Result<(), Error>
    where
        F::Elem: Send + Sync,
        T: AsRef<[F::Elem]> + Sync,
        U: AsRef<[F::Elem]> + AsMut<[F::Elem]> + Send,
    {
        check_piece_count!(data => self, data);
        check_piece_count!(parity => self, parity);
        check_slices!(multi => data, multi => parity);

        if self.is_leopard_gf8_family() {
            return self.encode_leopard_gf8_sep(data, parity);
        }
        if self.is_leopard_gf16_family() {
            return self.encode_leopard_gf16_sep(data, parity);
        }

        if self.fast_one_parity_enabled() {
            self.encode_fast_one_parity(data, parity);
            return Ok(());
        }

        let shard_len = data[0].as_ref().len();
        let decision = self.parallel_policy(shard_len, parity.len());
        if let Some(execution) = NativeEncodeExecution::for_parallel_decision(
            self.data_shard_count(),
            self.parity_shard_count(),
            shard_len,
            decision,
        ) && let Some(mut prepared) =
            self.prepare_native_encode(data, parity, shard_len, execution)
        {
            let chunk_len = prepared.execute()?;
            if decision.use_parallel {
                self.runtime_profile_metrics.record_code_some(
                    true,
                    shard_len,
                    self.data_shard_count(),
                    self.parity_shard_count(),
                    chunk_len,
                );
            }
            return Ok(());
        }
        let parity_rows = self.get_parity_rows();
        if decision.use_parallel {
            self.code_some_slices_par_chunked_with_jobs(
                &parity_rows,
                data,
                parity,
                decision.chunk_len,
                decision.jobs,
            );
        } else {
            self.code_some_slices(&parity_rows, data, parity);
        }

        Ok(())
    }

    /// Parallel version of [`encode_single_sep`](Self::encode_single_sep).
    #[cfg(feature = "std")]
    pub fn encode_single_sep_par<U>(
        &self,
        i_data: usize,
        single_data: &[F::Elem],
        parity: &mut [U],
    ) -> Result<(), Error>
    where
        F::Elem: Send + Sync,
        U: AsRef<[F::Elem]> + AsMut<[F::Elem]> + Send,
    {
        if self.is_leopard_gf8_family() || self.is_leopard_gf16_family() {
            return Err(Error::UnsupportedCodecFamily);
        }
        check_slice_index!(data => self, i_data);
        check_piece_count!(parity => self, parity);
        check_slices!(multi => parity, single => single_data);

        let parity_rows = self.get_parity_rows();
        let decision = self.parallel_policy(single_data.len(), parity.len());
        if !decision.use_parallel {
            self.code_single_slice(&parity_rows, i_data, single_data, parity);
            return Ok(());
        }
        self.code_single_slice_par_chunked(
            &parity_rows,
            i_data,
            single_data,
            parity,
            decision.chunk_len,
            decision.jobs,
        );

        Ok(())
    }

    /// Auto-parallelizing version of [`encode_single_sep`](Self::encode_single_sep).
    ///
    /// Uses the parallel policy to choose between serial and parallel execution.
    #[cfg(feature = "std")]
    pub fn encode_single_sep_opt<U>(
        &self,
        i_data: usize,
        single_data: &[F::Elem],
        parity: &mut [U],
    ) -> Result<(), Error>
    where
        F::Elem: Send + Sync,
        U: AsRef<[F::Elem]> + AsMut<[F::Elem]> + Send,
    {
        self.encode_single_sep_par(i_data, single_data, parity)
    }

    /// Auto-parallelizing version of [`encode_single`](Self::encode_single).
    #[cfg(feature = "std")]
    pub fn encode_single_opt<T, U>(&self, i_data: usize, mut shards: T) -> Result<(), Error>
    where
        F::Elem: Send + Sync,
        T: AsRef<[U]> + AsMut<[U]>,
        U: AsRef<[F::Elem]> + AsMut<[F::Elem]> + Send,
    {
        if self.is_leopard_gf8_family() || self.is_leopard_gf16_family() {
            return Err(Error::UnsupportedCodecFamily);
        }
        let slices = shards.as_mut();

        check_slice_index!(data => self, i_data);
        check_piece_count!(all=> self, slices);
        check_slices!(multi => slices);

        let (mut_input, output) = slices.split_at_mut(self.data_shard_count());
        let input = mut_input[i_data].as_ref();
        self.encode_single_sep_par(i_data, input, output)
    }

    /// Parallel version of [`encode`](Self::encode).
    #[cfg(feature = "std")]
    pub fn encode_par<T, U>(&self, mut shards: T) -> Result<(), Error>
    where
        F::Elem: Send + Sync,
        T: AsRef<[U]> + AsMut<[U]>,
        U: AsRef<[F::Elem]> + AsMut<[F::Elem]> + Send + Sync,
    {
        let slices: &mut [U] = shards.as_mut();

        check_piece_count!(all => self, slices);
        check_slices!(multi => slices);

        let (input, output) = slices.split_at_mut(self.data_shard_count());
        self.encode_sep_par(&*input, output)
    }
}

#[cfg(all(test, feature = "std", rse_aarch64_neon))]
mod parallel_codegen_tests {
    use super::split_neon_encode_jobs;

    #[test]
    fn native_parallel_selection_preserves_the_measured_boundary() {
        for (data, parity, shard_len, jobs, eligible) in [
            (10, 4, 1024 * 1024, 2, true),
            (10, 4, 1024 * 1024, 4, true),
            (10, 4, 1024 * 1024, 5, false),
            (10, 4, 1024 * 1024, 33, false),
            (10, 4, 1024 * 1024 - 1, 4, false),
            (12, 4, 1024 * 1024, 4, false),
            (4, 2, 1024 * 1024, 4, false),
        ] {
            let decision = crate::ParallelDecision {
                use_parallel: true,
                jobs,
                chunk_len: 64 * 1024,
            };
            assert_eq!(
                super::NativeEncodeExecution::for_parallel_decision(
                    data, parity, shard_len, decision
                )
                .is_some(),
                eligible
            );
        }
    }

    #[test]
    fn transposed_jobs_are_bounded_disjoint_and_cover_all_bytes() {
        for shard_len in [65, 1024, 262_145, 1_048_639] {
            for budget in [2, 3, 4] {
                let mut parity = vec![vec![0_u8; shard_len]; 4];
                let mut views: Vec<_> = parity.iter_mut().map(Vec::as_mut_slice).collect();
                let mut jobs = split_neon_encode_jobs(&mut views, shard_len, budget);
                assert!(jobs.len() <= budget);
                assert!(!jobs.spilled());
                for job in &mut jobs {
                    assert!(!job.outputs.spilled());
                    assert_eq!(job.start % 64, 0);
                    for output in &mut job.outputs {
                        for value in output.iter_mut() {
                            *value += 1;
                        }
                    }
                }
                drop(jobs);
                assert!(parity.iter().flatten().all(|&value| value == 1));
            }
        }
    }
}
