# Changelog

All notable changes to this project are documented in this file.

- **Versions 0.9.0 – 6.0.0**: originally authored by [Darren Ldl](https://github.com/darrenldl), with later maintenance by the [rust-rse](https://github.com/rust-rse) community (2021–2022).
- **Current mainline repository**: maintained under [houseme/rustfs-erasure-codec](https://github.com/houseme/rustfs-erasure-codec).
- Format follows [Keep a Changelog](https://keepachangelog.com/) for recent versions; older entries keep their simpler historical flat-list format.

---

## Unreleased

### Fixed
- Make the independent reconstruction hotspot gate compare every baseline scenario and reject empty, duplicate, dropped or malformed evidence, non-finite metrics/limits and conflicting workload/provenance fields. Reuse the main gate's number and provenance normalization.
- Make the native recovery-policy test use deterministic disabled/small/large L2 estimates instead of assuming every host permits a 256 KiB chunk; verify both chunk sizes and job counts.
- Make the Galois ABBA analyzer reject empty/incomplete campaigns, duplicate rows, revision/backend conflicts and either-side drift. Failed campaigns return nonzero and withhold all candidate attribution instead of emitting an empty PASS.
- Replace the TypeScript facade's manually maintained raw WASM ABI with independent factories around complete, version-matched wasm-bindgen output. Preserve result codes and data-only recovery while supporting real Node/browser initialization and memory growth.
- Remove the fuzz assumption that zeroing a shard must invalidate a codeword; compare reconstructed bytes against the original complete codeword instead.
- Gate test-only NEON thread-local profile capture on `std`, allowing `no_std + simd-accel` test builds.
- Finish Classic inverse planning before initializing missing destinations; singular custom matrices leave outputs missing, and empty selective requests skip unused inverses.
- Preserve effective encoding matrices and resolved codec families when cloning; codec equality and prepared reconstruction workspaces now distinguish incompatible matrices.
- Use one consistent XOR parity matrix for single-parity fast mode across encoding, updates, incremental encoding, and recovery; reject conflicting custom matrices.
- Validate Leopard shard counts, actual lengths, byte views, required masks, and field-order boundaries before decoding; publish only requested recovered shards and never expose stale fixed buffers after a later output error.
- Reject malformed optimized/workspace inputs and preserve selective parity recovery without restoring unrequested shards.
- Bound full and incremental Classic encoding/reconstruction by the configured parallel job limit, and account for field element width in byte budgets.
- Reject unequal verification-stream lengths while retaining documented zero-padding for encoding, including short reads and partial final blocks.
- Return stable WASM errors for invalid counts, buffer shapes, and availability flags; validate JavaScript integers before wasm32 coercion and copy-back, preserving existing error codes and data-only reconstruction.
- Check GF16 workspace element and byte-size multiplication before allocation, preventing release-mode size wraparound.


### Changed
- Borrow Classic coefficient rows and serial destination references through iteration instead of returning and moving temporary inline view collections. Bulk and stream adapters collect only where contiguous slices are required, retaining exact reservation hints and publication/error behavior. Add an end-to-end wide-parity allocation regression that reproduces four requests on the prior source and enforces a three-request budget.
- Construct scoped Classic recovery plans in their execution storage through the same shape/plan fillers used by owned workspaces. Avoid large temporary Result transfers while preserving complete-layout early returns, validation order, erasure selection and write-before-publication contracts; throughput acceptance is measured separately.
- Bound generated-native byte views to 12 data/4 parity inline slots through the shared checked constructor, retaining Leopard 32/32 storage. Keep the native preparation object within 512 bytes without changing validation, kernels or dispatch limits.
- Validate Classic recovery lengths once across the present prefix and erasure suffix, preserving complete-layout fast returns, error priority, workspace layout checks and publication order without adding a shape-specific path.
- Reserve progressive DecodeIdx missing-index metadata by parity count, avoiding heap spills for warmed 32+16 batches while preserving existing XOR, mask and error semantics.
- Limit retained Leopard encoder work plus scratch to 4 MiB per family per thread, checking actual allocation capacity. Oversized calls keep resources local and preserve small warm caches; repeated oversized encodes may allocate again. Remove unused native selector stubs without expanding dispatch.
- Prepare checked GF8 shard views and select a generated ISA kernel once per encode operation; share serial/parallel setup and never fall back after a kernel writes parity.
- Use the resolved codec family as the sole algorithm state and one Classic recovery plan across generic, optimized, workspace, and stream entry points.
- Add opt-in safe GF8 byte views to `Field`, replacing element-size-based reinterpretation in specialized kernels while preserving generic field arithmetic and the default encoded format.
- Share immutable Classic codec matrices; derive FFT family metadata from shard counts and retain independent reconstruction caches for cloned codecs.
- Correct public builder examples for non-exhaustive codec options.
- Make benchmark gates reject empty comparisons, duplicate/dropped cases, invalid metrics, and conflicting provenance; deliberate partial profiles and asymmetric missing provenance require explicit opt-in and remain visible in reports.


### Performance
- Fuse NEON parity accumulators around each streamed input vector so all outputs share its load and nibble extraction; capture checked buffer pointers before the vector loop to remove repeated slice checks.
- Skip all-present Classic plan construction through one validated fast return, retain zero-allocation wide layouts, and execute prepared native views by reference.
- Remove per-chunk kernel-selection reductions and duplicate policy decisions from generated encoding; inline the measured NEON route's four-job metadata.
- Remove unused Classic matrix construction from Leopard GF8 setup. Skip inversion for parity-only recovery and compose selective parity rows directly instead of cloning all shards or recovering unrequested data.
- Reuse one bounded recovery executor for every output width; apply each stage policy once and keep wide workspace layout validation free of rebuilt index lists.
- Apply cache-line-aligned L2 budgets to parallel chunks, cache hardware cache-size detection, and reuse generated SIMD in the parallel entry point's serial fallback.
- Disable shared Leopard GF8 atomic profiling updates unless `benchmark-metrics` is enabled.
- Cache one bounded GF8/GF16 FFT encoding plan per thread and convert GF16 input directly into initialized work lanes instead of allocating complete converted shards.
- Warmed 10+4 encodes with 32 KiB shards now allocate no heap memory for either Leopard family; `no_std` retains local plan construction.
- Reuse stream verification scratch, parallel read-length metadata, and missing-shard buffers across blocks; use optimized encoding dispatch and inline common shard views.
- Reuse owned GF8 error-locator/FFT decode plans and initialized working buffers across matching erasure patterns, with per-thread retention capped at 64 KiB of plans and 4 MiB of work. Inline common recovery output metadata and limit tail transforms to active bytes.
- Convert GF16 recovery inputs one chunk at a time, reuse at most 4 MiB of decode work per calling/Rayon worker thread, and avoid whole-capacity tail transforms. Warmed 10+4 recovery of one 32 KiB data shard now allocates only the 32 KiB temporary output for either Leopard family.
- Prepare a stream recovery matrix and ordered survivor/output rows once after the first validated block; reuse them across blocks and short tails, including with the inversion cache disabled.
- Use the generated NEON encoder inside disjoint parallel jobs for 10+4 layouts with shards of at least 1 MiB and a 2-4 job budget. Other layouts, larger budgets, explicit backend overrides, and x86 keep their existing dispatch; broader activation awaits stable measurements.


### Validation
- Validate runtime `0a2d5ee` on native Linux x86_64 (491 tests), macOS/ARM64 Docker std+SIMD+metrics (488 tests) and genuine no_std+SIMD (297 tests), plus four pinned release fixtures, 14 Node tests and eight Chrome tests. Archive both intermediate and final controlled downstream results: final degraded-read throughput point estimates are -0.25% versus merged main and +1.14% versus the previous runtime, with overlapping phase confidence intervals. Preserve the complete 66-case campaign and its global rejection after seven drift failures; keep all candidate ratios null and #2853 open.
- Extend the recovery probe from 58 to 66 cases with ordinary Classic data recovery at downstream-relevant 4+2/16 KiB and 10+4/32 KiB sizes, one/two missing data shards, and separate retained-slot/Option<Vec> adapter costs. Preserve complete preflight/final byte and presence oracles.
- Complete two controlled-I/O downstream ABBAs for earlier runtime `9fcae45`, with 320 raw samples and all drift gates passing. That runtime remains 1.50% slower for integrity-enabled degraded reads versus merged main; its +0.29% point estimate versus the previous runtime has overlapping confidence intervals and is not a proven speedup. Keep #2853 open and archive every result.
- Complete an independent pinned ECStore four-cell ABBA with 160 raw samples: healthy reads remain essentially flat; degraded-read throughput observations are -1.47% with bitrot and -2.62% without. Preserve confidence intervals and keep the optimization follow-up open rather than describing the drift pass as a speedup.
- Archive all 232 native Linux x86 VM ABBA observations with source/build/resource provenance and cleanup checks. Reject the entire latency/allocation attribution campaign after seven of 58 cases fail the dual 5% drift gate; retain null candidate deltas and do not retry a selected subset.
- Validate earlier runtime `9fcae45` on native Linux x86_64 (490 tests), ARM64 Linux Docker std+SIMD+metrics (487 tests) and no_std+SIMD (297 tests), plus four pinned release compatibility fixtures and real Node/Chrome integration. Record the earlier 85 downstream tests and generated-WASM CI (14 Node/eight Chromium) against `c68c257` separately. Correct nested-child test double counting from original logs and preserve source/VM/physical-host evidence boundaries.
- Pass 61 Python regression tests, including 29 end-to-end ABBA analyzer cases and the original empty-CSV reproducer.
- The 2026-10-17 completion audit passes 486 workspace SIMD/metrics tests and 291 no_std tests, 400 small and five wide cross-revision byte comparisons, focused AddressSanitizer tests, 30,000 final fuzz executions, all-feature Clippy and x86/i686/wasm32 compile checks. Record unexecuted platform/runtime acceptance explicitly in `docs/erasure-codec-completion-audit-2026-10-17.md`.
- The benchmark regression gate passes 32 Python tests, covering empty/incomplete artifacts, alias conflicts, strict metadata, numeric thread budgets and legacy compatibility.
- Archive rejected grouping and unstable separate-process probes alongside the retained fused NEON results. All 18 final same-buffer paired cells pass two 5% drift checks; local 32 KiB 10+4/12+4/8+4 latency observations improve about 8.7%/9.8%/5.2%, with a roughly 8 ns small 4+2 tradeoff recorded in the review.
- Direct generated-NEON tests compare all six supported shapes against scalar field arithmetic across 258 shape/length combinations, including every vector tail, unaligned buffers, zero/identity/high coefficients, empty checked invocations, and untouched output guards.
- The 2026-10-16 refactor passed stable 1.99.1 workspace SIMD/metrics tests, no_std SIMD tests, all-feature Clippy, focused AddressSanitizer, x86 library/test and wasm32 compile checks, and both baseline correctness reproducers.
- Archive all 43 final A/B/B/A cases and both superseded drafts. Allocation observations repeat, but latency drift rejects the overall comparison; see `docs/erasure-codec-review-refactor-2026-10-16.md` and backlog #2853.
- Workspace tests with SIMD and metrics, production SIMD tests, `no_std` tests, all-feature Clippy (including fuzz targets), and Rust 1.98.1 library compilation passed.
- x86_64 and wasm32 cross-compilation passed; 120 Classic/GF8/GF16 parity fixtures matched the previous revision byte for byte.
- AddressSanitizer passed 22 field/Leopard regression tests; 11 original release reproducers, 18 Python gate tests, and 6 JavaScript wrapper tests passed.
- For warmed 10+4 / 32 KiB shards, all-present Leopard reconstruction now performs zero allocations; requested-one-data recovery reduced cumulative allocation bytes from 1,017,342 to 590,672 (GF8) and from 1,476,060 to 1,049,390 (GF16).
- Local small-output parallel A/B/B/A checks included baseline drift in some cases; no general throughput, x86 runtime, browser integration, or downstream RustFS performance claim is made.
- Follow-up recovery/stream/SIMD changes passed workspace SIMD+metrics tests, no_std tests, all-feature Clippy, focused AddressSanitizer regressions, Rust 1.98.1 compilation, and x86 cross-compilation.
- Archive all follow-up A/B/B/A observations, including rejected unstable/broader NEON trials. Only the measured 10+4/1 MiB/small-budget NEON route is enabled; local latency observations are not cross-machine or downstream RustFS guarantees.


### Added
- Add a pinned RustFS benchmark overlay with degraded-reader scenarios, independent full-plaintext preflight and an observed output sink; require identical overlays and isolated release measurements for downstream comparisons.
- Run CI and security regressions on the review-fixes branch, gate releases on real generated-WASM Node/Chromium integration, and select the core package for genuine no_std and no_std+SIMD tests without WASM feature unification.
- Add a locked zero-runtime-dependency WASM package build and real Node/browser tests, including a complete independent parity oracle, fixed golden vector, isolated instances and offline package-install smoke coverage.
- Preserve the DecodeIdx workspace allocation study and all exploratory ABBA samples; keep the simpler capacity correction instead of introducing an API from preparation-excluding, unreserved timing.
- Add `--strict-provenance` to the benchmark gate, requiring comparable CPU/compiler/build/backend/features/thread budgets and stream I/O metadata, with consistent JSON/CSV aliases and no override for supplied conflicts.
- Exercise bounded family/matrix/fast/API/error/workspace combinations through a shared deterministic and libFuzzer contract helper; preserve a reusable cross-revision parity probe including wide FFT chunk boundaries.
- Add explicit-presence `reconstruct_stream_from` for independent non-Seek readers/writers. Share one block recovery engine with the legacy Cursor adapter, retaining plans and buffers and documenting partial-output error semantics.
- Expose a versioned `EncodingFormat<F>` with explicit portable metadata and share its effective identity across codec equality and prepared recovery workspaces; remove duplicated identity state without serializing on hot paths.
- Extend serial encode probes to all six generated layouts (58 total recovery-path cases), independently verify parity, and preserve a standalone paired-comparison probe with per-phase iteration counts.
- Expand the recovery probe to 43 cases with poisoned-output/presence preflights, constructors, serial encoding, optimized recovery, and validated all-present workloads.
- Add the `recovery_paths` JSONL benchmark with deterministic warm fixtures, cumulative allocation counts, latency samples, backend/features/revision provenance, and documented measurement limits.


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
