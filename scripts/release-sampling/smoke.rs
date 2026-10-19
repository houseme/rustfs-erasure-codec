pub(super) fn release_profile() {
    assert_backend_override_honored_if_strict();
    for &case in smoke_cases() {
        for operation in [
            Operation::Encode,
            Operation::Update,
            Operation::Verify,
            Operation::Reconstruct,
            Operation::ReconstructData,
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
                "smoke-results.json",
                |_| {
                    let mut prepared = PreparedSmoke::new(case, operation);
                    move |iterations| prepared.run(iterations)
                },
                |r| r.ns_per_iter,
                |r| write_results(std::slice::from_ref(r)),
            );
        }
    }
}

struct PreparedSmoke {
    case: BenchCase,
    operation: Operation,
    seed: u64,
    codec: ReedSolomon,
}
impl PreparedSmoke {
    fn new(case: BenchCase, operation: Operation) -> Self {
        Self {
            case,
            operation,
            seed: derived_seed(operation, case),
            codec: ReedSolomon::new(case.data_shards, case.parity_shards).unwrap(),
        }
    }
    fn run(&mut self, iterations: usize) -> super::sampling::Output<SmokeResult> {
        let result = execute_smoke_kernel(self.case, self.operation, iterations, &self.codec);
        let workload = super::workload::Workload::generated(
            self.seed,
            smoke_contract(self.case, self.operation),
            u64::from(matches!(self.operation, Operation::Update)),
        );
        super::sampling::Output {
            result,
            fixture: super::layout::OptionalFixture(None),
            workload: super::workload::OptionalWorkload(Some(workload)),
        }
    }
}

#[test]
fn smoke_worker_keeps_codec_and_original_iteration_work() {
    let case = BenchCase {
        data_shards: 4,
        parity_shards: 2,
        shard_size: 1061,
        label: "test",
    };
    for operation in [
        Operation::Encode,
        Operation::Update,
        Operation::Verify,
        Operation::Reconstruct,
        Operation::ReconstructData,
    ] {
        let mut prepared = PreparedSmoke::new(case, operation);
        let identity = &prepared.codec as *const ReedSolomon;
        for _ in 0..12 {
            let result = prepared.run(2);
            assert_eq!(&prepared.codec as *const ReedSolomon, identity);
            assert_eq!(result.result.seed, prepared.seed);
            assert!(result.result.ns_per_iter > 0.);
            assert!(result.workload.0.is_some());
        }
    }
}
