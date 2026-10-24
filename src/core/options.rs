/// Selects the codec algorithm family for encoding and reconstruction.
///
/// - [`Classic`](CodecFamily::Classic): Standard Reed-Solomon over GF(2^8) using
///   Vandermonde/Cauchy matrix multiplication. Supports all shard counts up to
///   `Field::ORDER`, incremental `update`, and `encode_single`.
///
/// - [`LeopardGF8`](CodecFamily::LeopardGF8): FFT-based Leopard codec over GF(2^8).
///   Uses Fermat-number FFT with Forney syndrome decoding. Requires shard lengths
///   that are multiples of 64 bytes. Does **not** support `encode_single` or `update`.
///   Supports up to 256 total shards (data + parity).
///
/// - [`LeopardGF16`](CodecFamily::LeopardGF16): FFT-based Leopard codec over GF(2^16).
///   Supports up to 65536 total shards (data + parity).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodecFamily {
    Classic,
    LeopardGF8,
    LeopardGF16,
}

/// Controls optional automatic selection of a Leopard codec when the requested
/// [`CodecFamily`] is [`Classic`](CodecFamily::Classic).
///
/// Auto-selection only takes effect on a field that explicitly supports the
/// built-in GF(2^8) byte kernels through [`crate::Field::as_gf8_slice`] and
/// [`crate::Field::as_gf8_slice_mut`]; otherwise the mode is ignored and the codec stays
/// Classic. When the caller sets an explicit non-Classic `codec_family`, this
/// mode is likewise ignored — the explicit family wins.
///
/// The resolution, as a function of `total = data + parity` shards, mirrors the
/// klauspost/reedsolomon `New()` behaviour:
///
/// | `total` | [`AsNeeded`](LeopardMode::AsNeeded) | [`PreferLeopard`](LeopardMode::PreferLeopard) | [`PreferGF16`](LeopardMode::PreferGF16) |
/// |---------|-----------|---------------|------------|
/// | ≤ 256   | Classic   | LeopardGF8¹   | LeopardGF16 |
/// | 257..=65536 | LeopardGF16 | LeopardGF16 | LeopardGF16 |
///
/// ¹ [`PreferLeopard`](LeopardMode::PreferLeopard) selects [`LeopardGF8`](CodecFamily::LeopardGF8)
///   only when `total ≤ 256` **and** `parity ≤ 128`; otherwise it falls back to
///   [`LeopardGF16`](CodecFamily::LeopardGF16).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum LeopardMode {
    /// Never auto-select Leopard. The codec behaves exactly as an explicit
    /// [`CodecFamily::Classic`] construction — byte-for-byte identical to
    /// releases before auto-activation existed. This is the default.
    #[default]
    Disabled,
    /// Use Classic while it can represent the configuration (`total ≤ 256`), and
    /// automatically switch to [`LeopardGF16`](CodecFamily::LeopardGF16) once the
    /// shard count exceeds what GF(2^8) can address.
    AsNeeded,
    /// Always use [`LeopardGF16`](CodecFamily::LeopardGF16) (for any supported
    /// shard count up to 65536).
    PreferGF16,
    /// Prefer a Leopard codec whenever possible: [`LeopardGF8`](CodecFamily::LeopardGF8)
    /// for small byte-friendly configurations (`total ≤ 256` and `parity ≤ 128`),
    /// otherwise [`LeopardGF16`](CodecFamily::LeopardGF16).
    PreferLeopard,
}

/// Selects the encoding matrix construction strategy for [`CodecFamily::Classic`].
///
/// This option is ignored for Leopard families (they use FFT-based encoding).
///
/// - [`Vandermonde`](MatrixMode::Vandermonde): Standard Vandermonde matrix.
/// - [`Cauchy`](MatrixMode::Cauchy): Cauchy matrix construction.
/// - [`JerasureLike`](MatrixMode::JerasureLike): Jerasure-compatible matrix layout.
/// - [`Custom`](MatrixMode::Custom): User-supplied matrix via
///   [`ReedSolomon::with_custom_matrix`](crate::ReedSolomon::with_custom_matrix).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatrixMode {
    Vandermonde,
    Cauchy,
    JerasureLike,
    Custom,
}

