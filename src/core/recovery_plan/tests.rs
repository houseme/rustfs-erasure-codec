use super::*;
use crate::{CodecOptions, MatrixMode, galois_8, galois_16};
use alloc::{vec, vec::Vec};

fn shape(data: usize, total: usize, present: &[usize]) -> ClassicReconstructShape {
    ClassicReconstructShape {
        shard_len: 1,
        number_present: present.len(),
        valid_indices: present.iter().copied().take(data).collect(),
        invalid_indices: (0..total)
            .filter(|index| !present.contains(index))
            .collect(),
    }
}

fn compare_composed_rows<F: Field>(codec: &ReedSolomon<F>, present: &[usize]) {
    let data = codec.data_shard_count();
    let total = codec.total_shard_count();
    let layout = shape(data, total, present);
    let mut selected = Matrix::<F>::new(data, data);
    for (row, &index) in layout.valid_indices.iter().enumerate() {
        for column in 0..data {
            selected.set(
                row,
                column,
                codec.encoding_format().matrix().get(index, column),
            );
        }
    }
    let inverse = selected.invert();
    for parity in data..total {
        if present.contains(&parity) {
            continue;
        }
        for request_data in [false, true] {
            let mut required = vec![false; total];
            required[parity] = true;
            if request_data {
                for &index in &layout.invalid_indices {
                    if index < data && index % 2 == 0 {
                        required[index] = true;
                    }
                }
            }
            let planned = codec.prepare_classic_reconstruct_plan(
                layout.clone(),
                ReconstructionGoal::Required(&required),
            );
            let inverse = match (&inverse, planned) {
                (Err(_), Err(error)) => {
                    assert_eq!(error, Error::InvalidCustomMatrix);
                    continue;
                }
                (Ok(inverse), Ok(plan)) => {
                    let encoding = codec.encoding_format().matrix().get_row(parity);
                    // Independent dense definition, including every identity row.
                    let expected: Vec<_> = (0..data)
                        .map(|column| {
                            encoding.iter().copied().enumerate().fold(
                                F::zero(),
                                |sum, (row, coefficient)| {
                                    F::add(sum, F::mul(coefficient, inverse.get(row, column)))
                                },
                            )
                        })
                        .collect();
                    let rows: Vec<_> = plan.parity_rows(codec).collect();
                    assert_eq!(rows, [expected.as_slice()], "present={present:?}");
                    for (&index, row) in plan.data_targets().iter().zip(plan.data_rows()) {
                        assert_eq!(row, inverse.get_row(index));
                    }
                    inverse
                }
                _ => panic!("different singularity result for survivors {present:?}"),
            };
            assert_eq!(inverse.row_count(), data);
        }
    }
}

fn all_small<F: Field>() {
    for mode in [
        MatrixMode::Vandermonde,
        MatrixMode::Cauchy,
        MatrixMode::JerasureLike,
    ] {
        let codec = ReedSolomon::<F>::with_options(
            4,
            4,
            CodecOptions::builder()
                .matrix_mode(mode)
                .inversion_cache(false)
                .build(),
        )
        .unwrap();
        for mask in 0u16..256 {
            if mask.count_ones() >= 4 {
                let present: Vec<_> = (0..8).filter(|&i| mask & (1 << i) != 0).collect();
                compare_composed_rows(&codec, &present);
            }
        }
    }
}

#[test]
fn selective_parity_rows_match_dense_inverse_for_all_small_gf8_survivors() {
    all_small::<galois_8::Field>();
}

#[test]
fn selective_parity_rows_match_dense_inverse_for_all_small_gf16_survivors() {
    all_small::<galois_16::Field>();
}

