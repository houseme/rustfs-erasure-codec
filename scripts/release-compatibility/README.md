# Published 9.0.0 compatibility probe

This independent, unpublished tooling workspace links the actual crates.io
`rustfs-erasure-codec =9.0.0` and the repository's exact `9.1.0` path package.
A shared probe is instantiated against each API; no old crate is added to the
production workspace or distributable.

```bash
cargo run --locked --manifest-path scripts/release-compatibility/Cargo.toml --target-dir target
```

All 200 configurations must encode/verify and roundtrip a missing data shard.
Every complete parity array is compared element for element as typed byte vectors from the built-in
u8/[u8;2] element representation. The probe covers five layouts, four matrix options,
Classic GF8/tower GF16, Leopard GF8/GF16 and aligned/partial lengths, with
simd-accel on both versions. Fast-one-parity mode is disabled: this compares valid
normal formats, not the previous inconsistent fast/incremental behavior.

The checked-in lockfile records the consumer dependency graph. Exact version
requirements prevent silently reusing this evidence for another release. CI runs
this executable in the release-tooling job. Its result is functional compatibility,
not a latency or throughput benchmark.

The optional resource report extends the original 15-case catalog with 30
incremental-update cases: GF8/tower GF16, four lengths, zero/one/two changed shards,
and a 33-parity boundary case for each field/change count. One existing command
and allocator counter cover all 45 cases and preserve existing files. Reports
retain two warmups and all three counted repetitions per version. Update results
are checked against full re-encoding on each version and against one another;
input, parity cloning and expected-output construction stay outside counting.
These are allocation/reallocation request counts and requested bytes, not RSS,
peak live memory, latency or formal release evidence.

```bash
RUSTUP_TOOLCHAIN=stable RAYON_NUM_THREADS=4 RS_PARALLEL_POLICY_MAX_JOBS=4 RUSTFLAGS='' \
  cargo run --locked --quiet --release \
  --manifest-path scripts/release-compatibility/Cargo.toml --target-dir target \
  -- --allocation-report /path/to/new-resources.json
```
