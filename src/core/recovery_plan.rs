extern crate alloc;

use alloc::sync::Arc;

use smallvec::SmallVec;

use crate::matrix::Matrix;
use crate::{Error, Field, ReconstructShard};

use super::ReedSolomon;

#[derive(Clone, Copy)]
pub(crate) enum ReconstructionGoal<'a> {
    All,
    Data,
    Required(&'a [bool]),
}

/// Validated, immutable erasure layout shared by all Classic entry points.
#[derive(Clone)]
pub(crate) struct ClassicReconstructShape {
    pub(crate) shard_len: usize,
    pub(crate) number_present: usize,
    pub(crate) valid_indices: SmallVec<[usize; 32]>,
    pub(crate) invalid_indices: SmallVec<[usize; 32]>,
}

impl ClassicReconstructShape {
    pub(crate) fn missing_data_count(&self, data_shards: usize) -> usize {
        self.invalid_indices
            .partition_point(|&index| index < data_shards)
    }
}

/// Matrix planning is completed before any missing destination is initialized.
/// The inverse is retained independently of the codec cache for workspace and
/// stream reuse. A parity-only operation borrows encoding rows directly.
pub(crate) struct ClassicReconstructPlan<F: Field> {
    pub(crate) shape: ClassicReconstructShape,
    pub(crate) data_targets: SmallVec<[usize; 32]>,
    pub(crate) parity_targets: SmallVec<[usize; 32]>,
    pub(crate) data_only: bool,
    pub(crate) direct_parity: bool,
    decode_matrix: Option<Arc<Matrix<F>>>,
    parity_decode_rows: SmallVec<[SmallVec<[F::Elem; 32]>; 4]>,
}

impl<F: Field> ClassicReconstructPlan<F> {
    pub(crate) fn has_outputs(&self) -> bool {
        !self.data_targets.is_empty() || !self.parity_targets.is_empty()
    }

    pub(crate) fn data_rows(&self) -> SmallVec<[&[F::Elem]; 32]> {
        self.data_targets
            .iter()
            .map(|&index| {
                self.decode_matrix
                    .as_ref()
                    .expect("requested missing data requires an inverse")
                    .get_row(index)
            })
            .collect()
    }

    pub(crate) fn parity_rows<'a>(
        &'a self,
        codec: &'a ReedSolomon<F>,
    ) -> SmallVec<[&'a [F::Elem]; 32]> {
        if self.direct_parity {
            self.parity_decode_rows
                .iter()
                .map(|row| row.as_slice())
                .collect()
        } else {
            self.parity_targets
                .iter()
                .map(|&index| codec.encoding_format().matrix().get_row(index))
                .collect()
        }
    }
}

impl<F: Field> ReedSolomon<F> {
    #[inline]
    fn scan_classic_reconstruct_shape(
        &self,
        lengths: impl ExactSizeIterator<Item = Option<usize>>,
        mut observe: impl FnMut(usize, bool),
    ) -> Result<(usize, usize), Error> {
        if lengths.len() < self.total_shard_count() {
            return Err(Error::TooFewShards);
        }
        if lengths.len() > self.total_shard_count() {
            return Err(Error::TooManyShards);
        }
        let mut shard_len = 0;
        let mut number_present = 0;
        for (index, length) in lengths.enumerate() {
            if let Some(length) = length {
                if length == 0 {
                    return Err(Error::EmptyShard);
                }
                if shard_len != 0 && length != shard_len {
                    return Err(Error::IncorrectShardSize);
                }
                shard_len = length;
                number_present += 1;
            }
            observe(index, length.is_some());
        }
        Ok((shard_len, number_present))
    }

    /// Keep the validated all-present path free of shape and matrix planning.
    #[inline]
    pub(crate) fn with_classic_reconstruct_plan<T: ReconstructShard<F>>(
        &self,
        shards: &mut [T],
        goal: ReconstructionGoal<'_>,
        execute: impl FnOnce(&mut [T], &ClassicReconstructPlan<F>) -> Result<(), Error>,
    ) -> Result<(), Error> {
        if let ReconstructionGoal::Required(required) = goal
            && required.len() != self.data_shard_count()
            && required.len() != self.total_shard_count()
        {
            return Err(Error::InvalidShardFlags);
        }
        let (_, number_present) = self
            .scan_classic_reconstruct_shape(shards.iter().map(ReconstructShard::len), |_, _| {})?;
        if number_present == self.total_shard_count() {
            #[cfg(feature = "std")]
            self.runtime_profile_metrics.record_reconstruct(
                matches!(goal, ReconstructionGoal::Data),
                0,
                0,
                true,
            );
            return Ok(());
        }
        let shape = self.classic_reconstruct_shape(shards.iter().map(ReconstructShard::len))?;
        let plan = self.prepare_classic_reconstruct_plan(shape, goal)?;
        execute(shards, &plan)
    }

