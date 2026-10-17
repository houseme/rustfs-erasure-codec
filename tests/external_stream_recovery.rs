#![cfg(feature = "std")]

use std::io::{self, Read, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use rustfs_erasure_codec::galois_8::ReedSolomon;
use rustfs_erasure_codec::stream::{StreamError, StreamErrorKind, StreamIoMode, StreamOptions};
use rustfs_erasure_codec::{CodecFamily, CodecOptions, Error, MatrixMode};

const BLOCK: usize = 1024;
const MODES: [StreamIoMode; 3] = [
    StreamIoMode::Auto,
    StreamIoMode::Serial,
    StreamIoMode::Parallel,
];

// Neither endpoint implements Seek or wraps a Cursor. Both force partial I/O
// and interruptions so the public API must fill/write complete logical blocks.
struct FragmentedReader<'a> {
    data: &'a [u8],
    position: usize,
    interrupt: bool,
    fail_at: Option<usize>,
    calls: usize,
}

impl<'a> FragmentedReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            position: 0,
            interrupt: true,
            fail_at: None,
            calls: 0,
        }
    }
}

impl Read for FragmentedReader<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.calls += 1;
        if self.interrupt {
            self.interrupt = false;
            return Err(io::ErrorKind::Interrupted.into());
        }
        if self.fail_at.is_some_and(|limit| self.position >= limit) {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        let available = self.data.len() - self.position;
        let until_failure = self
            .fail_at
            .map_or(usize::MAX, |limit| limit - self.position);
        let len = buffer.len().min(17).min(available).min(until_failure);
        buffer[..len].copy_from_slice(&self.data[self.position..self.position + len]);
        self.position += len;
        self.interrupt = true;
        Ok(len)
    }
}

#[derive(Default)]
struct FragmentedWriter {
    data: Vec<u8>,
    interrupt: bool,
    fail_at: Option<usize>,
    zero_at: Option<usize>,
    calls: usize,
    flushes: usize,
}

impl Write for FragmentedWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.calls += 1;
        if self.interrupt {
            self.interrupt = false;
            return Err(io::ErrorKind::Interrupted.into());
        }
        if self.fail_at.is_some_and(|limit| self.data.len() >= limit) {
            return Err(io::ErrorKind::BrokenPipe.into());
        }
        if self.zero_at.is_some_and(|limit| self.data.len() >= limit) {
            return Ok(0);
        }
        let limit = self.fail_at.or(self.zero_at).unwrap_or(usize::MAX);
        let len = bytes.len().min(29).min(limit - self.data.len());
        self.data.extend_from_slice(&bytes[..len]);
        self.interrupt = true;
        Ok(len)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.flushes += 1;
        Ok(())
    }
}

fn options(mode: StreamIoMode) -> StreamOptions {
    StreamOptions::new()
        .with_block_size(BLOCK)
        .with_io_mode(mode)
}

fn encoded(codec: &ReedSolomon, len: usize) -> Vec<Vec<u8>> {
    let mut shards: Vec<Vec<u8>> = (0..codec.total_shard_count())
        .map(|index| {
            (0..len)
                .map(|offset| (index * 31 + (offset % 251) * 7) as u8)
                .collect()
        })
        .collect();
    codec.encode(&mut shards).unwrap();
    shards
}

fn readers_for<'a>(shards: &'a [Vec<u8>], missing: &[usize]) -> Vec<Option<FragmentedReader<'a>>> {
    shards
        .iter()
        .enumerate()
        .map(|(index, shard)| (!missing.contains(&index)).then(|| FragmentedReader::new(shard)))
        .collect()
}

fn writers_for(count: usize, missing: &[usize]) -> Vec<Option<FragmentedWriter>> {
    (0..count)
        .map(|index| missing.contains(&index).then(FragmentedWriter::default))
        .collect()
}

fn assert_codec_error(error: &StreamError, expected: Error) {
    assert!(matches!(error.kind, StreamErrorKind::Codec(actual) if actual == expected));
}

