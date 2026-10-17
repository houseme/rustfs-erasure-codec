//! Canonical identity for the effective encoding format.

extern crate alloc;

use alloc::sync::Arc;
use alloc::vec::Vec;

use crate::Field;
use crate::matrix::Matrix;

use super::CodecFamily;

/// Version of the binary descriptor returned by [`EncodingFormat::stable_id_bytes`].
///
/// Version 1 fixes the built-in algorithms and their shard layouts. A future
/// change that alters encoded bytes must use a new descriptor version or family
/// tag; performance-only changes do not change the identifier.
pub const ENCODING_FORMAT_VERSION: u16 = 1;

const ID_MAGIC: &[u8; 8] = b"RSEFMT\0\0";
const FIXED_ID_BYTES: usize = 59;
const MAX_ID_BYTES: usize = 64 * 1024 * 1024;

/// Immutable description of the effective encoding format for field `F`.
///
/// Codec equality and reconstruction-workspace compatibility use this same
/// value. Cloning shares the matrix allocation and copies only its small
/// configuration; comparing clones performs no coefficient scan. Neither
/// operation serializes metadata or copies matrix coefficients.
///
/// This typed value remains useful for custom fields without a portable field
/// identity. [`Self::stable_id_bytes`] returns a persistent identifier only when
/// the field explicitly supplies a stable algebra and element-encoding contract.
#[derive(Debug)]
pub struct EncodingFormat<F: Field> {
    data_shards: usize,
    parity_shards: usize,
    family: CodecFamily,
    matrix: Arc<Matrix<F>>,
}

impl<F: Field> Clone for EncodingFormat<F> {
    fn clone(&self) -> Self {
        Self {
            data_shards: self.data_shards,
            parity_shards: self.parity_shards,
            family: self.family,
            matrix: Arc::clone(&self.matrix),
        }
    }
}

impl<F: Field> PartialEq for EncodingFormat<F> {
    fn eq(&self, other: &Self) -> bool {
        self.data_shards == other.data_shards
            && self.parity_shards == other.parity_shards
            && self.family == other.family
            && (Arc::ptr_eq(&self.matrix, &other.matrix)
                || (self.matrix.row_count() == other.matrix.row_count()
                    && self.matrix.col_count() == other.matrix.col_count()
                    && (0..self.matrix.row_count())
                        .all(|row| self.matrix.get_row(row) == other.matrix.get_row(row))))
    }
}

impl<F: Field> EncodingFormat<F> {
    pub(super) fn new(
        data_shards: usize,
        parity_shards: usize,
        family: CodecFamily,
        matrix: Arc<Matrix<F>>,
    ) -> Self {
        Self {
            data_shards,
            parity_shards,
            family,
            matrix,
        }
    }

    /// Number of data shards.
    #[inline]
    pub fn data_shard_count(&self) -> usize {
        self.data_shards
    }

    /// Number of parity shards.
    #[inline]
    pub fn parity_shard_count(&self) -> usize {
        self.parity_shards
    }

    /// Number of data and parity shards combined.
    #[inline]
    pub fn total_shard_count(&self) -> usize {
        // Construction already checked that the sum fits in usize.
        self.data_shards + self.parity_shards
    }

    /// Resolved algorithm family, including automatic Leopard selection.
    #[inline]
    pub fn codec_family(&self) -> CodecFamily {
        self.family
    }

    /// Version of the persistent identifier schema.
    pub fn descriptor_version(&self) -> u16 {
        ENCODING_FORMAT_VERSION
    }

    /// Effective systematic matrix dimensions, including the identity data rows.
    ///
    /// Returns `(0, 0)` for Leopard families, whose fixed FFT algorithm defines
    /// the encoding instead of an explicit Classic matrix.
    pub fn matrix_shape(&self) -> (usize, usize) {
        (self.matrix.row_count(), self.matrix.col_count())
    }

    /// Borrow one effective matrix row, or `None` for an out-of-range row.
    ///
    /// Leopard formats have no explicit rows. For Classic, the first
    /// `data_shard_count()` rows form the identity and the remaining rows are
    /// the effective parity coefficients, including explicit fast-parity mode.
    pub fn matrix_row(&self, row: usize) -> Option<&[F::Elem]> {
        (row < self.matrix.row_count()).then(|| self.matrix.get_row(row))
    }

    #[inline]
    pub(crate) fn matrix(&self) -> &Matrix<F> {
        &self.matrix
    }

