#![cfg(feature = "std")]

use rustfs_erasure_codec::{
    CodecFamily, CodecOptions, Error, Field, MatrixMode, ReconstructInitResult, ReconstructShard,
    ReedSolomon, ShardSlot, galois_8, galois_16,
};

fn encoded<F: Field>(codec: &ReedSolomon<F>, len: usize) -> Vec<Vec<F::Elem>> {
    let mut shards: Vec<Vec<_>> = (0..codec.total_shard_count())
        .map(|row| {
            (0..len)
                .map(|offset| F::nth((row * 37 + offset * 19 + 11) % F::ORDER))
                .collect()
        })
        .collect();
    codec.encode(&mut shards).unwrap();
    shards
}

fn slots<F: Field>(expected: &[Vec<F::Elem>], missing: &[usize]) -> Vec<ShardSlot<Vec<F::Elem>>> {
    expected
        .iter()
        .enumerate()
        .map(|(index, bytes)| {
            if missing.contains(&index) {
                ShardSlot::new_missing(vec![F::one(); bytes.len()])
            } else {
                ShardSlot::new_present(bytes.clone())
            }
        })
        .collect()
}

fn options(expected: &[Vec<u8>], missing: &[usize]) -> Vec<Option<Vec<u8>>> {
    expected
        .iter()
        .enumerate()
        .map(|(index, bytes)| (!missing.contains(&index)).then(|| bytes.clone()))
        .collect()
}

#[test]
fn generic_parallel_plans_match_serial_for_fields_matrices_and_targets() {
    fn exercise<F: Field>()
    where
        F::Elem: Send + Sync,
    {
        for (mode, len) in [
            (MatrixMode::Vandermonde, 65),
            (MatrixMode::Cauchy, 65),
            (MatrixMode::JerasureLike, 65),
            (MatrixMode::Custom, 65),
            (MatrixMode::Cauchy, 512 * 1024 + 65),
        ] {
            for jobs in [1, 2] {
                let settings = CodecOptions::builder()
                    .matrix_mode(mode)
                    .max_parallel_jobs(jobs)
                    .inversion_cache(false)
                    .build();
                let codec = if mode == MatrixMode::Custom {
                    let reference = ReedSolomon::<F>::new(4, 3).unwrap();
                    let rows: Vec<_> = (4..7)
                        .map(|index| {
                            reference
                                .encoding_format()
                                .matrix_row(index)
                                .unwrap()
                                .to_vec()
                        })
                        .collect();
                    ReedSolomon::<F>::with_custom_matrix(4, 3, &rows, settings).unwrap()
                } else {
                    ReedSolomon::<F>::with_options(4, 3, settings).unwrap()
                };
                let expected = encoded(&codec, len);
                let missing = [0, 1, 4];
                for goal in ["all", "data", "selected", "parity", "none"] {
                    let initial = slots::<F>(&expected, &missing);
                    let required = match goal {
                        "selected" => [true, false, false, false, true, false, false],
                        "parity" => [false, false, false, false, true, false, false],
                        _ => [false; 7],
                    };
                    let plan = match goal {
                        "all" => codec.prepare_reconstruct_workspace(&initial),
                        "data" => codec.prepare_reconstruct_data_workspace(&initial),
                        _ => codec.prepare_reconstruct_some_workspace(&initial, &required),
                    }
                    .unwrap();
                    let mut serial = initial.clone();
                    codec
                        .reconstruct_with_workspace(&mut serial, &plan)
                        .unwrap();
                    let mut parallel = initial;
                    let pointers: Vec<_> = parallel
                        .iter()
                        .map(|slot| slot.as_inner().as_ptr())
                        .collect();
                    let cache_before = codec.reconstruction_cache_stats();
                    for _ in 0..2 {
                        for &index in &missing {
                            parallel[index].mark_missing();
                            parallel[index].as_inner_mut().fill(F::one());
                        }
                        codec.reset_runtime_profile_stats();
                        codec
                            .reconstruct_with_workspace_par(&mut parallel, &plan)
                            .unwrap();
                        assert_eq!(parallel, serial, "goal={goal}, len={len}, jobs={jobs}");
                        assert!(
                            parallel
                                .iter()
                                .zip(&pointers)
                                .all(|(slot, ptr)| slot.as_inner().as_ptr() == *ptr)
                        );
                        assert_eq!(codec.reconstruction_cache_stats(), cache_before);
                        if cfg!(feature = "benchmark-metrics") {
                            let stats = codec.runtime_profile_stats();
                            assert!(
                                stats.parallel_policy_total_jobs
                                    <= jobs * stats.parallel_policy_calls
                            );
                            if jobs == 1 || len == 65 {
                                assert_eq!(stats.code_some_parallel_calls, 0);
                            } else if goal != "none"
                                && std::thread::available_parallelism().unwrap().get() >= 2
                            {
                                assert!(stats.code_some_parallel_calls > 0);
                            }
                        }
                    }
                }
            }
        }
    }
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(2)
        .build()
        .unwrap();
    pool.install(|| {
        exercise::<galois_8::Field>();
        exercise::<galois_16::Field>();
    });
}

