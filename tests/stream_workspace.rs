#![cfg(feature = "std")]

use rustfs_erasure_codec::stream::{StreamErrorKind, StreamIoMode, StreamOptions, StreamWorkspace};
use rustfs_erasure_codec::{CodecFamily, CodecOptions, Error, MatrixMode, galois_8::ReedSolomon};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::io::{self, Cursor, Read, Write};

thread_local! {
    static REQUESTS: Cell<Option<(usize, usize)>> = const { Cell::new(None) };
}
struct Counter;
fn record(size: usize) {
    let _ = REQUESTS.try_with(|requests| {
        if let Some((calls, blocks)) = requests.get() {
            requests.set(Some((calls + 1, blocks + usize::from(size >= 1024))));
        }
    });
}
// SAFETY: Forward each original allocation contract to System. Thread-local
// bookkeeping neither allocates nor accesses the allocated memory.
unsafe impl GlobalAlloc for Counter {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record(layout.size());
        // SAFETY: Preserve the caller's valid allocation layout.
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record(layout.size());
        // SAFETY: Preserve the layout and zero-initialization requirement.
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        record(size);
        // SAFETY: Forward the live System allocation and requested size.
        unsafe { System.realloc(ptr, layout, size) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: Forward the original System allocation and matching layout.
        unsafe { System.dealloc(ptr, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: Counter = Counter;
fn counted(run: impl FnOnce()) -> (usize, usize) {
    struct Stop;
    impl Drop for Stop {
        fn drop(&mut self) {
            REQUESTS.with(|requests| requests.set(None));
        }
    }
    REQUESTS.with(|requests| assert!(requests.replace(Some((0, 0))).is_none()));
    let _stop = Stop;
    run();
    REQUESTS.with(|requests| requests.get().unwrap())
}

fn encoded(codec: &ReedSolomon, len: usize) -> Vec<Vec<u8>> {
    let mut shards: Vec<Vec<_>> = (0..codec.total_shard_count())
        .map(|row| {
            (0..len)
                .map(|index| (index.wrapping_mul(37) ^ (index / 131) ^ (row * 53)) as u8)
                .collect()
        })
        .collect();
    codec.encode(&mut shards).unwrap();
    shards
}
fn opts(mode: StreamIoMode, block: usize) -> StreamOptions {
    StreamOptions::new()
        .with_block_size(block)
        .with_io_mode(mode)
}
fn recover(
    codec: &ReedSolomon,
    expected: &[Vec<u8>],
    missing: &[usize],
    options: &StreamOptions,
    workspace: &mut StreamWorkspace,
) {
    let mut readers: Vec<_> = expected
        .iter()
        .enumerate()
        .map(|(index, shard)| (!missing.contains(&index)).then_some(shard.as_slice()))
        .collect();
    let mut writers: Vec<_> = (0..expected.len())
        .map(|index| missing.contains(&index).then(Vec::new))
        .collect();
    codec
        .reconstruct_stream_from_with_workspace(&mut readers, &mut writers, options, workspace)
        .unwrap();
    for &index in missing {
        assert_eq!(writers[index].as_ref().unwrap(), &expected[index]);
    }
}

#[test]
fn workspace_adapts_across_modes_formats_lengths_and_missing_patterns() {
    let mut workspace = StreamWorkspace::new();
    for (data, parity, matrix, len, block) in [
        (4, 2, MatrixMode::Vandermonde, 3179, 1024),
        (10, 4, MatrixMode::Cauchy, 6127, 2048),
        (2, 1, MatrixMode::Vandermonde, 65, 1024),
        (4, 2, MatrixMode::Cauchy, 3197, 2048),
        (4, 2, MatrixMode::Vandermonde, 0, 1024),
        (4, 2, MatrixMode::Vandermonde, 3179, 1024),
    ] {
        let codec = ReedSolomon::with_options(
            data,
            parity,
            CodecOptions::builder()
                .matrix_mode(matrix)
                .inversion_cache(false)
                .build(),
        )
        .unwrap();
        let expected = if len == 0 {
            vec![vec![]; data + parity]
        } else {
            encoded(&codec, len)
        };
        for mode in [
            StreamIoMode::Serial,
            StreamIoMode::Parallel,
            StreamIoMode::Auto,
        ] {
            let options = opts(mode, block);
            let mut readers: Vec<_> = expected[..data].iter().map(Vec::as_slice).collect();
            let mut writers = vec![Vec::new(); parity];
            codec
                .encode_stream_with_workspace(&mut readers, &mut writers, &options, &mut workspace)
                .unwrap();
            assert_eq!(writers, expected[data..]);
            let mut readers: Vec<_> = expected.iter().map(Vec::as_slice).collect();
            assert!(
                codec
                    .verify_stream_with_workspace(&mut readers, &options, &mut workspace)
                    .unwrap()
            );
            recover(&codec, &expected, &[0], &options, &mut workspace);
            recover(&codec, &expected, &[data], &options, &mut workspace);
            if parity > 1 {
                recover(&codec, &expected, &[0, data], &options, &mut workspace);
            }
        }
    }
    workspace.clear();
    let codec = ReedSolomon::new(4, 2).unwrap();
    recover(
        &codec,
        &encoded(&codec, 1059),
        &[0, 4],
        &opts(StreamIoMode::Serial, 1024),
        &mut workspace,
    );
}

#[test]
fn warmed_calls_reuse_buffers_and_the_plan_without_heap_growth() {
    let codec = ReedSolomon::with_options(
        4,
        2,
        CodecOptions::builder()
            .inversion_cache(false)
            .max_parallel_jobs(1)
            .build(),
    )
    .unwrap();
    let original = encoded(&codec, 32 * 1024 + 65);
    let mut workspace = StreamWorkspace::new();
    for repetition in 0..4 {
        let options = opts(
            StreamIoMode::Serial,
            if repetition % 2 == 0 { 2048 } else { 1024 },
        );
        let expected: Vec<_> = original
            .iter()
            .map(|shard| shard[..shard.len() - repetition * 13].to_vec())
            .collect();
        let mut readers: Vec<_> = expected[..4].iter().map(Vec::as_slice).collect();
        let mut writers: Vec<_> = (0..2)
            .map(|_| Vec::with_capacity(expected[0].len()))
            .collect();
        let encode_requests = counted(|| {
            codec
                .encode_stream_with_workspace(&mut readers, &mut writers, &options, &mut workspace)
                .unwrap()
        });
        assert_eq!(writers, expected[4..]);
        let mut readers: Vec<_> = expected.iter().map(Vec::as_slice).collect();
        let verify_requests = counted(|| {
            assert!(
                codec
                    .verify_stream_with_workspace(&mut readers, &options, &mut workspace)
                    .unwrap()
            )
        });
        let mut readers: Vec<_> = expected
            .iter()
            .enumerate()
            .map(|(index, shard)| (![0, 4].contains(&index)).then_some(shard.as_slice()))
            .collect();
        let mut writers: Vec<_> = (0..6)
            .map(|index| {
                [0, 4]
                    .contains(&index)
                    .then(|| Vec::with_capacity(expected[0].len()))
            })
            .collect();
        let reconstruct_requests = counted(|| {
            codec
                .reconstruct_stream_from_with_workspace(
                    &mut readers,
                    &mut writers,
                    &options,
                    &mut workspace,
                )
                .unwrap()
        });
        assert_eq!(writers[0].as_ref().unwrap(), &expected[0]);
        assert_eq!(writers[4].as_ref().unwrap(), &expected[4]);
        if repetition > 0 {
            // Both APIs receive fresh readers and preallocated caller output
            // storage. Only the API execution enters the allocation window.
            let mut readers: Vec<_> = expected[..4].iter().map(Vec::as_slice).collect();
            let mut writers: Vec<_> = (0..2)
                .map(|_| Vec::with_capacity(expected[0].len()))
                .collect();
            let legacy_encode_requests = counted(|| {
                codec
                    .encode_stream(&mut readers, &mut writers, &options)
                    .unwrap()
            });
            assert_eq!(writers, expected[4..]);

            let mut readers: Vec<_> = expected.iter().map(Vec::as_slice).collect();
            let legacy_verify_requests =
                counted(|| assert!(codec.verify_stream(&mut readers, &options).unwrap()));

            let mut readers: Vec<_> = expected
                .iter()
                .enumerate()
                .map(|(index, shard)| (![0, 4].contains(&index)).then_some(shard.as_slice()))
                .collect();
            let mut writers: Vec<_> = (0..6)
                .map(|index| {
                    [0, 4]
                        .contains(&index)
                        .then(|| Vec::with_capacity(expected[0].len()))
                })
                .collect();
            let legacy_reconstruct_requests = counted(|| {
                codec
                    .reconstruct_stream_from(&mut readers, &mut writers, &options)
                    .unwrap()
            });
            assert_eq!(writers[0].as_ref().unwrap(), &expected[0]);
            assert_eq!(writers[4].as_ref().unwrap(), &expected[4]);

            for (operation, legacy, requests) in [
                ("encode", legacy_encode_requests, encode_requests),
                ("verify", legacy_verify_requests, verify_requests),
                (
                    "reconstruct_from",
                    legacy_reconstruct_requests,
                    reconstruct_requests,
                ),
            ] {
                println!(
                    "STREAM_WORKSPACE_ALLOCATIONS operation={operation} repetition={repetition} data=4 parity=2 shard_len={} block_size={} inversion_cache=false io_mode=serial legacy_requests={} workspace_requests={} legacy_ge_1k_requests={} workspace_ge_1k_requests={}",
                    expected[0].len(),
                    options.block_size,
                    legacy.0,
                    requests.0,
                    legacy.1,
                    requests.1
                );
                assert!(
                    legacy.0 > requests.0,
                    "{operation} failed to reduce warm allocation requests"
                );
                assert_eq!(
                    requests.1, 0,
                    "{operation} reallocated block storage or recovery matrix"
                );
                // The Windows debug environment reader can allocate on each
                // parallel-policy query. Other contexts have no warm requests.
                if !cfg!(all(target_os = "windows", debug_assertions)) {
                    assert_eq!(requests.0, 0, "{operation} made warm heap requests");
                }
            }
        }
    }
}

#[test]
fn padding_and_corruption_do_not_leak_previous_call_contents() {
    let codec = ReedSolomon::new(4, 2).unwrap();
    let mut workspace = StreamWorkspace::new();
    for mode in [StreamIoMode::Serial, StreamIoMode::Parallel] {
        let options = opts(mode, 1024);
        for lengths in [
            [4097, 4097, 4097, 4097],
            [3171, 1025, 17, 0],
            [0, 0, 0, 0],
            [13, 0, 7, 0],
        ] {
            let data: Vec<Vec<_>> = lengths
                .iter()
                .enumerate()
                .map(|(row, &len)| vec![(row * 61 + 13) as u8; len])
                .collect();
            let mut ordinary_readers: Vec<_> = data.iter().map(Vec::as_slice).collect();
            let mut readers = ordinary_readers.clone();
            let mut ordinary = vec![Vec::new(); 2];
            let mut parity = vec![Vec::new(); 2];
            codec
                .encode_stream(&mut ordinary_readers, &mut ordinary, &options)
                .unwrap();
            codec
                .encode_stream_with_workspace(&mut readers, &mut parity, &options, &mut workspace)
                .unwrap();
            assert_eq!(parity, ordinary);
            let max_len = *lengths.iter().max().unwrap();
            let mut padded = data;
            for shard in &mut padded {
                shard.resize(max_len, 0);
            }
            padded.extend(parity);
            let mut readers: Vec<_> = padded.iter().map(Vec::as_slice).collect();
            assert!(
                codec
                    .verify_stream_with_workspace(&mut readers, &options, &mut workspace)
                    .unwrap()
            );
            if max_len != 0 {
                padded[4][0] ^= 1;
                let mut readers: Vec<_> = padded.iter().map(Vec::as_slice).collect();
                assert!(
                    !codec
                        .verify_stream_with_workspace(&mut readers, &options, &mut workspace)
                        .unwrap()
                );
                padded[4][0] ^= 1;
                let mut readers: Vec<_> = padded.iter().map(Vec::as_slice).collect();
                assert!(
                    codec
                        .verify_stream_with_workspace(&mut readers, &options, &mut workspace)
                        .unwrap()
                );
            }
        }
    }
}

struct FragmentedReader<'a> {
    cursor: Cursor<&'a [u8]>,
    fail_after: Option<u64>,
    interrupted: bool,
}
impl Read for FragmentedReader<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if self.interrupted {
            self.interrupted = false;
            return Err(io::ErrorKind::Interrupted.into());
        }
        self.interrupted = true;
        if self
            .fail_after
            .is_some_and(|position| self.cursor.position() >= position)
        {
            return Err(io::Error::other("injected read failure"));
        }
        let len = buffer.len().min(31);
        self.cursor.read(&mut buffer[..len])
    }
}
struct FailingWriter {
    bytes: Vec<u8>,
    fail_after: Option<usize>,
}
impl Write for FailingWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let allowed = self
            .fail_after
            .unwrap_or(usize::MAX)
            .saturating_sub(self.bytes.len());
        if allowed == 0 {
            return Err(io::Error::other("injected write failure"));
        }
        self.bytes.write(&bytes[..bytes.len().min(allowed).min(29)])
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn io_failures_and_truncation_allow_retry_with_restored_streams() {
    let codec = ReedSolomon::new(4, 2).unwrap();
    let expected = encoded(&codec, 3179);
    let mut workspace = StreamWorkspace::new();
    for mode in [StreamIoMode::Serial, StreamIoMode::Parallel] {
        let options = opts(mode, 1024);
        for fail_read in [true, false] {
            let mut readers: Vec<_> = expected
                .iter()
                .enumerate()
                .map(|(index, bytes)| {
                    (index != 0).then_some(FragmentedReader {
                        cursor: Cursor::new(bytes.as_slice()),
                        fail_after: (fail_read && index == 1).then_some(1024),
                        interrupted: true,
                    })
                })
                .collect();
            let mut writers: Vec<_> = (0..6)
                .map(|index| {
                    (index == 0).then_some(FailingWriter {
                        bytes: Vec::new(),
                        fail_after: (!fail_read).then_some(1100),
                    })
                })
                .collect();
            let error = codec
                .reconstruct_stream_from_with_workspace(
                    &mut readers,
                    &mut writers,
                    &options,
                    &mut workspace,
                )
                .unwrap_err();
            assert_eq!(error.shard_index, usize::from(fail_read));
            assert!(matches!(
                (&error.kind, fail_read),
                (StreamErrorKind::Read(_), true) | (StreamErrorKind::Write(_), false)
            ));
            let written = if fail_read { 1024 } else { 1100 };
            assert_eq!(writers[0].as_ref().unwrap().bytes, expected[0][..written]);
            for reader in readers.iter_mut().flatten() {
                reader.cursor.set_position(0);
                reader.fail_after = None;
            }
            writers[0].as_mut().unwrap().bytes.clear();
            writers[0].as_mut().unwrap().fail_after = None;
            codec
                .reconstruct_stream_from_with_workspace(
                    &mut readers,
                    &mut writers,
                    &options,
                    &mut workspace,
                )
                .unwrap();
            assert_eq!(writers[0].as_ref().unwrap().bytes, expected[0]);
        }
        let mut truncated = expected.clone();
        truncated[1].truncate(1025);
        let mut readers: Vec<_> = truncated
            .iter()
            .enumerate()
            .map(|(index, bytes)| (index != 0).then_some(bytes.as_slice()))
            .collect();
        let mut writers: Vec<_> = (0..6).map(|index| (index == 0).then(Vec::new)).collect();
        let error = codec
            .reconstruct_stream_from_with_workspace(
                &mut readers,
                &mut writers,
                &options,
                &mut workspace,
            )
            .unwrap_err();
        assert_eq!(error.shard_index, 1);
        assert!(matches!(
            error.kind,
            StreamErrorKind::Codec(Error::IncorrectShardSize)
        ));
        assert_eq!(writers[0].as_ref().unwrap(), &expected[0][..1024]);
        recover(&codec, &expected, &[0], &options, &mut workspace);
    }
}

#[test]
fn invalid_layouts_and_singular_new_formats_preserve_the_reusable_plan() {
    #[derive(Clone, Copy)]
    struct UnreachableIo;
    impl Read for UnreachableIo {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            panic!("invalid metadata reached a reader")
        }
    }
    impl Write for UnreachableIo {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            panic!("invalid metadata reached a writer")
        }
        fn flush(&mut self) -> io::Result<()> {
            panic!("stream API flushed a writer")
        }
    }
    let codec = ReedSolomon::new(2, 2).unwrap();
    let expected = encoded(&codec, 2113);
    let mut workspace = StreamWorkspace::new();
    let options = opts(StreamIoMode::Serial, 1024);
    recover(&codec, &expected, &[0, 1], &options, &mut workspace);
    let failure = codec
        .encode_stream_with_workspace(
            &mut [UnreachableIo],
            &mut [UnreachableIo; 2],
            &options,
            &mut workspace,
        )
        .unwrap_err();
    assert!(matches!(
        failure.kind,
        StreamErrorKind::Codec(Error::TooFewShards)
    ));
    let failure = codec
        .verify_stream_with_workspace(&mut [UnreachableIo; 3], &options, &mut workspace)
        .unwrap_err();
    assert!(matches!(
        failure.kind,
        StreamErrorKind::Codec(Error::TooFewShards)
    ));
    let failure = codec
        .reconstruct_stream_from_with_workspace(
            &mut [Some(UnreachableIo); 4],
            &mut [Some(UnreachableIo), None, None, None],
            &options,
            &mut workspace,
        )
        .unwrap_err();
    assert_eq!(failure.shard_index, 0);
    assert!(matches!(
        failure.kind,
        StreamErrorKind::Codec(Error::InvalidShardFlags)
    ));
    for (reader_count, writer_count, missing, error) in [
        (3, 4, vec![0], Error::TooFewShards),
        (4, 5, vec![0], Error::TooManyShards),
        (4, 4, vec![0, 1, 2], Error::TooFewShardsPresent),
    ] {
        let mut readers: Vec<_> = (0..reader_count)
            .map(|index| {
                (!missing.contains(&index)).then_some(Cursor::new(expected[index % 4].as_slice()))
            })
            .collect();
        let mut writers: Vec<_> = (0..writer_count)
            .map(|index| missing.contains(&index).then(Vec::new))
            .collect();
        let failure = codec
            .reconstruct_stream_from_with_workspace(
                &mut readers,
                &mut writers,
                &options,
                &mut workspace,
            )
            .unwrap_err();
        assert!(matches!(failure.kind, StreamErrorKind::Codec(actual) if actual == error));
        assert!(
            readers
                .iter()
                .flatten()
                .all(|reader| reader.position() == 0)
        );
        assert!(writers.iter().flatten().all(Vec::is_empty));
    }
    let singular =
        ReedSolomon::with_custom_matrix(2, 2, &[vec![1, 1], vec![1, 1]], CodecOptions::default())
            .unwrap();
    let mut readers = [
        None,
        None,
        Some(expected[2].as_slice()),
        Some(expected[3].as_slice()),
    ];
    let mut writers = [Some(Vec::new()), Some(Vec::new()), None, None];
    let error = singular
        .reconstruct_stream_from_with_workspace(
            &mut readers,
            &mut writers,
            &options,
            &mut workspace,
        )
        .unwrap_err();
    assert!(matches!(
        error.kind,
        StreamErrorKind::Codec(Error::InvalidCustomMatrix)
    ));
    assert!(writers.iter().flatten().all(Vec::is_empty));
    for family in [CodecFamily::LeopardGF8, CodecFamily::LeopardGF16] {
        let other =
            ReedSolomon::with_options(2, 2, CodecOptions::builder().codec_family(family).build())
                .unwrap();
        let error = other
            .encode_stream_with_workspace(
                &mut [UnreachableIo; 2],
                &mut [UnreachableIo; 2],
                &options,
                &mut workspace,
            )
            .unwrap_err();
        assert!(matches!(
            error.kind,
            StreamErrorKind::Codec(Error::UnsupportedCodecFamily)
        ));
        let error = other
            .verify_stream_with_workspace(&mut [UnreachableIo; 4], &options, &mut workspace)
            .unwrap_err();
        assert!(matches!(
            error.kind,
            StreamErrorKind::Codec(Error::UnsupportedCodecFamily)
        ));
        let mut readers = [
            None,
            None,
            Some(expected[2].as_slice()),
            Some(expected[3].as_slice()),
        ];
        let before = readers;
        let error = other
            .reconstruct_stream_from_with_workspace(
                &mut readers,
                &mut writers,
                &options,
                &mut workspace,
            )
            .unwrap_err();
        assert!(matches!(
            error.kind,
            StreamErrorKind::Codec(Error::UnsupportedCodecFamily)
        ));
        assert_eq!(readers, before);
    }
    recover(&codec, &expected, &[0, 1], &options, &mut workspace);
}

