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

Tests the full encode-erase-reconstruct cycle. Encodes data, marks selected shards missing and overwrites their buffers, then checks exact recovered data and parity bytes against the original codeword and verifies the result. Zeroing bytes need not change a codeword, so the target does not assume that an intermediate `verify()` must fail.

**Input wire format:**
```
[data_shards: u8, parity_shards: u8, shard_size: u8, run_count: u8, interval: u8, corrupt_count: u8, corrupt_index: u8, ...data...]
```

### `fuzz_codec_contracts`

**Source:** [`fuzz_targets/fuzz_codec_contracts.rs`](fuzz_targets/fuzz_codec_contracts.rs)

**Shared checks:** [`codec_contracts.rs`](codec_contracts.rs), also imported directly by [`tests/codec_contract_matrix.rs`](../tests/codec_contract_matrix.rs).

This is the bounded cross-API target. Every input, including an empty or truncated input, executes a case: missing header bytes default to zero, and the payload is repeated or defaults to zero. There is no exact-length payload gate and no input-controlled repetition count. Each case uses 1–8 data shards, 1–4 parity shards, and at most 192 bytes per valid shard (malformed enlarged shards add one byte).

The target compares normal and selected encode APIs, verifies valid codewords, then checks full, data-only, or selective recovery against the original bytes. Unrequested missing shards must remain missing. Classic custom matrices are derived by encoding basis vectors through a known Cauchy codec; fast single-parity custom matrices therefore use the required all-ones row. Arbitrary custom coefficients are not assumed to be MDS. Leopard ignores the matrix/fast options according to its existing contract; GF16 construction is checked for the expected rejection on big-endian targets.

**Input wire format:** a 16-byte header followed by an optional payload.

| Offset | Interpretation |
| --- | --- |
| 0 | Family `% 3`: Classic, Leopard GF8, Leopard GF16 |
| 1 | Matrix `% 4`: Vandermonde, Cauchy, JerasureLike, Custom |
| 2 | Low bit enables `fast_one_parity` |
| 3 | Data shard count: `1 + value % 8` |
| 4 | Parity shard count: `1 + value % 4` |
| 5 | Classic size index `% 6` into `[1, 63, 64, 65, 128, 192]`; Leopard size `64 * (1 + value % 3)` |
| 6 | Encode API `% 4`: `encode`, `encode_sep`, `encode_opt`, `encode_sep_opt` |
| 7 | Recovery API `% 7`: `reconstruct`, `reconstruct_data`, `reconstruct_some`, `reconstruct_opt`, `reconstruct_data_opt`, `reconstruct_some_opt`, prepared workspace |
| 8 | Missing count: `value % (parity_shards + 1)` |
| 9 | First missing index modulo total shard count |
| 10–11 | Little-endian requested-shard bit mask |
| 12 | Low bit selects requested-mask length: 0 = data count, 1 = total count |
| 13 | Malformed-input scenario `% 10`, listed below |
| 14 | Malformed-input index selector; missing-index stride is `1 + value`, with collisions advanced to the next present shard |
| 15 | Wrapping payload salt; zero keeps an empty payload all-zero |
| 16+ | Payload bytes repeated across shards |

Malformed-input scenarios run after the valid case and assert exact errors plus unchanged buffers:

| Scenario | Boundary checked |
| --- | --- |
| 0 | Valid case only |
| 1 | One shortened shard; a one-byte shard is enlarged instead to keep it nonempty |
| 2 | One enlarged shard |
| 3 | All shards empty |
| 4 | One shard too few |
| 5 | One shard too many |
| 6 | Too few surviving shards |
| 7 | Invalid requested-mask length through both selective APIs |
| 8 | Workspace used by a codec with different effective matrix coefficients |
| 9 | Workspace reused with changed lengths/presence pattern, plus malformed custom-matrix construction |

Workspace preparation is explicitly rejected for Leopard. Non-Classic custom-matrix construction is also checked for its expected error. Wrong-matrix Classic workspaces use nonzero parity-row scaling, preserving MDS while guaranteeing a different coding matrix.

The deterministic integration tests traverse every family × matrix × fast flag × encode API × recovery API combination, malformed entry points, minimum/maximum dimensions, zero codewords, and short/extreme byte strings. The original two targets keep their documented wire formats and broader configurations; this target is intended for frequent bounded smoke fuzzing.

## How to Run

Prerequisites: install `cargo-fuzz` and use nightly Rust.

```bash
# Install cargo-fuzz
cargo install cargo-fuzz

# Run a fuzz target
cargo fuzz run fuzz_encode_verify
cargo fuzz run fuzz_encode_reconstruct
cargo fuzz run fuzz_codec_contracts

# Run with a specific corpus directory
cargo fuzz run fuzz_codec_contracts corpus/fuzz_codec_contracts -- -max_total_time=30

# Run the same contract helper without libFuzzer
cargo test -p rustfs-erasure-codec --test codec_contract_matrix
```

> **Note:** `cargo-fuzz` requires a nightly Rust toolchain. Use `rustup run nightly cargo fuzz run ...` if your default toolchain is stable.

## Crash Artifacts

`cargo-fuzz` writes runtime corpora and crash artifacts under ignored local directories such as `corpus/`, `artifacts/`, and `target/`. Investigated crash artifacts should be minimized into regression tests or kept outside version control.

## Authors

- Original fuzzing suite by Darren Ldl
- Built on top of the `rustfs-erasure-codec` library (see [../README.md](../README.md))