    pub(crate) fn classic_reconstruct_shape(
        &self,
        lengths: impl ExactSizeIterator<Item = Option<usize>>,
    ) -> Result<ClassicReconstructShape, Error> {
        let mut shape = ClassicReconstructShape {
            shard_len: 0,
            number_present: 0,
            valid_indices: SmallVec::new(),
            invalid_indices: SmallVec::new(),
        };
        let (shard_len, number_present) =
            self.scan_classic_reconstruct_shape(lengths, |index, present| {
                if present {
                    if !shape.invalid_indices.is_empty()
                        && shape.valid_indices.len() < self.data_shard_count()
                    {
                        shape.valid_indices.push(index);
                    }
                } else {
                    // Before the first erasure every earlier index is present.
                    // Defer survivor storage until recovery is actually needed,
                    // keeping all-present wide layouts allocation-free.
                    if shape.invalid_indices.is_empty() {
                        shape
                            .valid_indices
                            .extend(0..index.min(self.data_shard_count()));
                    }
                    shape.invalid_indices.push(index);
                }
            })?;
        shape.shard_len = shard_len;
        shape.number_present = number_present;
        Ok(shape)
    }

    /// Reusing a plan validates the layout without allocating new index lists.
    pub(crate) fn validate_classic_plan_layout(
        &self,
        lengths: impl ExactSizeIterator<Item = Option<usize>>,
        expected: &ClassicReconstructShape,
    ) -> Result<(), Error> {
        let mut missing = expected.invalid_indices.iter().copied().peekable();
        let mut same_pattern = true;
        let (shard_len, _) = self.scan_classic_reconstruct_shape(lengths, |index, present| {
            let was_missing = missing.peek() == Some(&index);
            if was_missing {
                missing.next();
            }
            same_pattern &= present != was_missing;
        })?;
        if !same_pattern {
            return Err(Error::InvalidShardFlags);
        }
        if shard_len != expected.shard_len {
            return Err(Error::IncorrectShardSize);
        }
        Ok(())
    }

    pub(crate) fn prepare_classic_reconstruct_plan(
        &self,
        shape: ClassicReconstructShape,
        goal: ReconstructionGoal<'_>,
    ) -> Result<ClassicReconstructPlan<F>, Error> {
        if shape.number_present < self.data_shard_count() {
            return Err(Error::TooFewShardsPresent);
        }
        if let ReconstructionGoal::Required(required) = goal
            && required.len() != self.data_shard_count()
            && required.len() != self.total_shard_count()
        {
            return Err(Error::InvalidShardFlags);
        }
        let selected = |index: usize| match goal {
            ReconstructionGoal::All => true,
            ReconstructionGoal::Data => index < self.data_shard_count(),
            ReconstructionGoal::Required(required) => required.get(index).copied().unwrap_or(false),
        };
        // Missing indices are ordered by shard index, so data and parity are
        // adjacent slices of one list rather than separately stored copies.
        let (missing_data, missing_parity) = shape
            .invalid_indices
            .split_at(shape.missing_data_count(self.data_shard_count()));
        let data_targets: SmallVec<[usize; 32]> = missing_data
            .iter()
            .copied()
            .filter(|&index| selected(index))
            .collect();
        let parity_targets: SmallVec<[usize; 32]> = missing_parity
            .iter()
            .copied()
            .filter(|&index| selected(index))
            .collect();
        // Selecting requested missing outputs first makes an empty request a
        // validated no-op even when the unused survivor matrix is singular.
        let needs_inverse =
            !missing_data.is_empty() && (!data_targets.is_empty() || !parity_targets.is_empty());
        let decode_matrix = if needs_inverse {
            Some(self.get_data_decode_matrix(&shape.valid_indices, &shape.invalid_indices)?)
        } else {
            None
        };
        let direct_parity = matches!(goal, ReconstructionGoal::Required(_))
            && !missing_data.is_empty()
            && !parity_targets.is_empty();
        let mut parity_decode_rows = SmallVec::new();
        if direct_parity {
            let inverse = decode_matrix
                .as_ref()
                .expect("missing data needs an inverse");
            for &index in &parity_targets {
                let parity = self.encoding_format().matrix().get_row(index);
                let row = (0..self.data_shard_count())
                    .map(|column| {
                        parity.iter().copied().enumerate().fold(
                            F::zero(),
                            |sum, (data, coefficient)| {
                                F::add(sum, F::mul(coefficient, inverse.get(data, column)))
                            },
                        )
                    })
                    .collect();
                parity_decode_rows.push(row);
            }
        }
        Ok(ClassicReconstructPlan {
            shape,
            data_targets,
            parity_targets,
            data_only: matches!(goal, ReconstructionGoal::Data),
            direct_parity,
            decode_matrix,
            parity_decode_rows,
        })
    }
}
