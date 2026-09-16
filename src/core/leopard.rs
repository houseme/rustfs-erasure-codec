extern crate alloc;

use alloc::vec::Vec;

use crate::Field;
use crate::errors::Error;
use crate::matrix::Matrix;

use super::{CodecFamily, LeopardMode};

/// Maximum total shards (`data + parity`) addressable by each Leopard family.
///
/// GF(2^8) Leopard is bounded by the byte field order; GF(2^16) Leopard uses
/// 16-bit arithmetic and reaches 65536. This is the single source of truth for
/// the family-aware cap enforced in the constructors via
/// [`max_total_shards_for_family`] (returning [`Error::TooManyShards`]); the
/// per-family validators check only field/endian preconditions, not the cap.
pub(crate) const LEOPARD_GF8_MAX_SHARDS: usize = 256;
pub(crate) const LEOPARD_GF16_MAX_SHARDS: usize = 65536;

/// Parity ceiling at or below which [`LeopardMode::PreferLeopard`] resolves to
/// GF(2^8) Leopard. Above it a GF(2^8) Leopard codec can be constructed but not
/// encoded, so resolution falls back to GF(2^16).
const LEOPARD_GF8_MAX_PARITY: usize = 128;

/// Whether `F` is a byte-oriented field, i.e. `F::Elem` is exactly one byte.
///
/// This is the soundness precondition for every Leopard codec: they reinterpret
/// `F::Elem` slices as raw `u8` bytes (see [`AsLeopardU8`]). Gating on the
/// element size — rather than `F::ORDER == 256` — is the precise condition and
/// also rejects any hypothetical field whose order is 256 but whose element has
/// a wider representation.
pub(crate) fn is_byte_field<F: Field>() -> bool {
    core::mem::size_of::<F::Elem>() == 1
}

/// Trait for safely reinterpreting `F::Elem` as `u8` for leopard encode.
///
/// Only implemented for `u8` (i.e., `galois_8::Field`). This enables the generic
/// `encode_sep` to call the `u8`-specific leopard FFT engine without `unsafe`.
pub(crate) trait AsLeopardU8: Sized {
    fn slice_to_u8(slice: &[Self]) -> &[u8];
    fn slice_to_u8_mut(slice: &mut [Self]) -> &mut [u8];
}