#[test]
fn option_parallel_workspace_batches_full_data_and_selective_recovery() {
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(2)
        .build()
        .unwrap();
    pool.install(|| {
        for (data, parity, missing, len) in [
            (4, 3, vec![0, 1, 4], 512 * 1024 + 65),
            (33, 5, vec![0, 1, 33, 34], 65),
            (4, 3, vec![4, 5], 65),
            (4, 1, vec![0], 65),
            (4, 3, vec![], 65),
        ] {
            let codec = galois_8::ReedSolomon::with_options(
                data,
                parity,
                CodecOptions::builder()
                    .fast_one_parity(parity == 1)
                    .max_parallel_jobs(2)
                    .build(),
            )
            .unwrap();
            let expected = encoded(&codec, len);
            for goal in ["all", "data", "selected", "none"] {
                let initial = options(&expected, &missing);
                let required: Vec<_> = (0..data + parity)
                    .map(|index| goal == "selected" && (index == 0 || index >= data))
                    .collect();
                let plan = match goal {
                    "all" => codec.prepare_reconstruct_opt_workspace(&initial),
                    "data" => codec.prepare_reconstruct_data_workspace(&initial),
                    _ => codec.prepare_reconstruct_some_workspace(&initial, &required),
                }
                .unwrap();
                let mut serial = initial.clone();
                codec
                    .reconstruct_opt_with_workspace(&mut serial, &plan)
                    .unwrap();
                let mut parallel = initial;
                let cache_before = codec.reconstruction_cache_stats();
                codec
                    .reconstruct_opt_with_workspace_par(&mut parallel, &plan)
                    .unwrap();
                assert_eq!(parallel, serial, "data={data}, goal={goal}");
                assert_eq!(codec.reconstruction_cache_stats(), cache_before);
                for (index, shard) in parallel.iter().enumerate() {
                    if let Some(bytes) = shard {
                        assert_eq!(bytes, &expected[index]);
                    }
                }
            }
        }
    });
}

#[test]
fn parallel_workspace_validation_precedes_all_destination_changes() {
    let codec = galois_8::ReedSolomon::new(4, 3).unwrap();
    let expected = encoded(&codec, 65);
    let initial = options(&expected, &[0]);
    let plan = codec.prepare_reconstruct_opt_workspace(&initial).unwrap();
    for (mut invalid, error) in [
        (options(&expected, &[1]), Error::InvalidShardFlags),
        (options(&expected[..6], &[0]), Error::TooFewShards),
        (
            options(&vec![vec![0; 64]; 7], &[0]),
            Error::IncorrectShardSize,
        ),
        (options(&vec![vec![]; 7], &[0]), Error::EmptyShard),
    ] {
        let before = invalid.clone();
        assert_eq!(
            codec.reconstruct_opt_with_workspace_par(&mut invalid, &plan),
            Err(error)
        );
        assert_eq!(invalid, before);
        assert_eq!(
            codec.reconstruct_with_workspace_par(&mut invalid, &plan),
            Err(error)
        );
        assert_eq!(invalid, before);
    }
    for (family, matrix, error) in [
        (
            CodecFamily::Classic,
            MatrixMode::Cauchy,
            Error::InvalidCustomMatrix,
        ),
        (
            CodecFamily::LeopardGF8,
            MatrixMode::Vandermonde,
            Error::UnsupportedCodecFamily,
        ),
        (
            CodecFamily::LeopardGF16,
            MatrixMode::Vandermonde,
            Error::UnsupportedCodecFamily,
        ),
    ] {
        let different = galois_8::ReedSolomon::with_options(
            4,
            3,
            CodecOptions::builder()
                .codec_family(family)
                .matrix_mode(matrix)
                .build(),
        )
        .unwrap();
        let mut candidate = initial.clone();
        assert_eq!(
            different.reconstruct_opt_with_workspace_par(&mut candidate, &plan),
            Err(error)
        );
        assert_eq!(candidate, initial);
        assert_eq!(
            different.reconstruct_with_workspace_par(&mut candidate, &plan),
            Err(error)
        );
        assert_eq!(candidate, initial);
    }
}

