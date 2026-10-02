extern crate alloc;

#[cfg(feature = "std")]
use rayon::prelude::*;

use alloc::vec;

use crate::Field;
use crate::errors::Error;

use super::{
    CODE_SLICE_DEFAULT_CHUNK_BYTES, CODE_SLICE_LARGE_CHUNK_BYTES, CODE_SLICE_MIN_CHUNK_BYTES,
    ReedSolomon, leopard,
};

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
        let data_shard_count = self.data_shard_count;
        let chunk_count = shard_len.div_ceil(chunk_len);
        if outputs.len() <= 2 && chunk_count > 1 {
            self.runtime_profile_metrics
                .record_code_some_small_output_chunk_parallel(outputs.len(), chunk_count);
            if outputs.len() == 1 {
                let matrix_row = matrix_rows[0];
                outputs[0]
                    .as_mut()
                    .par_chunks_mut(chunk_len)
                    .enumerate()
                    .for_each(|(chunk_idx, output_chunk)| {
                        let start = chunk_idx * chunk_len;
                        let end = start + output_chunk.len();

                        F::mul_slice(matrix_row[0], &inputs[0].as_ref()[start..end], output_chunk);
                        for i_input in 1..data_shard_count {
                            F::mul_slice_add(
                                matrix_row[i_input],
                                &inputs[i_input].as_ref()[start..end],
                                output_chunk,
                            );
                        }
                    });
            } else {
                let matrix_row0 = matrix_rows[0];
                let matrix_row1 = matrix_rows[1];
                let (first, second) = outputs.split_at_mut(1);
                let output0 = first[0].as_mut();
                let output1 = second[0].as_mut();

                output0
                    .par_chunks_mut(chunk_len)
                    .zip(output1.par_chunks_mut(chunk_len))
                    .enumerate()
                    .for_each(|(chunk_idx, (output0_chunk, output1_chunk))| {
                        let start = chunk_idx * chunk_len;
                        let end = start + output0_chunk.len();
                        let input0 = &inputs[0].as_ref()[start..end];

                        F::mul_slice(matrix_row0[0], input0, output0_chunk);
                        F::mul_slice(matrix_row1[0], input0, output1_chunk);
                        for i_input in 1..data_shard_count {
                            let input_chunk = &inputs[i_input].as_ref()[start..end];
                            F::mul_slice_add(matrix_row0[i_input], input_chunk, output0_chunk);
                            F::mul_slice_add(matrix_row1[i_input], input_chunk, output1_chunk);
                        }
                    });
            }
        } else {
            outputs
                .par_iter_mut()
                .enumerate()
                .for_each(|(i_row, output)| {
                    let matrix_row = matrix_rows[i_row];
                    let output = output.as_mut();

                    let mut start = 0;
                    while start < shard_len {
                        let end = core::cmp::min(start + chunk_len, shard_len);
                        let output_chunk = &mut output[start..end];

                        F::mul_slice(matrix_row[0], &inputs[0].as_ref()[start..end], output_chunk);
                        for i_input in 1..data_shard_count {
                            F::mul_slice_add(
                                matrix_row[i_input],
                                &inputs[i_input].as_ref()[start..end],
                                output_chunk,
                            );
                        }

                        start = end;
                    }
                });
        }
    }

    #[cfg(feature = "std")]
    fn code_single_slice_par_chunked<U: AsMut<[F::Elem]> + Send>(
        &self,
        matrix_rows: &[&[F::Elem]],
        i_input: usize,
        input: &[F::Elem],
        outputs: &mut [U],
        chunk_len: usize,
    ) where
        F::Elem: Send + Sync,
    {
        let shard_len = input.len();
        if shard_len == 0 {
            return;
        }

        self.runtime_profile_metrics
            .record_code_single(true, shard_len, outputs.len(), chunk_len);
        outputs
            .par_iter_mut()
            .enumerate()
            .for_each(|(i_row, output)| {
                let coefficient = matrix_rows[i_row][i_input];
                let output = output.as_mut();

                let mut start = 0;
                while start < shard_len {
                    let end = core::cmp::min(start + chunk_len, shard_len);
                    let output_chunk = &mut output[start..end];
                    let input_chunk = &input[start..end];
                    if i_input == 0 {
                        F::mul_slice(coefficient, input_chunk, output_chunk);
                    } else {
                        F::mul_slice_add(coefficient, input_chunk, output_chunk);
                    }
                    start = end;
                }
            });
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
        self.ensure_classic_family_execution()?;
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
            self.code_some_slices(&parity_rows, data, parity);
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
