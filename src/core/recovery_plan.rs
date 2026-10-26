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
    // Complete requests borrow the ordered missing indices from shape.
    // A proper subset owns one filtered list; its combined inline capacity
    // preserves the two previous 32-index target buffers without a new spill.
    selected_targets: Option<SmallVec<[usize; 64]>>,
    data_target_count: usize,
    pub(crate) data_only: bool,
    pub(crate) direct_parity: bool,
    decode_matrix: Option<Arc<Matrix<F>>>,
    parity_decode_rows: SmallVec<[SmallVec<[F::Elem; 32]>; 4]>,
}

/// A bounded range of the plan's data rows followed by its parity rows.
/// The plan owns every coefficient; execution only borrows it, without building
/// a second table of slice references for either stage or their combination.
#[cfg(feature = "std")]
pub(crate) struct ClassicReconstructRows<'a, F: Field> {
    plan: &'a ClassicReconstructPlan<F>,
    encoding_matrix: &'a Matrix<F>,
    start: usize,
    len: usize,
}

#[cfg(feature = "std")]
impl<F: Field> ClassicReconstructRows<'_, F> {
    pub(crate) fn split_at(&self, index: usize) -> (Self, Self) {
        assert!(index <= self.len);
        (
            Self {
                len: index,
                ..*self
            },
            Self {
                start: self.start + index,
                len: self.len - index,
                ..*self
            },
        )
    }
}

#[cfg(feature = "std")]
impl<F: Field> crate::matrix::MatrixRows<F::Elem> for ClassicReconstructRows<'_, F> {
    fn len(&self) -> usize {
        self.len
    }

    fn row(&self, index: usize) -> &[F::Elem] {
        assert!(index < self.len);
        let index = self.start + index;
        let data_targets = self.plan.data_targets();
        if index < data_targets.len() {
            return self
                .plan
                .decode_matrix
                .as_ref()
                .expect("requested missing data requires an inverse")
                .get_row(data_targets[index]);
        }
        let index = index - data_targets.len();
        if self.plan.direct_parity {
            self.plan.parity_decode_rows[index].as_slice()
        } else {
            self.encoding_matrix
                .get_row(self.plan.parity_targets()[index])
        }
    }
}

impl<F: Field> ClassicReconstructPlan<F> {
    fn new(shape: ClassicReconstructShape, goal: ReconstructionGoal<'_>) -> Self {
        Self {
            shape,
            selected_targets: None,
            data_target_count: 0,
            data_only: matches!(goal, ReconstructionGoal::Data),
            direct_parity: false,
            decode_matrix: None,
            parity_decode_rows: SmallVec::new(),
        }
    }

    fn target_indices(&self) -> &[usize] {
        self.selected_targets
            .as_deref()
            .unwrap_or(self.shape.invalid_indices.as_slice())
    }

    pub(crate) fn data_targets(&self) -> &[usize] {
        &self.target_indices()[..self.data_target_count]
    }

    pub(crate) fn parity_targets(&self) -> &[usize] {
        if self.data_only {
            &[]
        } else {
            &self.target_indices()[self.data_target_count..]
        }
    }

    pub(crate) fn has_outputs(&self) -> bool {
        !self.data_targets().is_empty() || !self.parity_targets().is_empty()
    }

