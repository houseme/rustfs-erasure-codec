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