#[test]
fn selective_parity_rows_keep_custom_singularity_and_general_tower_terms() {
    let custom = galois_8::ReedSolomon::with_custom_matrix(
        4,
        4,
        &[
            vec![0, 1, 3, 5],
            vec![1, 0, 7, 9],
            vec![1, 0, 7, 9],
            vec![0, 0, 0, 0],
        ],
        CodecOptions::builder().inversion_cache(false).build(),
    )
    .unwrap();
    for mask in 0u16..256 {
        if mask.count_ones() >= 4 {
            let present: Vec<_> = (0..8).filter(|&i| mask & (1 << i) != 0).collect();
            compare_composed_rows(&custom, &present);
        }
    }
    for mode in [MatrixMode::Vandermonde, MatrixMode::Cauchy] {
        let wide = galois_16::ReedSolomon::with_options(
            2,
            255,
            CodecOptions::builder()
                .matrix_mode(mode)
                .inversion_cache(false)
                .build(),
        )
        .unwrap();
        for missing in [vec![0, 256], vec![1, 255], vec![0, 1, 256]] {
            let present: Vec<_> = (0..257).filter(|i| !missing.contains(i)).collect();
            compare_composed_rows(&wide, &present);
        }
    }
}

#[test]
fn selective_parity_rows_match_dense_inverse_for_large_shifted_erasures() {
    fn check<F: Field>() {
        for mode in [MatrixMode::Vandermonde, MatrixMode::Cauchy] {
            let codec = ReedSolomon::<F>::with_options(
                32,
                16,
                CodecOptions::builder()
                    .matrix_mode(mode)
                    .inversion_cache(false)
                    .build(),
            )
            .unwrap();
            for missing in [1, 2, 15] {
                for start in [0, 7, 16] {
                    let present: Vec<_> = (0..47)
                        .filter(|index| !(start..start + missing).contains(index))
                        .collect();
                    compare_composed_rows(&codec, &present);
                }
            }
        }
    }
    check::<galois_8::Field>();
    check::<galois_16::Field>();
}