#[test]
fn non_send_containers_keep_serial_initialization_and_failure_order() {
    use std::{cell::RefCell, rc::Rc, thread::ThreadId};

    struct LocalShard {
        slot: ShardSlot<Vec<u8>>,
        index: usize,
        owner: ThreadId,
        initialized: Rc<RefCell<Vec<usize>>>,
    }
    impl ReconstructShard<galois_8::Field> for LocalShard {
        fn len(&self) -> Option<usize> {
            assert_eq!(std::thread::current().id(), self.owner);
            <ShardSlot<Vec<u8>> as ReconstructShard<galois_8::Field>>::len(&self.slot)
        }
        fn get(&mut self) -> Option<&mut [u8]> {
            assert_eq!(std::thread::current().id(), self.owner);
            <ShardSlot<Vec<u8>> as ReconstructShard<galois_8::Field>>::get(&mut self.slot)
        }
        fn get_or_initialize(&mut self, len: usize) -> ReconstructInitResult<'_, galois_8::Field> {
            assert_eq!(std::thread::current().id(), self.owner);
            self.initialized.borrow_mut().push(self.index);
            <ShardSlot<Vec<u8>> as ReconstructShard<galois_8::Field>>::get_or_initialize(
                &mut self.slot,
                len,
            )
        }
    }
    let codec = galois_8::ReedSolomon::with_options(
        4,
        3,
        CodecOptions::builder().max_parallel_jobs(2).build(),
    )
    .unwrap();
    let expected = encoded(&codec, 512 * 1024 + 65);
    let mut initial = slots::<galois_8::Field>(&expected, &[0, 1, 4]);
    initial[1].as_inner_mut().pop();
    let plan = codec.prepare_reconstruct_workspace(&initial).unwrap();
    let mut serial = initial.clone();
    assert_eq!(
        codec.reconstruct_with_workspace(&mut serial, &plan),
        Err(Error::IncorrectShardSize)
    );
    let initialized = Rc::new(RefCell::new(Vec::new()));
    let mut local: Vec<_> = initial
        .into_iter()
        .enumerate()
        .map(|(index, slot)| LocalShard {
            slot,
            index,
            owner: std::thread::current().id(),
            initialized: initialized.clone(),
        })
        .collect();
    assert_eq!(
        codec.reconstruct_with_workspace_par(&mut local, &plan),
        Err(Error::IncorrectShardSize)
    );
    assert_eq!(*initialized.borrow(), [0, 1]);
    for (actual, expected) in local.iter().zip(&serial) {
        assert_eq!(&actual.slot, expected);
    }
    assert_eq!(local[0].slot.as_inner(), &expected[0]);
    assert!(!local[1].slot.is_present());
    assert!(!local[4].slot.is_present());
}

#[test]
fn immutable_workspace_can_be_shared_by_concurrent_callers() {
    let codec = galois_8::ReedSolomon::with_options(
        4,
        3,
        CodecOptions::builder().max_parallel_jobs(2).build(),
    )
    .unwrap();
    let expected = encoded(&codec, 512 * 1024 + 65);
    let initial = options(&expected, &[0, 4]);
    let plan = codec.prepare_reconstruct_opt_workspace(&initial).unwrap();
    let cache_before = codec.reconstruction_cache_stats();
    std::thread::scope(|scope| {
        let mut workers = Vec::new();
        for bulk in [false, true, false, true] {
            let codec = &codec;
            let plan = &plan;
            let initial = &initial;
            let expected = &expected;
            workers.push(scope.spawn(move || {
                let mut shards = initial.clone();
                for _ in 0..2 {
                    shards[0] = None;
                    shards[4] = None;
                    if bulk {
                        codec
                            .reconstruct_opt_with_workspace_par(&mut shards, plan)
                            .unwrap();
                    } else {
                        codec
                            .reconstruct_with_workspace_par(&mut shards, plan)
                            .unwrap();
                    }
                    for (shard, expected) in shards.iter().zip(expected) {
                        assert_eq!(shard.as_ref().unwrap(), expected);
                    }
                }
            }));
        }
        for worker in workers {
            worker.join().unwrap();
        }
    });
    assert_eq!(codec.reconstruction_cache_stats(), cache_before);
}
