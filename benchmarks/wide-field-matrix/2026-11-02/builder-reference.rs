#![cfg(feature = "experimental-matrix-inline512")]
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
        for c in 0..data * 2 {
            work[pivot][c] = F::mul(work[pivot][c], scale);
        }
        let pivot_row = work[pivot].clone();
        for (r, row) in work.iter_mut().enumerate() {
            if r == pivot {
                continue;
            }
            let factor = row[pivot];
            for c in 0..data * 2 {
                row[c] = F::add(row[c], F::mul(factor, pivot_row[c]));
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
    for data in [16, 22, 23, 24, 32] {
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
    check::<galois_8::Field>();
    check::<galois_16::Field>();
    check::<ThreeByteField>();
}
