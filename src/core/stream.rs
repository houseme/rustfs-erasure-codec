//! Streaming encode/verify/reconstruct for large data.
//!
//! Provides [`encode_stream`](crate::galois_8::ReedSolomon::encode_stream),
//! [`verify_stream`](crate::galois_8::ReedSolomon::verify_stream),
//! [`reconstruct_stream_from`](crate::galois_8::ReedSolomon::reconstruct_stream_from), and
//! [`reconstruct_stream`](crate::galois_8::ReedSolomon::reconstruct_stream) methods that
//! process data in configurable blocks, avoiding the need to load entire
//! files into memory.
//!
//! # Example
//!
//! ```no_run
//! use rustfs_erasure_codec::galois_8::ReedSolomon;
//! use rustfs_erasure_codec::stream::StreamOptions;
//! use std::fs::File;
//! use std::io::BufReader;
//!
//! let rs = ReedSolomon::new(10, 4).unwrap();
//!
//! let mut data_readers: Vec<BufReader<File>> = (0..10)
//!     .map(|i| BufReader::new(File::open(format!("data_{i}.bin")).unwrap()))
//!     .collect();
//! let mut parity_writers: Vec<File> = (0..4)
//!     .map(|i| File::create(format!("parity_{i}.bin")).unwrap())
//!     .collect();
//!
//! let opts = StreamOptions::new().with_block_size(4 * 1024 * 1024);
//! rs.encode_stream(&mut data_readers, &mut parity_writers, &opts).unwrap();
//! ```

use std::io::Error;

use smallvec::SmallVec;

use super::recovery_plan::ClassicReconstructRows;
use super::{ClassicReconstructPlan, ReconstructionGoal};

/// Lower bound for the read/write block size (1 KiB).
const MIN_BLOCK_SIZE_BYTES: usize = 1024;
/// Upper bound for the read/write block size (16 MiB).
const MAX_BLOCK_SIZE_BYTES: usize = 16 * 1024 * 1024;

// ---------------------------------------------------------------------------
// StreamOptions
// ---------------------------------------------------------------------------

/// I/O scheduling mode for streaming operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamIoMode {
    /// Choose serial or parallel I/O using conservative built-in thresholds.
    Auto,
    /// Read and write shard streams serially.
    Serial,
    /// Read and write shard streams with rayon parallel iterators.
    Parallel,
}

/// Configuration for streaming encode/verify/reconstruct operations.
#[derive(Debug, Clone)]
pub struct StreamOptions {
    /// Block size in bytes for each read/write cycle. Default: 4 MiB.
    ///
    /// Larger blocks reduce syscall overhead but increase memory usage.
    /// Recommended range: 256 KiB – 16 MiB.
    pub block_size: usize,
    /// I/O scheduling mode for stream reads and writes. Default: Auto.
    pub io_mode: StreamIoMode,
}

impl Default for StreamOptions {
    fn default() -> Self {
        Self {
            block_size: 4 * 1024 * 1024,
            io_mode: StreamIoMode::Auto,
        }
    }
}

impl StreamOptions {
    /// Create a new `StreamOptions` with default settings.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the block size (clamped to `[1 KiB, 16 MiB]`).
    pub fn with_block_size(mut self, size: usize) -> Self {
        self.block_size = size.clamp(MIN_BLOCK_SIZE_BYTES, MAX_BLOCK_SIZE_BYTES);
        self
    }

    /// Set the stream I/O scheduling mode.
    pub fn with_io_mode(mut self, mode: StreamIoMode) -> Self {
        self.io_mode = mode;
        self
    }
}

// ---------------------------------------------------------------------------
// StreamError
// ---------------------------------------------------------------------------

/// Error returned by streaming operations.
#[derive(Debug)]
pub struct StreamError {
    /// Index of the shard that caused the error.
    pub shard_index: usize,
    /// The kind of error.
    pub kind: StreamErrorKind,
}

/// Classification of [`StreamError`].
#[derive(Debug)]
pub enum StreamErrorKind {
    /// I/O error while reading a shard.
    Read(std::io::Error),
    /// I/O error while writing a shard.
    Write(std::io::Error),
    /// Error from the underlying codec (encode/verify/reconstruct).
    Codec(crate::Error),
}

impl core::fmt::Display for StreamError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match &self.kind {
            StreamErrorKind::Read(e) => {
                write!(f, "read error on shard {}: {}", self.shard_index, e)
            }
            StreamErrorKind::Write(e) => {
                write!(f, "write error on shard {}: {}", self.shard_index, e)
            }
            StreamErrorKind::Codec(e) => {
                write!(f, "codec error on shard {}: {}", self.shard_index, e)
            }
        }
    }
}

impl std::error::Error for StreamError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.kind {
            StreamErrorKind::Read(e) => Some(e),
            StreamErrorKind::Write(e) => Some(e),
            StreamErrorKind::Codec(e) => Some(e),
        }
    }
}

impl StreamError {
    fn read(shard_index: usize, e: std::io::Error) -> Self {
        Self {
            shard_index,
            kind: StreamErrorKind::Read(e),
        }
    }

    fn write(shard_index: usize, e: std::io::Error) -> Self {
        Self {
            shard_index,
            kind: StreamErrorKind::Write(e),
        }
    }

    fn codec(shard_index: usize, e: crate::Error) -> Self {
        Self {
            shard_index,
            kind: StreamErrorKind::Codec(e),
        }
    }
}

/// Preserve the public stream count errors before allocation, I/O or cursor reset.
fn validate_stream_count(actual: usize, expected: usize) -> Result<(), StreamError> {
    match actual.cmp(&expected) {
        core::cmp::Ordering::Less => Err(StreamError::codec(0, crate::Error::TooFewShards)),
        core::cmp::Ordering::Greater => Err(StreamError::codec(0, crate::Error::TooManyShards)),
        core::cmp::Ordering::Equal => Ok(()),
    }
}

fn take_stream_error(
    first_error: &std::sync::Mutex<Option<StreamError>>,
    fallback: impl FnOnce() -> StreamError,
) -> StreamError {
    let mut guard = match first_error.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    guard.take().unwrap_or_else(fallback)
}