impl AsLeopardU8 for u8 {
    #[inline]
    fn slice_to_u8(slice: &[u8]) -> &[u8] {
        slice
    }
    #[inline]
    fn slice_to_u8_mut(slice: &mut [u8]) -> &mut [u8] {
        slice
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum FamilyState<F: Field> {
    Classic,
    LeopardGF8(LeopardGF8Codec<F>),
    LeopardGF16,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LeopardGF8Codec<F: Field> {
    data_shards: usize,
    parity_shards: usize,
    total_shards: usize,
    setup_rows: usize,
    setup_cols: usize,
    parity_rows: Vec<Vec<F::Elem>>,
    _marker: core::marker::PhantomData<F>,
}

impl<F: Field> LeopardGF8Codec<F> {
    pub(crate) fn new(
        data_shards: usize,
        parity_shards: usize,
        setup_matrix: Matrix<F>,
    ) -> Result<Self, Error> {
        validate_leopard_gf8::<F>(data_shards, parity_shards)?;

        Ok(Self {
            data_shards,
            parity_shards,
            total_shards: data_shards.saturating_add(parity_shards),
            setup_rows: setup_matrix.row_count(),
            setup_cols: setup_matrix.col_count(),
            parity_rows: (data_shards..(data_shards + parity_shards))
                .map(|row| setup_matrix.get_row(row).to_vec())
                .collect(),
            _marker: core::marker::PhantomData,
        })
    }

    pub(crate) fn data_shards(&self) -> usize {
        self.data_shards
    }

    pub(crate) fn parity_shards(&self) -> usize {
        self.parity_shards
    }

    pub(crate) fn total_shards(&self) -> usize {
        self.total_shards
    }

    pub(crate) fn setup_shape(&self) -> (usize, usize) {
        (self.setup_rows, self.setup_cols)
    }

    pub(crate) fn parity_rows(&self) -> Vec<&[F::Elem]> {
        self.parity_rows.iter().map(|row| row.as_slice()).collect()
    }
}

pub(crate) fn leopard_gf8_state<F: Field>(
    family_state: &FamilyState<F>,
) -> Result<&LeopardGF8Codec<F>, Error> {
    match family_state {
        FamilyState::LeopardGF8(codec) => Ok(codec),
        FamilyState::Classic => Err(Error::UnsupportedCodecFamily),
        FamilyState::LeopardGF16 => Err(Error::UnsupportedLeopardPrototype),
    }
}

pub(crate) fn validate_leopard_shard_len(shard_len: usize) -> Result<(), Error> {
    if shard_len == 0 || !shard_len.is_multiple_of(LEOPARD_SHARD_MULTIPLE) {
        return Err(Error::IncorrectShardSize);
    }

    Ok(())
}

/// Required byte multiple (and cache-line alignment) of every Leopard shard.
///
/// Leopard shards must be a non-zero multiple of this value; see
/// [`validate_leopard_shard_len`]. Equal to
/// [`SHARD_ALIGNMENT`](crate::galois_8::SHARD_ALIGNMENT).
pub const LEOPARD_SHARD_MULTIPLE: usize = 64;

/// Computes a per-shard length, in bytes, that Leopard will accept for a payload
/// of `data_len` bytes spread across `data_shards` data shards.
///
/// The result is **always a non-zero multiple of [`LEOPARD_SHARD_MULTIPLE`]**, so
/// it is guaranteed to pass [`validate_leopard_shard_len`], for every input:
///
/// * `data_len == 0` (or any payload smaller than one block) clamps up to
///   [`LEOPARD_SHARD_MULTIPLE`].
/// * `data_shards == 0` is treated as a single shard (no divide-by-zero).
/// * Payloads near [`usize::MAX`] saturate to the largest multiple of
///   [`LEOPARD_SHARD_MULTIPLE`] that fits in a `usize`, rather than overflowing.
pub fn leopard_aligned_shard_len(data_len: usize, data_shards: usize) -> usize {
    // A zero shard count is nonsensical; treat the whole payload as one shard
    // instead of dividing by zero.
    let shards = if data_shards == 0 { 1 } else { data_shards };

    // Bytes per shard, rounding the payload up so every byte has a home.
    // `div_ceil` cannot overflow and `shards >= 1`, so this is total.
    let per_shard = data_len.div_ceil(shards);

    // Round up to the next multiple of LEOPARD_SHARD_MULTIPLE. `per_shard +
    // (MULTIPLE - remainder)` can overflow near usize::MAX, so saturate.
    let remainder = per_shard % LEOPARD_SHARD_MULTIPLE;
    let rounded = if remainder == 0 {
        per_shard
    } else {
        per_shard.saturating_add(LEOPARD_SHARD_MULTIPLE - remainder)
    };

    // Guarantee a non-zero result (covers data_len == 0), then floor back onto a
    // 64-boundary in case the saturation above landed on usize::MAX (which is
    // not itself a multiple of 64). For all normal inputs this is a no-op.
    let clamped = rounded.max(LEOPARD_SHARD_MULTIPLE);
    clamped - (clamped % LEOPARD_SHARD_MULTIPLE)
}

// Colocated with `leopard_aligned_shard_len`; the dispatch helpers below are
// unrelated, so allow the "items after test module" style lint here.
#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod leopard_shard_len_tests {
    use super::{LEOPARD_SHARD_MULTIPLE, leopard_aligned_shard_len, validate_leopard_shard_len};

    #[test]
    fn zero_data_len_clamps_to_multiple() {
        let len = leopard_aligned_shard_len(0, 4);
        assert_eq!(len, LEOPARD_SHARD_MULTIPLE);
        assert!(validate_leopard_shard_len(len).is_ok());
    }

    #[test]
    fn non_multiple_payload_rounds_up() {
        // ceil(100 / 4) = 25 bytes/shard -> rounds up to 64.
        let len = leopard_aligned_shard_len(100, 4);
        assert_eq!(len, 64);
        // 65 bytes/shard -> rounds up to 128.
        let len2 = leopard_aligned_shard_len(65 * 4, 4);
        assert_eq!(len2, 128);
        assert!(len.is_multiple_of(LEOPARD_SHARD_MULTIPLE));
        assert!(validate_leopard_shard_len(len).is_ok());
        assert!(validate_leopard_shard_len(len2).is_ok());
    }

    #[test]
    fn zero_shards_treated_as_one() {
        // ceil(200 / 1) = 200 -> rounds up to 256.
        let len = leopard_aligned_shard_len(200, 0);
        assert_eq!(len, 256);
        assert!(validate_leopard_shard_len(len).is_ok());
    }

    #[test]
    fn near_usize_max_saturates_to_valid_multiple() {
        let len = leopard_aligned_shard_len(usize::MAX, 1);
        assert_ne!(len, 0);
        assert!(len.is_multiple_of(LEOPARD_SHARD_MULTIPLE));
        assert!(validate_leopard_shard_len(len).is_ok());
        // Largest 64-multiple representable in a usize.
        assert_eq!(len, usize::MAX - (usize::MAX % LEOPARD_SHARD_MULTIPLE));
    }
}

pub(crate) fn build_family_state<F: Field>(
    codec_family: CodecFamily,
    data_shards: usize,
    parity_shards: usize,
    setup_matrix: &Matrix<F>,
) -> Result<FamilyState<F>, Error> {
    match codec_family {
        CodecFamily::Classic => Ok(FamilyState::Classic),
        CodecFamily::LeopardGF8 => Ok(FamilyState::LeopardGF8(LeopardGF8Codec::new(
            data_shards,
            parity_shards,
            {
                let mut matrix = Matrix::new(setup_matrix.row_count(), setup_matrix.col_count());
                for row in 0..setup_matrix.row_count() {
                    for col in 0..setup_matrix.col_count() {
                        matrix.set(row, col, setup_matrix.get(row, col));
                    }
                }
                matrix
            },
        )?)),
        CodecFamily::LeopardGF16 => Ok(FamilyState::LeopardGF16),
    }
}

pub(crate) fn validate_leopard_family<F: Field>(
    codec_family: CodecFamily,
    data_shards: usize,
    parity_shards: usize,
) -> Result<(), Error> {
    match codec_family {
        CodecFamily::Classic => Ok(()),
        CodecFamily::LeopardGF8 => validate_leopard_gf8::<F>(data_shards, parity_shards),
        CodecFamily::LeopardGF16 => validate_leopard_gf16::<F>(data_shards, parity_shards),
    }
}

/// Maximum representable `data + parity` shard count for a given codec family.
///
/// The generic `total > F::ORDER` guard in `with_options` is only correct for
/// [`CodecFamily::Classic`]: the Leopard codecs run on the GF(2^8) field
/// (`F::ORDER == 256`) but [`CodecFamily::LeopardGF16`] internally uses GF(2^16)
/// arithmetic and supports up to 65536 shards. Using this family-aware cap is
/// what makes an explicit (or auto-selected) `LeopardGF16` codec with more than
/// 256 total shards constructible at all.
pub(crate) fn max_total_shards_for_family<F: Field>(codec_family: CodecFamily) -> usize {
    match codec_family {
        CodecFamily::Classic => F::ORDER,
        CodecFamily::LeopardGF8 => LEOPARD_GF8_MAX_SHARDS,
        CodecFamily::LeopardGF16 => LEOPARD_GF16_MAX_SHARDS,
    }
}

/// Resolve the effective [`CodecFamily`] for an optional [`LeopardMode`].
///
/// Returns the family the codec should actually build. Auto-selection is only
/// eligible when the caller left `codec_family` at [`CodecFamily::Classic`] on a
/// byte-oriented field (`is_byte_field`, i.e. the GF(2^8) field the Leopard
/// codecs require); otherwise the requested family is returned unchanged. Once
/// eligible, `mode` alone decides — [`LeopardMode::Disabled`] maps back to
/// `Classic`, so the default is preserved byte-for-byte.
///
/// The mapping over `total = data + parity` mirrors klauspost/reedsolomon:
///
/// - [`Disabled`](LeopardMode::Disabled): always Classic.
/// - [`AsNeeded`](LeopardMode::AsNeeded): Classic while it fits the byte field,
///   else GF16.
/// - [`PreferGF16`](LeopardMode::PreferGF16): always GF16.
/// - [`PreferLeopard`](LeopardMode::PreferLeopard): GF8 when it fits the byte
///   field and `parity ≤ LEOPARD_GF8_MAX_PARITY`, else GF16.
pub(crate) fn resolve_codec_family(
    requested: CodecFamily,
    mode: LeopardMode,
    is_byte_field: bool,
    total_shards: usize,
    parity_shards: usize,
) -> CodecFamily {
    // Only an untouched Classic request on a byte-oriented field is eligible for
    // auto-selection; everything else passes through unchanged. `mode` is then
    // the single source of truth for the resolved family.
    if requested != CodecFamily::Classic || !is_byte_field {
        return requested;
    }

    match mode {
        LeopardMode::Disabled => CodecFamily::Classic,
        LeopardMode::AsNeeded => {
            if total_shards <= LEOPARD_GF8_MAX_SHARDS {
                CodecFamily::Classic
            } else {
                CodecFamily::LeopardGF16
            }
        }
        LeopardMode::PreferGF16 => CodecFamily::LeopardGF16,
        LeopardMode::PreferLeopard => {
            if total_shards <= LEOPARD_GF8_MAX_SHARDS && parity_shards <= LEOPARD_GF8_MAX_PARITY {
                CodecFamily::LeopardGF8
            } else {
                CodecFamily::LeopardGF16
            }
        }
    }
}

fn leopard_shard_combo_fits(data_shards: usize, parity_shards: usize, order: usize) -> bool {
    let Some(m) = parity_shards.checked_next_power_of_two() else {
        return false;
    };
    if data_shards == 0 || parity_shards == 0 || m == 0 || m > order {
        return false;
    }

    let Some(rounded_data) = data_shards.div_ceil(m).checked_mul(m) else {
        return false;
    };
    rounded_data <= order.saturating_sub(m)
}

fn validate_leopard_gf8<F: Field>(data_shards: usize, parity_shards: usize) -> Result<(), Error> {
    // Soundness gate: Leopard reinterprets `F::Elem` as raw bytes.
    if !is_byte_field::<F>() {
        return Err(Error::UnsupportedCodecFamily);
    }
    if !leopard_shard_combo_fits(data_shards, parity_shards, LEOPARD_GF8_MAX_SHARDS) {
        return Err(Error::TooManyShards);
    }
    Ok(())
}

fn validate_leopard_gf16<F: Field>(data_shards: usize, parity_shards: usize) -> Result<(), Error> {
    // Soundness gate: Leopard reinterprets `F::Elem` as raw bytes.
    if !is_byte_field::<F>() {
        return Err(Error::UnsupportedCodecFamily);
    }

    // The GF16 Leopard codec handles bytes as little-endian `u16` split-layout
    // pairs. Big-endian correctness is not yet verified end to end, so reject at
    // construction rather than risk silently producing wrong results
    // (rustfs/backlog#1238). Little-endian builds are unaffected; GF8 Leopard is
    // byte-oriented and endian-agnostic, so it is not gated here.
    if cfg!(target_endian = "big") {
        return Err(Error::UnsupportedCodecFamily);
    }

    if !leopard_shard_combo_fits(data_shards, parity_shards, LEOPARD_GF16_MAX_SHARDS) {
        return Err(Error::TooManyShards);
    }

    Ok(())
}

/// Dispatch encode to the Leopard GF8 FFT engine.
///
/// Accepts `u8` slices directly. The caller (`encode_leopard_gf8_sep`) is responsible
/// for converting from `F::Elem` to `u8` (safe because Leopard GF8 is only
/// instantiated for `galois_8::Field` where `Elem = u8`).
pub(crate) fn leopard_gf8_encode(
    data_shards: usize,
    parity_shards: usize,
    data: &[&[u8]],
    parity: &mut [&mut [u8]],
) -> Result<(), Error> {
    super::leopard_gf8::encode_with_tables(data_shards, parity_shards, data, parity)?;
    Ok(())
}

/// Dispatch encode to the Leopard GF16 FFT engine.
pub(crate) fn leopard_gf16_encode(
    data_shards: usize,
    parity_shards: usize,
    data: &[&[u8]],
    parity: &mut [&mut [u8]],
) -> Result<(), Error> {
    super::leopard_gf16::encode::encode_with_tables16(data_shards, parity_shards, data, parity)?;
    Ok(())
}

/// Dispatch reconstruct to the Leopard GF16 Forney decoder.
pub(crate) fn leopard_gf16_reconstruct(
    present: &[bool],
    outputs: &mut [&mut [u8]],
    input_data: &[Option<&[u8]>],
    data_shards: usize,
    parity_shards: usize,
) -> Result<(), Error> {
    let tables = super::leopard_gf16::init_leopard_gf16_tables();
    super::leopard_gf16::decode::reconstruct_with_tables16(
        present,
        outputs,
        input_data,
        data_shards,
        parity_shards,
        tables,
    )
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod auto_activation_tests {
    use super::{CodecFamily, LeopardMode, max_total_shards_for_family, resolve_codec_family};
    use crate::{galois_8, galois_16};

    // Resolution helper for the common case: an untouched `Classic` request on a
    // byte-oriented field, which is where auto-selection actually applies.
    fn resolve(mode: LeopardMode, total: usize, parity: usize) -> CodecFamily {
        resolve_codec_family(CodecFamily::Classic, mode, true, total, parity)
    }

    #[test]
    fn disabled_is_always_classic() {
        for &(total, parity) in &[(2usize, 1usize), (256, 1), (257, 1), (65536, 4)] {
            assert_eq!(
                resolve(LeopardMode::Disabled, total, parity),
                CodecFamily::Classic
            );
        }
    }

    #[test]
    fn as_needed_switches_to_gf16_past_256() {
        assert_eq!(resolve(LeopardMode::AsNeeded, 2, 1), CodecFamily::Classic);
        assert_eq!(resolve(LeopardMode::AsNeeded, 256, 1), CodecFamily::Classic);
        assert_eq!(
            resolve(LeopardMode::AsNeeded, 257, 1),
            CodecFamily::LeopardGF16
        );
        assert_eq!(
            resolve(LeopardMode::AsNeeded, 65536, 4),
            CodecFamily::LeopardGF16
        );
    }

    #[test]
    fn prefer_gf16_is_always_gf16() {
        assert_eq!(
            resolve(LeopardMode::PreferGF16, 2, 1),
            CodecFamily::LeopardGF16
        );
        assert_eq!(
            resolve(LeopardMode::PreferGF16, 256, 1),
            CodecFamily::LeopardGF16
        );
        assert_eq!(
            resolve(LeopardMode::PreferGF16, 65536, 4),
            CodecFamily::LeopardGF16
        );
    }

    #[test]
    fn prefer_leopard_uses_gf8_only_when_small_and_low_parity() {
        // total <= 256 and parity <= 128 -> GF8
        assert_eq!(
            resolve(LeopardMode::PreferLeopard, 256, 128),
            CodecFamily::LeopardGF8
        );
        assert_eq!(
            resolve(LeopardMode::PreferLeopard, 6, 2),
            CodecFamily::LeopardGF8
        );
        // parity > 128 -> GF16 (D5: avoid configs that construct as GF8 but can't encode)
        assert_eq!(
            resolve(LeopardMode::PreferLeopard, 256, 129),
            CodecFamily::LeopardGF16
        );
        // total > 256 -> GF16
        assert_eq!(
            resolve(LeopardMode::PreferLeopard, 257, 1),
            CodecFamily::LeopardGF16
        );
    }

    #[test]
    fn explicit_family_is_never_rewritten() {
        for mode in [
            LeopardMode::AsNeeded,
            LeopardMode::PreferGF16,
            LeopardMode::PreferLeopard,
        ] {
            assert_eq!(
                resolve_codec_family(CodecFamily::LeopardGF8, mode, true, 10, 2),
                CodecFamily::LeopardGF8
            );
            assert_eq!(
                resolve_codec_family(CodecFamily::LeopardGF16, mode, true, 10, 2),
                CodecFamily::LeopardGF16
            );
        }
    }

    #[test]
    fn non_byte_field_ignores_mode() {
        // A non-byte-oriented field (e.g. GF(2^16) element = 2 bytes) never
        // auto-activates Leopard; the request stays Classic.
        assert_eq!(
            resolve_codec_family(CodecFamily::Classic, LeopardMode::PreferGF16, false, 300, 4),
            CodecFamily::Classic
        );
    }

    #[test]
    fn family_aware_caps() {
        // Classic caps at the field order; Leopard caps by family regardless of F::ORDER.
        assert_eq!(
            max_total_shards_for_family::<galois_8::Field>(CodecFamily::Classic),
            256
        );
        assert_eq!(
            max_total_shards_for_family::<galois_16::Field>(CodecFamily::Classic),
            65536
        );
        // Leopard runs on the GF(2^8) field (F::ORDER = 256) but GF16 allows 65536.
        assert_eq!(
            max_total_shards_for_family::<galois_8::Field>(CodecFamily::LeopardGF8),
            256
        );
        assert_eq!(
            max_total_shards_for_family::<galois_8::Field>(CodecFamily::LeopardGF16),
            65536
        );
    }
}
