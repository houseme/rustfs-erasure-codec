# rustfs-erasure-codec

[![CI](https://github.com/houseme/rustfs-erasure-codec/actions/workflows/ci.yml/badge.svg)](https://github.com/houseme/rustfs-erasure-codec/actions/workflows/ci.yml)
[![Crates](https://img.shields.io/crates/v/rustfs-erasure-codec.svg)](https://crates.io/crates/rustfs-erasure-codec)
[![Documentation](https://docs.rs/rustfs-erasure-codec/badge.svg)](https://docs.rs/rustfs-erasure-codec)
[![dependency status](https://deps.rs/repo/github/houseme/rustfs-erasure-codec/status.svg)](https://deps.rs/repo/github/houseme/rustfs-erasure-codec)
[![Crates.io Total Downloads](https://img.shields.io/crates/d/rustfs-erasure-codec)](https://crates.io/crates/rustfs-erasure-codec)
[![Crates.io License](https://img.shields.io/crates/l/rustfs-erasure-codec)](https://crates.io/crates/rustfs-erasure-codec)

English | [Chinese](README_CN.md)

`rustfs-erasure-codec` is a modern Rust implementation of Reed-Solomon erasure coding for
memory-resident, progressive, and block-streaming workloads.

The current `8.0.1` line provides:

- classic Reed-Solomon over `GF(2^8)` and `GF(2^16)`
- runtime-dispatched SIMD backends for `galois_8`
- Leopard GF8 and Leopard GF16 codec families
- incremental and targeted recovery APIs
- reusable verification and reconstruction buffers
- block-based streaming encode, verify, and reconstruct APIs
- `no_std` support and a WASM companion crate

WASM bindings live in [wasm/README.md](wasm/README.md).

## Highlights

- `galois_8::ReedSolomon` is the main optimized path for general-purpose use.
- `galois_16::ReedSolomon` remains available for classic `GF(2^16)` workflows.
- `CodecOptions` controls codec family, matrix mode, inversion-cache behavior, and parallel policy.
- `VerifyWorkspace`, `ShardSlot<T>`, and aligned-shard helpers reduce hot-path allocation churn.
- `galois_8::OptionVecReconstructWorkspace` reuses planning for repeated `Option<Vec<u8>>` reconstruct calls with a
  stable missing pattern.
- `decode_idx(...)`, `reconstruct_some(...)`, and `ShardByShard` cover progressive and selective workflows.
- `stream::StreamOptions` provides block-based streaming on the classic `galois_8` path.

## Install

Add the crate:

```toml
[dependencies]
rustfs-erasure-codec = "8.0.1"
```

Enable SIMD acceleration when throughput matters:

```toml
[dependencies]
rustfs-erasure-codec = { version = "8.0.1", features = ["simd-accel"] }
```

Or enable a narrower backend set:

```toml
[dependencies]
rustfs-erasure-codec = { version = "8.0.1", features = ["simd-neon"] }   # aarch64
# rustfs-erasure-codec = { version = "8.0.1", features = ["simd-ssse3"] } # x86_64
# rustfs-erasure-codec = { version = "8.0.1", features = ["simd-avx2"] }  # x86_64
# rustfs-erasure-codec = { version = "8.0.1", features = ["simd-avx512"] }# x86_64
# rustfs-erasure-codec = { version = "8.0.1", features = ["simd-gfni"] }  # x86_64
# rustfs-erasure-codec = { version = "8.0.1", features = ["simd-vsx"] }   # powerpc64
```

Notes:

- `std` is enabled by default.
- `simd-accel` is the umbrella feature that enables all supported SIMD backends.
- Runtime dispatch is safe: unsupported ISAs fall back to scalar execution.

## Quick Start

```rust
use rustfs_erasure_codec::galois_8::ReedSolomon;
use rustfs_erasure_codec::VerifyWorkspace;

fn main() {
    let rs = ReedSolomon::new(3, 2).unwrap();

    let mut shards = vec![
        vec![0, 1, 2, 3],
        vec![4, 5, 6, 7],
        vec![8, 9, 10, 11],
        vec![0, 0, 0, 0],
        vec![0, 0, 0, 0],
    ];

    rs.encode(&mut shards).unwrap();

    let original = shards.clone();
    let mut missing: Vec<Option<Vec<u8>>> = shards.into_iter().map(Some).collect();
    missing[0] = None;
    missing[4] = None;

    rs.reconstruct(&mut missing).unwrap();

    let rebuilt: Vec<Vec<u8>> = missing.into_iter().map(|shard| shard.unwrap()).collect();
    let mut workspace = VerifyWorkspace::new(&rs, rebuilt[0].len());

    assert!(rs.verify_with_workspace(&rebuilt, &mut workspace).unwrap());
    assert_eq!(rebuilt, original);
}
```

For repeated verification calls, prefer `verify_with_workspace(...)` or
`verify_with_buffer(...)` over plain `verify(...)`.

For repeated `Option<Vec<u8>>` reconstruct calls that keep the same missing
pattern, prepare a reusable reconstruct workspace once and reuse it across
calls:

```rust
use rustfs_erasure_codec::galois_8::ReedSolomon;

let rs = ReedSolomon::new(10, 4).unwrap();
let mut shards = vec![vec![0u8; 1024]; 14];
rs.encode( & mut shards).unwrap();

let mut missing: Vec<Option<Vec<u8> > > = shards.into_iter().map(Some).collect();
missing[0] = None;
missing[10] = None;

let workspace = rs.prepare_reconstruct_opt_workspace( & missing).unwrap();
rs.reconstruct_opt_with_workspace( & mut missing, & workspace).unwrap();
```

## Memory Reuse Helpers

For repeated reconstruct flows, `ShardSlot<T>` lets you keep ownership of missing-shard buffers:

```rust
use rustfs_erasure_codec::galois_8::{ReedSolomon, mark_missing_slots, shards_to_slots};

fn main() {
    let rs = ReedSolomon::new(4, 2).unwrap();

    let mut shards = vec![
        vec![0, 1, 2, 3],
        vec![4, 5, 6, 7],
        vec![8, 9, 10, 11],
        vec![12, 13, 14, 15],
        vec![0, 0, 0, 0],
        vec![0, 0, 0, 0],
    ];
    rs.encode(&mut shards).unwrap();

    let mut slots = shards_to_slots(&shards);
    mark_missing_slots(&mut slots, &[1, 5]);
    rs.reconstruct(&mut slots).unwrap();

    assert!(slots[1].is_present());
    assert!(slots[5].is_present());
}
```

For SIMD-sensitive `galois_8` workloads, aligned shard helpers are available:

- `rustfs_erasure_codec::galois_8::alloc_aligned_shards(...)`
- `galois_8::ReedSolomon::alloc_aligned(...)`

## Codec Families

`CodecOptions::codec_family` selects the algorithm family:

| Family        | Status                                   | Notes                                                                                                                                                                                                                |
|---------------|------------------------------------------|----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `Classic`     | fully supported                          | Default family. Supports `update`, `encode_single*`, `decode_idx`, `reconstruct_some`, and matrix-mode selection.                                                                                                    |
| `LeopardGF8`  | supported on the `galois_8` path         | FFT-based codec over `GF(2^8)`. Requires shard lengths that are multiples of 64 bytes and supports up to 256 total shards. Classic-only APIs such as `update`, `encode_single*`, and `decode_idx` are not supported. |
| `LeopardGF16` | supported for high shard-count workflows | FFT-based codec over `GF(2^16)` for larger total shard counts. Classic-only APIs such as `update`, `encode_single*`, and `decode_idx` are not supported.                                                             |

Example:

```rust
use rustfs_erasure_codec::galois_8::ReedSolomon;
use rustfs_erasure_codec::{CodecFamily, CodecOptions};

let rs = ReedSolomon::with_options(
32,
16,
CodecOptions {
codec_family: CodecFamily::LeopardGF8,
..CodecOptions::default ()
},
)
.unwrap();
```

Important Leopard-family notes:

- shard lengths must be multiples of 64 bytes
- all shard buffers must be the same length
- `decode_idx(...)`, `update(...)`, and `encode_single*` remain Classic-only

## Matrix Modes

`CodecOptions::matrix_mode` applies to `CodecFamily::Classic`:

- `Vandermonde`
- `Cauchy`
- `JerasureLike`
- `Custom`

If you need compatibility with established classic payload layouts, stay on
`MatrixMode::Vandermonde`.

Minimal custom-matrix example:

```rust
use rustfs_erasure_codec::galois_8::ReedSolomon;
use rustfs_erasure_codec::CodecOptions;

let custom_rows = vec![vec![1u8, 1, 1], vec![1u8, 2, 4]];
let rs = ReedSolomon::with_custom_matrix(3, 2, & custom_rows, CodecOptions::default ()).unwrap();
```

## Progressive And Targeted APIs

### Progressive Recovery

`decode_idx(...)` is available on classic `galois_8::ReedSolomon` and is useful when input shards arrive in phases
instead of a single reconstruct call.

### Targeted Recovery

`reconstruct_some(...)` reconstructs only the shards you mark as required.

### Shard-By-Shard Encoding

`ShardByShard` provides a stateful progressive encoder for workflows that feed data shards incrementally.

## Streaming API

The streaming API lives under `rustfs_erasure_codec::stream` and is available with the default `std` feature.

Main entry points:

- `encode_stream(...)`
- `verify_stream(...)`
- `reconstruct_stream(...)`

Current scope and limitations:

- implemented on the classic `galois_8` path
- tuned for block-based processing via `StreamOptions`
- `reconstruct_stream(...)` currently uses `Cursor<Vec<u8>>`; present cursors are read from position `0`
- inputs are validated up front (shard counts, equal present-shard lengths, block size); invalid inputs return a `StreamError` instead of producing wrong or empty output
- Leopard-family streaming (encode, verify, reconstruct) returns `UnsupportedCodecFamily`

Use this path when your data should be processed in bounded blocks instead of holding the full shard matrix in memory.

## Runtime Backend Control

The `galois_8` path exposes runtime backend inspection and override hooks.

Environment variables:

- `RSE_BACKEND_OVERRIDE`
- `RSE_STRICT_BACKEND_OVERRIDE=1`
- `RUST_REED_SOLOMON_ERASURE_ARCH`

An unset or `auto` `RSE_BACKEND_OVERRIDE` allows generated SIMD encode code when the platform supports it. Any recognised explicit override, including `scalar`, uses the selected generic backend for encode and bypasses generated SIMD codegen. This makes `RSE_BACKEND_OVERRIDE=scalar` a reliable way to avoid generated SIMD execution.

Public helpers:

- `galois_8::active_backend_name()`
- `galois_8::active_backend_kind()`
- `galois_8::active_backend_id()`

## Tuning And Profiling

Useful `CodecOptions` knobs:

- `fast_one_parity`
- `inversion_cache`
- `inversion_cache_capacity`
- `max_parallel_jobs`

Parallel-policy environment variables:

- `RS_PARALLEL_POLICY_MIN_PARALLEL_SHARD_BYTES`
- `RS_PARALLEL_POLICY_MIN_BYTES_PER_JOB`
- `RS_PARALLEL_POLICY_MAX_JOBS`
- `RS_PARALLEL_POLICY_L2_CACHE_BYTES`
- `RS_PARALLEL_POLICY_DEBUG`

Optional metrics/profile surfaces:

- `benchmark-metrics` feature
- `leopard_gf8_profile_stats()`
- `reset_leopard_gf8_profile_stats()`

## Validation And Benchmarks

Common workflows:

```bash
# Run tests
cargo test --workspace

# Run benchmarks
cargo bench --features simd-accel

# Run release validation
bash scripts/release-check.sh

# Run extended validation
VALIDATION_PROFILE=extended bash scripts/release-check.sh

# Collect x86_64 SIMD benchmark artifacts
bash scripts/collect_x86_simd_benchmarks.sh
```

Useful references:

- [docs/benchmark-methodology.md](docs/benchmark-methodology.md)
- [docs/README-performance-index.md](docs/README-performance-index.md)
- [docs/README.md](docs/README.md)
- [scripts/README.md](scripts/README.md)

## Provenance

Versions `0.9.0` through `6.0.0` were originally created by
[Darren Ldl](https://github.com/darrenldl) and later maintained by the
[rust-rse](https://github.com/rust-rse) community.

The current `8.0.1` line in this repository is maintained under
[houseme/rustfs-erasure-codec](https://github.com/houseme/rustfs-erasure-codec)
and reflects the Rust 2024 rewrite, runtime SIMD architecture, and Leopard work.

## Contributing

Contributions are welcome. For backend-sensitive, benchmark-sensitive, or codec-family work, include focused validation
where possible.

## License

This project is released under the MIT License. See [LICENSE](LICENSE).

The bundled `simd_c` sources derive from
[Nicolas Trangez's Haskell implementation](https://github.com/NicolasT/reedsolomon)
and remain under the MIT License as well.
