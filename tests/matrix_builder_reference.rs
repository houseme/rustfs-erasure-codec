#![cfg(feature = "experimental-matrix-builder")]
use rustfs_erasure_codec::{Field, ReedSolomon, galois_8, galois_16};

struct ThreeByteField;
impl ThreeByteField {
    fn wrap(v: u8) -> [u8; 3] {
        [v, v.rotate_left(1), v.rotate_left(3)]
    }
}
impl Field for ThreeByteField {
    const ORDER: usize = 256;
    type Elem = [u8; 3];
    fn add(a: Self::Elem, b: Self::Elem) -> Self::Elem {
        Self::wrap(galois_8::add(a[0], b[0]))
    }
    fn mul(a: Self::Elem, b: Self::Elem) -> Self::Elem {
        Self::wrap(galois_8::mul(a[0], b[0]))
    }
    fn div(a: Self::Elem, b: Self::Elem) -> Self::Elem {
        Self::wrap(galois_8::div(a[0], b[0]))
    }
    fn exp(a: Self::Elem, n: usize) -> Self::Elem {
        Self::wrap(galois_8::exp(a[0], n))
    }
    fn zero() -> Self::Elem {
        Self::wrap(0)
    }
    fn one() -> Self::Elem {
        Self::wrap(1)
    }
    fn nth_internal(n: usize) -> Self::Elem {
        Self::wrap(n as u8)
    }
}

struct OptInThreeByteField;
impl OptInThreeByteField {
    fn wrap(v: u8) -> [u8; 3] {
        [v, v.rotate_left(1), v.rotate_left(3)]
    }
}
impl Field for OptInThreeByteField {
    const ORDER: usize = 256;
    type Elem = [u8; 3];
    fn supports_matrix_workspace_reuse() -> bool {
        true
    }

    fn add(a: Self::Elem, b: Self::Elem) -> Self::Elem {
        Self::wrap(galois_8::add(a[0], b[0]))
    }
    fn mul(a: Self::Elem, b: Self::Elem) -> Self::Elem {
        Self::wrap(galois_8::mul(a[0], b[0]))
    }
    fn div(a: Self::Elem, b: Self::Elem) -> Self::Elem {
        Self::wrap(galois_8::div(a[0], b[0]))
    }
    fn exp(a: Self::Elem, n: usize) -> Self::Elem {
        Self::wrap(galois_8::exp(a[0], n))
    }
    fn zero() -> Self::Elem {
        Self::wrap(0)
    }
    fn one() -> Self::Elem {
        Self::wrap(1)
    }
    fn nth_internal(n: usize) -> Self::Elem {
        Self::wrap(n as u8)
    }
}

// A separate scalar elimination implementation checks all encoding rows,
// including the systematic data rows and each parity row.
fn reference<F: Field>(data: usize, parity: usize) -> Vec<Vec<F::Elem>> {
    let mut work = vec![vec![F::zero(); data * 2]; data];
    for (r, row) in work.iter_mut().enumerate() {
        for (c, value) in row[..data].iter_mut().enumerate() {
            *value = F::exp(F::nth(r), c);
        }
        row[data + r] = F::one();
    }
    for pivot in 0..data {
        let found = (pivot..data)
            .find(|&r| work[r][pivot] != F::zero())
            .unwrap();
        work.swap(pivot, found);
        let scale = F::div(F::one(), work[pivot][pivot]);
        for value in &mut work[pivot] {
            *value = F::mul(*value, scale);
        }
        let pivot_row = work[pivot].clone();
        for (r, row) in work.iter_mut().enumerate() {
            if r == pivot {
                continue;
            }
            let factor = row[pivot];
            for (value, &pivot_value) in row.iter_mut().zip(&pivot_row) {
                *value = F::add(*value, F::mul(factor, pivot_value));
            }
        }
    }
    (0..data + parity)
        .map(|r| {
            (0..data)
                .map(|c| {
                    (0..data).fold(F::zero(), |sum, k| {
                        F::add(sum, F::mul(F::exp(F::nth(r), k), work[k][data + c]))
                    })
                })
                .collect()
        })
        .collect()
}

fn check<F: Field>() {
    for data in [16, 22, 23, 24, 32, 64] {
        for parity in [2, 16] {
            let expected = reference::<F>(data, parity);
            let codec = ReedSolomon::<F>::new(data, parity).unwrap();
            for (row, expected) in expected.iter().enumerate() {
                assert_eq!(
                    codec.encoding_format().matrix_row(row).unwrap(),
                    expected,
                    "data={data} parity={parity} row={row}"
                );
            }
        }
    }
}

#[test]
fn vandermonde_rows_match_independent_scalar_reference_across_the_inline_boundary() {
    assert!(galois_8::Field::supports_matrix_workspace_reuse());
    assert!(galois_16::Field::supports_matrix_workspace_reuse());
    assert!(!ThreeByteField::supports_matrix_workspace_reuse());
    assert!(OptInThreeByteField::supports_matrix_workspace_reuse());
    check::<galois_8::Field>();
    check::<galois_16::Field>();
    check::<ThreeByteField>();
    check::<OptInThreeByteField>();
}

// Deliberately violates nth() uniqueness to exercise the singular-matrix
// error path rather than relying only on valid-field round trips.
struct SingularField;
impl Field for SingularField {
    const ORDER: usize = 256;
    type Elem = u8;
    fn supports_matrix_workspace_reuse() -> bool {
        true
    }
    fn add(a: u8, b: u8) -> u8 {
        galois_8::add(a, b)
    }
    fn mul(a: u8, b: u8) -> u8 {
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
        if n == 1 { 0 } else { n as u8 }
    }
}

#[test]
fn singular_vandermonde_returns_the_same_error_at_the_workspace_boundary() {
    for data in [22, 23, 24] {
        assert_eq!(
            ReedSolomon::<SingularField>::new(data, 2).err(),
            Some(rustfs_erasure_codec::Error::InvalidCustomMatrix),
            "data={data}"
        );
    }
}
