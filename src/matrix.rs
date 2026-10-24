#![allow(dead_code)]
extern crate alloc;

#[cfg(test)]
use alloc::vec;
use alloc::vec::Vec;

use crate::Field;
use smallvec::SmallVec;

#[derive(Debug, PartialEq)]
pub enum Error {
    EmptyInput,
    InconsistentRowSizes,
    IncompatibleDimensions,
    NonSquare,
    SingularMatrix,
}

macro_rules! acc {
    (
        $m:ident, $r:expr, $c:expr
    ) => {
        $m.data[$r * $m.col_count + $c]
    };
}

pub fn flatten<T>(m: Vec<Vec<T>>) -> Vec<T> {
    let mut result: Vec<T> = Vec::with_capacity(m.len() * m[0].len());
    for row in m {
        for v in row {
            result.push(v);
        }
    }
    result
}

#[derive(PartialEq, Debug, Clone)]
pub struct Matrix<F: Field> {
    row_count: usize,
    col_count: usize,
    // Row-major flattened storage. The inline SmallVec capacity keeps matrices
    // up to 32x32 on the stack before spilling to the heap.
    data: SmallVec<[F::Elem; 1024]>,
}

/// Static row access for either borrowed matrix ranges or existing decode views.
/// No trait objects or owned reference tables are needed by the coding loops.
pub(crate) trait MatrixRows<E> {
    fn len(&self) -> usize;
    fn row(&self, index: usize) -> &[E];
}

impl<E> MatrixRows<E> for [&[E]] {
    fn len(&self) -> usize {
        <[&[E]]>::len(self)
    }

    fn row(&self, index: usize) -> &[E] {
        self[index]
    }
}

impl<E, const N: usize> MatrixRows<E> for [&[E]; N] {
    fn len(&self) -> usize {
        N
    }

    fn row(&self, index: usize) -> &[E] {
        self[index]
    }
}

pub(crate) struct MatrixRowRange<'a, F: Field> {
    matrix: &'a Matrix<F>,
    start: usize,
    len: usize,
}

impl<F: Field> MatrixRowRange<'_, F> {
    pub(crate) fn get_row(&self, index: usize) -> &[F::Elem] {
        assert!(index < self.len);
        self.matrix.get_row(self.start + index)
    }
}

impl<F: Field> MatrixRows<F::Elem> for MatrixRowRange<'_, F> {
    fn len(&self) -> usize {
        self.len
    }

    fn row(&self, index: usize) -> &[F::Elem] {
        self.get_row(index)
    }
}

fn calc_matrix_row_start_end(col_count: usize, row: usize) -> (usize, usize) {
    let start = row * col_count;
    let end = start + col_count;

    (start, end)
}

impl<F: Field> Matrix<F> {
    fn calc_row_start_end(&self, row: usize) -> (usize, usize) {
        calc_matrix_row_start_end(self.col_count, row)
    }

    pub fn new(rows: usize, cols: usize) -> Matrix<F> {
        let data = SmallVec::from_elem(F::zero(), rows * cols);

        Matrix {
            row_count: rows,
            col_count: cols,
            data,
        }
    }

    pub fn new_with_data(init_data: Vec<Vec<F::Elem>>) -> Result<Matrix<F>, Error> {
        if init_data.is_empty() {
            return Err(Error::EmptyInput);
        }

        let rows = init_data.len();
        let cols = init_data[0].len();

        for r in init_data.iter() {
            if r.len() != cols {
                return Err(Error::InconsistentRowSizes);
            }
        }

        let data = SmallVec::from_vec(flatten(init_data));

        Ok(Matrix {
            row_count: rows,
            col_count: cols,
            data,
        })
    }

    #[cfg(test)]
    pub fn make_random(size: usize) -> Matrix<F>
    where
        rand::distr::StandardUniform: rand::distr::Distribution<F::Elem>,
    {
        let mut vec: Vec<Vec<F::Elem>> = vec![vec![Default::default(); size]; size];
        for v in vec.iter_mut() {
            crate::tests::fill_random(v);
        }

        Matrix::new_with_data(vec).unwrap()
    }

