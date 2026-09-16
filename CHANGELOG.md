# Changelog

All notable changes to this project are documented in this file.

- **Versions 0.9.0 – 6.0.0**: originally authored by [Darren Ldl](https://github.com/darrenldl), with later maintenance by the [rust-rse](https://github.com/rust-rse) community (2021–2022).
- **Current mainline repository**: maintained under [houseme/rustfs-erasure-codec](https://github.com/houseme/rustfs-erasure-codec).
- Format follows [Keep a Changelog](https://keepachangelog.com/) for recent versions; older entries keep their simpler historical flat-list format.

---

## Unreleased

## 9.0.0 (2026-09-16)

> Major release: remove the bundled C SIMD backend and make the GF(2^8) SIMD
> stack pure Rust across the supported backend families.

### Breaking
- Removed the bundled `simd_c` backend and its `simd_c/reedsolomon.c` /
  `simd_c/reedsolomon.h` sources from the crate.
- Removed `RSE_BACKEND_OVERRIDE=simd-c`; `simd-c` is no longer a recognised
  runtime backend override.
- Removed `BackendKind::SimdC` and `BackendId::SimdC`. Public backend
  inspection can now report only `Scalar` or `RustSimd` backend kinds.
- Removed the `RUST_REED_SOLOMON_ERASURE_ARCH` C-backend build control.

### Changed
- `simd-neon`, `simd-ssse3`, `simd-avx2`, `simd-avx512`, and `simd-gfni` no
  longer pull in the optional `cc` build dependency or the optional `libc`
  runtime dependency.
- x86_64 fallback selection now drops directly from available Rust SIMD
  backends to `scalar-rust`.
- Backend smoke and benchmark scripts no longer include `simd-c` in strict
  override matrices or summary ordering.
- Strict backend validation now reports unknown override names as not honoured
  instead of treating them like `auto`.

### Validation
- `cargo check --no-default-features`
- `cargo check --features "std simd-accel"`
- `cargo test --workspace --features "simd-accel benchmark-metrics"`
- `cargo fmt --all --check`
- `git diff --check`
- `cargo package --allow-dirty --no-verify`

## 8.0.3 (2026-09-16)

> Patch release: klauspost/reedsolomon compatibility hardening, SIMD loop cleanup, and reproducible backend benchmark governance.

### Fixed
- Hardened Leopard shard-combination validation so `LeopardGF8` and `LeopardGF16` reject invalid data/parity totals at construction time instead of relying on later codec-path failures.
- Rejected Leopard families from the classic `decode_idx` path with explicit unsupported-family errors.
- Accepted data-shard-length required masks in `reconstruct_some` and `reconstruct_some_opt`, matching the common klauspost/reedsolomon selective-recovery calling pattern.
- Reworked the x86_64 generated AVX2 full-encode dispatch into stack-bounded per-parity kernels after downstream RustFS e2e coverage exposed stack overflows on small worker-thread stacks.
- Fixed x86_64 backend selection when only a single SIMD feature such as `simd-avx2` is enabled.

### Changed
- Switched aarch64 NEON and x86 SIMD hot loops to fixed-size chunk views for clearer backend kernels without changing the public API or encoded data format.
- Refreshed the English and Chinese README structure around installation, codec families, memory reuse, streaming, backend overrides, and release validation.

### Added
- Added a dedicated `galois_backend` A/B/B/A benchmark gate that captures raw Criterion estimates per stage and rejects performance conclusions when A1/A2 baseline drift fails.
- Documented the backend benchmark drift gate in `docs/benchmark-methodology.md`.

### Validation
- Local validation covered formatting, workspace tests, SIMD/metrics tests, Clippy with `-D warnings`, backend consistency, and aarch64 backend smoke coverage.
- aarch64 `auto` backend ABBA validation passed 3 consecutive drift-gated runs.
- x86_64 remote ABBA validation remained intentionally inconclusive because repeated runs showed unstable A1/A2 baseline drift; no x86_64 performance improvement or regression is claimed for this release.
- Pull request CI passed Linux, Windows, macOS ARM64, Linux ARM64, ppc64le VSX, ASan, cargo-audit, typos, and backend override regression jobs before merge.

## 8.0.1 (2026-07-23)

> Patch release: safely guard generated AVX2 encode dispatch while preserving the public API and the on-wire Reed-Solomon format.

### Fixed
- Generated x86_64 AVX2 encode dispatch now checks runtime availability before entering AVX2 code and falls back to the generic encoder when AVX2 is unavailable, preventing illegal-instruction failures on older x86_64 CPUs.
- A recognised `RSE_BACKEND_OVERRIDE`, including `scalar`, consistently bypasses generated encode code as well as the selected multiplication backend.
- Updated the `cfg_aliases` build dependency to avoid a nightly compiler warning promoted to an error by the workspace Clippy gate.

### Validation
- Added deterministic generated-vs-generic conformance coverage across supported layouts and SIMD tail boundaries, plus a test-only forced-AVX2-filter 4+2 encode, verify, and reconstruction round trip.
- Verified the std and no_std SIMD combinations on a real no-AVX2 x86_64 host, including Rust 1.96 MSRV checks; CI covers Linux, Windows, macOS ARM64, Linux ARM64, ppc64le VSX, ASan, and explicit scalar/AVX2/GFNI backend overrides.
- The downstream RustFS 6-drive `EC:2` multipart acceptance matrix remains tracked in rustfs/backlog#1453 and rustfs/rustfs#5076.

### Compatibility
- No public Rust API, ABI, feature name, default std dispatch contract, or encoded data format changed.
- `RSE_BACKEND_OVERRIDE` must be set before the process first uses the codec; generated-encode permission is cached consistently with backend selection.
- In no_std builds compiled with `target-feature=+avx2`, deployment CPUs must support AVX2.

### Internal
- Updated the `cc` workspace build dependency to 1.3.0 and refreshed its lockfile-resolved transitive dependencies.

## 8.0.0 (2026-07-14)

> Major release: the full security / performance / robustness audit against [`klauspost/reedsolomon`](https://github.com/klauspost/reedsolomon), plus optional Leopard auto-activation. **The only breaking change for existing callers is that `CodecOptions` (and the new `LeopardMode`) are now `#[non_exhaustive]`.** Callers that only use `ReedSolomon::new` / `encode*` / `reconstruct*` / `verify*` — including downstream RustFS via `rustfs-ecstore` — need no source changes, only the dependency bump to `8`.

### Breaking
- `CodecOptions` is now `#[non_exhaustive]`. Construct it via `CodecOptions::default()` (optionally with `..Default::default()` **inside this crate**) or the `CodecOptions::builder()`; downstream struct literals such as `CodecOptions { codec_family: …, ..Default::default() }` no longer compile and must move to the builder (e.g. `CodecOptions::builder().codec_family(…).build()`). No field was removed or renamed, so builder-based and `default()`-based construction is unaffected.
- `LeopardMode` (and the `CodecFamily` / `MatrixMode` selectors) are `#[non_exhaustive]` enums; exhaustive downstream `match` needs a wildcard arm. Adding variants in future minor releases is therefore non-breaking.

### Added
- **Optional Leopard auto-activation (`LeopardMode`):** `Disabled` (default), `AsNeeded`, `PreferGF16`, `PreferLeopard`, via `CodecOptions::leopard_mode` / `CodecOptionsBuilder::leopard_mode`. When `codec_family` is left at `Classic` on a byte-oriented field, the codec can auto-select a Leopard family as a function of the total shard count, mirroring klauspost/reedsolomon `New()`. `Disabled` is the default and is byte-for-byte identical to prior releases.
- `CodecOptions::builder()` fluent builder, and the `CodecFamily` / `MatrixMode` / `LeopardMode` selectors re-exported at the crate root.
- Aligned-shard helpers and `LEOPARD_SHARD_MULTIPLE` / shard-length utilities.

### Performance
- **Classic GF(2¹⁶):** SIMD-accelerate `mul_slice` via a tower-field decomposition reusing the GF(2⁸) SIMD kernels (AVX2 / SSSE3 / GFNI / NEON), and SIMD-ize its de/re-interleave layout conversion so the x86 GFNI/AVX2 path is no longer bottlenecked on scalar byte shuffling.
- **Leopard GF(2¹⁶) — FFT butterfly multiply** (`mulgf16` / `mulgf16_xor`): SIMD 4-nibble shuffle-table kernel (AVX2 / SSSE3 / NEON).
- **Leopard GF(2¹⁶) — decode error-locator FWHT:** SIMD butterflies (AVX2 / SSE2 / NEON).
- **Leopard GF(2¹⁶) — decode plan cache:** memoise the error locator and FFT/IFFT schedules (keyed by shard counts + erasure pattern), so repeated same-pattern reconstructs skip the fixed per-call cost (large speedups for small-shard / repeated-pattern reconstructs).
- **Leopard GF(2¹⁶) — layout fusion:** fuse the split→user byte-layout conversion into a single pass on both encode and decode (drops an intermediate buffer, a second whole-shard pass, and an allocation); ~+12–18 % encode on the measured configuration.
- **Leopard GF(2¹⁶) — parallel reconstruct:** shards larger than one 64 KiB work chunk now recover across cores (rayon). Warm reconstruct scales ~3.7× at 1 MiB / ~4.9× at 4 MiB on aarch64, exceeding encode throughput for ≥ 256 KiB shards. Single-chunk (≤ 64 KiB) and `no_std` keep the serial path.

### Fixed
- **GF(2¹⁶) alignment / endianness soundness:** replace unaligned host-endian `u16` reinterpretation with explicit little-endian encode/decode (removes potential UB), and **reject GF16 Leopard on big-endian targets** with an error instead of silently corrupting data.
- **Family-aware shard-count guard:** the `total > F::ORDER` guard is now family-aware, so an explicit (or auto-selected) `CodecFamily::LeopardGF16` codec with more than 256 total shards is constructible — previously it was unconditionally rejected with `TooManyShards` before family validation ran. `LeopardGF16` no longer builds an unused GF(2⁸) Vandermonde matrix (which panicked via `Field::nth` past 256 and needlessly allocated up to a `total × data` matrix); it relies solely on its GF(2¹⁶) FFT tables. The Leopard soundness gate now checks `size_of::<Field::Elem>() == 1` instead of `Field::ORDER == 256`.
- **`reconstruct_opt` panic on large shards:** a `LeopardGF16` codec reconstructing a shard at/above the parallel threshold (256 KiB) misdispatched to the Classic inversion-matrix path and panicked (or silently returned wrong data). All `*_opt` entry points now route any Leopard family to the Leopard FFT reconstruct.
- **Multi-chunk partial-last-chunk overrun:** a shard whose length is not a multiple of 64 KiB could over-run the SIMD multiply (or panic on the identity fast path) on its final chunk; fixed by bounding the work lane to the chunk size.
- **AVX2 tail heap overflow** in the GF16 path, plus SIMD `unsafe` SAFETY documentation / lint fixes.
- Integer-overflow hardening (checked / saturating) for capacity, alignment, and shift arithmetic; production-path panic (`unwrap` / `expect`) triage and cleanup.
- **ppc64le VSX backend now actually builds** (it never compiled before), with a dedicated ppc64le CI job.
- Gate the AVX2 FWHT path on `feature = "std"` (runtime feature detection is std-only).

### Internal / CI
- x86_64 Linux SIMD verification: all-features clippy gate + AddressSanitizer; workspace clippy lint baseline; `actions/checkout@v7`.
- Collapse repeated SIMD `cfg` predicates into `cfg` aliases; dedup the `mul_simd` and shard-count-cap dispatch.
- Make the NEON profile-stats and `RS_*` policy env-var tests deterministic under the parallel test runner.
- `chore(deps)`: `spin` 0.12.2.

### Compatibility
- **On-wire byte layout is unchanged**, including the Go/klauspost-compatible Leopard GF16 split layout (verified byte-exact by known-answer tests against Go reference vectors). Data written by 7.x is reconstructed unchanged.
- Verified against downstream **RustFS** (`rustfs-ecstore`): its Classic GF(2⁸) usage compiles against 8.0.0 with no call changes.

## 7.0.2 (2026-07-13)

> Maintained in [houseme/reed-solomon-erasure](https://github.com/houseme/reed-solomon-erasure)
> Patch release: streaming-path correctness and robustness hardening from a deep audit

### Fixed
- `reconstruct_stream`: validate that all present shards read the same length within a block, returning `IncorrectShardSize` instead of silently zero-padding a truncated / length-mismatched shard into wrong recovered data (which previously returned `Ok`).
- `reconstruct_stream`: reset each present cursor's position to `0` before reading, so a cursor left at end-of-buffer (e.g. just written to) is no longer misread as empty, which previously skipped recovery while returning `Ok`.
- `encode_stream` / `verify_stream` / `reconstruct_stream`: clamp `block_size` on entry to `[1 KiB, 16 MiB]`, preventing `block_size = 0` from silently producing empty output and preventing huge values from triggering `Vec::with_capacity` allocation failures.
- `encode_stream` / `verify_stream` / `reconstruct_stream`: validate stream counts at runtime (returning `TooFewShards`) instead of `debug_assert`, which is removed in release builds and could otherwise cause slice out-of-bounds panics or silent partial encoding.
- `encode_stream` / `verify_stream` / `reconstruct_stream`: reject Leopard-family codecs with `UnsupportedCodecFamily` instead of failing with an opaque `IncorrectShardSize` on the first non-64-aligned block.
- `reconstruct_stream`: return `Ok` for an empty dataset (no present shards), consistent with `encode_stream` on empty input, instead of `TooFewShardsPresent`.
- Parallel stream writes now report a write error on the fallback path (instead of a fabricated `Read(0)`) and keep the smallest `shard_index`, making error reporting deterministic.

### Added
- Regression tests covering the streaming-path fixes above.

### Changed
- Documented that `reconstruct_stream` reads present cursors from position `0`, and documented the `encode_stream` zero-padding contract for unequal inputs and its interop limitations.
- Simplified `use_parallel_stream_io_auto` by removing dead branches fully dominated by the final threshold (no behavior change).
- Raised the workspace minimum supported Rust version from 1.95 to 1.96.
- Refreshed direct dependencies: `rand` 0.10.2, `spin` 0.12.1, and `wasm-bindgen` 0.2.126, along with their resolved transitive dependencies.
- Polished the English and Chinese README language links, examples, and codec-family tables for readability.

## 7.0.1 (2026-06-28)

> Maintained in [houseme/reed-solomon-erasure](https://github.com/houseme/reed-solomon-erasure)
> Patch release: stream path benchmark governance and low-allocation hot-path tuning

### Added
- Stream-path benchmark harness covering `encode_stream`, `verify_stream`, and `reconstruct_stream`.
- Structured stream benchmark artifacts at `target/benchmark-smoke/stream-path-results.{json,csv}` with quick / fast / extended profiles.
- `StreamIoMode::{Auto, Serial, Parallel}` and `StreamOptions::with_io_mode(...)` for explicit stream I/O scheduling control.
- Stream benchmark regression support in `scripts/check_benchmark_regression.py`, including `stream_block_size`, `ns_per_block`, and stream-operation thresholds.
- Release-check stream path gate via `RUN_STREAM_PATH_GATE` and `RSE_STREAM_PATH_BASELINE`.
- Archived stream-path before/after benchmark artifacts under `benchmarks/stream-path/2026-06-28-cooldown/`.

### Changed
- Reduced stream block read overhead by avoiding unconditional full-buffer padding and only zero-filling short-read ranges.
- Reused stream read-length scratch state on serial read paths to reduce per-block temporary allocation churn.
- Added Auto stream I/O selection that avoids rayon scheduling overhead for small-block / low-shard workloads while keeping parallel I/O available for larger cases.
- Reworked `reconstruct_stream` to precompute present/missing metadata, reuse the outer reconstruction container, and retain present-shard buffer allocations across blocks.
- Refined the reconstruct stream read hot path after review by removing the per-block `indexed` temporary vector while preserving the faster small-block serial path.
- Updated benchmark methodology, streaming API docs, release checklist guidance, and performance index entries for stream path validation.

### Fixed
- Prevented broad stream fast-profile regressions through cooldown-based benchmark reruns and `ns_per_block` release gating.
- Corrected stale streaming API documentation examples to use `StreamIoMode` / `with_io_mode(...)`.
- Kept `verify_stream` workspace changes data-driven by documenting rejected variants where benchmark evidence did not justify extra complexity.

## 7.0.0 (2026-06-16)

> Maintained in [houseme/reed-solomon-erasure](https://github.com/houseme/reed-solomon-erasure)
> Current line: Rust 2024 rewrite and runtime SIMD/Leopard work

### Added
- Runtime-dispatched GF(2^8) backend architecture under `src/galois_8/`, replacing the prior single-file layout with dedicated modules (`backend`, `policy`, `profile`, `scalar`, `legacy`, `x86`, `aarch64`).
- New Rust SIMD backend implementations and metadata:
  - x86 backends: `ssse3`, `avx2`, `avx512`, `gfni` (`src/galois_8/x86/*.rs`)
  - aarch64 backend: `neon` (`src/galois_8/aarch64/neon.rs`)
  - legacy C fallback path retained in `src/galois_8/legacy/simd_c.rs`
- Backend runtime control and observability:
  - `RSE_BACKEND_OVERRIDE` backend selection override
  - `RSE_STRICT_BACKEND_OVERRIDE` strict override validation path
  - exported runtime backend metadata APIs (`active_backend_name/id/kind`)
- Public tuning/profile API surface:
  - `CodecOptions`, `MatrixMode`
  - `ParallelPolicy`, `ParallelDecision`, `PARALLEL_POLICY_VERSION`
  - `ReconstructionCacheStats`, `ReconstructionCacheAnalysis`, `RuntimeProfileStats`
- Parallel policy environment controls:
  - `RS_PARALLEL_POLICY_MIN_PARALLEL_SHARD_BYTES`
  - `RS_PARALLEL_POLICY_MIN_BYTES_PER_JOB`
  - `RS_PARALLEL_POLICY_MAX_JOBS`
- New benchmark and validation assets:
  - benches: `benches/throughput_matrix.rs`, `benches/galois_backend.rs`, `benches/common/mod.rs`
  - tests: `tests/benchmark_smoke.rs`, `tests/golden_vectors.rs`, `tests/selftest.rs`, `tests/common/mod.rs`
  - x86 benchmark artifact: `benchmarks/x86_64-simd/2026-05-25-amd-epyc-9v45.json`
- New release/perf automation scripts:
  - `scripts/release-check.sh` (fast/extended profiles)
  - `scripts/check_backend_consistency.sh` (arch-aware backend sweep)
  - `scripts/check_benchmark_regression.py` (operation-threshold regression gate)
  - `scripts/collect_x86_simd_benchmarks.sh`
  - `scripts/summarize_x86_simd_benchmarks.py`

### Changed
- Hot-path compute behavior in `src/core.rs`:
  - consolidated parallel scheduler/policy decisions
  - optimized required-only reconstruct paths (`reconstruct_some`)
  - optimized verify scratch-buffer path
  - expanded reconstruction cache policy/capacity behavior and profiling counters
- Project baseline and dependency stack:
  - Rust edition `2018 -> 2024`
  - `rust-version = 1.95`
  - `std` feature now includes `rayon`
  - cache backend switched from `lru` to `hashlink`
  - dependency updates across runtime/dev/build (`rand`, `quickcheck`, `criterion`, `parking_lot`, `smallvec`, `spin`, `libm`, `cc`)
- Validation/reporting workflow:
  - benchmark smoke tests now export structured output to `target/benchmark-smoke/smoke-results.json` and `target/benchmark-smoke/smoke-results.csv`
  - release checks now support profile-based execution (`VALIDATION_PROFILE=fast|extended`)
- Build/docs alignment:
  - README updated to describe Rust SIMD runtime dispatch as preferred path with legacy C fallback
  - `build.rs` and package metadata updated to match new backend/validation workflow

### Fixed
- Restored std and no-std validation coverage paths in mainline test flow.
- Fixed compatibility with upgraded randomness and quickcheck interfaces after dependency updates.
- Removed unsafe unwrap-dependent paths introduced by dependency evolution and tightened unsafe boundaries with clippy-focused cleanup.
- Resolved post-sync x86 SIMD conflict and restored backend correctness validation coverage in runtime dispatch paths.

> Maintainers: [Michael Vines](https://github.com/mvines) and community ([rust-rse](https://github.com/rust-rse))

## 6.0.0
- Use LruCache instead of InversionTree for caching data decode matrices
  - See [PR #104](https://github.com/rust-rse/reed-solomon-erasure/pull/104)
- Minor code duplication
  - See [PR #102](https://github.com/rust-rse/reed-solomon-erasure/pull/102)
- Dependencies update
  - Updated `smallvec` from `0.6.1` to `1.8.0`

## 5.0.3
- Fixed cross build bug for aarch64 with simd-accel
  - See [PR #100](https://github.com/rust-rse/reed-solomon-erasure/pull/100)

## 5.0.2
* Add support for `RUST_REED_SOLOMON_ERASURE_ARCH` environment variable and stop using `native` architecture for SIMD code
  - See [PR #98](https://github.com/rust-rse/reed-solomon-erasure/pull/98)

## 5.0.1
- The `simd-accel` feature now builds on M1 Macs
  - See [PR #92](https://github.com/rust-rse/reed-solomon-erasure/pull/92)
- Minor code cleanup

> Note: [Darren Ldl](https://github.com/darrenldl) stepped back from maintenance. Versions 5.0.0–6.0.0 were community-maintained.

## 5.0.0
- Merged several PRs
- Not fully reviewed as I am no longer maintaining this crate

## 4.0.2
- Updated build.rs to respect RUSTFLAGS's target-cpu if available
  - See [PR #75](https://github.com/darrenldl/reed-solomon-erasure/pull/75)
- Added AVX512 support
  - See [PR #69](https://github.com/darrenldl/reed-solomon-erasure/pull/69)
- Disabled SIMD acceleration when MSVC is being used to build the library
  - See [PR #67](https://github.com/darrenldl/reed-solomon-erasure/pull/67)
- Dependencies update
  - Updated `smallvec` from `0.6` to `1.2`

## 4.0.1
- Updated SIMD C code for Windows compatibility
  - Removed include of `unistd.h` in `simd_c/reedsolomon.c`
  - Removed GCC `nonnull` attribute in `simd_c/reedsolomon.h`
  - See PR [#63](https://github.com/darrenldl/reed-solomon-erasure/pull/63) [#64](https://github.com/darrenldl/reed-solomon-erasure/pull/64) for details
- Replaced use of `libc::uint8_t` in `src/galois_8.rs` with `u8`

## 4.0.0
- Major API restructure: removed `Shard` type in favor of generic functions
- The logic of this crate is now generic over choice of finite field
- The SIMD acceleration feature for GF(2^8) is now activated with the `simd-accel` Cargo feature. Pure-rust behavior is default.
- Ran rustfmt
- Adds a GF(2^16) implementation

## 3.1.2 (not published)
- Doc fix
  - Added space before parentheses in code comments and documentation
- Disabled SIMD C code for Android and iOS targets entirely

## 3.1.1
- Fixed `Matrix::augment`
  - The error checking code was incorrect
  - Since this method is used in internal code only, and the only use case is a correct use case, the error did not lead to any bugs
- Fixed benchmark data
  - Previously used MB=10^6 bytes while I should have used MB=2^20 bytes
  - Table in README has been updated accordingly
    - The `>= 2.1.0` data is obtained by measuring again with the corrected `rse-benchmark` code
    - The `2.0.X` and `1.X.X` data are simply adjusted by multiplying `10^6` then dividing by `2^20`
- Dependencies update
  - Updated `rand` from `0.4` to `0.5.4`
- Added special handling in `build.rs` for CC options on Android and iOS
  - `-march=native` is not available for GCC on Android, see issue #23

## 3.1.0
- Impl'd `std::error::Error` for `rustfs_erasure_codec::Error` and `rustfs_erasure_codec::SBSError`
  - See issue [#17](https://github.com/darrenldl/reed-solomon-erasure/issues/17), suggested by [DrPeterVanNostrand](https://github.com/DrPeterVanNostrand)
- Added fuzzing suite
  - No code changes due to this as no bugs were found
- Upgraded InversionTree QuickCheck test
  - No code changes due to this as no bugs were found
- Upgraded test suite for main codec methods (e.g. encode, reconstruct)
  - A lot of heavy QuickCheck tests were added
  - No code changes due to this as no bugs were found
- Upgraded test suite for ShardByShard methods
  - A lot of heavy QuickCheck tests were added
  - No code changes due to this as no bugs were found
- Minor code refactoring in `reconstruct_internal` method
  - This means `reconstruct` and related methods are slightly more optimized

## 3.0.3
- Added QuickCheck tests to the test suite
  - InversionTree is heavily tested now
- No code changes as no bugs were found
- Deps update
  - Updated rayon from 0.9 to 1.0

## 3.0.2
- Same as 3.0.1, but 3.0.1 had unapplied changes

## 3.0.1 (yanked)
- Updated doc for `with_buffer` variants of verifying methods
  - Stated explicitly that the buffer contains the correct parity shards after a successful call
- Added tests for the above statement

## 3.0.0
- Added `with_buffer` variants for verifying methods
  - This gives user the option of reducing heap allocation(s)
- Core code clean up, improvements, and review, added more AUDIT comments
- Improved shard utils
- Added code to remove leftover parity shards in `reconstruct_data_shards`
  - This means one fewer gotcha of using the methods
- `ShardByShard` code review and overhaul
- `InversionTree` code review and improvements

## 2.4.0
- Added more flexibility for `convert_2D_slices` macro
  - Now accepts expressions rather than just identifiers
  - The change requires change of syntax

## 2.3.3
- Replaced all slice splitting functions in `misc_utils` with std lib ones or rayon ones
  - This means there are fewer heap allocations in general

## 2.3.2
- Made `==`(`eq`) for `ReedSolomon` more reasonable
  - Previously `==` would compare
    - data shard count
    - parity shard count
    - total shard count
    - internal encoding matrix
    - internal `ParallelParam`
  - Now it only compares
    - data shard count
    - parity shard count

## 2.3.1
- Added info on encoding behaviour to doc

## 2.3.0
- Made Reed-Solomon codec creation methods return error instead of panic when shard numbers are not correct

## 2.2.0
- Fixed SBS error checking code
- Documentation fixes and polishing
- Renamed `Error::InvalidShardsIndicator` to `Error::InvalidShardFlags`
- Added more details to documentation on error handling
- Error handling code overhaul and checks for all method variants
- Dead commented out code cleanup and indent fix

## 2.1.0
- Added Nicolas's SIMD C code files, gaining major speedup on supported CPUs
- Added support for "shard by shard" encoding, allowing easier streamed encoding
- Added functions for shard by shard encoding

## 2.0.0
- Complete rewrite of most code following Klaus Post's design
- Added optimsations (parallelism, loop unrolling)
- 4-5x faster than `1.X.X`

## 1.1.1
- Documentation polish
- Added documentation badge to README
- Optimised internal matrix related operations
  - This largely means `decode_missing` is faster

## 1.1.0
- Added more helper functions
- Added more tests

## 1.0.1
- Added more tests
- Fixed decode_missing
  - Previously may reconstruct the missing shards with incorrect length

## 1.0.0
- Added more tests
- Added integration with Codecov (via kcov)
- Code refactoring
- Added integration with Coveralls (via kcov)

## 0.9.1
- Code restructuring
- Added documentation

## 0.9.0
- Base version