/// Configuration options for constructing a [`ReedSolomon`](crate::ReedSolomon) codec.
///
/// Use `CodecOptions::default()` for sensible defaults, or the builder methods:
///
/// ```rust
/// use rustfs_erasure_codec::{CodecOptions, CodecFamily};
///
/// let opts = CodecOptions::builder()
///     .codec_family(CodecFamily::LeopardGF8)
///     .fast_one_parity(true)
///     .build();
/// ```
///
/// `CodecOptions` is `#[non_exhaustive]`: construct it from
/// [`CodecOptions::default()`] followed by field assignments, or via the
/// [`builder`](CodecOptions::builder). Downstream struct literals, including
/// struct-update expressions, are not supported. This lets future fields be added without a
/// breaking change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct CodecOptions {
    /// When `true`, Classic codecs with one parity shard use an all-ones parity
    /// row, enabling the XOR-only fast path. This changes the coding format for
    /// matrix modes whose usual parity row is not all ones. Custom matrices
    /// must already supply an all-ones row. Ignored by Leopard and when there
    /// is more than one parity shard. Default: `false`.
    pub fast_one_parity: bool,
    /// When `true`, cache the inverted decode matrix for repeated reconstruction
    /// with the same selected survivor rows. Unselected trailing parity erasures
    /// share the same entry. Default: `true`.
    pub inversion_cache: bool,
    /// Capacity of the inversion cache (LRU). `0` means automatic sizing based on
    /// shard counts. Default: `0`.
    pub inversion_cache_capacity: usize,
    /// The codec algorithm family to use. Default: [`CodecFamily::Classic`].
    pub codec_family: CodecFamily,
    /// Optional automatic Leopard selection when `codec_family` is
    /// [`CodecFamily::Classic`]. Default: [`LeopardMode::Disabled`] (no
    /// auto-selection; behaviour identical to prior releases). See [`LeopardMode`].
    pub leopard_mode: LeopardMode,
    /// The matrix construction strategy (only used for [`CodecFamily::Classic`]).
    /// Default: [`MatrixMode::Vandermonde`].
    pub matrix_mode: MatrixMode,
    /// Maximum number of parallel jobs for encode/decode operations.
    /// `0` means automatic (uses `available_parallelism()`). Default: `0`.
    pub max_parallel_jobs: usize,
}

impl Default for CodecOptions {
    fn default() -> Self {
        Self {
            fast_one_parity: false,
            inversion_cache: true,
            inversion_cache_capacity: 0,
            codec_family: CodecFamily::Classic,
            leopard_mode: LeopardMode::Disabled,
            matrix_mode: MatrixMode::Vandermonde,
            max_parallel_jobs: 0,
        }
    }
}

/// Builder for [`CodecOptions`].
///
/// Created via [`CodecOptions::builder()`]. All methods chain; call [`build()`](CodecOptionsBuilder::build) to obtain the final `CodecOptions`.
#[derive(Debug, Clone, Copy)]
pub struct CodecOptionsBuilder {
    options: CodecOptions,
}

impl CodecOptions {
    /// Create a new builder with default options.
    pub fn builder() -> CodecOptionsBuilder {
        CodecOptionsBuilder {
            options: CodecOptions::default(),
        }
    }
}

impl CodecOptionsBuilder {
    /// Set the codec algorithm family.
    pub fn codec_family(mut self, family: CodecFamily) -> Self {
        self.options.codec_family = family;
        self
    }

    /// Set the optional automatic Leopard selection mode (see [`LeopardMode`]).
    /// Only takes effect when [`codec_family`](CodecOptionsBuilder::codec_family)
    /// is [`CodecFamily::Classic`] and the field supports the GF(2^8) byte kernels.
    pub fn leopard_mode(mut self, mode: LeopardMode) -> Self {
        self.options.leopard_mode = mode;
        self
    }

    /// Set the matrix construction strategy.
    pub fn matrix_mode(mut self, mode: MatrixMode) -> Self {
        self.options.matrix_mode = mode;
        self
    }

    /// Enable or disable the XOR-only fast path for single parity shards.
    pub fn fast_one_parity(mut self, enabled: bool) -> Self {
        self.options.fast_one_parity = enabled;
        self
    }

    /// Enable or disable the inversion cache.
    pub fn inversion_cache(mut self, enabled: bool) -> Self {
        self.options.inversion_cache = enabled;
        self
    }

    /// Set the inversion cache capacity. `0` for automatic sizing.
    pub fn inversion_cache_capacity(mut self, capacity: usize) -> Self {
        self.options.inversion_cache_capacity = capacity;
        self
    }

    /// Set the maximum number of parallel jobs. `0` for automatic.
    pub fn max_parallel_jobs(mut self, jobs: usize) -> Self {
        self.options.max_parallel_jobs = jobs;
        self
    }

    /// Build the final [`CodecOptions`].
    pub fn build(self) -> CodecOptions {
        self.options
    }
}