    pub fn identity(size: usize) -> Matrix<F> {
        let mut result = Self::new(size, size);
        for i in 0..size {
            acc!(result, i, i) = F::one();
        }
        result
    }

    pub fn col_count(&self) -> usize {
        self.col_count
    }

    pub fn row_count(&self) -> usize {
        self.row_count
    }

    pub fn get(&self, r: usize, c: usize) -> F::Elem {
        acc!(self, r, c)
    }

    pub fn set(&mut self, r: usize, c: usize, val: F::Elem) {
        acc!(self, r, c) = val;
    }

    pub fn multiply(&self, rhs: &Matrix<F>) -> Result<Matrix<F>, Error> {
        if self.col_count != rhs.row_count {
            return Err(Error::IncompatibleDimensions);
        }
        let mut result = Self::new(self.row_count, rhs.col_count);
        if rhs.col_count == 0 {
            return Ok(result);
        }
        let byte_rows = F::as_gf8_slice(&[]).is_some();
        for row in 0..self.row_count {
            let start = row * rhs.col_count;
            let output = &mut result.data[start..start + rhs.col_count];
            for (inner, &coefficient) in self.get_row(row).iter().enumerate() {
                if coefficient != F::zero() {
                    Self::add_scaled_slice(coefficient, rhs.get_row(inner), output, byte_rows);
                }
            }
        }
        Ok(result)
    }

    pub fn augment(&self, rhs: &Matrix<F>) -> Result<Matrix<F>, Error> {
        if self.row_count != rhs.row_count {
            return Err(Error::IncompatibleDimensions);
        }
        let mut result = Self::new(self.row_count, self.col_count + rhs.col_count);
        for r in 0..self.row_count {
            for c in 0..self.col_count {
                acc!(result, r, c) = acc!(self, r, c);
            }
            let self_column_count = self.col_count;
            for c in 0..rhs.col_count {
                acc!(result, r, self_column_count + c) = acc!(rhs, r, c);
            }
        }

        Ok(result)
    }

    pub fn sub_matrix(&self, rmin: usize, cmin: usize, rmax: usize, cmax: usize) -> Matrix<F> {
        let mut result = Self::new(rmax - rmin, cmax - cmin);
        for r in rmin..rmax {
            for c in cmin..cmax {
                acc!(result, r - rmin, c - cmin) = acc!(self, r, c);
            }
        }
        result
    }

