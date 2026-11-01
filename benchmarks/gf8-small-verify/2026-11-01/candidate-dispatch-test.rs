// Experimental integration test: install only into a disposable source tree
// with candidate-single-scratch.patch and the test-only counter applied.
use std::sync::atomic::Ordering;

use rustfs_erasure_codec::RSE_EXPERIMENT_VERIFY_HITS;
use rustfs_erasure_codec::{CodecOptions, Error, Field, MatrixMode, ReedSolomon};

struct ThreeByteField;
impl ThreeByteField {
    fn wrap(value: u8) -> [u8; 3] {
        [value, value.rotate_left(1), value.rotate_left(3)]
    }
}
impl Field for ThreeByteField {
    const ORDER: usize = 256;
    type Elem = [u8; 3];

    fn add(a: Self::Elem, b: Self::Elem) -> Self::Elem {
        Self::wrap(rustfs_erasure_codec::galois_8::Field::add(a[0], b[0]))
    }
    fn mul(a: Self::Elem, b: Self::Elem) -> Self::Elem {
        Self::wrap(rustfs_erasure_codec::galois_8::Field::mul(a[0], b[0]))
    }
    fn div(a: Self::Elem, b: Self::Elem) -> Self::Elem {
        Self::wrap(rustfs_erasure_codec::galois_8::Field::div(a[0], b[0]))
    }
    fn exp(value: Self::Elem, n: usize) -> Self::Elem {
        Self::wrap(rustfs_erasure_codec::galois_8::Field::exp(value[0], n))
    }
    fn zero() -> Self::Elem {
        Self::wrap(rustfs_erasure_codec::galois_8::Field::zero())
    }
    fn one() -> Self::Elem {
        Self::wrap(rustfs_erasure_codec::galois_8::Field::one())
    }
    fn nth_internal(n: usize) -> Self::Elem {
        Self::wrap(rustfs_erasure_codec::galois_8::Field::nth_internal(n))
    }
}

fn check<F: Field>() {
    for mode in [
        MatrixMode::Vandermonde,
        MatrixMode::Cauchy,
        MatrixMode::JerasureLike,
        MatrixMode::Custom,
    ] {
        for (data, parity, fast, lengths) in [
            (4, 2, false, &[2047, 2048, 2049, 4096, 4097][..]),
            (10, 4, false, &[1023, 1024, 1025, 4096, 4097][..]),
            (4, 1, false, &[2048, 2049, 4096, 4097][..]),
            (4, 1, true, &[2048, 2049, 4096, 4097][..]),
        ] {
            let options = CodecOptions::builder()
                .matrix_mode(mode)
                .fast_one_parity(fast)
                .build();
            let codec = if mode == MatrixMode::Custom {
                let reference = ReedSolomon::<F>::new(data, parity).unwrap();
                let mut rows: Vec<Vec<F::Elem>> = (data..data + parity)
                    .map(|row| {
                        reference
                            .encoding_format()
                            .matrix_row(row)
                            .unwrap()
                            .to_vec()
                    })
                    .collect();
                rows.reverse();
                if fast {
                    rows[0].fill(F::one());
                }
                ReedSolomon::<F>::with_custom_matrix(data, parity, &rows, options).unwrap()
            } else {
                ReedSolomon::<F>::with_options(data, parity, options).unwrap()
            };
            for &len in lengths {
                let mut shards: Vec<Vec<F::Elem>> = (0..data + parity)
                    .map(|row| {
                        (0..len)
                            .map(|offset| F::nth((row * 73 + offset * 19 + 1) % F::ORDER))
                            .collect()
                    })
                    .collect();
                codec.encode(&mut shards).unwrap();
                let expected_hits = usize::from(len <= 4096 && parity * len > 4096);
                let context =
                    format!("mode={mode:?}, data={data}, parity={parity}, fast={fast}, len={len}");
                RSE_EXPERIMENT_VERIFY_HITS.store(0, Ordering::Relaxed);
                assert!(codec.verify(&shards).unwrap(), "{context}");
                assert_eq!(
                    RSE_EXPERIMENT_VERIFY_HITS.load(Ordering::Relaxed),
                    expected_hits,
                    "{context}"
                );
                let mut scratch = vec![vec![F::zero(); len]; parity];
                assert!(
                    codec.verify_with_buffer(&shards, &mut scratch).unwrap(),
                    "{context}"
                );
                assert_eq!(
                    RSE_EXPERIMENT_VERIFY_HITS.load(Ordering::Relaxed),
                    expected_hits,
                    "buffer path: {context}"
                );
                for row in 0..data + parity {
                    shards[row][len - 1] = F::add(shards[row][len - 1], F::one());
                    RSE_EXPERIMENT_VERIFY_HITS.store(0, Ordering::Relaxed);
                    assert!(
                        !codec.verify(&shards).unwrap(),
                        "corrupt row={row}: {context}"
                    );
                    assert_eq!(
                        RSE_EXPERIMENT_VERIFY_HITS.load(Ordering::Relaxed),
                        expected_hits,
                        "corrupt row={row}: {context}"
                    );
                    shards[row][len - 1] = F::add(shards[row][len - 1], F::one());
                }
                if parity > 1 {
                    shards[data][0] = F::add(shards[data][0], F::one());
                    shards[data + parity - 1].pop();
                    let before = shards.clone();
                    RSE_EXPERIMENT_VERIFY_HITS.store(0, Ordering::Relaxed);
                    assert_eq!(
                        codec.verify(&shards),
                        Err(Error::IncorrectShardSize),
                        "{context}"
                    );
                    assert_eq!(
                        RSE_EXPERIMENT_VERIFY_HITS.load(Ordering::Relaxed),
                        0,
                        "invalid input: {context}"
                    );
                    assert_eq!(shards, before, "{context}");
                }
            }
        }
    }
}

// One test prevents other test threads from altering the process-wide counter.
#[test]
fn candidate_dispatch_and_error_order_at_both_scratch_boundaries() {
    check::<rustfs_erasure_codec::galois_8::Field>();
    check::<rustfs_erasure_codec::galois_16::Field>();
    check::<ThreeByteField>();
}