    /// Build a complete, versioned identifier suitable for storing with metadata.
    ///
    /// The identifier is the full descriptor, not a hash. It is independent of
    /// host endianness, pointer size, CPU backend, cache settings, and parallel
    /// scheduling. Matrix selection options are represented by their effective
    /// coefficients, so an equivalent custom matrix has the same identifier.
    /// Nothing is inserted into encoded shards. Store the bytes separately and
    /// compare them with the configured codec's identifier before using its data.
    ///
    /// Version 1 layout, in order (all integers are unsigned big-endian):
    ///
    /// | Field | Encoding |
    /// | --- | --- |
    /// | Magic | 8 bytes: `RSEFMT` followed by two zero bytes |
    /// | Schema version | `u16`, currently 1 |
    /// | Algorithm and shard layout | `u8`: 0 = Classic systematic matrix; 1 = Leopard GF8 byte FFT; 2 = Leopard GF16 FFT with 64-byte low/high split blocks |
    /// | Field identifier | `u32` UTF-8 byte length, followed by that many bytes |
    /// | Stable element width | `u32` bytes per field element |
    /// | Field order | `u64`, describing the input field `F` |
    /// | Data and parity shard counts | two `u64` values |
    /// | Effective matrix dimensions | row count then column count, each `u64` |
    /// | Matrix coefficients | row-major, using the field's stable fixed-width element encoding |
    ///
    /// The field identifier describes `F`'s algebra and input element layout.
    /// The family tag additionally specifies Leopard's fixed transform algebra
    /// and shard layout; Leopard GF16 over byte input is distinct from Classic
    /// over the built-in tower GF16 field.
    ///
    /// Returns `None` if the field does not opt in via [`Field::stable_format`],
    /// a hook cannot encode a coefficient at its declared width, a length is not
    /// representable, the descriptor would exceed 64 MiB, or allocation fails.
    /// Custom implementations must uphold their stable-field contract; this
    /// method cannot establish that two user-supplied field names mean the same
    /// algebra. Serialization is explicit and never runs in encode, decode,
    /// equality, or workspace execution.
    pub fn stable_id_bytes(&self) -> Option<Vec<u8>> {
        let (field_id, element_width) = F::stable_format()?;
        if field_id.is_empty() || element_width == 0 {
            return None;
        }
        let field_id_len = u32::try_from(field_id.len()).ok()?;
        let element_width_u32 = u32::try_from(element_width).ok()?;
        let field_order = u64::try_from(F::ORDER).ok()?;
        let data_shards = u64::try_from(self.data_shards).ok()?;
        let parity_shards = u64::try_from(self.parity_shards).ok()?;
        let rows = u64::try_from(self.matrix.row_count()).ok()?;
        let cols = u64::try_from(self.matrix.col_count()).ok()?;
        let coefficient_bytes = self
            .matrix
            .row_count()
            .checked_mul(self.matrix.col_count())?
            .checked_mul(element_width)?;
        let total_bytes = FIXED_ID_BYTES
            .checked_add(field_id.len())?
            .checked_add(coefficient_bytes)?;
        if total_bytes > MAX_ID_BYTES || element_width > MAX_ID_BYTES {
            return None;
        }

        let mut bytes = Vec::new();
        bytes.try_reserve_exact(total_bytes).ok()?;
        bytes.extend_from_slice(ID_MAGIC);
        bytes.extend_from_slice(&ENCODING_FORMAT_VERSION.to_be_bytes());
        bytes.push(match self.family {
            CodecFamily::Classic => 0,
            CodecFamily::LeopardGF8 => 1,
            CodecFamily::LeopardGF16 => 2,
        });
        bytes.extend_from_slice(&field_id_len.to_be_bytes());
        bytes.extend_from_slice(field_id.as_bytes());
        bytes.extend_from_slice(&element_width_u32.to_be_bytes());
        for value in [field_order, data_shards, parity_shards, rows, cols] {
            bytes.extend_from_slice(&value.to_be_bytes());
        }
        let mut offset = bytes.len();
        bytes.resize(total_bytes, 0);
        for row in 0..self.matrix.row_count() {
            for &element in self.matrix.get_row(row) {
                let end = offset.checked_add(element_width)?;
                if F::write_format_element(element, &mut bytes[offset..end])? != element_width {
                    return None;
                }
                offset = end;
            }
        }
        Some(bytes)
    }
}
