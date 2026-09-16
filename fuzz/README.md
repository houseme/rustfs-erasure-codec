# Fuzz Testing

This directory contains fuzz testing targets for the `rustfs-erasure-codec` library using [`libfuzzer-sys`](https://github.com/rust-fuzz/libfuzzer) (via [`cargo-fuzz`](https://github.com/rust-fuzz/cargo-fuzz)).

## Targets

### `fuzz_encode_verify`

**Source:** [`fuzz_targets/fuzz_encode_verify.rs`](fuzz_targets/fuzz_encode_verify.rs)

Tests the encode-verify round-trip correctness. Generates random shard configurations, encodes data into parity shards, and asserts that `verify()` returns `true` for the encoded output.

**Input wire format:**
```
[data_shards: u8, parity_shards: u8, shard_size: u8, run_count: u8, ...data...]
```

### `fuzz_encode_reconstruct`

**Source:** [`fuzz_targets/fuzz_encode_reconstruct.rs`](fuzz_targets/fuzz_encode_reconstruct.rs)

Tests the full encode-corrupt-reconstruct-verify cycle. Encodes data, corrupts a configurable number of shards, verifies that `verify()` returns `false`, then reconstructs and verifies recovery.

**Input wire format:**
```
[data_shards: u8, parity_shards: u8, shard_size: u8, run_count: u8, interval: u8, corrupt_count: u8, corrupt_index: u8, ...data...]
```

## How to Run

Prerequisites: install `cargo-fuzz` and use nightly Rust.

```bash
# Install cargo-fuzz
cargo install cargo-fuzz

# Run a fuzz target
cargo fuzz run fuzz_encode_verify
cargo fuzz run fuzz_encode_reconstruct

# Run with a specific corpus directory
cargo fuzz run fuzz_encode_verify -- corpus/
```

> **Note:** `cargo-fuzz` requires a nightly Rust toolchain. Use `rustup run nightly cargo fuzz run ...` if your default toolchain is stable.

## Crash Artifacts

`cargo-fuzz` writes runtime corpora and crash artifacts under ignored local directories such as `corpus/`, `artifacts/`, and `target/`. Investigated crash artifacts should be minimized into regression tests or kept outside version control.

## Authors

- Original fuzzing suite by Darren Ldl
- Built on top of the `rustfs-erasure-codec` library (see [../README.md](../README.md))