fn assert_no_io(readers: &[Option<FragmentedReader<'_>>], writers: &[Option<FragmentedWriter>]) {
    assert!(readers.iter().flatten().all(|reader| reader.calls == 0));
    assert!(
        writers
            .iter()
            .flatten()
            .all(|writer| writer.calls == 0 && writer.flushes == 0)
    );
}

#[test]
fn independent_streams_recover_from_current_positions_with_partial_io() {
    let codecs = [
        ReedSolomon::new(4, 2).unwrap(),
        ReedSolomon::with_options(
            4,
            2,
            CodecOptions::builder()
                .matrix_mode(MatrixMode::Cauchy)
                .build(),
        )
        .unwrap(),
        ReedSolomon::with_custom_matrix(
            4,
            2,
            &[vec![1; 4], vec![1, 2, 4, 8]],
            CodecOptions::default(),
        )
        .unwrap(),
    ];
    for codec in codecs {
        let expected = encoded(&codec, BLOCK * 3 + 37);
        for mode in MODES {
            for missing in [&[1, 5][..], &[4, 5], &[0, 3], &[]] {
                let mut readers = readers_for(&expected, missing);
                for reader in readers.iter_mut().flatten() {
                    reader.position = 7;
                }
                let mut writers = writers_for(expected.len(), missing);
                for writer in writers.iter_mut().flatten() {
                    writer.data.extend_from_slice(b"prefix");
                }
                codec
                    .reconstruct_stream_from(&mut readers, &mut writers, &options(mode))
                    .unwrap();
                for &index in missing {
                    let writer = writers[index].as_ref().unwrap();
                    assert_eq!(&writer.data[..6], b"prefix");
                    assert_eq!(&writer.data[6..], &expected[index][7..]);
                    assert_eq!(writer.flushes, 0);
                }
                assert!(
                    readers
                        .iter()
                        .flatten()
                        .all(|reader| reader.position == expected[0].len())
                );
            }
        }
    }
}

#[test]
fn independent_streams_preserve_fast_parity_and_zero_block_size_clamp() {
    let codec =
        ReedSolomon::with_options(4, 1, CodecOptions::builder().fast_one_parity(true).build())
            .unwrap();
    let expected = encoded(&codec, BLOCK * 2 + 37);
    for mode in MODES {
        for missing in 0..expected.len() {
            let mut readers = readers_for(&expected, &[missing]);
            let mut writers = writers_for(expected.len(), &[missing]);
            let opts = StreamOptions {
                block_size: 0,
                io_mode: mode,
            };
            codec
                .reconstruct_stream_from(&mut readers, &mut writers, &opts)
                .unwrap();
            assert_eq!(writers[missing].as_ref().unwrap().data, expected[missing]);
        }
    }
}

#[test]
fn explicit_empty_present_streams_are_valid_but_absent_inputs_are_not() {
    let codec = ReedSolomon::new(4, 2).unwrap();
    let empty = vec![Vec::new(); 6];
    for mode in MODES {
        for missing in [&[4, 5][..], &[]] {
            let mut readers = readers_for(&empty, missing);
            let mut writers = writers_for(6, missing);
            codec
                .reconstruct_stream_from(&mut readers, &mut writers, &options(mode))
                .unwrap();
            assert!(
                writers
                    .iter()
                    .flatten()
                    .all(|writer| writer.calls == 0 && writer.data.is_empty())
            );
        }
        let mut readers = readers_for(&empty, &[0, 1, 2, 3, 4, 5]);
        let mut writers = writers_for(6, &[0, 1, 2, 3, 4, 5]);
        let error = codec
            .reconstruct_stream_from(&mut readers, &mut writers, &options(mode))
            .unwrap_err();
        assert_codec_error(&error, Error::TooFewShardsPresent);
        assert_no_io(&readers, &writers);
    }
}

#[test]
fn invalid_counts_layouts_and_recoverability_fail_before_io() {
    let codec = ReedSolomon::new(4, 2).unwrap();
    let expected = encoded(&codec, BLOCK + 37);
    for mode in MODES {
        for case in 0..7 {
            let mut readers = readers_for(&expected, &[1, 5]);
            let mut writers = writers_for(6, &[1, 5]);
            let expected_error = match case {
                0 => {
                    readers.pop();
                    Error::TooFewShards
                }
                1 => {
                    readers.push(None);
                    Error::TooManyShards
                }
                2 => {
                    writers.pop();
                    Error::TooFewShards
                }
                3 => {
                    writers.push(None);
                    Error::TooManyShards
                }
                4 => {
                    writers[0] = Some(FragmentedWriter::default());
                    Error::InvalidShardFlags
                }
                5 => {
                    writers[1] = None;
                    Error::InvalidShardFlags
                }
                6 => {
                    readers[2] = None;
                    writers[2] = Some(FragmentedWriter::default());
                    Error::TooFewShardsPresent
                }
                _ => unreachable!(),
            };
            let error = codec
                .reconstruct_stream_from(&mut readers, &mut writers, &options(mode))
                .unwrap_err();
            assert_codec_error(&error, expected_error);
            if case == 4 || case == 5 {
                assert_eq!(error.shard_index, case - 4);
            }
            assert_no_io(&readers, &writers);
        }
    }
}

#[test]
fn leopard_stream_recovery_is_rejected_before_io() {
    let expected = vec![vec![0u8; 64]; 6];
    for family in [CodecFamily::LeopardGF8, CodecFamily::LeopardGF16] {
        if family == CodecFamily::LeopardGF16 && cfg!(target_endian = "big") {
            continue;
        }
        let codec =
            ReedSolomon::with_options(4, 2, CodecOptions::builder().codec_family(family).build())
                .unwrap();
        let mut readers = readers_for(&expected, &[1]);
        let mut writers = writers_for(6, &[1]);
        let error = codec
            .reconstruct_stream_from(&mut readers, &mut writers, &options(StreamIoMode::Parallel))
            .unwrap_err();
        assert_codec_error(&error, Error::UnsupportedCodecFamily);
        assert_no_io(&readers, &writers);
    }
}

#[test]
fn unequal_lengths_do_not_write_the_invalid_block() {
    let codec = ReedSolomon::new(4, 2).unwrap();
    let expected = encoded(&codec, BLOCK * 3 + 37);
    for mode in MODES {
        for short_len in [0, BLOCK - 1, BLOCK + 17, expected[0].len() - 1] {
            let mut readers = readers_for(&expected, &[1, 5]);
            readers[2] = Some(FragmentedReader::new(&expected[2][..short_len]));
            let mut writers = writers_for(6, &[1, 5]);
            let error = codec
                .reconstruct_stream_from(&mut readers, &mut writers, &options(mode))
                .unwrap_err();
            assert_eq!(error.shard_index, 2);
            assert_codec_error(&error, Error::IncorrectShardSize);
            let completed = short_len / BLOCK * BLOCK;
            for index in [1, 5] {
                assert_eq!(
                    writers[index].as_ref().unwrap().data,
                    expected[index][..completed]
                );
            }
        }
    }
}

#[test]
fn reader_errors_preserve_only_previously_completed_output_blocks() {
    let codec = ReedSolomon::new(4, 2).unwrap();
    let expected = encoded(&codec, BLOCK * 3 + 37);
    for mode in MODES {
        for fail_at in [0, BLOCK + 17] {
            let mut readers = readers_for(&expected, &[1, 5]);
            readers[2].as_mut().unwrap().fail_at = Some(fail_at);
            let mut writers = writers_for(6, &[1, 5]);
            let error = codec
                .reconstruct_stream_from(&mut readers, &mut writers, &options(mode))
                .unwrap_err();
            assert_eq!(error.shard_index, 2);
            assert!(
                matches!(error.kind, StreamErrorKind::Read(ref error) if error.kind() == io::ErrorKind::PermissionDenied)
            );
            let completed = fail_at / BLOCK * BLOCK;
            for index in [1, 5] {
                assert_eq!(
                    writers[index].as_ref().unwrap().data,
                    expected[index][..completed]
                );
            }
        }
    }
}

#[test]
fn writer_errors_and_write_zero_report_shard_and_partial_output() {
    let codec = ReedSolomon::new(4, 2).unwrap();
    let expected = encoded(&codec, BLOCK * 3 + 37);
    let limit = BLOCK + 11;
    for mode in MODES {
        for write_zero in [false, true] {
            let mut readers = readers_for(&expected, &[1]);
            let mut writers = writers_for(6, &[1]);
            let writer = writers[1].as_mut().unwrap();
            if write_zero {
                writer.zero_at = Some(limit);
            } else {
                writer.fail_at = Some(limit);
            }
            let error = codec
                .reconstruct_stream_from(&mut readers, &mut writers, &options(mode))
                .unwrap_err();
            assert_eq!(error.shard_index, 1);
            let expected_kind = if write_zero {
                io::ErrorKind::WriteZero
            } else {
                io::ErrorKind::BrokenPipe
            };
            assert!(
                matches!(error.kind, StreamErrorKind::Write(ref error) if error.kind() == expected_kind)
            );
            assert_eq!(writers[1].as_ref().unwrap().data, expected[1][..limit]);
        }
    }
}

struct GeneratedReader {
    shard: usize,
    position: usize,
    len: usize,
    written: Arc<[AtomicUsize; 2]>,
}

impl Read for GeneratedReader {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        if self.position == self.len {
            return Ok(0);
        }
        let written = self
            .written
            .iter()
            .map(|count| count.load(Ordering::Relaxed))
            .min()
            .unwrap();
        if self.position >= written + BLOCK {
            return Err(io::Error::other(
                "input advanced beyond the current output block",
            ));
        }
        let len = bytes
            .len()
            .min(self.len - self.position)
            .min(written + BLOCK - self.position);
        for (offset, byte) in bytes[..len].iter_mut().enumerate() {
            *byte = (self.shard * 31 + ((self.position + offset) % 251) * 7) as u8;
        }
        self.position += len;
        Ok(len)
    }
}