#[test]
fn cursor_workspace_matches_legacy_positions_empty_data_and_partial_outputs() {
    let codec = ReedSolomon::new(4, 2).unwrap();
    let expected = encoded(&codec, 3179);
    let mut workspace = StreamWorkspace::new();
    for mode in [StreamIoMode::Serial, StreamIoMode::Parallel] {
        for truncated in [false, true] {
            let mut ordinary: Vec<_> = expected
                .iter()
                .enumerate()
                .map(|(index, bytes)| {
                    let mut cursor = Cursor::new(if [0, 4].contains(&index) {
                        Vec::new()
                    } else {
                        bytes.clone()
                    });
                    cursor.set_position(17);
                    cursor
                })
                .collect();
            if truncated {
                ordinary[1].get_mut().truncate(1025);
            }
            let mut retained = ordinary.clone();
            let options = opts(mode, 1024);
            let a = codec.reconstruct_stream(&mut ordinary, &options);
            let b =
                codec.reconstruct_stream_with_workspace(&mut retained, &options, &mut workspace);
            assert_eq!(a.is_ok(), b.is_ok());
            assert_eq!(retained, ordinary);
            assert_eq!(retained[0].position(), 17);
        }
    }
    let mut empty = vec![Cursor::new(Vec::new()); 6];
    codec
        .reconstruct_stream_with_workspace(&mut empty, &StreamOptions::default(), &mut workspace)
        .unwrap();
    assert!(empty.iter().all(|cursor| cursor.get_ref().is_empty()));
}