    pub(crate) fn row_range(&self, start: usize, end: usize) -> MatrixRowRange<'_, F> {
        assert!(start <= end && end <= self.row_count);
        MatrixRowRange {
            matrix: self,
            start,
            len: end - start,
        }
    }

    pub fn get_row(&self, row: usize) -> &[F::Elem] {
        let (start, end) = self.calc_row_start_end(row);

        &self.data[start..end]
    }

    pub fn swap_rows(&mut self, r1: usize, r2: usize) {
        let (r1_s, _) = self.calc_row_start_end(r1);
        let (r2_s, _) = self.calc_row_start_end(r2);

        if r1 == r2 {
        } else {
            for i in 0..self.col_count {
                self.data.swap(r1_s + i, r2_s + i);
            }
        }
    }

    pub fn is_square(&self) -> bool {
        self.row_count == self.col_count
    }

    #[inline]
    fn add_scaled_slice(
        scale: F::Elem,
        input: &[F::Elem],
        output: &mut [F::Elem],
        byte_rows: bool,
    ) {
        if byte_rows {
            F::mul_slice_add(scale, input, output);
        } else {
            // Block-oriented fields can need plane conversion and large scratch.
            // Matrix rows keep their scalar arithmetic with contiguous traversal.
            for (output, &input) in output.iter_mut().zip(input) {
                *output = F::add(*output, F::mul(scale, input));
            }
        }
    }

    /// Eliminate with a normalized pivot row whose preceding columns are zero.
    /// Borrow disjoint rows without copying the pivot into scratch.
    #[inline]
    fn add_scaled_pivot_row(
        &mut self,
        source: usize,
        destination: usize,
        scale: F::Elem,
        byte_rows: bool,
    ) {
        debug_assert_ne!(source, destination);
        let width = self.col_count;
        let (input, output): (&[F::Elem], &mut [F::Elem]) = if source < destination {
            let (before, after) = self.data.split_at_mut(destination * width);
            (
                &before[source * width..(source + 1) * width],
                &mut after[..width],
            )
        } else {
            let (before, after) = self.data.split_at_mut(source * width);
            (
                &after[..width],
                &mut before[destination * width..(destination + 1) * width],
            )
        };
        // Earlier forward pivots made this prefix zero. Row swaps only involve
        // remaining rows, and back substitution preserves the same invariant.
        // Retain full byte-field batch rows: trimming each pivot introduces
        // varying SIMD tails. Scalar arithmetic can omit the zero prefix.
        debug_assert!(input[..source].iter().all(|&value| value == F::zero()));
        debug_assert_eq!(input[source], F::one());
        debug_assert_eq!(output[source], scale);
        let zero_prefix = if byte_rows { 0 } else { source + 1 };
        Self::add_scaled_slice(
            scale,
            &input[zero_prefix..],
            &mut output[zero_prefix..],
            byte_rows,
        );
        if !byte_rows {
            // The unit pivot contributes scale directly; keep the field's
            // addition operation while omitting multiplication by one.
            output[source] = F::add(output[source], scale);
        }
    }

    pub fn gaussian_elim(&mut self) -> Result<(), Error> {
        let byte_rows = F::as_gf8_slice(&[]).is_some();
        for r in 0..self.row_count {
            if acc!(self, r, r) == F::zero() {
                for r_below in r + 1..self.row_count {
                    if acc!(self, r_below, r) != F::zero() {
                        self.swap_rows(r, r_below);
                        break;
                    }
                }
            }
            // If we couldn't find one, the matrix is singular.
            if acc!(self, r, r) == F::zero() {
                return Err(Error::SingularMatrix);
            }
            // Scale to 1.
            if acc!(self, r, r) != F::one() {
                let scale = F::div(F::one(), acc!(self, r, r));
                let start = r * self.col_count;
                let zero_prefix = if byte_rows { 0 } else { r + 1 };
                for value in &mut self.data[start + zero_prefix..start + self.col_count] {
                    *value = F::mul(scale, *value);
                }
                if !byte_rows {
                    // Scalar normalization need not multiply its known unit pivot.
                    acc!(self, r, r) = F::one();
                }
            }
            // Make everything below the 1 be a 0 by subtracting
            // a multiple of it.  (Subtraction and addition are
            // both exclusive or in the Galois field.)
            for r_below in r + 1..self.row_count {
                if acc!(self, r_below, r) != F::zero() {
                    let scale = acc!(self, r_below, r);
                    self.add_scaled_pivot_row(r, r_below, scale, byte_rows);
                }
            }
        }

        // Now clear the part above the main diagonal.
        for d in 0..self.row_count {
            for r_above in 0..d {
                if acc!(self, r_above, d) != F::zero() {
                    let scale = acc!(self, r_above, d);
                    self.add_scaled_pivot_row(d, r_above, scale, byte_rows);
                }
            }
        }
        Ok(())
    }

    pub fn invert(&self) -> Result<Matrix<F>, Error> {
        if !self.is_square() {
            return Err(Error::NonSquare);
        }

        let row_count = self.row_count;
        let col_count = self.col_count;

        // Build [self | I] in its final zeroed storage. A separate identity
        // matrix would allocate and copy another square buffer for wide layouts.
        // Preserve the complete augmented row width and elimination order.
        let mut work = Self::new(row_count, col_count * 2);
        for row in 0..row_count {
            let start = row * work.col_count;
            work.data[start..start + col_count].copy_from_slice(self.get_row(row));
            acc!(work, row, col_count + row) = F::one();
        }
        work.gaussian_elim()?;

        Ok(work.sub_matrix(0, row_count, col_count, col_count * 2))
    }

    pub fn vandermonde(rows: usize, cols: usize) -> Matrix<F> {
        let mut result = Self::new(rows, cols);

        for r in 0..rows {
            // doesn't matter what `r_a` is as long as it's unique.
            // then the vandermonde matrix is invertible.
            let r_a = F::nth(r);
            for c in 0..cols {
                acc!(result, r, c) = F::exp(r_a, c);
            }
        }

        result
    }
}

