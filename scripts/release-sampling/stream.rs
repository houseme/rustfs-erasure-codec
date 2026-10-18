pub(super) fn release_profile() {
    assert_backend_override_honored_if_strict();
    for &case in stream_cases() {
        for operation in [
            StreamOperation::Encode,
            StreamOperation::Verify,
            StreamOperation::Reconstruct,
        ] {
            let key = format!(
                "{}:{}:{}:{}:{}",
                operation.as_str(),
                case.case.data_shards,
                case.case.parity_shards,
                case.case.shard_size,
                case.block_size
            );
            super::sampling::run(
                &key,
                "stream-path-results.json",
                |n| run_stream_operation(case, operation, n),
                |r| r.ns_per_iter,
                |r| write_results(std::slice::from_ref(r)),
            );
        }
    }
}
