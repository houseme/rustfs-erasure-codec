# rustfs-erasure-codec

[![CI](https://github.com/houseme/rustfs-erasure-codec/actions/workflows/ci.yml/badge.svg)](https://github.com/houseme/rustfs-erasure-codec/actions/workflows/ci.yml)
[![Crates](https://img.shields.io/crates/v/rustfs-erasure-codec.svg)](https://crates.io/crates/rustfs-erasure-codec)
[![Documentation](https://docs.rs/rustfs-erasure-codec/badge.svg)](https://docs.rs/rustfs-erasure-codec)
[![dependency status](https://deps.rs/repo/github/houseme/rustfs-erasure-codec/status.svg)](https://deps.rs/repo/github/houseme/rustfs-erasure-codec)
[![Crates.io Total Downloads](https://img.shields.io/crates/d/rustfs-erasure-codec)](https://crates.io/crates/rustfs-erasure-codec)
[![Crates.io License](https://img.shields.io/crates/l/rustfs-erasure-codec)](https://crates.io/crates/rustfs-erasure-codec)

English | [Chinese](README_CN.md)

`rustfs-erasure-codec` is a Rust 2024 Reed-Solomon erasure-coding library for
memory-resident shards, targeted recovery, progressive recovery, and block-based
streaming workloads.

The current `8.0.3` line provides:

- classic Reed-Solomon over `GF(2^8)` and `GF(2^16)`
- Leopard GF8 and Leopard GF16 codec families
- runtime-dispatched SIMD backends for `galois_8`
- reusable verification and reconstruction workspaces
- targeted and progressive recovery APIs
- block streaming encode, verify, and reconstruct APIs
- `no_std` support and a WASM companion crate

WASM bindings live in [wasm/README.md](wasm/README.md).

## Install

Default `std` build:

```toml
[dependencies]
rustfs-erasure-codec = "8.0.3"
```

Enable all supported SIMD backends:

```toml
[dependencies]
rustfs-erasure-codec = { version = "8.0.3", features = ["simd-accel"] }
```

Enable only the backend family you deploy:

```toml
[dependencies]
rustfs-erasure-codec = { version = "8.0.3", features = ["simd-neon"] }   # aarch64
# rustfs-erasure-codec = { version = "8.0.3", features = ["simd-ssse3"] } # x86_64
# rustfs-erasure-codec = { version = "8.0.3", features = ["simd-avx2"] }  # x86_64
# rustfs-erasure-codec = { version = "8.0.3", features = ["simd-avx512"] }# x86_64
# rustfs-erasure-codec = { version = "8.0.3", features = ["simd-gfni"] }  # x86_64
# rustfs-erasure-codec = { version = "8.0.3", features = ["simd-vsx"] }   # powerpc64
```

Runtime dispatch is guarded. Unsupported ISAs fall back to scalar execution.

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

For repeated verification, prefer `verify_with_workspace(...)` or
`verify_with_buffer(...)` over plain `verify(...)`.

For repeated `Option<Vec<u8>>` recovery with a stable missing pattern, prepare a
workspace once:

```rust
use rustfs_erasure_codec::galois_8::ReedSolomon;

let rs = ReedSolomon::new(10, 4).unwrap();
let mut shards = vec![vec![0u8; 1024]; 14];
rs.encode(&mut shards).unwrap();

let mut missing: Vec<Option<Vec<u8>>> = shards.into_iter().map(Some).collect();
missing[0] = None;
missing[10] = None;

let workspace = rs.prepare_reconstruct_opt_workspace(&missing).unwrap();
rs.reconstruct_opt_with_workspace(&mut missing, &workspace).unwrap();
```

## Main APIs

| Area | APIs |
|---|---|
| Classic coding | `galois_8::ReedSolomon`, `galois_16::ReedSolomon` |
| Codec selection | `CodecOptions`, `CodecFamily`, `MatrixMode`, `LeopardMode` |
| Verification reuse | `VerifyWorkspace`, `verify_with_workspace`, `verify_with_buffer` |
| Reconstruction reuse | `OptionVecReconstructWorkspace`, `ShardSlot<T>` |
| Targeted recovery | `reconstruct_some`, `reconstruct_some_opt` |
| Progressive recovery | `decode_idx` |
| Incremental encoding | `ShardByShard` |
| Streaming | `stream::encode_stream`, `stream::verify_stream`, `stream::reconstruct_stream` |

## Codec Families

`CodecOptions::codec_family` selects the algorithm family.

| Family | Status | Notes |
|---|---|---|
| `Classic` | fully supported | Default family. Supports matrix modes, `update`, `encode_single*`, `decode_idx`, and `reconstruct_some`. |
| `LeopardGF8` | supported on `galois_8` | FFT-based GF(2^8). Requires 64-byte-aligned shard lengths and supports up to 256 total shards. Classic-only APIs are rejected. |
| `LeopardGF16` | supported for high shard counts | FFT-based GF(2^16). Intended for larger total shard counts. Classic-only APIs are rejected. |

Example:

```rust
use rustfs_erasure_codec::galois_8::ReedSolomon;
use rustfs_erasure_codec::{CodecFamily, CodecOptions};

let rs = ReedSolomon::with_options(
    32,
    16,
    CodecOptions {
        codec_family: CodecFamily::LeopardGF8,
        ..CodecOptions::default()
    },
)
.unwrap();
```

Leopard-family constraints:

- shard lengths must be multiples of 64 bytes
- all shard buffers must have the same length
- `decode_idx(...)`, `update(...)`, and `encode_single*` remain Classic-only

## Matrix Modes

`CodecOptions::matrix_mode` applies to `CodecFamily::Classic`:

- `Vandermonde`
- `Cauchy`
- `JerasureLike`
- `Custom`

For compatibility with established classic payload layouts, keep
`MatrixMode::Vandermonde`.

```rust
use rustfs_erasure_codec::galois_8::ReedSolomon;
use rustfs_erasure_codec::CodecOptions;

let custom_rows = vec![vec![1u8, 1, 1], vec![1u8, 2, 4]];
let rs = ReedSolomon::with_custom_matrix(3, 2, &custom_rows, CodecOptions::default()).unwrap();
```

## Memory Reuse

`ShardSlot<T>` lets repeated reconstruct flows retain missing-shard buffers:

```rust
use rustfs_erasure_codec::galois_8::{mark_missing_slots, shards_to_slots, ReedSolomon};

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
```

For SIMD-sensitive `galois_8` workloads, aligned shard helpers are available:

- `rustfs_erasure_codec::galois_8::alloc_aligned_shards(...)`
- `galois_8::ReedSolomon::alloc_aligned(...)`

## Streaming

The streaming API lives under `rustfs_erasure_codec::stream` and is available
with the default `std` feature.

Main entry points:

- `encode_stream(...)`
- `verify_stream(...)`
- `reconstruct_stream(...)`

Current scope:

- implemented on the Classic `galois_8` path
- tuned for block-based processing via `StreamOptions`
- validates shard counts, block sizes, and equal present-shard lengths up front
- rejects Leopard-family codecs with `UnsupportedCodecFamily`

Use this path when data should be processed in bounded blocks instead of holding
the full shard matrix in memory.

## Runtime Backend Control

Environment variables:

- `RSE_BACKEND_OVERRIDE`
- `RSE_STRICT_BACKEND_OVERRIDE=1`
- `RUST_REED_SOLOMON_ERASURE_ARCH`

An unset or `auto` `RSE_BACKEND_OVERRIDE` allows generated SIMD encode code when
the platform supports it. Any recognised explicit override, including `scalar`,
uses the selected generic backend and bypasses generated SIMD codegen.

Inspection helpers:

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

Optional metrics:

- `benchmark-metrics` feature
- `leopard_gf8_profile_stats()`
- `reset_leopard_gf8_profile_stats()`

## Validation And Benchmarks

Common workflows:

```bash
cargo test --workspace
cargo test --workspace --features "simd-accel benchmark-metrics"
cargo clippy --workspace --all-targets --features "simd-accel benchmark-metrics" -- -D warnings
cargo bench --bench galois_backend --features "std simd-accel"
```

Backend-sensitive performance work should use the ABBA drift gate:

```bash
RSE_ABBA_BACKEND=auto \
RSE_ABBA_SAMPLE_SIZE=20 \
RSE_ABBA_WARMUP_TIME=2 \
RSE_ABBA_MEASUREMENT_TIME=2 \
bash scripts/run_galois_backend_abba.sh <baseline-ref> <candidate-ref>
```

Treat performance conclusions as invalid when the A1/A2 baseline drift gate
fails.

Useful references:

- [docs/benchmark-methodology.md](docs/benchmark-methodology.md)
- [docs/README-performance-index.md](docs/README-performance-index.md)
- [docs/ec-klauspost-feature-comparison.md](docs/ec-klauspost-feature-comparison.md)
- [scripts/README.md](scripts/README.md)

## Provenance

Versions `0.9.0` through `6.0.0` were originally created by
[Darren Ldl](https://github.com/darrenldl) and later maintained by the
[rust-rse](https://github.com/rust-rse) community.

The current `8.0.3` line is maintained under
[houseme/rustfs-erasure-codec](https://github.com/houseme/rustfs-erasure-codec)
and reflects the Rust 2024 rewrite, runtime SIMD architecture, Leopard codec
families, and RustFS compatibility hardening.

## Contributing

Contributions are welcome. For backend-sensitive, benchmark-sensitive, or
codec-family work, include focused validation where possible.

## License

This project is released under the MIT License. See [LICENSE](LICENSE).

The bundled `simd_c` sources derive from
[Nicolas Trangez's Haskell implementation](https://github.com/NicolasT/reedsolomon)
and remain under the MIT License as well.