#[cfg(test)]
mod tests {
    extern crate alloc;

    use alloc::{vec, vec::Vec};

    use super::{Error, Matrix};
    use crate::galois_8;

    #[test]
    fn borrowed_row_ranges_preserve_length_coefficients_and_row_identity() {
        use super::MatrixRows;
        fn check<F: crate::Field>() {
            for (rows, columns) in [(0, 0), (1, 0), (4, 3), (65, 40)] {
                let mut matrix = Matrix::<F>::new(rows, columns);
                for row in 0..rows {
                    for column in 0..columns {
                        matrix.set(row, column, F::nth((row * columns + column) % F::ORDER));
                    }
                }
                for start in 0..=rows {
                    for end in start..=rows {
                        let range = matrix.row_range(start, end);
                        assert_eq!(range.len(), end - start);
                        for index in 0..range.len() {
                            assert!(core::ptr::eq(
                                range.row(index),
                                matrix.get_row(start + index)
                            ));
                        }
                    }
                }
            }
        }
        check::<galois_8::Field>();
        check::<crate::galois_16::Field>();
    }

    #[test]
    #[should_panic]
    fn borrowed_row_range_rejects_access_beyond_its_own_end() {
        let matrix = Matrix::<galois_8::Field>::new(4, 3);
        matrix.row_range(1, 2).get_row(1);
    }

    macro_rules! matrix {
        (
            $(
                [ $( $x:expr ),+ ]
            ),*
        ) => (
            Matrix::<galois_8::Field>::new_with_data(vec![ $( vec![$( $x ),*] ),* ]).unwrap()
        );
        ($rows:expr, $cols:expr) => (Matrix::new($rows, $cols));
    }

    #[cfg(feature = "std")]
    mod active_pivot_suffix {
        use super::*;
        use std::cell::{Cell, RefCell};

        std::thread_local! {
            static SCALAR_PRODUCTS: Cell<usize> = const { Cell::new(0) };
            static BATCH_WIDTHS: RefCell<Vec<usize>> = const { RefCell::new(Vec::new()) };
        }

        struct ObservedField<const BYTE_ROWS: bool>;
        impl<const BYTE_ROWS: bool> crate::Field for ObservedField<BYTE_ROWS> {
            const ORDER: usize = 256;
            type Elem = u8;
            fn as_gf8_slice(input: &[u8]) -> Option<&[u8]> {
                BYTE_ROWS.then_some(input)
            }
            fn as_gf8_slice_mut(input: &mut [u8]) -> Option<&mut [u8]> {
                BYTE_ROWS.then_some(input)
            }
            fn add(a: u8, b: u8) -> u8 {
                galois_8::add(a, b)
            }
            fn mul(a: u8, b: u8) -> u8 {
                SCALAR_PRODUCTS.with(|count| count.set(count.get() + 1));
                galois_8::mul(a, b)
            }
            fn div(a: u8, b: u8) -> u8 {
                galois_8::div(a, b)
            }
            fn exp(a: u8, n: usize) -> u8 {
                galois_8::exp(a, n)
            }
            fn zero() -> u8 {
                0
            }
            fn one() -> u8 {
                1
            }
            fn nth_internal(n: usize) -> u8 {
                n as u8
            }
            fn mul_slice_add(scale: u8, input: &[u8], output: &mut [u8]) {
                BATCH_WIDTHS.with(|widths| widths.borrow_mut().push(input.len()));
                <galois_8::Field as crate::Field>::mul_slice_add(scale, input, output);
            }
        }

