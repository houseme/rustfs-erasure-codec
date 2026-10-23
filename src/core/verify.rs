extern crate alloc;

use alloc::vec;
use alloc::vec::Vec;

use smallvec::SmallVec;

use crate::Field;
use crate::errors::Error;

use super::{ReedSolomon, VERIFY_INLINE_SCRATCH_ELEMS, VerifyWorkspace};

impl<F: Field> ReedSolomon<F> {
    fn check_classic_parity<T, U>(
        &self,
        inputs: &[T],
        to_check: &[T],
        buffer: &mut [U],
    ) -> Result<bool, Error>
    where
        T: AsRef<[F::Elem]>,
        U: AsRef<[F::Elem]> + AsMut<[F::Elem]>,
    {
        self.encode_classic_matrix(inputs, buffer)?;
        Ok(buffer
            .iter()
            .zip(to_check)
            .all(|(expected, actual)| expected.as_ref() == actual.as_ref()))
    }

    fn verify_leopard_with_buffer<T, U>(
        &self,
        slices: &[T],
        parity: &mut [U],
    ) -> Result<bool, Error>
    where
        T: AsRef<[F::Elem]>,
        U: AsRef<[F::Elem]> + AsMut<[F::Elem]>,
    {
        let data = &slices[0..self.data_shard_count()];
        let to_check = &slices[self.data_shard_count()..];
        if self.is_leopard_gf8_family() {
            self.encode_leopard_gf8_sep(data, parity)?;
        } else {
            self.encode_leopard_gf16_sep(data, parity)?;
        }

        for (expected, actual) in parity.iter().zip(to_check.iter()) {
            if expected.as_ref() != actual.as_ref() {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Verify Leopard parity shards by re-encoding and comparing.
    fn verify_leopard<T: AsRef<[F::Elem]>>(&self, slices: &[T]) -> Result<bool, Error> {
        let slice_len = slices[0].as_ref().len();
        let mut parity_bufs: Vec<Vec<F::Elem>> = (0..self.parity_shard_count())
            .map(|_| vec![F::zero(); slice_len])
            .collect();
        self.verify_leopard_with_buffer(slices, &mut parity_bufs)
    }

    /// Verify that parity shards are consistent with data shards.
    ///
    /// Returns `true` if valid, `false` if corrupted.
    pub fn verify<T: AsRef<[F::Elem]>>(&self, slices: &[T]) -> Result<bool, Error> {
        check_piece_count!(all => self, slices);
        check_slices!(multi => slices);

        if self.is_leopard_gf8_family() || self.is_leopard_gf16_family() {
            return self.verify_leopard(slices);
        }

        let slice_len = slices[0].as_ref().len();
        let data = &slices[0..self.data_shard_count()];
        let to_check = &slices[self.data_shard_count()..];

        if self.fast_one_parity_enabled() {
            let mut buffer = vec![F::zero(); slice_len];
            self.encode_fast_one_parity(data, core::slice::from_mut(&mut buffer));
            return Ok(buffer.as_slice() == to_check[0].as_ref());
        }

        let scratch_len = self.parity_shard_count() * slice_len;
        let mut scratch: SmallVec<[F::Elem; VERIFY_INLINE_SCRATCH_ELEMS]> =
            SmallVec::with_capacity(scratch_len);
        scratch.resize(scratch_len, F::zero());
        let mut buffer_views: SmallVec<[&mut [F::Elem]; 32]> =
            scratch.chunks_mut(slice_len).collect();

        self.check_classic_parity(data, to_check, &mut buffer_views)
    }

    /// Verify using a pre-allocated [`VerifyWorkspace`] to avoid repeated allocation.
    pub fn verify_with_workspace<T>(
        &self,
        slices: &[T],
        workspace: &mut VerifyWorkspace<F>,
    ) -> Result<bool, Error>
    where
        T: AsRef<[F::Elem]>,
    {
        check_piece_count!(all => self, slices);
        check_slices!(multi => slices);

        let slice_len = slices[0].as_ref().len();
        if self.is_leopard_gf8_family() || self.is_leopard_gf16_family() {
            workspace.prepare(self, slice_len);
            return self.verify_leopard_with_buffer(slices, workspace.as_mut_shards());
        }

        workspace.prepare(self, slice_len);
        self.verify_with_buffer(slices, workspace.as_mut_shards())
    }

    /// Verify using a caller-provided scratch buffer.
    pub fn verify_with_buffer<T, U>(&self, slices: &[T], buffer: &mut [U]) -> Result<bool, Error>
    where
        T: AsRef<[F::Elem]>,
        U: AsRef<[F::Elem]> + AsMut<[F::Elem]>,
    {
        check_piece_count!(all => self, slices);
        check_piece_count!(parity_buf => self, buffer);
        check_slices!(multi => slices, multi => buffer);

        if self.is_leopard_gf8_family() || self.is_leopard_gf16_family() {
            return self.verify_leopard_with_buffer(slices, buffer);
        }

        let data = &slices[0..self.data_shard_count()];
        let to_check = &slices[self.data_shard_count()..];

        if self.fast_one_parity_enabled() {
            self.encode_fast_one_parity(data, buffer);
            return Ok(buffer[0].as_ref() == to_check[0].as_ref());
        }

        self.check_classic_parity(data, to_check, buffer)
    }

    /// Parallel version of [`verify_with_buffer`](Self::verify_with_buffer).
    #[cfg(feature = "std")]
    pub fn verify_with_buffer_par<T, U>(
        &self,
        slices: &[T],
        buffer: &mut [U],
    ) -> Result<bool, Error>
    where
        F::Elem: Send + Sync,
        T: AsRef<[F::Elem]> + Sync,
        U: AsRef<[F::Elem]> + AsMut<[F::Elem]> + Send,
    {
        check_piece_count!(all => self, slices);
        check_piece_count!(parity_buf => self, buffer);
        check_slices!(multi => slices, multi => buffer);

        if self.is_leopard_gf8_family() || self.is_leopard_gf16_family() {
            return self.verify_leopard_with_buffer(slices, buffer);
        }

        let data = &slices[0..self.data_shard_count()];
        let to_check = &slices[self.data_shard_count()..];

        if self.fast_one_parity_enabled() {
            self.encode_fast_one_parity(data, buffer);
            return Ok(buffer[0].as_ref() == to_check[0].as_ref());
        }

        self.encode_sep_par(data, buffer)?;

        Ok(buffer
            .iter()
            .zip(to_check.iter())
            .all(|(expected, actual)| expected.as_ref() == actual.as_ref()))
    }

    /// Parallel version of [`verify`](Self::verify).
    #[cfg(feature = "std")]
    pub fn verify_par<T>(&self, slices: &[T]) -> Result<bool, Error>
    where
        F::Elem: Send + Sync,
        T: AsRef<[F::Elem]> + Sync,
    {
        check_piece_count!(all => self, slices);
        check_slices!(multi => slices);

        if self.is_leopard_gf8_family() || self.is_leopard_gf16_family() {
            return self.verify_leopard(slices);
        }

        let slice_len = slices[0].as_ref().len();
        let scratch_len = self.parity_shard_count() * slice_len;
        let mut scratch: SmallVec<[F::Elem; VERIFY_INLINE_SCRATCH_ELEMS]> =
            SmallVec::with_capacity(scratch_len);
        scratch.resize(scratch_len, F::zero());
        let mut buffer_views: SmallVec<[&mut [F::Elem]; 32]> =
            scratch.chunks_mut(slice_len).collect();

        self.verify_with_buffer_par(slices, &mut buffer_views)
    }
}
