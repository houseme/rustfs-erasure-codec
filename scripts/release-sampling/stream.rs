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

// These allocations remain inside the original per-iteration timed loop.
// Cloning an empty Vec would discard its reservation for every cloned writer.
fn reserved_stream_outputs(count: usize, shard_size: usize) -> Vec<Vec<u8>> {
    (0..count).map(|_| Vec::with_capacity(shard_size)).collect()
}

#[test]
fn stream_writers_keep_independent_capacity_across_multiple_blocks() {
    for count in [0, 1, 4] {
        for len in [0, 1061, 65536] {
            let mut outputs = reserved_stream_outputs(count, len);
            assert_eq!(outputs.len(), count);
            let pointers: Vec<_> = outputs.iter().map(Vec::as_ptr).collect();
            for writer in &mut outputs {
                assert!(writer.is_empty());
                assert!(writer.capacity() >= len);
                for start in (0..len).step_by(257) {
                    writer.extend(core::iter::repeat_n(0x5a, (len - start).min(257)));
                }
                assert_eq!(writer.len(), len);
                assert!(writer.iter().all(|&byte| byte == 0x5a));
            }
            assert!(
                outputs
                    .iter()
                    .zip(pointers)
                    .all(|(writer, pointer)| writer.as_ptr() == pointer)
            );
        }
    }
}
