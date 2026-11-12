use super::{
    MAX_BLOCK_SIZE_BYTES, MIN_BLOCK_SIZE_BYTES, RecoveryVecWriter, StreamError, StreamOptions,
    StreamReadBuffer, StreamReconstructPlan, read_block_with_mode, read_present_streams_with_mode,
    use_parallel_stream_io, validate_stream_count, write_block_with_mode,
    write_missing_streams_with_mode,
};
use crate::core::{ClassicReconstructPlan, EncodingFormat};
use crate::galois_8::{Field, ReedSolomon};
use smallvec::SmallVec;

struct CachedRecoveryPlan {
    format: EncodingFormat<Field>,
    plan: ClassicReconstructPlan<Field>,
}

/// Caller-owned buffers and a recovery plan reused across stream operations.
///
/// Pass this workspace to a codec's `*_stream_with_workspace` methods. Storage
/// is allocated lazily and adapts to the current shard counts and block size.
/// Encoding and verification share read and parity buffers; recovery retains
/// its own buffers and the last effective format and missing-shard plan. The
/// plan can be reused with another block size or stream length because each
/// block's actual lengths are checked before execution.
///
/// Buffers retain their high-water capacities until cleared. Capacities remain
/// available after successful calls, verification failures
/// and I/O errors. Use [`Self::clear`] to release them. The workspace owns no
/// readers, writers or references to caller data. Each concurrent operation
/// needs its own workspace. After an I/O error, callers must restore reader
/// positions and output contents before retrying, as for the ordinary stream
/// methods. Only Classic GF8 stream codecs are supported.
#[derive(Default)]
pub struct StreamWorkspace {
    read_buffers: Vec<StreamReadBuffer>,
    parity_buffers: Vec<Vec<u8>>,
    recovery_buffers: Vec<Vec<u8>>,
    missing_indices: SmallVec<[usize; 32]>,
    recovery_plan: Option<CachedRecoveryPlan>,
    data_outputs: SmallVec<[Vec<u8>; 32]>,
}

impl StreamWorkspace {
    /// Create an empty workspace; buffers are initialized on first use.
    pub fn new() -> Self {
        Self::default()
    }

    /// Release all retained buffers, metadata and the cached recovery plan.
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    fn prepare_read_buffers(&mut self, count: usize, parity: usize, block_size: usize) {
        if self.read_buffers.len() < count {
            self.read_buffers
                .resize_with(count, || StreamReadBuffer::new(0));
        }
        for buffer in &mut self.read_buffers[..count] {
            reserve_block(&mut buffer.bytes, block_size);
            buffer.read_len = 0;
            buffer.block_len = 0;
        }
        if self.parity_buffers.len() < parity {
            self.parity_buffers.resize_with(parity, Vec::new);
        }
    }
}

fn reserve_block(buffer: &mut Vec<u8>, block_size: usize) {
    if buffer.capacity() < block_size {
        buffer.reserve_exact(block_size.saturating_sub(buffer.len()));
    }
}

impl ReedSolomon {
    /// Encode streams while retaining scratch in a caller-owned workspace.
    ///
    /// Uses the same count validation, zero-padding, I/O policy and partial-write
    /// behavior as [`Self::encode_stream`]. A workspace can be reused across
    /// codecs, stream lengths, block sizes and I/O modes.
    ///
    /// ```
    /// use rustfs_erasure_codec::{galois_8::ReedSolomon, stream::{StreamOptions, StreamWorkspace}};
    /// let codec = ReedSolomon::new(2, 1)?;
    /// let mut workspace = StreamWorkspace::new();
    /// for _ in 0..2 {
    ///     let mut data = [b"abcd".as_slice(), b"efgh".as_slice()];
    ///     let mut parity = [Vec::new()];
    ///     codec.encode_stream_with_workspace(&mut data, &mut parity, &StreamOptions::default(), &mut workspace)?;
    ///     assert_eq!(parity[0].len(), 4);
    /// }
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn encode_stream_with_workspace(
        &self,
        data: &mut [impl std::io::Read + Send],
        parity: &mut [impl std::io::Write + Send],
        options: &StreamOptions,
        workspace: &mut StreamWorkspace,
    ) -> Result<(), StreamError> {
        if self.is_leopard_family() {
            return Err(StreamError::codec(0, crate::Error::UnsupportedCodecFamily));
        }
        let data_count = self.data_shard_count();
        let parity_count = self.parity_shard_count();
        validate_stream_count(data.len(), data_count)?;
        validate_stream_count(parity.len(), parity_count)?;
        let block_size = options
            .block_size
            .clamp(MIN_BLOCK_SIZE_BYTES, MAX_BLOCK_SIZE_BYTES);
        let parallel_read = use_parallel_stream_io(options, data_count);
        let parallel_write = use_parallel_stream_io(options, parity_count);
        workspace.prepare_read_buffers(data_count, parity_count, block_size);
        let StreamWorkspace {
            read_buffers,
            parity_buffers,
            ..
        } = workspace;
        let read_buffers = &mut read_buffers[..data_count];
        let parity_buffers = &mut parity_buffers[..parity_count];
        for buffer in parity_buffers.iter_mut() {
            reserve_block(buffer, block_size);
        }
        loop {
            let (all_eof, actual_len) =
                read_block_with_mode(data, read_buffers, block_size, parallel_read)?;
            if all_eof {
                return Ok(());
            }
            for buffer in parity_buffers.iter_mut() {
                buffer.resize(actual_len, 0);
            }
            self.encode_sep_opt(read_buffers, parity_buffers)
                .map_err(|error| StreamError::codec(0, error))?;
            write_block_with_mode(
                parity,
                parity_buffers,
                actual_len,
                data_count,
                parallel_write,
            )?;
        }
    }