        fn invert_fixture<const BYTE_ROWS: bool>() -> (usize, Vec<usize>) {
            SCALAR_PRODUCTS.with(|count| count.set(0));
            BATCH_WIDTHS.with(|widths| widths.borrow_mut().clear());
            let input =
                Matrix::<ObservedField<BYTE_ROWS>>::new_with_data(vec![vec![1, 1], vec![1, 2]])
                    .unwrap();
            let inverse = input.invert().unwrap();
            // Independent 2x2 formula: det = 1*2 + 1*1 = 3 in GF8.
            assert_eq!(
                inverse.get_row(0),
                &[galois_8::div(2, 3), galois_8::div(1, 3)]
            );
            assert_eq!(
                inverse.get_row(1),
                &[galois_8::div(1, 3), galois_8::div(1, 3)]
            );
            (
                SCALAR_PRODUCTS.with(Cell::get),
                BATCH_WIDTHS.with(|widths| widths.take()),
            )
        }

        #[test]
        fn scalar_elimination_skips_zero_prefixes_and_unit_pivots() {
            // Three lower-tail updates, two normalization products and two
            // upper-tail updates; known pivot/prefix values need no products.
            assert_eq!(invert_fixture::<false>(), (7, vec![]));
        }

        #[test]
        fn byte_elimination_preserves_full_vector_rows() {
            BATCH_WIDTHS.with(|widths| widths.borrow_mut().clear());
            let input = Matrix::<ObservedField<true>>::vandermonde(32, 32);
            let inverse = input.invert().unwrap();
            assert_eq!(inverse.row_count(), 32);
            let widths = BATCH_WIDTHS.with(|widths| widths.take());
            assert!(!widths.is_empty());
            assert!(
                widths.iter().all(|&width| width == 64),
                "keep full byte rows available to the existing SIMD operators"
            );
        }

        #[test]
        fn byte_elimination_retains_normalization_and_batch_widths() {
            assert_eq!(invert_fixture::<true>(), (4, vec![4, 4]));
        }
    }

    #[test]
    fn test_matrix_col_count() {
        let m1 = matrix!([1, 0, 0]);
        let m2 = matrix!([0, 0, 0], [0, 0, 0]);
        let m3: Matrix<galois_8::Field> = Matrix::new(1, 4);

        assert_eq!(3, m1.col_count());
        assert_eq!(3, m2.col_count());
        assert_eq!(4, m3.col_count());
    }

    #[test]
    fn test_matrix_row_count() {
        let m1 = matrix!([1, 0, 0]);
        let m2 = matrix!([0, 0, 0], [0, 0, 0]);
        let m3: Matrix<galois_8::Field> = Matrix::new(1, 4);

        assert_eq!(1, m1.row_count());
        assert_eq!(2, m2.row_count());
        assert_eq!(1, m3.row_count());
    }

    #[test]
    fn test_matrix_swap_rows() {
        {
            let mut m1 = matrix!([1, 2, 3], [4, 5, 6], [7, 8, 9]);
            let expect = matrix!([7, 8, 9], [4, 5, 6], [1, 2, 3]);
            m1.swap_rows(0, 2);
            assert_eq!(expect, m1);
        }
        {
            let mut m1 = matrix!([1, 2, 3], [4, 5, 6], [7, 8, 9]);
            let expect = m1.clone();
            m1.swap_rows(0, 0);
            assert_eq!(expect, m1);
            m1.swap_rows(1, 1);
            assert_eq!(expect, m1);
            m1.swap_rows(2, 2);
            assert_eq!(expect, m1);
        }
    }

    #[test]
    fn test_inconsistent_row_sizes() {
        let err = Matrix::<galois_8::Field>::new_with_data(vec![
            vec![1, 0, 0],
            vec![0, 1],
            vec![0, 0, 1],
        ]);

        assert_eq!(err, Err(Error::InconsistentRowSizes));
    }

    #[test]
    fn test_incompatible_multiply() {
        let m1 = matrix!([0, 1], [0, 1], [0, 1]);
        let m2 = matrix!([0, 1, 2]);

        assert_eq!(m1.multiply(&m2), Err(Error::IncompatibleDimensions));
    }

    #[test]
    fn test_incompatible_augment() {
        let m1 = matrix!([0, 1]);
        let m2 = matrix!([0, 1], [2, 3]);

        assert_eq!(m1.augment(&m2), Err(Error::IncompatibleDimensions));
    }