/// Record a parallel-write error, keeping the one with the smallest
/// `shard_index`. When multiple writers fail at once, the reported error no
/// longer depends on thread scheduling but deterministically points at the
/// lowest shard index, aiding diagnostics and reproduction.
fn record_min_error(first_error: &std::sync::Mutex<Option<StreamError>>, cand: StreamError) {
    if let Ok(mut guard) = first_error.lock() {
        let replace = match guard.as_ref() {
            Some(existing) => cand.shard_index < existing.shard_index,
            None => true,
        };
        if replace {
            *guard = Some(cand);
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn use_parallel_stream_io(options: &StreamOptions, stream_count: usize) -> bool {
    match options.io_mode {
        StreamIoMode::Serial => false,
        StreamIoMode::Parallel => true,
        StreamIoMode::Auto => use_parallel_stream_io_auto(options.block_size, stream_count),
    }
}

fn use_parallel_stream_io_auto(block_size: usize, stream_count: usize) -> bool {
    // Conservative single threshold: enable parallel I/O only when there are at
    // least 10 streams and blocks are at least 4 MiB. The two earlier
    // early-return guards (stream_count<2||block<256KiB and
    // stream_count<=6&&block<=1MiB) were strictly dominated by this predicate,
    // dead code that never changed the result, so they are removed to avoid
    // implying a tiered policy.
    block_size >= 4 * 1024 * 1024 && stream_count >= 10
}

/// Retain initialized I/O storage separately from the readable block prefix.
/// Generic codec inputs borrow this prefix directly, without collecting shard
/// views. Keeping `bytes.len()` initialized also avoids clearing a full block
/// again when checking EOF after a short tail.
struct StreamReadBuffer {
    bytes: Vec<u8>,
    read_len: usize,
    block_len: usize,
}

impl StreamReadBuffer {
    fn new(block_size: usize) -> Self {
        Self {
            bytes: Vec::with_capacity(block_size),
            read_len: 0,
            block_len: 0,
        }
    }

    fn read_from<R: std::io::Read>(
        &mut self,
        reader: &mut R,
        max_len: usize,
    ) -> std::io::Result<()> {
        self.read_len = read_one_stream(reader, &mut self.bytes, max_len)?;
        Ok(())
    }

    fn finish_block(&mut self, block_len: usize) {
        if self.read_len < block_len {
            self.bytes[self.read_len..block_len].fill(0);
        }
        self.block_len = block_len;
    }
}

impl AsRef<[u8]> for StreamReadBuffer {
    fn as_ref(&self) -> &[u8] {
        &self.bytes[..self.block_len]
    }
}

/// Read every stream completely up to `max_len`, retrying interruptions.
/// Original lengths remain available for verification; codec inputs expose
/// equal-length prefixes with shorter streams zero-padded for encoding.
fn read_block_with_mode<R: std::io::Read + Send>(
    readers: &mut [R],
    buffers: &mut [StreamReadBuffer],
    max_len: usize,
    use_parallel_io: bool,
) -> Result<(bool, usize), StreamError> {
    if use_parallel_io {
        use rayon::prelude::*;

        readers
            .par_iter_mut()
            .zip(buffers.par_iter_mut())
            .enumerate()
            .try_for_each(|(index, (reader, buffer))| {
                buffer
                    .read_from(reader, max_len)
                    .map_err(|error| StreamError::read(index, error))
            })?;
    } else {
        for (index, (reader, buffer)) in readers.iter_mut().zip(buffers.iter_mut()).enumerate() {
            buffer
                .read_from(reader, max_len)
                .map_err(|error| StreamError::read(index, error))?;
        }
    }

    let actual_len = buffers
        .iter()
        .map(|buffer| buffer.read_len)
        .max()
        .unwrap_or(0);
    for buffer in buffers {
        buffer.finish_block(actual_len);
    }
    Ok((actual_len == 0, actual_len))
}

fn write_block_with_mode<W: std::io::Write + Send>(
    writers: &mut [W],
    buffers: &[Vec<u8>],
    len: usize,
    shard_offset: usize,
    use_parallel_io: bool,
) -> Result<(), StreamError> {
    if use_parallel_io {
        write_block_par(writers, buffers, len, shard_offset)
    } else {
        write_block(writers, buffers, len, shard_offset)
    }
}

fn prepare_read_buffer(buf: &mut Vec<u8>, max_len: usize) {
    match buf.len().cmp(&max_len) {
        core::cmp::Ordering::Less => buf.resize(max_len, 0),
        core::cmp::Ordering::Greater => buf.truncate(max_len),
        core::cmp::Ordering::Equal => {}
    }
}

fn read_one_stream<R: std::io::Read>(
    reader: &mut R,
    buf: &mut Vec<u8>,
    max_len: usize,
) -> Result<usize, std::io::Error> {
    prepare_read_buffer(buf, max_len);
    let mut total = 0;

    while total < max_len {
        match reader.read(&mut buf[total..]) {
            Ok(0) => break,
            Ok(n) => total += n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }

    Ok(total)
}

fn read_present_streams_with_mode<R: std::io::Read + Send>(
    readers: &mut [Option<R>],
    buffers: &mut [Vec<u8>],
    max_len: usize,
    use_parallel_io: bool,
) -> Result<(bool, usize), StreamError> {
    if use_parallel_io {
        use rayon::prelude::*;

        readers
            .par_iter_mut()
            .zip(buffers.par_iter_mut())
            .enumerate()
            .try_for_each(|(i, (reader, buf))| {
                if let Some(reader) = reader {
                    let total = read_one_stream(reader, buf, max_len)
                        .map_err(|e| StreamError::read(i, e))?;
                    buf.truncate(total);
                }
                Ok::<_, StreamError>(())
            })?;
    } else {
        for (i, (reader, buf)) in readers.iter_mut().zip(buffers.iter_mut()).enumerate() {
            if let Some(reader) = reader {
                let total =
                    read_one_stream(reader, buf, max_len).map_err(|e| StreamError::read(i, e))?;
                buf.truncate(total);
            }
        }
    }

    // Present buffers already own their exact read lengths. Missing buffers
    // may retain the previous block, so exclude them from both length checks.
    let actual_len = readers
        .iter()
        .zip(buffers.iter())
        .filter(|(reader, _)| reader.is_some())
        .map(|(_, buffer)| buffer.len())
        .max()
        .unwrap_or(0);
    if actual_len != 0 {
        // Within a block, every present shard must read the same number of
        // bytes. A present shard that hits EOF early (fewer bytes than the
        // others) is truncated or length-mismatched and must not be silently
        // zero-padded into reconstruction, which would produce wrong recovered
        // data while returning Ok. This matches the in-memory `reconstruct`'s
        // `IncorrectShardSize` check.
        if let Some(bad) = readers
            .iter()
            .zip(buffers.iter())
            .position(|(reader, buffer)| reader.is_some() && buffer.len() != actual_len)
        {
            return Err(StreamError::codec(bad, crate::Error::IncorrectShardSize));
        }
    }

    Ok((actual_len == 0, actual_len))
}

// Legacy in-memory recovery knows the complete output length. Reserve lazily
// on the first validated write so invalid layouts, lengths or decode plans do
// not allocate full-shard outputs, and generic writers retain bounded scratch.
struct RecoveryVecWriter<'a> {
    bytes: &'a mut Vec<u8>,
    reserve_len: usize,
}

impl std::io::Write for RecoveryVecWriter<'_> {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        // Each legacy missing shard starts empty; no bytes are initialized by
        // reserve_exact, and subsequent blocks reuse the same allocation.
        if self.reserve_len != 0 {
            self.bytes
                .reserve_exact(core::mem::take(&mut self.reserve_len));
        }
        std::io::Write::write(self.bytes, buffer)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        std::io::Write::flush(self.bytes)
    }
}

fn write_missing_streams_with_mode<W: std::io::Write + Send>(
    writers: &mut [Option<W>],
    buffers: &[Vec<u8>],
    len: usize,
    use_parallel_io: bool,
) -> Result<(), StreamError> {
    if use_parallel_io {
        use rayon::prelude::*;

        writers
            .par_iter_mut()
            .zip(buffers.par_iter())
            .enumerate()
            .try_for_each(|(i, (writer, buf))| {
                if let Some(writer) = writer {
                    writer
                        .write_all(&buf[..len])
                        .map_err(|e| StreamError::write(i, e))?;
                }
                Ok(())
            })
    } else {
        for (i, (writer, buf)) in writers.iter_mut().zip(buffers).enumerate() {
            if let Some(writer) = writer {
                writer
                    .write_all(&buf[..len])
                    .map_err(|e| StreamError::write(i, e))?;
            }
        }
        Ok(())
    }
}

/// Write `len` bytes from each buffer to the corresponding writer.
fn write_block<W: std::io::Write>(
    writers: &mut [W],
    buffers: &[Vec<u8>],
    len: usize,
    shard_offset: usize,
) -> Result<(), StreamError> {
    for (i, (writer, buf)) in writers.iter_mut().zip(buffers.iter()).enumerate() {
        let mut written = 0;
        while written < len {
            match writer.write(&buf[written..len]) {
                Ok(0) => {
                    return Err(StreamError::write(
                        shard_offset + i,
                        std::io::Error::new(std::io::ErrorKind::WriteZero, "write returned 0"),
                    ));
                }
                Ok(n) => written += n,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(StreamError::write(shard_offset + i, e)),
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Parallel I/O helpers (rayon)
// ---------------------------------------------------------------------------

/// Parallel version of `write_block` — writes all shards concurrently.
fn write_block_par<W: std::io::Write + Send>(
    writers: &mut [W],
    buffers: &[Vec<u8>],
    len: usize,
    shard_offset: usize,
) -> Result<(), StreamError> {
    use rayon::prelude::*;

    let first_error: std::sync::Mutex<Option<StreamError>> = std::sync::Mutex::new(None);

    writers
        .par_iter_mut()
        .zip(buffers.par_iter())
        .enumerate()
        .try_for_each(|(i, (writer, buf))| {
            let mut written = 0;
            while written < len {
                match writer.write(&buf[written..len]) {
                    Ok(0) => {
                        record_min_error(
                            &first_error,
                            StreamError::write(
                                shard_offset + i,
                                std::io::Error::new(
                                    std::io::ErrorKind::WriteZero,
                                    "write returned 0",
                                ),
                            ),
                        );
                        return Err(());
                    }
                    Ok(n) => written += n,
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(e) => {
                        record_min_error(&first_error, StreamError::write(shard_offset + i, e));
                        return Err(());
                    }
                }
            }
            Ok(())
        })
        .map_err(|()| {
            // The fallback must also be a write error (not a fabricated read(0))
            // and point at this batch's starting shard index.
            take_stream_error(&first_error, || {
                StreamError::write(
                    shard_offset,
                    Error::other("parallel stream writer error was not reported"),
                )
            })
        })
}

// ---------------------------------------------------------------------------
// ReedSolomon streaming methods
// ---------------------------------------------------------------------------

/// A stream has a fixed missing-shard layout and codec, so only its byte views
/// change between blocks. Keep the decode matrix alive independently of the
/// codec's LRU cache, including when that cache is disabled or evicts entries.
struct StreamReconstructPlan<'codec> {
    codec: &'codec super::ReedSolomon<crate::galois_8::Field>,
    plan: ClassicReconstructPlan<crate::galois_8::Field>,
}

impl<'codec> StreamReconstructPlan<'codec> {
    fn new(
        codec: &'codec super::ReedSolomon<crate::galois_8::Field>,
        missing_indices: &[usize],
        shard_len: usize,
    ) -> Result<Self, crate::Error> {
        let mut missing = missing_indices.iter().copied().peekable();
        let lengths = (0..codec.total_shard_count()).map(|index| {
            if missing.peek() == Some(&index) {
                missing.next();
                None
            } else {
                Some(shard_len)
            }
        });
        let shape = codec.classic_reconstruct_shape(lengths)?;
        let plan = codec.prepare_classic_reconstruct_plan(shape, ReconstructionGoal::All)?;
        Ok(Self { codec, plan })
    }

    /// All present buffers have already passed the equal-length read check,
    /// and missing buffers have been resized to the same block length.
    fn reconstruct_block(
        &self,
        buffers: &mut [Vec<u8>],
        shard_len: usize,
        data_rows: &ClassicReconstructRows<'_, crate::galois_8::Field>,
        parity_rows: &ClassicReconstructRows<'_, crate::galois_8::Field>,
        data_outputs: &mut SmallVec<[Vec<u8>; 32]>,
    ) {
        let missing_data = self.plan.data_targets().len();
        let missing_parity = self.plan.parity_targets().len();
        self.codec.record_reconstruct_runtime(
            false,
            missing_data,
            missing_parity,
            missing_data + missing_parity == 0,
        );

        if missing_data != 0 {
            debug_assert!(data_outputs.is_empty());
            // Move only missing owners out so survivors can be borrowed directly.
            // Vec moves transfer metadata, without copying payloads or aliases.
            for &index in self.plan.data_targets() {
                data_outputs.push(core::mem::take(&mut buffers[index]));
            }
            self.codec
                .record_reconstruct_data_stage_runtime(shard_len, missing_data);
            self.codec.code_selected_inputs(
                data_rows,
                buffers,
                &self.plan.shape.valid_indices,
                data_outputs.as_mut_slice(),
            );
            // Restore canonical ownership before parity reads, I/O or probes.
            for (&index, output) in self.plan.data_targets().iter().zip(data_outputs.drain(..)) {
                buffers[index] = output;
            }
        }

        if missing_parity != 0 {
            // The data phase's views have ended; restored data can now be read
            // while only the disjoint parity buffers are mutated.
            let (data, parity) = buffers.split_at_mut(self.codec.data_shard_count());
            self.codec
                .record_reconstruct_parity_stage_runtime(shard_len, missing_parity);
            self.codec.code_selected_slices(
                parity_rows,
                data,
                parity,
                self.plan.parity_targets(),
                self.codec.data_shard_count(),
            );
        }
    }
}

impl super::ReedSolomon<crate::galois_8::Field> {
    /// Stream-encode data from readers into parity writers.
    ///
    /// Reads data shards in blocks of `options.block_size` bytes, encodes
    /// each block, and writes the resulting parity blocks.  This avoids
    /// loading the entire dataset into memory.
    ///
    /// All data streams should supply the same number of bytes. When they do
    /// not, shorter streams are zero-padded to the longest stream's length for
    /// each block, and no original-length metadata is persisted. Verification
    /// and reconstruction require those zero-padded, equal-length shards;
    /// callers must track the original shard lengths out of band to remove
    /// padding after recovery.
    ///
    /// # Errors
    ///
    /// Returns [`StreamError`] on I/O failure or codec error. Incorrect reader or
    /// writer counts return [`crate::Error::TooFewShards`] or
    /// [`crate::Error::TooManyShards`] before any I/O.
    ///
    /// # Limitations
    ///
    /// Only the classic Reed-Solomon family is supported. Leopard-family codecs
    /// return [`crate::Error::UnsupportedCodecFamily`] (wrapped in a
    /// [`StreamError`]), because their FFT engines require whole-shard,
    /// 64-byte-aligned processing that the block-wise streaming path cannot
    /// satisfy.
    pub fn encode_stream(
        &self,
        data: &mut [impl std::io::Read + Send],
        parity: &mut [impl std::io::Write + Send],
        options: &StreamOptions,
    ) -> Result<(), StreamError> {
        // block_size is a public field and may bypass with_block_size's clamp
        // (e.g. a struct literal). Clamp it to the valid range here: 0 would be
        // raised to the lower bound (otherwise reads hit EOF immediately and
        // silently produce empty output), and a huge value is capped to the
        // upper bound (otherwise Vec::with_capacity may OOM).
        let block_size = options
            .block_size
            .clamp(MIN_BLOCK_SIZE_BYTES, MAX_BLOCK_SIZE_BYTES);
        // Leopard families (GF8/GF16, both built on ReedSolomon<galois_8::Field>)
        // dispatch to FFT codecs that require whole-shard, 64-byte-aligned
        // processing: a non-64-multiple final block fails with IncorrectShardSize,
        // so streaming is effectively unusable. Reject them explicitly, matching
        // the documented limitation.
        if self.is_leopard_gf8_family() || self.is_leopard_gf16_family() {
            return Err(StreamError::codec(0, crate::Error::UnsupportedCodecFamily));
        }

        let data_count = self.data_shard_count();
        let parity_count = self.parity_shard_count();
        let use_parallel_read = use_parallel_stream_io(options, data_count);
        let use_parallel_write = use_parallel_stream_io(options, parity_count);

        // Validate stream counts at runtime (debug_assert is removed in release
        // builds, which would otherwise lead to slice out-of-bounds panics or
        // silent partial encoding). Consistent with the crate's
        // check_piece_count! used on non-streaming paths.
        validate_stream_count(data.len(), data_count)?;
        validate_stream_count(parity.len(), parity_count)?;

        let mut data_bufs: Vec<_> = (0..data_count)
            .map(|_| StreamReadBuffer::new(block_size))
            .collect();
        let mut parity_bufs: Vec<Vec<u8>> = (0..parity_count)
            .map(|_| Vec::with_capacity(block_size))
            .collect();

        loop {
            let (all_eof, actual_len) =
                read_block_with_mode(data, &mut data_bufs, block_size, use_parallel_read)?;
            if all_eof {
                break;
            }

            // Resize parity buffers to match actual length.
            for buf in parity_bufs.iter_mut() {
                buf.resize(actual_len, 0);
            }

            self.encode_sep_opt(&data_bufs, &mut parity_bufs)
                .map_err(|e| StreamError::codec(0, e))?;

            // Write parity using the selected stream I/O mode.
            write_block_with_mode(
                parity,
                &parity_bufs,
                actual_len,
                data_count,
                use_parallel_write,
            )?;
        }

        Ok(())
    }

    /// Stream-verify data + parity from readers.
    ///
    /// Reads all shards in blocks, verifying each block independently.
    /// Returns `Ok(true)` if every block is valid, `Ok(false)` if any block
    /// fails verification. All shard streams must have the same length;
    /// truncated streams return [`crate::Error::IncorrectShardSize`]. Incorrect
    /// shard counts return [`crate::Error::TooFewShards`] or
    /// [`crate::Error::TooManyShards`] before any I/O.
    ///
    /// # Limitations
    ///
    /// Only the classic Reed-Solomon family is supported; Leopard-family codecs
    /// return [`crate::Error::UnsupportedCodecFamily`].
    pub fn verify_stream(
        &self,
        shards: &mut [impl std::io::Read + Send],
        options: &StreamOptions,
    ) -> Result<bool, StreamError> {
        // block_size is a public field and may bypass with_block_size's clamp
        // (e.g. a struct literal). Clamp it to the valid range here: 0 would be
        // raised to the lower bound (otherwise reads hit EOF immediately and
        // silently produce empty output), and a huge value is capped to the
        // upper bound (otherwise Vec::with_capacity may OOM).
        let block_size = options
            .block_size
            .clamp(MIN_BLOCK_SIZE_BYTES, MAX_BLOCK_SIZE_BYTES);
        // Leopard families are unsupported for streaming (see encode_stream).
        if self.is_leopard_gf8_family() || self.is_leopard_gf16_family() {
            return Err(StreamError::codec(0, crate::Error::UnsupportedCodecFamily));
        }

        let total = self.total_shard_count();
        let use_parallel_read = use_parallel_stream_io(options, total);

        // Validate the shard count at runtime (debug_assert is removed in
        // release builds).
        validate_stream_count(shards.len(), total)?;

        let mut bufs: Vec<_> = (0..total)
            .map(|_| StreamReadBuffer::new(block_size))
            .collect();
        let mut verify_bufs: Vec<Vec<u8>> =
            (0..self.parity_shard_count()).map(|_| Vec::new()).collect();

        loop {
            let (all_eof, actual_len) =
                read_block_with_mode(shards, &mut bufs, block_size, use_parallel_read)?;
            if all_eof {
                break;
            }

            if let Some(bad) = bufs.iter().position(|buffer| buffer.read_len != actual_len) {
                return Err(StreamError::codec(bad, crate::Error::IncorrectShardSize));
            }

            for buf in &mut verify_bufs {
                buf.resize(actual_len, 0);
            }

            let valid = self
                .verify_with_buffer_opt(&bufs, &mut verify_bufs)
                .map_err(|e| StreamError::codec(0, e))?;
            if !valid {
                return Ok(false);
            }
        }

        Ok(true)
    }

    /// Stream-reconstruct missing shards.
    ///
    /// `shards` has one entry per total shard.  Present shards contain their
    /// data in a `Cursor<Vec<u8>>`; missing shards use an empty cursor
    /// (`Cursor::new(Vec::new())`).  Recovered data is written into the
    /// missing shards' cursors.
    ///
    /// Present cursors are read from the beginning: their position is reset to
    /// `0` before reading, so it is safe to pass cursors that were just written
    /// to (whose position sits at the end).
    ///
    /// The function reads blocks from present shards, reconstructs missing
    /// blocks, and writes recovered data into the missing cursors.  The set
    /// of missing shard indices must be consistent across all blocks. The
    /// decode plan and scratch buffers are reused across those blocks.
    /// For independent readers and writers, use [`Self::reconstruct_stream_from`].
    ///
    /// # Limitations
    ///
    /// Only the classic Reed-Solomon family is supported; Leopard-family codecs
    /// return [`crate::Error::UnsupportedCodecFamily`] rather than silently
    /// producing incorrect results.
    pub fn reconstruct_stream(
        &self,
        shards: &mut [std::io::Cursor<Vec<u8>>],
        options: &StreamOptions,
    ) -> Result<(), StreamError> {
        let total = self.total_shard_count();

        // Validate the shard count at runtime (debug_assert is removed in
        // release builds).
        validate_stream_count(shards.len(), total)?;

        // Leopard families are unsupported for streaming (see encode_stream).
        if self.is_leopard_gf8_family() || self.is_leopard_gf16_family() {
            return Err(StreamError::codec(0, crate::Error::UnsupportedCodecFamily));
        }

        let mut present_count = 0;
        let mut known_len = 0;
        let mut equal_lengths = true;
        for shard in shards.iter() {
            let len = shard.get_ref().len();
            if len != 0 {
                present_count += 1;
                if known_len == 0 {
                    known_len = len;
                }
                equal_lengths &= known_len == len;
            }
        }
        // An empty dataset (no present shards) has nothing to recover; return Ok,
        // consistent with encode_stream returning Ok for empty input (rather than
        // TooFewShardsPresent).
        if present_count == 0 {
            return Ok(());
        }
        if present_count < self.data_shard_count() {
            return Err(StreamError::codec(0, crate::Error::TooFewShardsPresent));
        }

        // An inconsistent layout must keep its existing block-by-block error
        // and partial-output behavior, so it receives no capacity hint.
        let output_len = if equal_lengths { known_len } else { 0 };
        let mut readers: SmallVec<[Option<&mut std::io::Cursor<Vec<u8>>>; 32]> =
            SmallVec::with_capacity(total);
        let mut writers: SmallVec<[Option<RecoveryVecWriter<'_>>; 32]> =
            SmallVec::with_capacity(total);
        for shard in shards {
            // Each cursor contributes only one mutable reference. Writing via
            // the underlying Vec preserves the legacy missing-cursor position.
            if shard.get_ref().is_empty() {
                readers.push(None);
                writers.push(Some(RecoveryVecWriter {
                    bytes: shard.get_mut(),
                    reserve_len: output_len,
                }));
            } else {
                shard.set_position(0);
                readers.push(Some(shard));
                writers.push(None);
            }
        }
        self.reconstruct_stream_from(&mut readers, &mut writers, options)
    }

    /// Reconstruct missing shards from independent readers into independent writers.
    ///
    /// Both slices have one entry per total shard. `Some(reader)` marks a
    /// present shard, including an empty stream; its writer must be `None`.
    /// `None` marks a missing shard and requires `Some(writer)` at that index.
    /// At least [`Self::data_shard_count`] readers must be present. Unlike
    /// [`Self::reconstruct_stream`], an all-`None` reader layout is an error.
    ///
    /// Readers are consumed from their current positions without seeking.
    /// All present streams must have equal lengths; short reads and
    /// [`std::io::ErrorKind::Interrupted`] are retried. Each block is fully read
    /// and length-checked before any of its recovered bytes are written. The
    /// decode plan and block buffers are reused, keeping scratch memory bounded
    /// by the shard count and clamped block size, independent of stream length.
    /// `options.io_mode` controls both reading and writing; compute remains serial.
    ///
    /// Writers receive recovered bytes through `write_all`; they are not
    /// flushed. On error, readers may have advanced and writers may contain
    /// earlier blocks and some or all of the current block. Outputs are not rolled back,
    /// and retrying requires callers to restore the stream positions themselves.
    ///
    /// # Errors
    ///
    /// Counts, codec family, reader/writer layout, and the number of available
    /// shards are checked before any I/O. Invalid layouts return
    /// [`crate::Error::InvalidShardFlags`] with the offending shard index.
    /// Unequal lengths return [`crate::Error::IncorrectShardSize`]. Read and
    /// write failures retain their shard index and underlying I/O error.
    /// Only Classic codecs are supported; Leopard returns
    /// [`crate::Error::UnsupportedCodecFamily`].
    ///
    /// # Example
    ///
    /// ```no_run
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// use rustfs_erasure_codec::{galois_8::ReedSolomon, stream::StreamOptions};
    /// use std::fs::File;
    ///
    /// let codec = ReedSolomon::new(2, 1)?;
    /// let mut readers = [None, Some(File::open("data1")?), Some(File::open("parity0")?)];
    /// let mut writers = [Some(File::create("data0.recovered")?), None, None];
    /// codec.reconstruct_stream_from(&mut readers, &mut writers, &StreamOptions::default())?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn reconstruct_stream_from<R: std::io::Read + Send, W: std::io::Write + Send>(
        &self,
        readers: &mut [Option<R>],
        writers: &mut [Option<W>],
        options: &StreamOptions,
    ) -> Result<(), StreamError> {
        let total = self.total_shard_count();
        for count in [readers.len(), writers.len()] {
            validate_stream_count(count, total)?;
        }
        if self.is_leopard_gf8_family() || self.is_leopard_gf16_family() {
            return Err(StreamError::codec(0, crate::Error::UnsupportedCodecFamily));
        }
        let mut missing_indices: SmallVec<[usize; 32]> = SmallVec::new();
        for (index, (reader, writer)) in readers.iter().zip(writers.iter()).enumerate() {
            if reader.is_some() == writer.is_some() {
                return Err(StreamError::codec(index, crate::Error::InvalidShardFlags));
            }
            if reader.is_none() {
                missing_indices.push(index);
            }
        }
        let present_count = total - missing_indices.len();
        if present_count < self.data_shard_count() {
            return Err(StreamError::codec(0, crate::Error::TooFewShardsPresent));
        }

        let block_size = options
            .block_size
            .clamp(MIN_BLOCK_SIZE_BYTES, MAX_BLOCK_SIZE_BYTES);
        let use_parallel_read = use_parallel_stream_io(options, present_count);
        let use_parallel_write = use_parallel_stream_io(options, missing_indices.len());

        // Retain both present and missing buffers between blocks.
        let mut bufs: Vec<Vec<u8>> = readers
            .iter()
            .map(|reader| {
                if reader.is_some() {
                    Vec::with_capacity(block_size)
                } else {
                    Vec::new()
                }
            })
            .collect();

        let (all_eof, mut actual_len) =
            read_present_streams_with_mode(readers, &mut bufs, block_size, use_parallel_read)?;
        if all_eof {
            return Ok(());
        }

        let plan = StreamReconstructPlan::new(self, &missing_indices, actual_len)
            .map_err(|e| StreamError::codec(0, e))?;
        let mut data_outputs: SmallVec<[Vec<u8>; 32]> =
            SmallVec::with_capacity(plan.plan.data_targets().len());
        let rows = plan.plan.row_views(self);
        let (data_rows, parity_rows) = rows.split_at(plan.plan.data_targets().len());

        loop {
            // Resize the retained missing buffers for the final partial block.
            // Present buffers already have their validated read length.
            for &idx in &missing_indices {
                bufs[idx].resize(actual_len, 0);
            }

            plan.reconstruct_block(
                &mut bufs,
                actual_len,
                &data_rows,
                &parity_rows,
                &mut data_outputs,
            );

            write_missing_streams_with_mode(writers, &bufs, actual_len, use_parallel_write)?;

            let (all_eof, next_len) =
                read_present_streams_with_mode(readers, &mut bufs, block_size, use_parallel_read)?;
            if all_eof {
                break;
            }
            actual_len = next_len;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::galois_8::ReedSolomon;

    fn make_codec(data: usize, parity: usize) -> ReedSolomon {
        ReedSolomon::new(data, parity).unwrap()
    }

    fn random_data(len: usize) -> Vec<u8> {
        // Simple deterministic fill for reproducibility.
        (0..len)
            .map(|i| (i.wrapping_mul(73).wrapping_add(17)) as u8)
            .collect()
    }

    #[test]
    fn test_encode_stream_basic() {
        let rs = make_codec(3, 2);
        let shard_size = 4096;

        let d0 = random_data(shard_size);
        let d1 = random_data(shard_size);
        let d2 = random_data(shard_size);

        let mut readers: Vec<&[u8]> = vec![d0.as_slice(), d1.as_slice(), d2.as_slice()];
        let mut writers: Vec<Vec<u8>> = vec![Vec::new(), Vec::new()];

        rs.encode_stream(&mut readers, &mut writers, &StreamOptions::default())
            .unwrap();

        assert_eq!(writers[0].len(), shard_size);
        assert_eq!(writers[1].len(), shard_size);

        // Verify parity is correct by verifying the full shard set.
        let all: Vec<&[u8]> = vec![
            d0.as_slice(),
            d1.as_slice(),
            d2.as_slice(),
            writers[0].as_slice(),
            writers[1].as_slice(),
        ];
        assert!(rs.verify(&all).unwrap());
    }

    #[test]
    fn test_encode_stream_multi_block() {
        let rs = make_codec(3, 2);
        let total_size = 10 * 1024; // 10 KiB
        let block_size = 4096;

        let d0 = random_data(total_size);
        let d1 = random_data(total_size);
        let d2 = random_data(total_size);

        let mut readers: Vec<&[u8]> = vec![d0.as_slice(), d1.as_slice(), d2.as_slice()];
        let mut writers: Vec<Vec<u8>> = vec![Vec::new(), Vec::new()];

        let opts = StreamOptions::new().with_block_size(block_size);
        rs.encode_stream(&mut readers, &mut writers, &opts).unwrap();

        assert_eq!(writers[0].len(), total_size);
        assert_eq!(writers[1].len(), total_size);
    }

    #[test]
    fn test_encode_stream_empty() {
        let rs = make_codec(3, 2);
        let empty: Vec<&[u8]> = vec![&[], &[], &[]];
        let mut readers = empty.clone();
        let mut writers: Vec<Vec<u8>> = vec![Vec::new(), Vec::new()];

        rs.encode_stream(&mut readers, &mut writers, &StreamOptions::default())
            .unwrap();

        // Empty input → empty output.
        assert!(writers[0].is_empty());
        assert!(writers[1].is_empty());
    }

    #[test]
    fn test_encode_stream_unequal_lengths() {
        let rs = make_codec(3, 2);

        let d0 = random_data(1000);
        let d1 = random_data(500);
        let d2 = random_data(800);

        let mut readers: Vec<&[u8]> = vec![d0.as_slice(), d1.as_slice(), d2.as_slice()];
        let mut writers: Vec<Vec<u8>> = vec![Vec::new(), Vec::new()];

        rs.encode_stream(&mut readers, &mut writers, &StreamOptions::default())
            .unwrap();

        // All outputs should be 1000 bytes (max of input lengths, since we
        // zero-pad short shards).
        assert_eq!(writers[0].len(), 1000);
        assert_eq!(writers[1].len(), 1000);
    }

    #[test]
    fn test_encode_stream_10x4() {
        let rs = make_codec(10, 4);
        let shard_size = 64 * 1024;

        let data: Vec<Vec<u8>> = (0..10).map(|_| random_data(shard_size)).collect();
        let mut readers: Vec<&[u8]> = data.iter().map(|d| d.as_slice()).collect();
        let mut writers: Vec<Vec<u8>> = vec![Vec::new(); 4];

        rs.encode_stream(&mut readers, &mut writers, &StreamOptions::default())
            .unwrap();

        // Verify.
        let mut all: Vec<&[u8]> = data.iter().map(|d| d.as_slice()).collect();
        for w in &writers {
            all.push(w.as_slice());
        }
        assert!(rs.verify(&all).unwrap());
    }

    #[test]
    fn test_verify_stream_valid() {
        let rs = make_codec(3, 2);
        let shard_size = 4096;

        let d = vec![random_data(shard_size); 3];
        let mut readers: Vec<&[u8]> = d.iter().map(|s| s.as_slice()).collect();
        let mut writers: Vec<Vec<u8>> = vec![Vec::new(); 2];
        rs.encode_stream(&mut readers, &mut writers, &StreamOptions::default())
            .unwrap();

        // Build full shard set for verification.
        let mut all_data: Vec<&[u8]> = d.iter().map(|s| s.as_slice()).collect();
        for w in &writers {
            all_data.push(w.as_slice());
        }
        let mut all_readers: Vec<&[u8]> = all_data;

        assert!(
            rs.verify_stream(&mut all_readers, &StreamOptions::default())
                .unwrap()
        );
    }

    #[test]
    fn test_verify_stream_corrupted() {
        let rs = make_codec(3, 2);
        let shard_size = 4096;

        let d = vec![random_data(shard_size); 3];
        let mut readers: Vec<&[u8]> = d.iter().map(|s| s.as_slice()).collect();
        let mut writers: Vec<Vec<u8>> = vec![Vec::new(); 2];
        rs.encode_stream(&mut readers, &mut writers, &StreamOptions::default())
            .unwrap();

        // Corrupt a parity shard.
        writers[0][0] ^= 0xFF;

        let mut all_data: Vec<&[u8]> = d.iter().map(|s| s.as_slice()).collect();
        for w in &writers {
            all_data.push(w.as_slice());
        }
        let mut all_readers: Vec<&[u8]> = all_data;

        assert!(
            !rs.verify_stream(&mut all_readers, &StreamOptions::default())
                .unwrap()
        );
    }

    #[test]
    fn test_reconstruct_stream_single_missing() {
        let rs = make_codec(3, 2);
        let shard_size = 4096;

        let d: Vec<Vec<u8>> = (0..3).map(|_| random_data(shard_size)).collect();
        let mut readers: Vec<&[u8]> = d.iter().map(|s| s.as_slice()).collect();
        let mut parity_writers: Vec<Vec<u8>> = vec![Vec::new(); 2];
        rs.encode_stream(&mut readers, &mut parity_writers, &StreamOptions::default())
            .unwrap();

        // data[0] is missing — use empty Cursor.
        let mut shards: Vec<std::io::Cursor<Vec<u8>>> = vec![
            std::io::Cursor::new(Vec::new()), // missing
            std::io::Cursor::new(d[1].clone()),
            std::io::Cursor::new(d[2].clone()),
            std::io::Cursor::new(parity_writers[0].clone()),
            std::io::Cursor::new(parity_writers[1].clone()),
        ];

        rs.reconstruct_stream(&mut shards, &StreamOptions::default())
            .unwrap();

        // Shard 0 should have been recovered into the cursor's inner Vec.
        assert_eq!(shards[0].get_ref(), &d[0]);
    }

    #[test]
    fn test_reconstruct_non_streaming() {
        // Verify basic encode + reconstruct works without streaming.
        let rs = make_codec(3, 2);
        let shard_size = 4096;

        let d: Vec<Vec<u8>> = (0..3).map(|_| random_data(shard_size)).collect();
        let data_refs: Vec<&[u8]> = d.iter().map(|s| s.as_slice()).collect();
        let mut p0 = vec![0u8; shard_size];
        let mut p1 = vec![0u8; shard_size];
        let mut parity_refs: Vec<&mut [u8]> = vec![&mut p0, &mut p1];
        rs.encode_sep(&data_refs, &mut parity_refs).unwrap();

        // Now reconstruct with shard 0 missing.
        let mut shards: Vec<Option<Vec<u8>>> = vec![
            None,
            Some(d[1].clone()),
            Some(d[2].clone()),
            Some(p0.clone()),
            Some(p1.clone()),
        ];
        rs.reconstruct(&mut shards).unwrap();

        assert_eq!(shards[0].as_ref().unwrap(), &d[0]);
    }

    #[test]
    fn test_reconstruct_stream_basic() {
        let rs = make_codec(3, 2);
        let shard_size = 64;

        let d: Vec<Vec<u8>> = (0..3).map(|_| random_data(shard_size)).collect();
        let mut readers: Vec<&[u8]> = d.iter().map(|s| s.as_slice()).collect();
        let mut parity_writers: Vec<Vec<u8>> = vec![Vec::new(); 2];
        rs.encode_stream(&mut readers, &mut parity_writers, &StreamOptions::default())
            .unwrap();

        // Verify parity is correct.
        let all: Vec<&[u8]> = vec![
            d[0].as_slice(),
            d[1].as_slice(),
            d[2].as_slice(),
            parity_writers[0].as_slice(),
            parity_writers[1].as_slice(),
        ];
        assert!(rs.verify(&all).unwrap());

        // Missing shard 0 — empty Cursor; present shards have data.
        let mut shards: Vec<std::io::Cursor<Vec<u8>>> = vec![
            std::io::Cursor::new(Vec::new()), // missing
            std::io::Cursor::new(d[1].clone()),
            std::io::Cursor::new(d[2].clone()),
            std::io::Cursor::new(parity_writers[0].clone()),
            std::io::Cursor::new(parity_writers[1].clone()),
        ];

        rs.reconstruct_stream(&mut shards, &StreamOptions::default())
            .unwrap();

        // Shard 0 recovered into the empty cursor's inner Vec.
        let recovered = shards[0].get_ref();
        assert_eq!(
            recovered.len(),
            d[0].len(),
            "recovered len {} != expected len {}",
            recovered.len(),
            d[0].len()
        );
        assert_eq!(
            recovered,
            &d[0],
            "recovered: {:?}, expected: {:?}",
            &recovered[..8],
            &d[0][..8]
        );
    }

    #[test]
    fn test_stream_options_builder() {
        let opts = StreamOptions::new().with_block_size(1024 * 1024);
        assert_eq!(opts.block_size, 1024 * 1024);
        assert_eq!(opts.io_mode, StreamIoMode::Auto);

        // Minimum is 1 KiB.
        let opts = StreamOptions::new().with_block_size(100);
        assert_eq!(opts.block_size, 1024);

        let opts = StreamOptions::new().with_io_mode(StreamIoMode::Serial);
        assert_eq!(opts.io_mode, StreamIoMode::Serial);
    }

    #[test]
    fn test_stream_io_auto_decision_thresholds() {
        let opts = StreamOptions::new()
            .with_block_size(64 * 1024)
            .with_io_mode(StreamIoMode::Auto);
        assert!(!use_parallel_stream_io(&opts, 14));

        let opts = StreamOptions::new()
            .with_block_size(1024 * 1024)
            .with_io_mode(StreamIoMode::Auto);
        assert!(!use_parallel_stream_io(&opts, 6));

        let opts = StreamOptions::new()
            .with_block_size(4 * 1024 * 1024)
            .with_io_mode(StreamIoMode::Auto);
        assert!(use_parallel_stream_io(&opts, 10));

        let opts = StreamOptions::new()
            .with_block_size(64 * 1024)
            .with_io_mode(StreamIoMode::Parallel);
        assert!(use_parallel_stream_io(&opts, 1));
    }

    #[test]
    fn test_stream_error_display() {
        let e = StreamError::read(
            3,
            std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "eof"),
        );
        let s = format!("{e}");
        assert!(s.contains("shard 3"));
        assert!(s.contains("eof"));

        let e = StreamError::codec(0, crate::Error::TooFewShardsPresent);
        let s = format!("{e}");
        assert!(s.contains("codec"));
    }

    // -----------------------------------------------------------------------
    // Concurrent stream tests (P0-2e-3)
    // -----------------------------------------------------------------------

    #[test]
    fn test_encode_stream_concurrent() {
        let rs = make_codec(4, 2);
        let shard_size = 8 * 1024;

        let data: Vec<Vec<u8>> = (0..4).map(|_| random_data(shard_size)).collect();
        let mut readers: Vec<&[u8]> = data.iter().map(|d| d.as_slice()).collect();
        let mut writers: Vec<Vec<u8>> = vec![Vec::new(); 2];

        rs.encode_stream(&mut readers, &mut writers, &StreamOptions::default())
            .unwrap();

        // Verify parity correctness.
        let mut all: Vec<&[u8]> = data.iter().map(|d| d.as_slice()).collect();
        for w in &writers {
            all.push(w.as_slice());
        }
        assert!(rs.verify(&all).unwrap());
    }

    #[test]
    fn test_verify_stream_concurrent() {
        let rs = make_codec(4, 2);
        let shard_size = 8 * 1024;

        let data: Vec<Vec<u8>> = (0..4).map(|_| random_data(shard_size)).collect();
        let mut readers: Vec<&[u8]> = data.iter().map(|d| d.as_slice()).collect();
        let mut writers: Vec<Vec<u8>> = vec![Vec::new(); 2];
        rs.encode_stream(&mut readers, &mut writers, &StreamOptions::default())
            .unwrap();

        // Valid case.
        let mut all: Vec<&[u8]> = data.iter().map(|d| d.as_slice()).collect();
        for w in &writers {
            all.push(w.as_slice());
        }
        let mut all_readers: Vec<&[u8]> = all;
        assert!(
            rs.verify_stream(&mut all_readers, &StreamOptions::default())
                .unwrap()
        );

        // Corrupted case.
        let mut corrupted = writers[0].clone();
        corrupted[0] ^= 0xFF;
        let mut all_corrupt: Vec<&[u8]> = data.iter().map(|d| d.as_slice()).collect();
        all_corrupt.push(corrupted.as_slice());
        all_corrupt.push(writers[1].as_slice());
        assert!(
            !rs.verify_stream(&mut all_corrupt, &StreamOptions::default())
                .unwrap()
        );
    }

    #[test]
    fn test_reconstruct_stream_concurrent() {
        let rs = make_codec(4, 2);
        let shard_size = 8 * 1024;

        let data: Vec<Vec<u8>> = (0..4).map(|_| random_data(shard_size)).collect();
        let mut readers: Vec<&[u8]> = data.iter().map(|d| d.as_slice()).collect();
        let mut parity_writers: Vec<Vec<u8>> = vec![Vec::new(); 2];
        rs.encode_stream(&mut readers, &mut parity_writers, &StreamOptions::default())
            .unwrap();

        // Missing shards: data[1] and parity[0].
        let mut shards: Vec<std::io::Cursor<Vec<u8>>> = vec![
            std::io::Cursor::new(data[0].clone()),
            std::io::Cursor::new(Vec::new()), // missing
            std::io::Cursor::new(data[2].clone()),
            std::io::Cursor::new(data[3].clone()),
            std::io::Cursor::new(Vec::new()), // missing
            std::io::Cursor::new(parity_writers[1].clone()),
        ];

        rs.reconstruct_stream(&mut shards, &StreamOptions::default())
            .unwrap();

        assert_eq!(shards[1].get_ref(), &data[1]);
    }

    #[test]
    fn test_concurrent_stream_large_blocks() {
        let rs = make_codec(10, 4);
        let total_size = 1024 * 1024; // 1 MiB
        let block_size = 256 * 1024; // 256 KiB blocks

        let data: Vec<Vec<u8>> = (0..10).map(|_| random_data(total_size)).collect();
        let mut readers: Vec<&[u8]> = data.iter().map(|d| d.as_slice()).collect();
        let mut writers: Vec<Vec<u8>> = vec![Vec::new(); 4];

        let opts = StreamOptions::new().with_block_size(block_size);
        rs.encode_stream(&mut readers, &mut writers, &opts).unwrap();

        // Verify all blocks.
        let mut all: Vec<&[u8]> = data.iter().map(|d| d.as_slice()).collect();
        for w in &writers {
            all.push(w.as_slice());
        }
        assert!(rs.verify(&all).unwrap());

        // Reconstruct with 2 missing data shards.
        let mut shards: Vec<std::io::Cursor<Vec<u8>>> = Vec::new();
        for d in &data {
            shards.push(std::io::Cursor::new(d.clone()));
        }
        shards[0] = std::io::Cursor::new(Vec::new());
        shards[5] = std::io::Cursor::new(Vec::new());
        for w in &writers {
            shards.push(std::io::Cursor::new(w.clone()));
        }

        rs.reconstruct_stream(&mut shards, &StreamOptions::default())
            .unwrap();

        assert_eq!(shards[0].get_ref(), &data[0]);
        assert_eq!(shards[5].get_ref(), &data[5]);
    }

    #[test]
    fn test_encode_stream_zero_length() {
        let rs = ReedSolomon::new(3, 2).unwrap();
        let mut readers: Vec<&[u8]> = vec![b""; 3];
        let mut writers: Vec<Vec<u8>> = vec![Vec::new(); 2];
        // Zero-length data should succeed (encode produces empty parity).
        rs.encode_stream(&mut readers, &mut writers, &StreamOptions::default())
            .unwrap();
        assert!(writers.iter().all(|w| w.is_empty()));
    }

    #[test]
    fn test_encode_stream_single_byte_block() {
        let rs = ReedSolomon::new(2, 1).unwrap();
        let d0 = vec![0xABu8; 4];
        let d1 = vec![0xCDu8; 4];
        let mut readers: Vec<&[u8]> = vec![d0.as_slice(), d1.as_slice()];
        let mut writers: Vec<Vec<u8>> = vec![Vec::new(); 1];
        let opts = StreamOptions::new().with_block_size(1);
        rs.encode_stream(&mut readers, &mut writers, &opts).unwrap();

        // Verify with the smallest possible block size.
        let mut all: Vec<&[u8]> = vec![d0.as_slice(), d1.as_slice()];
        for w in &writers {
            all.push(w.as_slice());
        }
        assert!(rs.verify(&all).unwrap());
    }

    #[test]
    fn test_stream_io_modes_encode_verify_match() {
        let rs = ReedSolomon::new(4, 2).unwrap();
        let shard_size = 32 * 1024;
        let data: Vec<Vec<u8>> = (0..4).map(|_| random_data(shard_size)).collect();

        let mut expected_parity = Vec::new();
        for mode in [
            StreamIoMode::Auto,
            StreamIoMode::Serial,
            StreamIoMode::Parallel,
        ] {
            let opts = StreamOptions::new()
                .with_block_size(8 * 1024)
                .with_io_mode(mode);
            let mut readers: Vec<&[u8]> = data.iter().map(|d| d.as_slice()).collect();
            let mut writers: Vec<Vec<u8>> = vec![Vec::new(); 2];

            rs.encode_stream(&mut readers, &mut writers, &opts).unwrap();
            if expected_parity.is_empty() {
                expected_parity = writers.clone();
            } else {
                assert_eq!(writers, expected_parity);
            }

            let mut all: Vec<&[u8]> = data.iter().map(|d| d.as_slice()).collect();
            for parity in &writers {
                all.push(parity.as_slice());
            }
            let mut verify_readers = all;
            assert!(rs.verify_stream(&mut verify_readers, &opts).unwrap());
        }
    }

    #[test]
    fn test_reconstruct_stream_minimum_present() {
        let rs = ReedSolomon::new(3, 2).unwrap();
        let shard_len = 16usize;
        let data: Vec<Vec<u8>> = (0..3).map(|i| vec![i as u8 + 1; shard_len]).collect();

        // Encode to get parity.
        let refs: Vec<&[u8]> = data.iter().map(|d| d.as_slice()).collect();
        let mut parity = vec![vec![0u8; shard_len]; 2];
        let mut par_refs: Vec<&mut [u8]> = parity.iter_mut().map(|p| &mut p[..]).collect();
        rs.encode_sep(&refs, &mut par_refs).unwrap();

        // Keep minimum viable: 3 of 5 shards (data_shard_count).
        // Present: shard 0 (data), shard 3 (parity), shard 4 (parity).
        // Missing: shard 1 (data), shard 2 (data).
        let mut shards: Vec<std::io::Cursor<Vec<u8>>> = vec![
            std::io::Cursor::new(data[0].clone()),
            std::io::Cursor::new(Vec::new()),
            std::io::Cursor::new(Vec::new()),
            std::io::Cursor::new(parity[0].clone()),
            std::io::Cursor::new(parity[1].clone()),
        ];

        rs.reconstruct_stream(&mut shards, &StreamOptions::default())
            .unwrap();

        // Verify reconstructed data shards match originals.
        assert_eq!(shards[1].get_ref(), &data[1], "shard 1 mismatch");
        assert_eq!(shards[2].get_ref(), &data[2], "shard 2 mismatch");
    }

    // -----------------------------------------------------------------------
    // Streaming-path audit regression tests
    // -----------------------------------------------------------------------

    fn leopard_gf8_codec(data: usize, parity: usize) -> ReedSolomon {
        use crate::{CodecFamily, CodecOptions};
        ReedSolomon::with_options(
            data,
            parity,
            CodecOptions {
                codec_family: CodecFamily::LeopardGF8,
                ..CodecOptions::default()
            },
        )
        .unwrap()
    }

    /// A truncated / length-mismatched present shard must error with
    /// `IncorrectShardSize` instead of silently recovering wrong data.
    #[test]
    fn test_reconstruct_stream_truncated_present_errors() {
        let rs = make_codec(3, 2);
        let shard = 4096;
        let d: Vec<Vec<u8>> = (0..3).map(|_| random_data(shard)).collect();
        let mut readers: Vec<&[u8]> = d.iter().map(|s| s.as_slice()).collect();
        let mut parity: Vec<Vec<u8>> = vec![Vec::new(); 2];
        rs.encode_stream(&mut readers, &mut parity, &StreamOptions::default())
            .unwrap();

        // shard0 missing; shard1 truncated to 50 bytes.
        let mut shards: Vec<std::io::Cursor<Vec<u8>>> = vec![
            std::io::Cursor::new(Vec::new()),
            std::io::Cursor::new(d[1][..50].to_vec()),
            std::io::Cursor::new(d[2].clone()),
            std::io::Cursor::new(parity[0].clone()),
            std::io::Cursor::new(parity[1].clone()),
        ];
        let err = rs
            .reconstruct_stream(&mut shards, &StreamOptions::default())
            .unwrap_err();
        assert!(matches!(
            err.kind,
            StreamErrorKind::Codec(crate::Error::IncorrectShardSize)
        ));
    }

    /// A present cursor whose position is not 0 (written first) still recovers
    /// correctly.
    #[test]
    fn test_reconstruct_stream_nonzero_cursor_position() {
        use std::io::Write;
        let rs = make_codec(3, 2);
        let shard = 4096;
        let d: Vec<Vec<u8>> = (0..3).map(|_| random_data(shard)).collect();
        let mut readers: Vec<&[u8]> = d.iter().map(|s| s.as_slice()).collect();
        let mut parity: Vec<Vec<u8>> = vec![Vec::new(); 2];
        rs.encode_stream(&mut readers, &mut parity, &StreamOptions::default())
            .unwrap();

        let mk = |bytes: &[u8]| {
            let mut c = std::io::Cursor::new(Vec::new());
            c.write_all(bytes).unwrap(); // position left at the end
            c
        };
        let mut shards = vec![
            std::io::Cursor::new(Vec::new()), // missing
            mk(&d[1]),
            mk(&d[2]),
            mk(&parity[0]),
            mk(&parity[1]),
        ];
        rs.reconstruct_stream(&mut shards, &StreamOptions::default())
            .unwrap();
        assert_eq!(shards[0].get_ref(), &d[0]);
    }

    /// `block_size = 0` (public field) is clamped and encodes normally instead
    /// of silently producing empty output.
    #[test]
    fn test_encode_stream_zero_block_size_clamped() {
        let rs = make_codec(3, 2);
        let shard = 4096;
        let d: Vec<Vec<u8>> = (0..3).map(|_| random_data(shard)).collect();
        let mut readers: Vec<&[u8]> = d.iter().map(|s| s.as_slice()).collect();
        let mut writers: Vec<Vec<u8>> = vec![Vec::new(); 2];
        let opts = StreamOptions {
            block_size: 0,
            io_mode: StreamIoMode::Serial,
        };
        rs.encode_stream(&mut readers, &mut writers, &opts).unwrap();
        assert_eq!(writers[0].len(), shard);
        assert_eq!(writers[1].len(), shard);
    }

    /// A mismatched stream count returns an error at runtime (no panic, no
    /// reliance on `debug_assert`).
    #[test]
    fn test_encode_stream_wrong_count_errors() {
        let rs = make_codec(3, 2);
        let d0 = random_data(64);
        let mut readers: Vec<&[u8]> = vec![d0.as_slice(), d0.as_slice()]; // one short
        let mut writers: Vec<Vec<u8>> = vec![Vec::new(); 2];
        let err = rs
            .encode_stream(&mut readers, &mut writers, &StreamOptions::default())
            .unwrap_err();
        assert!(matches!(
            err.kind,
            StreamErrorKind::Codec(crate::Error::TooFewShards)
        ));
    }

    /// Leopard families are rejected by all three streaming methods with
    /// `UnsupportedCodecFamily`.
    #[test]
    fn test_stream_rejects_leopard_family() {
        let rs = leopard_gf8_codec(4, 2);
        let d: Vec<Vec<u8>> = (0..6).map(|_| random_data(128)).collect();

        let mut enc_readers: Vec<&[u8]> = d[..4].iter().map(|s| s.as_slice()).collect();
        let mut writers: Vec<Vec<u8>> = vec![Vec::new(); 2];
        let err = rs
            .encode_stream(&mut enc_readers, &mut writers, &StreamOptions::default())
            .unwrap_err();
        assert!(matches!(
            err.kind,
            StreamErrorKind::Codec(crate::Error::UnsupportedCodecFamily)
        ));

        let mut ver_readers: Vec<&[u8]> = d.iter().map(|s| s.as_slice()).collect();
        let err = rs
            .verify_stream(&mut ver_readers, &StreamOptions::default())
            .unwrap_err();
        assert!(matches!(
            err.kind,
            StreamErrorKind::Codec(crate::Error::UnsupportedCodecFamily)
        ));

        let mut rec_shards: Vec<std::io::Cursor<Vec<u8>>> =
            d.iter().map(|s| std::io::Cursor::new(s.clone())).collect();
        let err = rs
            .reconstruct_stream(&mut rec_shards, &StreamOptions::default())
            .unwrap_err();
        assert!(matches!(
            err.kind,
            StreamErrorKind::Codec(crate::Error::UnsupportedCodecFamily)
        ));
    }

    /// An empty dataset (all empty cursors) reconstructs to Ok, consistent with
    /// `encode_stream` on empty input.
    #[test]
    fn test_reconstruct_stream_empty_dataset_ok() {
        let rs = make_codec(3, 2);
        let mut shards: Vec<std::io::Cursor<Vec<u8>>> =
            (0..5).map(|_| std::io::Cursor::new(Vec::new())).collect();
        rs.reconstruct_stream(&mut shards, &StreamOptions::default())
            .unwrap();
        assert!(shards.iter().all(|c| c.get_ref().is_empty()));
    }
}