    /// Verify streams with read buffers and parity scratch retained across calls.
    ///
    /// Uses [`Self::verify_stream`]'s equal-length and corruption checks. Every
    /// new call reads from its supplied readers' current positions, including
    /// after an earlier call returned `Ok(false)` or an error.
    pub fn verify_stream_with_workspace(
        &self,
        shards: &mut [impl std::io::Read + Send],
        options: &StreamOptions,
        workspace: &mut StreamWorkspace,
    ) -> Result<bool, StreamError> {
        if self.is_leopard_family() {
            return Err(StreamError::codec(0, crate::Error::UnsupportedCodecFamily));
        }
        let total = self.total_shard_count();
        validate_stream_count(shards.len(), total)?;
        let block_size = options
            .block_size
            .clamp(MIN_BLOCK_SIZE_BYTES, MAX_BLOCK_SIZE_BYTES);
        let parallel_read = use_parallel_stream_io(options, total);
        workspace.prepare_read_buffers(total, self.parity_shard_count(), block_size);
        let StreamWorkspace {
            read_buffers,
            parity_buffers,
            ..
        } = workspace;
        let read_buffers = &mut read_buffers[..total];
        let parity_buffers = &mut parity_buffers[..self.parity_shard_count()];
        loop {
            let (all_eof, actual_len) =
                read_block_with_mode(shards, read_buffers, block_size, parallel_read)?;
            if all_eof {
                return Ok(true);
            }
            if let Some(bad) = read_buffers
                .iter()
                .position(|buffer| buffer.read_len != actual_len)
            {
                return Err(StreamError::codec(bad, crate::Error::IncorrectShardSize));
            }
            for buffer in parity_buffers.iter_mut() {
                buffer.resize(actual_len, 0);
            }
            if !self
                .verify_with_buffer_opt(read_buffers, parity_buffers)
                .map_err(|error| StreamError::codec(0, error))?
            {
                return Ok(false);
            }
        }
    }

