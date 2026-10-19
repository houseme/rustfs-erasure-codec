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
            super::sampling::run_prepared(
                &key,
                "stream-path-results.json",
                |page_size| {
                    let mut prepared = PreparedStream::new(case, operation, page_size);
                    move |iterations| prepared.run(iterations)
                },
                |r| r.ns_per_iter,
                |r| write_results(std::slice::from_ref(r)),
            );
        }
    }
}

struct PreparedStream {
    stream_case: StreamCase,
    operation: StreamOperation,
    seed: u64,
    source: Vec<Vec<u8>>,
    options: StreamOptions,
    codec: ReedSolomon,
    fixture: super::layout::Fixture,
}
impl PreparedStream {
    fn new(stream_case: StreamCase, operation: StreamOperation, page_size: usize) -> Self {
        let case = stream_case.case;
        let seed = derived_seed(operation.seed_operation(), case);
        let options = StreamOptions::new()
            .with_block_size(stream_case.block_size)
            .with_io_mode(stream_io_mode());
        let mut source = if matches!(operation, StreamOperation::Encode) {
            make_data_shards(seed, case.data_shards, case.shard_size)
        } else {
            super::bench_common::make_full_shards(
                seed,
                case.data_shards,
                case.parity_shards,
                case.shard_size,
            )
        };
        let fixture = super::layout::Fixture::vec_buffers(&source, &[], page_size)
            .with_contract(release_contract(case, operation));
        let codec = ReedSolomon::new(case.data_shards, case.parity_shards).unwrap();
        if !matches!(operation, StreamOperation::Encode) {
            codec.encode(&mut source).unwrap();
        }
        let prepared = Self {
            stream_case,
            operation,
            seed,
            source,
            options,
            codec,
            fixture,
        };
        prepared.check_fixture();
        prepared
    }
    fn check_fixture(&self) {
        assert_eq!(
            super::layout::Fixture::vec_buffers(&self.source, &[], self.fixture.page_size())
                .with_contract(release_contract(self.stream_case.case, self.operation)),
            self.fixture
        );
    }
    fn run(&mut self, iterations: usize) -> super::sampling::Output<StreamResult> {
        assert!(iterations > 0);
        self.check_fixture();
        let case = self.stream_case.case;
        let elapsed = execute_release_kernel(self, iterations);
        self.check_fixture();
        let ns_per_iter = elapsed.as_nanos() as f64 / iterations as f64;
        let blocks_per_iter = case.shard_size.div_ceil(self.options.block_size).max(1);
        let logical_data_bytes = case.data_shards * case.shard_size;
        super::sampling::Output {
            result: StreamResult {
                operation: self.operation.as_str(),
                data_shards: case.data_shards,
                parity_shards: case.parity_shards,
                shard_size: case.shard_size,
                logical_data_bytes,
                case_label: case.label,
                seed: self.seed,
                stream_block_size: self.options.block_size,
                stream_io_backend: "memory",
                stream_io_mode: stream_io_mode_label(self.options.io_mode),
                blocks_per_iter,
                throughput_mb_s: logical_data_bytes as f64 / 1048576.0 / (ns_per_iter / 1e9),
                ns_per_iter,
                ns_per_block: ns_per_iter / blocks_per_iter as f64,
            },
            fixture: super::layout::OptionalFixture(Some(self.fixture)),
            workload: super::workload::OptionalWorkload(Some(super::workload::Workload::retained(
                self.seed,
                release_contract(case, self.operation),
            ))),
        }
    }
}

#[test]
fn all_stream_entries_retain_their_source_across_short_tail_blocks() {
    let case = StreamCase {
        case: BenchCase {
            data_shards: 4,
            parity_shards: 2,
            shard_size: 1061,
            label: "test",
        },
        block_size: 257,
    };
    for operation in [
        StreamOperation::Encode,
        StreamOperation::Verify,
        StreamOperation::Reconstruct,
    ] {
        let mut prepared = PreparedStream::new(case, operation, 16384);
        let fixture = prepared.fixture;
        let expected = if matches!(operation, StreamOperation::Encode) {
            make_data_shards(prepared.seed, case.case.data_shards, case.case.shard_size)
        } else {
            make_encoded_shards(case.case, prepared.seed)
        };
        assert_eq!(prepared.source, expected);
        for _ in 0..12 {
            let output = prepared.run(2);
            assert_eq!(output.fixture.0.unwrap(), fixture);
            assert_eq!(prepared.source, expected);
            assert_eq!(output.result.operation, operation.as_str());
            assert!(output.result.blocks_per_iter > 1);
            if !matches!(operation, StreamOperation::Encode) {
                assert!(prepared.codec.verify(&prepared.source).unwrap());
            }
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
