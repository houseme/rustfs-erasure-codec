use crate::Field;
use crate::errors::{Error, SBSError};

use super::ReedSolomon;

/// Bookkeeper for shard by shard encoding.
///
/// This is useful for avoiding incorrect use of
/// `encode_single` and `encode_single_sep`
///
/// # Use cases
///
/// Shard by shard encoding is useful for streamed data encoding
/// where you do not have all the needed data shards immediately,
/// but you want to spread out the encoding workload rather than
/// doing the encoding after everything is ready.
///
/// A concrete example would be network packets encoding,
/// where encoding packet by packet as you receive them may be more efficient
/// than waiting for N packets then encode them all at once.
///
/// # Example
///
/// ```
/// # #[macro_use] extern crate rustfs_erasure_codec;
/// # use rustfs_erasure_codec::*;
/// # fn main () {
/// use rustfs_erasure_codec::galois_8::Field;
/// let r: ReedSolomon<Field> = ReedSolomon::new(3, 2).unwrap();
///
/// let mut sbs = ShardByShard::new(&r);
///
/// let mut shards = shards!([0u8,  1,  2,  3,  4],
///                          [5,  6,  7,  8,  9],
///                          // say we don't have the 3rd data shard yet
///                          // and we want to fill it in later
///                          [0,  0,  0,  0,  0],
///                          [0,  0,  0,  0,  0],
///                          [0,  0,  0,  0,  0]);
///
/// // encode 1st and 2nd data shard
/// sbs.encode(&mut shards).unwrap();
/// sbs.encode(&mut shards).unwrap();
///
/// // fill in 3rd data shard
/// shards[2][0] = 10.into();
/// shards[2][1] = 11.into();
/// shards[2][2] = 12.into();
/// shards[2][3] = 13.into();
/// shards[2][4] = 14.into();
///
/// // now do the encoding
/// sbs.encode(&mut shards).unwrap();
///
/// assert!(r.verify(&shards).unwrap());
/// # }
/// ```
#[derive(PartialEq, Debug)]
pub struct ShardByShard<'a, F: 'a + Field> {
    codec: &'a ReedSolomon<F>,
    cur_input: usize,
}

impl<'a, F: 'a + Field> ShardByShard<'a, F> {
    /// Create a new shard-by-shard encoder.
    pub fn new(codec: &'a ReedSolomon<F>) -> ShardByShard<'a, F> {
        ShardByShard {
            codec,
            cur_input: 0,
        }
    }

    /// Returns `true` if all data shards have been encoded and parity is ready.
    pub fn parity_ready(&self) -> bool {
        self.cur_input == self.codec.data_shard_count()
    }

    /// Reset to accept a new batch of data shards.
    ///
    /// Returns [`SBSError::LeftoverShards`] if the previous batch was incomplete.
    pub fn reset(&mut self) -> Result<(), SBSError> {
        if self.cur_input > 0 && !self.parity_ready() {
            return Err(SBSError::LeftoverShards);
        }

        self.cur_input = 0;

        Ok(())
    }

    /// Force-reset without checking for leftover shards.
    pub fn reset_force(&mut self) {
        self.cur_input = 0;
    }

    /// Returns the index of the next data shard expected.
    pub fn cur_input_index(&self) -> usize {
        self.cur_input
    }

    fn return_ok_and_incre_cur_input(&mut self) -> Result<(), SBSError> {
        self.cur_input += 1;
        Ok(())
    }

    fn sbs_encode_checks<U: AsRef<[F::Elem]> + AsMut<[F::Elem]>>(
        &mut self,
        slices: &mut [U],
    ) -> Result<(), SBSError> {
        let internal_checks = |codec: &ReedSolomon<F>, data: &mut [U]| -> Result<(), Error> {
            check_piece_count!(all => codec, data);
            check_slices!(multi => data);

            Ok(())
        };

        if self.parity_ready() {
            return Err(SBSError::TooManyCalls);
        }

        match internal_checks(self.codec, slices) {
            Ok(()) => Ok(()),
            Err(e) => Err(SBSError::RSError(e)),
        }
    }

    fn sbs_encode_sep_checks<T: AsRef<[F::Elem]>, U: AsRef<[F::Elem]> + AsMut<[F::Elem]>>(
        &mut self,
        data: &[T],
        parity: &mut [U],
    ) -> Result<(), SBSError> {
        let internal_checks =
            |codec: &ReedSolomon<F>, data: &[T], parity: &mut [U]| -> Result<(), Error> {
                check_piece_count!(data => codec, data);
                check_piece_count!(parity => codec, parity);
                check_slices!(multi => data, multi => parity);

                Ok(())
            };

        if self.parity_ready() {
            return Err(SBSError::TooManyCalls);
        }

        match internal_checks(self.codec, data, parity) {
            Ok(()) => Ok(()),
            Err(e) => Err(SBSError::RSError(e)),
        }
    }

    /// Encode the next data shard in the batch.
    pub fn encode<T, U>(&mut self, mut shards: T) -> Result<(), SBSError>
    where
        T: AsRef<[U]> + AsMut<[U]>,
        U: AsRef<[F::Elem]> + AsMut<[F::Elem]>,
    {
        let shards = shards.as_mut();
        self.sbs_encode_checks(shards)?;

        self.codec
            .encode_single(self.cur_input, shards)
            .map_err(SBSError::RSError)?;

        self.return_ok_and_incre_cur_input()
    }

    /// Encode the next data shard using separate data and parity slices.
    pub fn encode_sep<T: AsRef<[F::Elem]>, U: AsRef<[F::Elem]> + AsMut<[F::Elem]>>(
        &mut self,
        data: &[T],
        parity: &mut [U],
    ) -> Result<(), SBSError> {
        self.sbs_encode_sep_checks(data, parity)?;

        self.codec
            .encode_single_sep(self.cur_input, data[self.cur_input].as_ref(), parity)
            .map_err(SBSError::RSError)?;

        self.return_ok_and_incre_cur_input()
    }
}
