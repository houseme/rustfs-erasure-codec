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
            super::sampling::run(
                &key,
                "smoke-results.json",
                |n| run_operation(case, operation, n),
                |r| r.ns_per_iter,
                |r| write_results(std::slice::from_ref(r)),
            );
        }
    }
}
