extern crate alloc;

use alloc::vec;

use crate::Field;
use crate::errors::Error;

use super::{
    CODE_SLICE_DEFAULT_CHUNK_BYTES, CODE_SLICE_LARGE_CHUNK_BYTES, CODE_SLICE_MIN_CHUNK_BYTES,
    ReedSolomon, leopard,
};

#[cfg(all(feature = "std", rse_aarch64_neon))]
struct NeonEncodeJob<'a> {
    start: usize,
    outputs: smallvec::SmallVec<[&'a mut [u8]; 4]>,
}

// Keep generated-kernel jobs entirely on the stack. Eligibility is limited to
// the measured 10+4, >=1 MiB, small-job workload; other work uses the generic scheduler.
#[cfg(all(feature = "std", rse_aarch64_neon))]
fn split_neon_encode_jobs<'a>(
    parity: &'a mut [&mut [u8]],
    shard_len: usize,
    job_limit: usize,
) -> smallvec::SmallVec<[NeonEncodeJob<'a>; 32]> {
    let group_len = shard_len.div_ceil(job_limit).div_ceil(64) * 64;
    let job_count = shard_len.div_ceil(group_len);
    let mut jobs: smallvec::SmallVec<[NeonEncodeJob<'a>; 32]> = (0..job_count)
        .map(|index| NeonEncodeJob {
            start: index * group_len,
            outputs: smallvec::SmallVec::new(),
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
            for (i_input, input) in inputs.iter().enumerate().take(self.data_shard_count) {
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
    pub(crate) fn code_some_slices_par_chunked<T, U>(
        &self,
        matrix_rows: &[&[F::Elem]],
        inputs: &[T],
        outputs: &mut [U],
        chunk_len: usize,
    ) where
        F::Elem: Send + Sync,
        T: AsRef<[F::Elem]> + Sync,
        U: AsMut<[F::Elem]> + Send,
    {
        let shard_len = inputs
            .first()
            .map(|input| input.as_ref().len())
            .unwrap_or(0);
        let jobs = self
            .parallel_policy_with(
                shard_len,
                outputs.len(),
                self.policy_cache.available_parallelism,
            )
            .jobs;
        self.code_some_slices_par_chunked_with_jobs(matrix_rows, inputs, outputs, chunk_len, jobs);
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
        let data_shard_count = self.data_shard_count;
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
        self.options.fast_one_parity && self.parity_shard_count == 1
    }

    /// Attempt SIMD codegen encode for GF(2^8) with common configurations.
    /// Returns `true` if the codegen path was used, `false` to fall back to generic path.
    fn try_encode_codegen<T: AsRef<[F::Elem]>, U: AsRef<[F::Elem]> + AsMut<[F::Elem]>>(
        &self,
        data: &[T],
        parity: &mut [U],
        _shard_len: usize,
    ) -> bool {
        if !leopard::is_byte_field::<F>() || !crate::galois_8::generated_encode_allowed() {
            return false;
        }

        // A one-byte element does not identify a finite field. Only fields
        // explicitly exposing the native GF8 representation may use this path.
        let mut _data_u8 = smallvec::SmallVec::<[&[u8]; 32]>::with_capacity(data.len());
        for input in data {
            let input = input.as_ref();
            let Some(bytes) = F::as_gf8_slice(input) else {
                return false;
            };
            if bytes.len() != input.len() {
                return false;
            }
            _data_u8.push(bytes);
        }
        let mut _parity_u8 = smallvec::SmallVec::<[&mut [u8]; 32]>::with_capacity(parity.len());
        for output in parity {
            let output = output.as_mut();
            let len = output.len();
            let Some(bytes) = F::as_gf8_slice_mut(output) else {
                return false;
            };
            if bytes.len() != len {
                return false;
            }
            _parity_u8.push(bytes);
        }
        let parity_rows = self.get_parity_rows();
        let mut _parity_refs = smallvec::SmallVec::<[&[u8]; 32]>::with_capacity(parity_rows.len());
        for row in &parity_rows {
            let Some(bytes) = F::as_gf8_slice(row) else {
                return false;
            };
            if bytes.len() != row.len() {
                return false;
            }
            _parity_refs.push(bytes);
        }

        // x86_64 AVX2 codegen path
        #[cfg(all(
            feature = "simd-avx2",
            target_arch = "x86_64",
            not(target_env = "msvc"),
            not(any(target_os = "android", target_os = "ios"))
        ))]
        {
            if crate::galois_8::x86::codegen::try_encode_codegen_avx2(
                self.data_shard_count,
                self.parity_shard_count,
                &_parity_refs,
                &_data_u8,
                &mut _parity_u8,
                _shard_len,
            ) {
                return true;
            }
        }

        // aarch64 NEON codegen path
        #[cfg(all(
            feature = "simd-neon",
            target_arch = "aarch64",
            not(target_env = "msvc"),
            not(any(target_os = "android", target_os = "ios"))
        ))]
        {
            if crate::galois_8::aarch64::codegen::try_encode_codegen_neon(
                self.data_shard_count,
                self.parity_shard_count,
                &_parity_refs,
                &_data_u8,
                &mut _parity_u8,
                _shard_len,
            ) {
                return true;
            }
        }

        false
    }

    /// Fuse native NEON encoding within a bounded set of disjoint byte ranges.
    /// Only byte views cross into Rayon, so `F` itself need not be `Sync`.
    #[cfg(all(feature = "std", rse_aarch64_neon))]
    fn try_encode_codegen_par<T, U>(
        &self,
        data: &[T],
        parity: &mut [U],
        shard_len: usize,
        decision: super::ParallelDecision,
    ) -> bool
    where
        T: AsRef<[F::Elem]>,
        U: AsMut<[F::Elem]>,
    {
        use rayon::prelude::*;
        use smallvec::SmallVec;

        let data_count = self.data_shard_count;
        let parity_count = self.parity_shard_count;
        // Stable measurements support only this shape and shard-size range.
        // Avoid extending the optimization to noisier small-shard results.
        if (data_count, parity_count) != (10, 4)
            || shard_len < 1024 * 1024
            || !decision.use_parallel
            || !(2..=4).contains(&decision.jobs)
            || !leopard::is_byte_field::<F>()
            || !crate::galois_8::generated_encode_allowed()
        {
            return false;
        }

        let mut data_bytes = SmallVec::<[&[u8]; 12]>::new();
        for input in data {
            let Some(bytes) = F::as_gf8_slice(input.as_ref()) else {
                return false;
            };
            if bytes.len() != shard_len {
                return false;
            }
            data_bytes.push(bytes);
        }
        let matrix_rows = self.get_parity_rows();
        let mut byte_rows = SmallVec::<[&[u8]; 4]>::new();
        for row in &matrix_rows {
            let Some(bytes) = F::as_gf8_slice(row) else {
                return false;
            };
            if bytes.len() != data_count {
                return false;
            }
            byte_rows.push(bytes);
        }
        let mut parity_bytes = SmallVec::<[&mut [u8]; 4]>::new();
        for output in parity {
            let Some(bytes) = F::as_gf8_slice_mut(output.as_mut()) else {
                return false;
            };
            if bytes.len() != shard_len {
                return false;
            }
            parity_bytes.push(bytes);
        }

        let mut jobs = split_neon_encode_jobs(&mut parity_bytes, shard_len, decision.jobs);
        let chunk_len = decision.chunk_len.max(1);
        let encoded = jobs
            .par_iter_mut()
            .map(|job| {
                let job_len = job.outputs[0].len();
                let mut encoded = true;
                for offset in (0..job_len).step_by(chunk_len) {
                    let end = offset.saturating_add(chunk_len).min(job_len);
                    let inputs: SmallVec<[&[u8]; 12]> = data_bytes
                        .iter()
                        .map(|input| &input[job.start + offset..job.start + end])
                        .collect();
                    let mut outputs: SmallVec<[&mut [u8]; 4]> = job
                        .outputs
                        .iter_mut()
                        .map(|output| &mut output[offset..end])
                        .collect();
                    encoded &= crate::galois_8::aarch64::codegen::try_encode_codegen_neon(
                        data_count,
                        parity_count,
                        &byte_rows,
                        &inputs,
                        &mut outputs,
                        end - offset,
                    );
                }
                encoded
            })
            .reduce(|| true, |left, right| left & right);
        if encoded {
            self.runtime_profile_metrics.record_code_some(
                true,
                shard_len,
                data_count,
                parity_count,
                chunk_len.min(jobs[0].outputs[0].len()),
            );
        }
        encoded
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

        let (mut_input, output) = slices.split_at_mut(self.data_shard_count);
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

        let (input, output) = slices.split_at_mut(self.data_shard_count);
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
        if self.try_encode_codegen(data, parity, shard_len) {
            return Ok(());
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
        let mut data_u8 = smallvec::SmallVec::<[&[u8]; 32]>::with_capacity(data.len());
        for shard in data {
            let slice = shard.as_ref();
            let bytes = F::as_gf8_slice(slice).ok_or(Error::UnsupportedCodecFamily)?;
            if bytes.len() != slice.len() {
                return Err(Error::IncorrectShardSize);
            }
            data_u8.push(bytes);
        }
        let mut parity_u8 = smallvec::SmallVec::<[&mut [u8]; 32]>::with_capacity(parity.len());
        for shard in parity {
            let slice = shard.as_mut();
            let len = slice.len();
            let bytes = F::as_gf8_slice_mut(slice).ok_or(Error::UnsupportedCodecFamily)?;
            if bytes.len() != len {
                return Err(Error::IncorrectShardSize);
            }
            parity_u8.push(bytes);
        }
        encode_fn(
            self.data_shard_count,
            self.parity_shard_count,
            &data_u8,
            &mut parity_u8,
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

        if new_data.len() != self.data_shard_count {
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

        let parity_rows = self.get_parity_rows();
        let shard_len = data[0].as_ref().len();
        let decision = self.parallel_policy(shard_len, parity.len());
        if !decision.use_parallel {
            if !self.try_encode_codegen(data, parity, shard_len) {
                self.code_some_slices(&parity_rows, data, parity);
            }
            return Ok(());
        }
        #[cfg(all(feature = "std", rse_aarch64_neon))]
        if self.try_encode_codegen_par(data, parity, shard_len, decision) {
            return Ok(());
        }
        self.code_some_slices_par_chunked(&parity_rows, data, parity, decision.chunk_len);

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
        let decision = self.parallel_policy(single_data.len(), parity.len());
        if decision.use_parallel {
            self.encode_single_sep_par(i_data, single_data, parity)
        } else {
            self.encode_single_sep(i_data, single_data, parity)
        }
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

        let (mut_input, output) = slices.split_at_mut(self.data_shard_count);
        let input = mut_input[i_data].as_ref();
        let decision = self.parallel_policy(input.len(), output.len());
        let parity_rows = self.get_parity_rows();
        if decision.use_parallel {
            self.code_single_slice_par_chunked(
                &parity_rows,
                i_data,
                input,
                output,
                decision.chunk_len,
                decision.jobs,
            );
        } else {
            self.code_single_slice(&parity_rows, i_data, input, output);
        }
        Ok(())
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

        let (input, output) = slices.split_at_mut(self.data_shard_count);
        self.encode_sep_par(&*input, output)
    }
}

#[cfg(all(test, feature = "std", rse_aarch64_neon))]
mod parallel_codegen_tests {
    use super::split_neon_encode_jobs;

    #[test]
    fn codegen_budget_above_measured_range_falls_back_without_writes() {
        let codec = crate::galois_8::ReedSolomon::new(10, 4).unwrap();
        let shard_len = 1024 * 1024;
        let data = vec![vec![7_u8; shard_len]; 10];
        for jobs in [5, 33] {
            let mut parity = vec![vec![0xa5_u8; shard_len]; 4];
            let original = parity.clone();
            let decision = crate::ParallelDecision {
                use_parallel: true,
                jobs,
                chunk_len: 64 * 1024,
            };
            assert!(!codec.try_encode_codegen_par(&data, &mut parity, shard_len, decision));
            assert_eq!(parity, original);
        }
    }

    #[test]
    fn transposed_jobs_are_bounded_disjoint_and_cover_all_bytes() {
        for shard_len in [65, 1024, 262_145, 1_048_639] {
            for budget in [2, 3, 7, 16, 32] {
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
