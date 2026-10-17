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