    #[cfg(feature = "std")]
    pub(crate) fn row_views<'a>(
        &'a self,
        codec: &'a ReedSolomon<F>,
    ) -> ClassicReconstructRows<'a, F> {
        // Preserve the inverse invariant check before destinations are touched.
        if !self.data_targets().is_empty() {
            self.decode_matrix
                .as_ref()
                .expect("requested missing data requires an inverse");
        }
        ClassicReconstructRows {
            plan: self,
            encoding_matrix: codec.encoding_format().matrix(),
            start: 0,
            len: self.data_targets().len() + self.parity_targets().len(),
        }
    }

    pub(crate) fn data_rows(&self) -> impl ExactSizeIterator<Item = &[F::Elem]> {
        self.data_targets().iter().map(|&index| {
            self.decode_matrix
                .as_ref()
                .expect("requested missing data requires an inverse")
                .get_row(index)
        })
    }

    pub(crate) fn parity_rows<'a>(
        &'a self,
        codec: &'a ReedSolomon<F>,
    ) -> impl Iterator<Item = &'a [F::Elem]> {
        // Exactly one slice is active. Chaining their exact size hints also
        // keeps bulk callers' existing SmallVec reservation behavior.
        let direct = if self.direct_parity {
            self.parity_decode_rows.as_slice()
        } else {
            &[]
        };
        let encoding = if self.direct_parity {
            &[]
        } else {
            self.parity_targets()
        };
        direct.iter().map(|row| row.as_slice()).chain(
            encoding
                .iter()
                .map(move |&index| codec.encoding_format().matrix().get_row(index)),
        )
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
        let data_shards = self.data_shard_count();
        let inline_inputs = data_shards.min(shape.valid_indices.inline_size());
        shape
            .valid_indices
            .extend(0..first_missing.min(inline_inputs));
        shape.invalid_indices.push(first_missing);
        // Keep selected inputs inline until every length and the quorum have
        // passed. Failed recovery never needs their spilled index table.
        for (index, length) in lengths {
            if state.observe(length)? {
                if shape.valid_indices.len() < inline_inputs {
                    shape.valid_indices.push(index);
                }
            } else {
                shape.invalid_indices.push(index);
            }
        }
        if state.number_present < data_shards {
            return Err(Error::TooFewShardsPresent);
        }
        shape.shard_len = state.shard_len;
        shape.number_present = state.number_present;
        let remaining = data_shards - shape.valid_indices.len();
        if remaining != 0 {
            // The erasure list completely describes the remaining survivors.
            // Fill their final storage without rereading shard lengths or
            // introducing an auxiliary presence bitmap/index list.
            shape.valid_indices.reserve_exact(remaining);
            let start = shape.valid_indices.last().map_or(0, |index| index + 1);
            let first_erasure = shape
                .invalid_indices
                .partition_point(|&index| index < start);
            let mut missing = shape.invalid_indices[first_erasure..]
                .iter()
                .copied()
                .peekable();
            shape.valid_indices.extend(
                (start..self.total_shard_count())
                    .filter(|index| {
                        if missing.peek() == Some(index) {
                            missing.next();
                            false
                        } else {
                            true
                        }
                    })
                    .take(remaining),
            );
        }
        debug_assert_eq!(shape.valid_indices.len(), data_shards);
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
        let missing_data_count = shape.missing_data_count(self.data_shard_count());
        let missing_data = &shape.invalid_indices[..missing_data_count];
        match goal {
            ReconstructionGoal::All | ReconstructionGoal::Data => {
                plan.data_target_count = missing_data_count;
            }
            ReconstructionGoal::Required(required) => {
                // Share the plan-owned erasure list when nothing is excluded.
                // Only a proper subset needs another index buffer. Scan each
                // request flag once, copying the accepted prefix after the
                // first exclusion rather than filtering it a second time.
                if let Some(first_excluded) = shape
                    .invalid_indices
                    .iter()
                    .position(|&index| !required.get(index).copied().unwrap_or(false))
                {
                    let targets = plan.selected_targets.get_or_insert_with(SmallVec::new);
                    targets.extend_from_slice(&shape.invalid_indices[..first_excluded]);
                    targets.extend(
                        shape.invalid_indices[first_excluded + 1..]
                            .iter()
                            .copied()
                            .filter(|&index| required.get(index).copied().unwrap_or(false)),
                    );
                    plan.data_target_count =
                        targets.partition_point(|&index| index < self.data_shard_count());
                } else {
                    plan.data_target_count = missing_data_count;
                }
            }
        }
        // Selecting requested missing outputs first makes an empty request a
        // validated no-op even when the unused survivor matrix is singular.
        let needs_inverse = !missing_data.is_empty()
            && (!plan.data_targets().is_empty() || !plan.parity_targets().is_empty());
        plan.decode_matrix = if needs_inverse {
            Some(self.get_data_decode_matrix(&shape.valid_indices, &shape.invalid_indices)?)
        } else {
            None
        };
        plan.direct_parity = matches!(goal, ReconstructionGoal::Required(_))
            && !missing_data.is_empty()
            && !plan.parity_targets().is_empty();
        if plan.direct_parity {
            let inverse = plan
                .decode_matrix
                .as_ref()
                .expect("missing data needs an inverse");
            // Borrow these fields separately while filling the independent rows.
            let targets = plan
                .selected_targets
                .as_deref()
                .unwrap_or(&shape.invalid_indices);
            for &index in &targets[plan.data_target_count..] {
                let parity = self.encoding_format().matrix().get_row(index);
                let row = (0..self.data_shard_count())
                    .map(|column| {
                        // Every surviving systematic data row of the inverse
                        // is a unit row at its selected-input column. Substitute
                        // that known term directly, then compose only missing
                        // data rows. Unrequested missing data still contributes
                        // algebraically without initializing its destination.
                        let survivor = shape.valid_indices[column];
                        let known = if survivor < self.data_shard_count() {
                            parity[survivor]
                        } else {
                            F::zero()
                        };
                        missing_data.iter().fold(known, |sum, &data| {
                            F::add(sum, F::mul(parity[data], inverse.get(data, column)))
                        })
                    })
                    .collect();
                plan.parity_decode_rows.push(row);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