    /// Recover independent streams using retained buffers and a cached plan.
    ///
    /// Reader/writer layouts, padding rules, I/O errors and partial outputs
    /// follow [`Self::reconstruct_stream_from`]. The cached plan is matched by
    /// effective encoding format and the complete missing pattern. New layouts
    /// replace the plan only after their first nonempty block passes validation
    /// and planning succeeds. Every block still checks all present lengths.
    /// Changing block size, total stream length or I/O mode reuses compatible
    /// coefficients. Callers restore stream positions and partial outputs before
    /// retrying an I/O failure with the same workspace.
    pub fn reconstruct_stream_from_with_workspace<
        R: std::io::Read + Send,
        W: std::io::Write + Send,
    >(
        &self,
        readers: &mut [Option<R>],
        writers: &mut [Option<W>],
        options: &StreamOptions,
        workspace: &mut StreamWorkspace,
    ) -> Result<(), StreamError> {
        let total = self.total_shard_count();
        for count in [readers.len(), writers.len()] {
            validate_stream_count(count, total)?;
        }
        if self.is_leopard_family() {
            return Err(StreamError::codec(0, crate::Error::UnsupportedCodecFamily));
        }
        let mut missing_count = 0;
        for (index, (reader, writer)) in readers.iter().zip(writers.iter()).enumerate() {
            if reader.is_some() == writer.is_some() {
                return Err(StreamError::codec(index, crate::Error::InvalidShardFlags));
            }
            missing_count += usize::from(reader.is_none());
        }
        let present_count = total - missing_count;
        if present_count < self.data_shard_count() {
            return Err(StreamError::codec(0, crate::Error::TooFewShardsPresent));
        }
        let block_size = options
            .block_size
            .clamp(MIN_BLOCK_SIZE_BYTES, MAX_BLOCK_SIZE_BYTES);
        let parallel_read = use_parallel_stream_io(options, present_count);
        let parallel_write = use_parallel_stream_io(options, missing_count);
        let StreamWorkspace {
            recovery_buffers,
            missing_indices,
            recovery_plan,
            data_outputs,
            ..
        } = workspace;
        if recovery_buffers.len() < total {
            recovery_buffers.resize_with(total, Vec::new);
        }
        let recovery_buffers = &mut recovery_buffers[..total];
        missing_indices.clear();
        data_outputs.clear();
        for (index, (reader, buffer)) in readers.iter().zip(recovery_buffers.iter_mut()).enumerate()
        {
            if reader.is_none() {
                missing_indices.push(index);
            } else {
                reserve_block(buffer, block_size);
            }
        }
        let (all_eof, mut actual_len) =
            read_present_streams_with_mode(readers, recovery_buffers, block_size, parallel_read)?;
        if all_eof {
            return Ok(());
        }
        if recovery_plan.as_ref().is_none_or(|cached| {
            &cached.format != self.encoding_format()
                || cached.plan.shape.invalid_indices.as_slice() != missing_indices.as_slice()
        }) {
            let fresh = StreamReconstructPlan::new(self, missing_indices, actual_len)
                .map_err(|error| StreamError::codec(0, error))?;
            *recovery_plan = Some(CachedRecoveryPlan {
                format: self.encoding_format().clone(),
                plan: fresh.plan,
            });
        }
        let cached = recovery_plan
            .as_ref()
            .expect("validated recovery plan is initialized");
        let plan = StreamReconstructPlan {
            codec: self,
            plan: &cached.plan,
        };
        data_outputs.reserve_exact(cached.plan.data_targets().len());
        let rows = cached.plan.row_views(self);
        let (data_rows, parity_rows) = rows.split_at(cached.plan.data_targets().len());
        loop {
            for &index in missing_indices.iter() {
                recovery_buffers[index].resize(actual_len, 0);
            }
            plan.reconstruct_block(
                recovery_buffers,
                actual_len,
                &data_rows,
                &parity_rows,
                data_outputs,
            );
            write_missing_streams_with_mode(writers, recovery_buffers, actual_len, parallel_write)?;
            let (all_eof, next_len) = read_present_streams_with_mode(
                readers,
                recovery_buffers,
                block_size,
                parallel_read,
            )?;
            if all_eof {
                return Ok(());
            }
            actual_len = next_len;
        }
    }

    /// Recover legacy in-memory cursors with retained stream scratch and planning.
    ///
    /// Uses [`Self::reconstruct_stream`]'s empty-shard detection, reset of present
    /// cursor positions, lazy output reservation and empty-dataset behavior.
    /// For explicit presence flags and independent streams use
    /// [`Self::reconstruct_stream_from_with_workspace`].
    pub fn reconstruct_stream_with_workspace(
        &self,
        shards: &mut [std::io::Cursor<Vec<u8>>],
        options: &StreamOptions,
        workspace: &mut StreamWorkspace,
    ) -> Result<(), StreamError> {
        let total = self.total_shard_count();
        validate_stream_count(shards.len(), total)?;
        if self.is_leopard_family() {
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
        if present_count == 0 {
            return Ok(());
        }
        if present_count < self.data_shard_count() {
            return Err(StreamError::codec(0, crate::Error::TooFewShardsPresent));
        }
        let output_len = if equal_lengths { known_len } else { 0 };
        let mut readers: SmallVec<[Option<&mut std::io::Cursor<Vec<u8>>>; 32]> =
            SmallVec::with_capacity(total);
        let mut writers: SmallVec<[Option<RecoveryVecWriter<'_>>; 32]> =
            SmallVec::with_capacity(total);
        for shard in shards {
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
        self.reconstruct_stream_from_with_workspace(&mut readers, &mut writers, options, workspace)
    }
}