    #[test]
    fn test_matrix_identity() {
        let m1 = Matrix::identity(3);
        let m2 = matrix!([1, 0, 0], [0, 1, 0], [0, 0, 1]);
        assert_eq!(m1, m2);
    }

    #[test]
    fn test_matrix_multiply() {
        let m1 = matrix!([1, 2], [3, 4]);
        let m2 = matrix!([5, 6], [7, 8]);
        let actual = m1.multiply(&m2).unwrap();
        let expect = matrix!([11, 22], [19, 42]);
        assert_eq!(actual, expect);
    }

    #[test]
    fn test_matrix_inverse_pass_cases() {
        {
            // Test case validating inverse of the input Matrix.
            let m = matrix!([56, 23, 98], [3, 100, 200], [45, 201, 123])
                .invert()
                .unwrap();
            let expect = matrix!([175, 133, 33], [130, 13, 245], [112, 35, 126]);
            assert_eq!(m, expect);
        }
        {
            // Test case validating inverse of the input Matrix.
            let m = matrix!(
                [1, 0, 0, 0, 0],
                [0, 1, 0, 0, 0],
                [0, 0, 0, 1, 0],
                [0, 0, 0, 0, 1],
                [7, 7, 6, 6, 1]
            )
            .invert()
            .unwrap();
            let expect = matrix!(
                [1, 0, 0, 0, 0],
                [0, 1, 0, 0, 0],
                [123, 123, 1, 122, 122],
                [0, 0, 1, 0, 0],
                [0, 0, 0, 1, 0]
            );
            assert_eq!(m, expect);
        }
    }

    #[test]
    fn test_matrix_inverse_non_square() {
        // Test case with a non-square matrix.
        assert_eq!(
            matrix!([56, 23], [3, 100], [45, 201]).invert(),
            Err(Error::NonSquare)
        );
    }

    fn assert_row_algebra<F: crate::Field>() {
        fn scalar_product<F: crate::Field>(
            left: &Matrix<F>,
            right: &Matrix<F>,
        ) -> Vec<Vec<F::Elem>> {
            (0..left.row_count())
                .map(|row| {
                    (0..right.col_count())
                        .map(|column| {
                            (0..left.col_count()).fold(F::zero(), |sum, inner| {
                                F::add(sum, F::mul(left.get(row, inner), right.get(inner, column)))
                            })
                        })
                        .collect()
                })
                .collect()
        }
        // Cover empty dimensions, rectangular rows and both sides of byte SIMD
        // boundaries. The oracle deliberately uses only scalar field arithmetic.
        for (rows, inner, columns) in [(0, 3, 5), (3, 0, 5), (3, 7, 0)]
            .into_iter()
            .chain([1, 3, 15, 16, 17, 31, 32, 33, 63, 64, 65].map(|columns| (3, 7, columns)))
        {
            let mut left = Matrix::<F>::new(rows, inner);
            let mut right = Matrix::<F>::new(inner, columns);
            for row in 0..rows {
                for column in 0..inner {
                    left.set(row, column, F::nth((row * 29 + column * 13) % F::ORDER));
                }
            }
            for row in 0..inner {
                for column in 0..columns {
                    right.set(row, column, F::nth((row * 17 + column * 7 + 1) % F::ORDER));
                }
            }
            let expected = scalar_product(&left, &right);
            let actual = left.multiply(&right).unwrap();
            assert_eq!(actual.row_count(), rows);
            assert_eq!(actual.col_count(), columns);
            for (row, expected) in expected.iter().enumerate() {
                assert_eq!(actual.get_row(row), expected.as_slice());
            }
        }
        for size in [1, 2, 3, 4, 7, 8, 10, 15, 16, 17, 31, 32, 33] {
            for reverse_identity in [false, true] {
                let mut matrix = if reverse_identity {
                    Matrix::<F>::identity(size)
                } else {
                    Matrix::<F>::vandermonde(size, size)
                };
                if reverse_identity {
                    for row in 0..size / 2 {
                        matrix.swap_rows(row, size - 1 - row);
                    }
                }
                let inverse = matrix.invert().unwrap();
                // Multiple right-hand sides exercise the triangular zero-prefix
                // invariant after swaps, normalization and both elimination passes.
                for columns in [1, 3, 17] {
                    let mut rhs = Matrix::<F>::new(size, columns);
                    for row in 0..size {
                        for column in 0..columns {
                            rhs.set(row, column, F::nth((row * 17 + column * 7 + 1) % F::ORDER));
                        }
                    }
                    let expected = scalar_product(&inverse, &rhs);
                    let mut augmented = matrix.augment(&rhs).unwrap();
                    augmented.gaussian_elim().unwrap();
                    for row in 0..size {
                        for column in 0..size {
                            assert_eq!(
                                augmented.get(row, column),
                                if row == column { F::one() } else { F::zero() }
                            );
                        }
                    }
                    for (row, expected) in expected.iter().enumerate() {
                        assert_eq!(&augmented.get_row(row)[size..], expected.as_slice());
                    }
                }
                for actual in [
                    scalar_product(&matrix, &inverse),
                    scalar_product(&inverse, &matrix),
                ] {
                    for (row, values) in actual.iter().enumerate() {
                        for (column, &value) in values.iter().enumerate() {
                            assert_eq!(value, if row == column { F::one() } else { F::zero() });
                        }
                    }
                }
            }
        }
        let mut singular = Matrix::<F>::identity(3);
        for column in 0..3 {
            singular.set(2, column, singular.get(1, column));
        }
        assert_eq!(singular.invert().err(), Some(Error::SingularMatrix));
        assert_eq!(
            Matrix::<F>::new(2, 3).invert().err(),
            Some(Error::NonSquare)
        );
    }

