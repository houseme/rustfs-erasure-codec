pub(super) fn release_profile() {
    assert_backend_override_honored_if_strict();
    for &case in small_file_cases() {
        for operation in [
            SmallFileOp::Standard(Operation::Encode),
            SmallFileOp::Standard(Operation::Verify),
            SmallFileOp::VerifyWithBuffer,
            SmallFileOp::Standard(Operation::Reconstruct),
            SmallFileOp::ReconstructOpt,
            SmallFileOp::ReconstructShardSlot,
            SmallFileOp::ReconstructSomeDataOnly,
            SmallFileOp::Standard(Operation::ReconstructData),
        ] {
            let key = format!(
                "{}:{}:{}:{}",
                operation.as_str(),
                case.data_shards,
                case.parity_shards,
                case.shard_size
            );
            if matches!(operation, SmallFileOp::VerifyWithBuffer) {
                super::sampling::run_prepared(
                    &key,
                    "small-file-results.json",
                    || prepare_buffered_verify(case),
                    |r| r.ns_per_iter,
                    |r| write_results(std::slice::from_ref(r)),
                );
                continue;
            }
            super::sampling::run(
                &key,
                "small-file-results.json",
                |n| run_operation(case, operation, n),
                |r| r.ns_per_iter,
                |r| write_results(std::slice::from_ref(r)),
            );
        }
    }
}

// Retain the original Vec entry and codec for the complete fixed schedule.
// The remaining operations keep their original allocation/timing contracts.
fn prepare_buffered_verify(
    case: BenchCase,
) -> impl FnMut(usize) -> super::sampling::Output<SmallFileResult> {
    let seed = derived_seed(Operation::Verify, case);
    let codec = ReedSolomon::new(case.data_shards, case.parity_shards).unwrap();
    let mut shards = make_full_shards(seed, case.data_shards, case.parity_shards, case.shard_size);
    codec.encode(&mut shards).unwrap();
    let mut scratch = vec![vec![0u8; case.shard_size]; case.parity_shards];
    let fixture = super::layout::Fixture::vec_buffers(&shards, &scratch);
    move |iterations| {
        assert_eq!(
            super::layout::Fixture::vec_buffers(&shards, &scratch),
            fixture
        );
        let start = Instant::now();
        for _ in 0..iterations {
            assert!(std::hint::black_box(
                codec
                    .verify_with_buffer(
                        std::hint::black_box(&shards),
                        std::hint::black_box(&mut scratch),
                    )
                    .unwrap()
            ));
        }
        let ns_per_iter = start.elapsed().as_nanos() as f64 / iterations as f64;
        assert_eq!(
            super::layout::Fixture::vec_buffers(&shards, &scratch),
            fixture
        );
        let logical_data_bytes = case.shard_size * case.data_shards;
        super::sampling::Output {
            result: SmallFileResult {
                operation: "verify_with_buffer",
                data_shards: case.data_shards,
                parity_shards: case.parity_shards,
                shard_size: case.shard_size,
                logical_data_bytes,
                case_label: case.label,
                seed,
                throughput_mb_s: logical_data_bytes as f64 / 1048576.0 / (ns_per_iter / 1e9),
                ns_per_iter,
            },
            fixture: super::layout::OptionalFixture(Some(fixture)),
        }
    }
}

#[test]
fn buffered_verify_preserves_vec_fixture_for_the_entire_worker() {
    let case = BenchCase {
        data_shards: 4,
        parity_shards: 2,
        shard_size: 1061,
        label: "test",
    };
    let mut operation = prepare_buffered_verify(case);
    let initial = operation(1).fixture.0.unwrap();
    for _ in 0..12 {
        let output = operation(2);
        assert_eq!(output.fixture.0.unwrap(), initial);
        assert_eq!(output.result.operation, "verify_with_buffer");
        assert_eq!(output.result.seed, derived_seed(Operation::Verify, case));
    }
}
