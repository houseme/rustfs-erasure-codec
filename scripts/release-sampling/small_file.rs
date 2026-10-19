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
            super::sampling::run_prepared(
                &key,
                "small-file-results.json",
                |page_size| {
                    let mut prepared = PreparedSmallFile::new(case, operation, page_size);
                    move |iterations| prepared.run(iterations)
                },
                |r| r.ns_per_iter,
                |r| write_results(std::slice::from_ref(r)),
            );
        }
    }
}

struct PreparedSmallFile {
    case: BenchCase,
    operation: SmallFileOp,
    seed: u64,
    source: Vec<Vec<u8>>,
    scratch: Vec<Vec<u8>>,
    required: Vec<bool>,
    codec: ReedSolomon,
    fixture: super::layout::Fixture,
}
impl PreparedSmallFile {
    fn new(case: BenchCase, operation: SmallFileOp, page_size: usize) -> Self {
        let seed = derived_seed(
            match operation {
                SmallFileOp::Standard(op) => op,
                SmallFileOp::VerifyWithBuffer => Operation::Verify,
                SmallFileOp::ReconstructOpt | SmallFileOp::ReconstructShardSlot => {
                    Operation::Reconstruct
                }
                SmallFileOp::ReconstructSomeDataOnly => Operation::ReconstructData,
            },
            case,
        );
        // All caller byte storage precedes version-dependent codec construction.
        let mut source =
            make_full_shards(seed, case.data_shards, case.parity_shards, case.shard_size);
        let scratch = if matches!(operation, SmallFileOp::VerifyWithBuffer) {
            vec![vec![0u8; case.shard_size]; case.parity_shards]
        } else {
            Vec::new()
        };
        let mut required = if matches!(operation, SmallFileOp::ReconstructSomeDataOnly) {
            vec![false; case.data_shards + case.parity_shards]
        } else {
            Vec::new()
        };
        if !required.is_empty() {
            required[0] = true;
        }
        let fixture = super::layout::Fixture::vec_buffers(&source, &scratch, page_size)
            .with_contract(release_contract(case, operation));
        let codec = ReedSolomon::new(case.data_shards, case.parity_shards).unwrap();
        if !matches!(operation, SmallFileOp::Standard(Operation::Encode)) {
            codec.encode(&mut source).unwrap();
        }
        let prepared = Self {
            case,
            operation,
            seed,
            source,
            scratch,
            required,
            codec,
            fixture,
        };
        prepared.check_fixture(page_size);
        prepared
    }
    fn check_fixture(&self, page_size: usize) {
        assert_eq!(
            super::layout::Fixture::vec_buffers(&self.source, &self.scratch, page_size)
                .with_contract(release_contract(self.case, self.operation)),
            self.fixture
        );
    }
    fn run(&mut self, iterations: usize) -> super::sampling::Output<SmallFileResult> {
        assert!(iterations > 0);
        // The private full-pointer identity is checked outside the inner timer.
        let page_size = self.fixture.page_size();
        self.check_fixture(page_size);
        let elapsed = execute_release_kernel(self, iterations);
        self.check_fixture(page_size);
        let ns_per_iter = elapsed.as_nanos() as f64 / iterations as f64;
        let logical_data_bytes = self.case.shard_size * self.case.data_shards;
        super::sampling::Output {
            result: SmallFileResult {
                operation: self.operation.as_str(),
                data_shards: self.case.data_shards,
                parity_shards: self.case.parity_shards,
                shard_size: self.case.shard_size,
                logical_data_bytes,
                case_label: self.case.label,
                seed: self.seed,
                throughput_mb_s: logical_data_bytes as f64 / 1048576.0 / (ns_per_iter / 1e9),
                ns_per_iter,
            },
            fixture: super::layout::OptionalFixture(Some(self.fixture)),
            workload: super::workload::OptionalWorkload(Some(super::workload::Workload::retained(
                self.seed,
                release_contract(self.case, self.operation),
            ))),
        }
    }
}

#[test]
fn every_small_file_entry_retains_its_caller_storage_and_correct_codeword() {
    for (data_shards, parity_shards) in [(4, 2), (10, 4)] {
        let case = BenchCase {
            data_shards,
            parity_shards,
            shard_size: 1061,
            label: "test",
        };
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
            for page_size in [4096, 16384, 65536] {
                let mut prepared = PreparedSmallFile::new(case, operation, page_size);
                let fixture = prepared.fixture;
                let mut expected = make_full_shards(
                    prepared.seed,
                    case.data_shards,
                    case.parity_shards,
                    case.shard_size,
                );
                let reference = ReedSolomon::new(case.data_shards, case.parity_shards).unwrap();
                reference.encode(&mut expected).unwrap();
                for _ in 0..12 {
                    let output = prepared.run(2);
                    assert_eq!(output.fixture.0.unwrap(), fixture);
                    assert_eq!(output.result.operation, operation.as_str());
                    assert_eq!(prepared.source, expected);
                    assert!(prepared.codec.verify(&prepared.source).unwrap());
                }
            }
        }
    }
}