    #[test]
    fn direct_inverse_matches_augmented_reference_across_storage_boundaries() {
        fn check<F: crate::Field>() {
            for size in [0, 1, 2, 7, 31, 32, 33, 40, 64] {
                for reversed in [false, true] {
                    let mut input = Matrix::<F>::identity(size);
                    for row in 0..size {
                        for column in 0..row {
                            input.set(
                                row,
                                column,
                                F::nth((row * 263 + column * 17 + 1) % F::ORDER),
                            );
                        }
                    }
                    if reversed {
                        for row in 0..size / 2 {
                            input.swap_rows(row, size - 1 - row);
                        }
                    }
                    let original: Vec<_> =
                        (0..size).map(|row| input.get_row(row).to_vec()).collect();
                    // Retain the old construction as a reference; its Gaussian
                    // operations and row widths must produce the same inverse.
                    let mut reference = input.augment(&Matrix::<F>::identity(size)).unwrap();
                    reference.gaussian_elim().unwrap();
                    let actual = input.invert().unwrap();
                    for (row, before) in original.iter().enumerate() {
                        assert_eq!(input.get_row(row), before.as_slice());
                        assert_eq!(actual.get_row(row), &reference.get_row(row)[size..]);
                    }
                    assert_eq!(actual.row_count(), size);
                    assert_eq!(actual.col_count(), size);
                }
            }
            for size in [3, 33, 40] {
                let mut input = Matrix::<F>::identity(size);
                input.set(size - 1, size - 1, F::zero());
                assert_eq!(input.invert().err(), Some(Error::SingularMatrix));
            }
            for (rows, columns) in [(0, 1), (1, 0), (2, 3)] {
                assert_eq!(
                    Matrix::<F>::new(rows, columns).invert().err(),
                    Some(Error::NonSquare)
                );
            }
        }
        check::<crate::galois_8::Field>();
        check::<crate::galois_16::Field>();
    }

    #[test]
    fn gf8_matrix_rows_match_independent_scalar_algebra() {
        assert_row_algebra::<crate::galois_8::Field>();
    }

    #[test]
    fn gf16_matrix_rows_match_independent_scalar_algebra() {
        assert_row_algebra::<crate::galois_16::Field>();
    }

    #[test]
    fn test_matrix_inverse_singular() {
        assert_eq!(
            matrix!([4, 2], [12, 6]).invert(),
            Err(Error::SingularMatrix)
        );
    }
}