#[cfg(feature = "std")]
#[test]
fn selective_parity_composition_multiplies_only_missing_data_rows() {
    use std::cell::Cell;
    std::thread_local! {
        static PRODUCTS: Cell<usize> = const { Cell::new(0) };
    }
    struct ObservedField;
    impl Field for ObservedField {
        const ORDER: usize = 256;
        type Elem = u8;
        fn add(a: u8, b: u8) -> u8 {
            galois_8::add(a, b)
        }
        fn mul(a: u8, b: u8) -> u8 {
            PRODUCTS.with(|count| count.set(count.get() + 1));
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
    }
    for (data, parity, losses) in [(8, 9, 1), (8, 9, 2), (8, 9, 4), (8, 9, 8), (32, 17, 1)] {
        let codec = ReedSolomon::<ObservedField>::new(data, parity).unwrap();
        let target = data + parity - 1;
        let present: Vec<_> = (losses..target).collect();
        let layout = shape(data, data + parity, &present);
        // Warm the same inverse entry before isolating coefficient planning.
        codec
            .prepare_classic_reconstruct_plan(layout.clone(), ReconstructionGoal::Data)
            .unwrap();
        let mut required = vec![false; data + parity];
        required[target] = true;
        PRODUCTS.with(|count| count.set(0));
        let plan = codec
            .prepare_classic_reconstruct_plan(layout, ReconstructionGoal::Required(&required))
            .unwrap();
        let products = PRODUCTS.with(Cell::get);
        assert!(plan.data_targets().is_empty());
        assert_eq!(plan.parity_targets(), &[target]);
        assert_eq!(products, data * losses, "k={data}, missing={losses}");
    }
}

#[test]
fn ordinary_targets_borrow_missing_indices_and_selected_targets_stay_exact() {
    fn check<F: Field>() {
        let codec = ReedSolomon::<F>::new(4, 4).unwrap();
        for goal in [ReconstructionGoal::All, ReconstructionGoal::Data] {
            let plan = codec
                .prepare_classic_reconstruct_plan(shape(4, 8, &[1, 3, 4, 6]), goal)
                .unwrap();
            assert!(plan.selected_targets.is_none());
            assert_eq!(plan.data_targets(), &[0, 2]);
            assert!(::core::ptr::eq(
                plan.data_targets(),
                &plan.shape.invalid_indices[..2]
            ));
            if matches!(goal, ReconstructionGoal::All) {
                assert_eq!(plan.parity_targets(), &[5, 7]);
                assert!(::core::ptr::eq(
                    plan.parity_targets(),
                    &plan.shape.invalid_indices[2..]
                ));
            } else {
                assert!(plan.parity_targets().is_empty());
            }
        }
        for selected in [vec![], vec![2], vec![7], vec![2, 7], vec![0, 2, 5, 7]] {
            let mut required = [false; 8];
            for &index in &selected {
                required[index] = true;
            }
            let plan = codec
                .prepare_classic_reconstruct_plan(
                    shape(4, 8, &[1, 3, 4, 6]),
                    ReconstructionGoal::Required(&required),
                )
                .unwrap();
            // A workspace must not retain the caller's mutable request flags.
            required.fill(false);
            assert_eq!(
                plan.data_targets()
                    .iter()
                    .chain(plan.parity_targets())
                    .copied()
                    .collect::<Vec<_>>(),
                selected
            );
            assert_eq!(
                plan.data_targets(),
                selected
                    .iter()
                    .copied()
                    .filter(|&i| i < 4)
                    .collect::<Vec<_>>()
            );
            assert_eq!(
                plan.parity_targets(),
                selected
                    .iter()
                    .copied()
                    .filter(|&i| i >= 4)
                    .collect::<Vec<_>>()
            );
        }
    }
    check::<galois_8::Field>();
    check::<galois_16::Field>();
}

#[test]
fn target_storage_preserves_inline_capacity_without_growing_index_metadata() {
    use ::core::mem::size_of;
    assert!(
        size_of::<Option<SmallVec<[usize; 64]>>>() + size_of::<usize>()
            <= 2 * size_of::<SmallVec<[usize; 32]>>()
    );
    let selected: SmallVec<[usize; 64]> = (0..64).collect();
    assert!(!selected.spilled());
}

#[test]
fn complete_required_targets_borrow_shape_for_full_and_short_owned_requests() {
    fn exercise<F: Field>() {
        let codec = ReedSolomon::<F>::with_options(
            4,
            100,
            CodecOptions::builder()
                .matrix_mode(MatrixMode::Cauchy)
                .build(),
        )
        .unwrap();
        for missing in [
            vec![],
            (0..4).collect(),
            (4..101).collect(),
            ::core::iter::once(0).chain(4..100).collect(),
        ] {
            let present: Vec<_> = (0..104).filter(|i| !missing.contains(i)).collect();
            for mask_len in [4, 104] {
                for excluded in [None, Some(0), Some(1), Some(32), Some(64), Some(96)] {
                    if excluded.is_some_and(|position| position >= missing.len()) {
                        continue;
                    }
                    let mut required: Vec<_> =
                        (0..mask_len).map(|i| missing.contains(&i)).collect();
                    if let Some(position) = excluded {
                        let index = missing[position];
                        if index < mask_len {
                            required[index] = false;
                        }
                    }
                    let expected: Vec<_> = missing
                        .iter()
                        .copied()
                        .filter(|&i| required.get(i).copied().unwrap_or(false))
                        .collect();
                    let plan = codec
                        .prepare_classic_reconstruct_plan(
                            shape(4, 104, &present),
                            ReconstructionGoal::Required(&required),
                        )
                        .unwrap();
                    let complete = expected.len() == missing.len();
                    assert_eq!(plan.selected_targets.is_none(), complete);
                    assert_eq!(plan.target_indices(), expected);
                    if complete {
                        assert!(::core::ptr::eq(
                            plan.target_indices(),
                            plan.shape.invalid_indices.as_slice()
                        ));
                    }
                    // The indices belong to the plan, not to these flags.
                    required.fill(false);
                    drop(required);
                    assert_eq!(plan.target_indices(), expected);
                    assert_eq!(
                        plan.data_targets().len(),
                        expected.iter().filter(|i| **i < 4).count()
                    );
                    assert_eq!(
                        plan.parity_targets().len(),
                        expected.iter().filter(|i| **i >= 4).count()
                    );
                }
            }
        }
    }
    exercise::<galois_8::Field>();
    exercise::<galois_16::Field>();
}