struct CheckingWriter {
    parity: usize,
    period: Vec<u8>,
    written: Arc<[AtomicUsize; 2]>,
}

impl Write for CheckingWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let start = self.written[self.parity].load(Ordering::Relaxed);
        assert!(
            bytes
                .iter()
                .enumerate()
                .all(|(offset, &byte)| byte == self.period[(start + offset) % self.period.len()])
        );
        self.written[self.parity].fetch_add(bytes.len(), Ordering::Relaxed);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn generated_streams_progress_blockwise_without_buffering_whole_shards() {
    let codec = ReedSolomon::new(4, 2).unwrap();
    let period = encoded(&codec, 251);
    let len = BLOCK * 65 + 37;
    for mode in MODES {
        let written = Arc::new([AtomicUsize::new(0), AtomicUsize::new(0)]);
        let mut readers: Vec<_> = (0..6)
            .map(|index| {
                (index < 4).then(|| GeneratedReader {
                    shard: index,
                    position: 0,
                    len,
                    written: Arc::clone(&written),
                })
            })
            .collect();
        let mut writers: Vec<_> = (0..6)
            .map(|index| {
                (index >= 4).then(|| CheckingWriter {
                    parity: index - 4,
                    period: period[index].clone(),
                    written: Arc::clone(&written),
                })
            })
            .collect();
        codec
            .reconstruct_stream_from(&mut readers, &mut writers, &options(mode))
            .unwrap();
        assert!(
            written
                .iter()
                .all(|count| count.load(Ordering::Relaxed) == len)
        );
    }
}

#[cfg(feature = "benchmark-metrics")]
#[test]
fn independent_streams_prepare_one_decode_plan_for_all_blocks() {
    let codec = ReedSolomon::new(4, 2).unwrap();
    let expected = encoded(&codec, BLOCK * 7 + 37);
    let mut readers = readers_for(&expected, &[1, 5]);
    let mut writers = writers_for(6, &[1, 5]);
    codec
        .reconstruct_stream_from(&mut readers, &mut writers, &options(StreamIoMode::Serial))
        .unwrap();
    let stats = codec.reconstruction_cache_stats();
    assert_eq!(stats.requests, 1);
    assert_eq!(stats.inserts, 1);
    assert_eq!(stats.hits, 0);
}
