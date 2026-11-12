extern crate alloc;

use alloc::{sync::Arc, vec::Vec};
use smallvec::SmallVec;

use crate::{Field, matrix::Matrix};

/// Shared inverse coefficients in selected-survivor column order.
/// Keeping the Arc inside each variant preserves dense matrix storage and
/// lets a compact inverse own only its missing rows and their indices.
#[derive(Debug)]
pub(crate) enum DecodeMatrix<F: Field> {
    Dense(Arc<Matrix<F>>),
    Compact(Arc<CompactDecodeMatrix<F>>),
}

#[derive(Debug)]
pub(crate) struct CompactDecodeMatrix<F: Field> {
    data_shards: usize,
    missing: SmallVec<[usize; 4]>,
    coefficients: Vec<F::Elem>,
    zero: F::Elem,
    one: F::Elem,
}

impl<F: Field> Clone for DecodeMatrix<F> {
    fn clone(&self) -> Self {
        match self {
            Self::Dense(matrix) => Self::Dense(Arc::clone(matrix)),
            Self::Compact(matrix) => Self::Compact(Arc::clone(matrix)),
        }
    }
}

impl<F: Field> DecodeMatrix<F> {
    pub(crate) fn dense(matrix: Matrix<F>) -> Self {
        Self::Dense(Arc::new(matrix))
    }

    pub(crate) fn compact(
        data_shards: usize,
        missing: &[usize],
        coefficients: Vec<F::Elem>,
    ) -> Self {
        debug_assert!(data_shards >= 64);
        debug_assert!((1..=4).contains(&missing.len()));
        debug_assert!(missing.windows(2).all(|pair| pair[0] < pair[1]));
        debug_assert!(missing.iter().all(|&index| index < data_shards));
        debug_assert_eq!(coefficients.len(), missing.len() * data_shards);
        Self::Compact(Arc::new(CompactDecodeMatrix {
            data_shards,
            missing: SmallVec::from_slice(missing),
            coefficients,
            zero: F::zero(),
            one: F::one(),
        }))
    }

    /// Borrow a missing data row for coding directly into its destination.
    /// Callers select this row from the validated missing-data target list.
    pub(crate) fn missing_row(&self, data_index: usize) -> &[F::Elem] {
        match self {
            Self::Dense(matrix) => matrix.get_row(data_index),
            Self::Compact(matrix) => {
                let row = matrix
                    .missing
                    .binary_search(&data_index)
                    .expect("requested inverse row belongs to a missing data shard");
                let start = row * matrix.data_shards;
                &matrix.coefficients[start..start + matrix.data_shards]
            }
        }
    }

    /// Read any logical row, including a surviving systematic data row.
    /// All selected systematic inputs precede selected parity inputs, so a
    /// survivor's unit column is its index minus earlier missing data rows.
    pub(crate) fn get(&self, data_index: usize, input_column: usize) -> F::Elem {
        match self {
            Self::Dense(matrix) => matrix.get(data_index, input_column),
            Self::Compact(matrix) => {
                assert!(data_index < matrix.data_shards && input_column < matrix.data_shards);
                match matrix.missing.binary_search(&data_index) {
                    Ok(row) => matrix.coefficients[row * matrix.data_shards + input_column],
                    Err(earlier_missing) => {
                        if input_column == data_index - earlier_missing {
                            matrix.one
                        } else {
                            matrix.zero
                        }
                    }
                }
            }
        }
    }

    pub(crate) fn retained_bytes(&self) -> usize {
        match self {
            Self::Dense(matrix) => matrix.retained_bytes(),
            Self::Compact(matrix) => {
                let coefficient_bytes = matrix
                    .coefficients
                    .capacity()
                    .saturating_mul(core::mem::size_of::<F::Elem>());
                let index_bytes = if matrix.missing.spilled() {
                    matrix
                        .missing
                        .capacity()
                        .saturating_mul(core::mem::size_of::<usize>())
                } else {
                    0
                };
                core::mem::size_of::<CompactDecodeMatrix<F>>()
                    .saturating_add(coefficient_bytes)
                    .saturating_add(index_bytes)
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn row_count(&self) -> usize {
        match self {
            Self::Dense(matrix) => matrix.row_count(),
            Self::Compact(matrix) => matrix.data_shards,
        }
    }

    #[cfg(test)]
    pub(crate) fn ptr_eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Dense(left), Self::Dense(right)) => Arc::ptr_eq(left, right),
            (Self::Compact(left), Self::Compact(right)) => Arc::ptr_eq(left, right),
            _ => false,
        }
    }
}
