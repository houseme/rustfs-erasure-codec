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

/// The same validation state is carried across the present prefix and the
/// recovery suffix, and is also used when checking a reused workspace layout.
#[derive(Default)]
struct ClassicShardLengths {
    shard_len: usize,
    number_present: usize,
}

impl ClassicShardLengths {
    #[inline]
    fn observe(&mut self, length: Option<usize>) -> Result<bool, Error> {
        let Some(length) = length else {
            return Ok(false);
        };
        if length == 0 {
            return Err(Error::EmptyShard);
        }
        if self.shard_len != 0 && length != self.shard_len {
            return Err(Error::IncorrectShardSize);
        }
        self.shard_len = length;
        self.number_present += 1;
        Ok(true)
    }

    /// Stop at the first erasure without reading any suffix length twice.
    #[inline]
    fn scan_present_prefix(
        &mut self,
        lengths: &mut impl Iterator<Item = (usize, Option<usize>)>,
    ) -> Result<Option<usize>, Error> {
        for (index, length) in lengths {
            if !self.observe(length)? {
                return Ok(Some(index));
            }
        }
        Ok(None)
    }
}

/// Validated, immutable erasure layout shared by all Classic entry points.
#[derive(Clone, Default)]
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
    fn new(shape: ClassicReconstructShape, goal: ReconstructionGoal<'_>) -> Self {
        Self {
            shape,
            data_targets: SmallVec::new(),
            parity_targets: SmallVec::new(),
            data_only: matches!(goal, ReconstructionGoal::Data),
            direct_parity: false,
            decode_matrix: None,
            parity_decode_rows: SmallVec::new(),
        }
    }

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
    fn validate_classic_shard_count(&self, count: usize) -> Result<(), Error> {
        if count < self.total_shard_count() {
            return Err(Error::TooFewShards);
        }
        if count > self.total_shard_count() {
            return Err(Error::TooManyShards);
        }
        Ok(())
    }

    #[inline]
    fn scan_classic_reconstruct_shape(
        &self,
        lengths: impl ExactSizeIterator<Item = Option<usize>>,
        mut observe: impl FnMut(usize, bool),
    ) -> Result<(usize, usize), Error> {
        self.validate_classic_shard_count(lengths.len())?;
        let mut state = ClassicShardLengths::default();
        for (index, length) in lengths.enumerate() {
            observe(index, state.observe(length)?);
        }
        Ok((state.shard_len, state.number_present))
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
        self.validate_classic_shard_count(shards.len())?;
        let mut lengths = shards.iter().map(ReconstructShard::len).enumerate();
        let mut state = ClassicShardLengths::default();
        let Some(first_missing) = state.scan_present_prefix(&mut lengths)? else {
            #[cfg(feature = "std")]
            self.runtime_profile_metrics.record_reconstruct(
                matches!(goal, ReconstructionGoal::Data),
                0,
                0,
                true,
            );
            return Ok(());
        };
        // Build the scoped plan in its final storage. Returning large owned
        // shapes/plans through Result needlessly moves their inline metadata.
        let mut plan = ClassicReconstructPlan::new(ClassicReconstructShape::default(), goal);
        self.fill_classic_reconstruct_shape_from_suffix(
            &mut plan.shape,
            lengths,
            state,
            first_missing,
        )?;
        self.fill_classic_reconstruct_plan(&mut plan, goal)?;
        execute(shards, &plan)
    }

    pub(crate) fn classic_reconstruct_shape(
        &self,
        lengths: impl ExactSizeIterator<Item = Option<usize>>,
    ) -> Result<ClassicReconstructShape, Error> {
        self.validate_classic_shard_count(lengths.len())?;
        let mut lengths = lengths.enumerate();
        let mut state = ClassicShardLengths::default();
        if let Some(first_missing) = state.scan_present_prefix(&mut lengths)? {
            let mut shape = ClassicReconstructShape::default();
            self.fill_classic_reconstruct_shape_from_suffix(
                &mut shape,
                lengths,
                state,
                first_missing,
            )?;
            Ok(shape)
        } else {
            // Explicit workspace preparation still needs a shape for later
            // layout validation, but an all-present shape needs no index storage.
            Ok(ClassicReconstructShape {
                shard_len: state.shard_len,
                number_present: state.number_present,
                valid_indices: SmallVec::new(),
                invalid_indices: SmallVec::new(),
            })
        }
    }

    /// Fill a fresh shape without returning its inline index buffers by value.
    fn fill_classic_reconstruct_shape_from_suffix(
        &self,
        shape: &mut ClassicReconstructShape,
        lengths: impl Iterator<Item = (usize, Option<usize>)>,
        mut state: ClassicShardLengths,
        first_missing: usize,
    ) -> Result<(), Error> {
        shape
            .valid_indices
            .extend(0..first_missing.min(self.data_shard_count()));
        shape.invalid_indices.push(first_missing);
        // Do not decide quorum or initialize destinations until every remaining
        // present length has been checked, even if too many erasures are known.
        for (index, length) in lengths {
            if state.observe(length)? {
                if shape.valid_indices.len() < self.data_shard_count() {
                    shape.valid_indices.push(index);
                }
            } else {
                shape.invalid_indices.push(index);
            }
        }
        shape.shard_len = state.shard_len;
        shape.number_present = state.number_present;
        Ok(())
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
        let mut plan = ClassicReconstructPlan::new(shape, goal);
        self.fill_classic_reconstruct_plan(&mut plan, goal)?;
        Ok(plan)
    }

    /// Complete a fresh plan before any caller can initialize destinations.
    /// Workspaces own the result; scoped calls keep it in its execution frame.
    fn fill_classic_reconstruct_plan(
        &self,
        plan: &mut ClassicReconstructPlan<F>,
        goal: ReconstructionGoal<'_>,
    ) -> Result<(), Error> {
        let shape = &plan.shape;
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
        plan.data_targets.extend(
            missing_data
                .iter()
                .copied()
                .filter(|&index| selected(index)),
        );
        plan.parity_targets.extend(
            missing_parity
                .iter()
                .copied()
                .filter(|&index| selected(index)),
        );
        // Selecting requested missing outputs first makes an empty request a
        // validated no-op even when the unused survivor matrix is singular.
        let needs_inverse = !missing_data.is_empty()
            && (!plan.data_targets.is_empty() || !plan.parity_targets.is_empty());
        plan.decode_matrix = if needs_inverse {
            Some(self.get_data_decode_matrix(&shape.valid_indices, &shape.invalid_indices)?)
        } else {
            None
        };
        plan.direct_parity = matches!(goal, ReconstructionGoal::Required(_))
            && !missing_data.is_empty()
            && !plan.parity_targets.is_empty();
        if plan.direct_parity {
            let inverse = plan
                .decode_matrix
                .as_ref()
                .expect("missing data needs an inverse");
            for &index in &plan.parity_targets {
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
                plan.parity_decode_rows.push(row);
            }
        }
        Ok(())
    }
}
