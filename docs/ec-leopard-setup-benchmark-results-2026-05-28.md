# Leopard Setup Benchmark Results (2026-05-28)

## Goal

This note records the first isolated Leopard benchmark artifact after `CodecFamily` and the constructible
`LeopardGF8` prototype skeleton were introduced.

This benchmark is intentionally **not** part of the classic encode/verify/reconstruct regression ledger.

## What Is Measured

Current scope:

- `CodecFamily::LeopardGF8`
- constructor-time family setup only
- explicit prototype encode path
- no verify / reconstruct execution yet

The benchmark therefore measures:

- family-specific setup overhead
- internal setup metadata creation
- first prototype family-specific encode throughput

It does **not** measure:

- Leopard reconstruct throughput
- parity compatibility

## Current Artifact

- `target/benchmark-smoke/leopard-setup-32x16_1m.csv`
- `target/benchmark-smoke/leopard-setup-32x16_1m.json`
- `target/benchmark-smoke/leopard-setup-64x32_1m.csv`
- `target/benchmark-smoke/leopard-setup-64x32_4m.csv`
- `target/benchmark-smoke/leopard-encode-64x32_1m.csv`
- `target/benchmark-smoke/leopard-encode-64x32_4m.csv`

## Current Result

- case: `32x16_1m`
- operation: `leopard_setup`
- `throughput_mb_s = 6286.8629`
- `ns_per_iter = 5089979.00`
- setup shape: `48 x 32`

- case: `64x32_1m`
  - operation: `leopard_setup`
  - `throughput_mb_s = 1874.6247`
  - `ns_per_iter = 34140166.50`
  - setup shape: `96 x 64`

- case: `64x32_4m`
  - operation: `leopard_setup`
  - `throughput_mb_s = 7526.3957`
  - `ns_per_iter = 34013625.00`
  - setup shape: `96 x 64`

- case: `64x32_1m`
  - operation: `leopard_encode`
  - previous prototype-route baseline: `5.7160 MB/s`
  - current specialized-kernel-route reading: `5.5964 MB/s`
  - after adding pure-Rust butterfly/mul/xor helpers: `10.2512 MB/s`
  - after removing inner-loop temporary allocations: `13.0268 MB/s`
  - after reverting `zero` reuse on the main path: `15.1617 MB/s`
  - current `ns_per_iter = 4221155354.00`

- case: `64x32_4m`
  - operation: `leopard_encode`
  - previous prototype-route baseline: `5.7897 MB/s`
  - after the pure-Rust helper pass and allocation cleanup: `12.8469 MB/s`
  - after reverting `zero` reuse on the main path: `15.1377 MB/s`
  - current `ns_per_iter = 16911405041.50`

## Interpretation

This result should be read as:

- the explicit Leopard family path can now be constructed cleanly
- setup metadata generation is benchmarkable in isolation
- the benchmark track is separated from classic-path ledgers as intended
- the explicit LeopardGF8 encode route is now wired through the new `leopard_gf8` module and benchmarkable

It should **not** be read as proof of competitive Leopard runtime throughput yet. The current encode path is still a
prototype family-specific route, and the very low `64x32_*` throughput numbers show that this is not yet a true
algorithmic Leopard implementation.

Latest interpretation:

- the specialized route is now hitting the new `leopard_gf8` module instead of the older matrix-style prototype path
- the first pure-Rust helper completion pass removed the 4-lane butterfly panic and raised `64x32_1m` from `5.5964 MB/s` to `10.2512 MB/s`
- removing the inner-loop temporary allocations raised `64x32_1m` again to `13.0268 MB/s` and `64x32_4m` to `12.8469 MB/s`
- this confirms the new module is now doing meaningful algorithm work, even though it is still far from a finished Leopard kernel

Latest A/B for `64x32_1m`:

- `baseline`: `16.0869 MB/s`
- `reuse_zero_only`: `15.6621 MB/s`

Interpretation:

- reusing the zero buffer is slightly slower than the baseline in the current pure-Rust kernel
- reverting `zero` reuse on the main path recovers most of the gap, bringing the mainline `64x32_1m` result back up to `15.1617 MB/s`
- the next cleanup should keep `zero` reuse out of the mainline before continuing with other micro-optimizations

Refined A/B after restoring the stable mainline:

- `baseline`: `15.4498 MB/s`
- `reuse_zero_only`: `15.4406 MB/s`
- `xor_clone_only`: `15.3955 MB/s`

Current conclusion:

- neither `reuse_zero_only` nor `xor_clone_only` is a strong win relative to the current stable baseline
- the next optimization pass should move away from these two knobs and target a different local hotspot inside
  `ifft_dit_encoder8(...)`

After reverting the later `fill(0)` removal and restoring the stable mainline:

- `64x32_1m`: `15.0824 MB/s`
- `64x32_4m`: `15.2183 MB/s`

This confirms that the stable mainline sits in the `15 MB/s` band, and the next A/B work should target a different
local hotspot than whole-chunk zeroing.

After restructuring the later-group work-buffer flow so transformed chunks run in the second half and XOR directly
back into the accumulation half:

- `64x32_1m`: `15.5209 MB/s`
- `64x32_4m`: `15.7723 MB/s`

Interpretation:

- this coarse-grained data-movement restructuring is a real win
- it moves the current pure-Rust mainline closer to the `16 MB/s` A/B reference than the earlier micro-optimizations

Follow-up result:

- replacing zero-source `copy_from_slice(...)` with direct `fill(0)` in the data-load path was a regression and was
  reverted
- recovered mainline after the revert:
  - `64x32_1m`: `15.6166 MB/s`
  - `64x32_4m`: `15.8417 MB/s`

Latest result:

- tightening the full/remainder real-data load path in `ifft_dit_encoder8(...)` improved the mainline again:
  - `64x32_1m`: `15.6608 MB/s`
  - `64x32_4m`: `16.0810 MB/s`

This suggests the real data-shard load path is a better optimization target than the previously tested `zero` and
`xor_clone` micro-knobs.

After reverting the regressive later-layer call-organization experiment:

- `64x32_1m`: `16.1089 MB/s`
- `64x32_4m`: `15.8374 MB/s`

This is the current best stable mainline after keeping the stronger load-path improvements and dropping the weaker
call-organization tweak.

FlatWork lane-view container capacity tuning (`SmallVec<[&mut [u8]; 96]>` vs a larger inline capacity) was
effectively flat:

- `64x32_1m`: `15.6476 MB/s`
- `64x32_4m`: `16.0906 MB/s`

Interpretation:

- lane-view container inline-capacity sizing is not a major hotspot at this stage
- future FlatWork work should focus on deeper work-buffer access/layout changes rather than this micro-tuning

Current full `FlatWork`-path checkpoint:

- `64x32_1m`: `15.6476 MB/s`
- `64x32_4m`: `16.0906 MB/s`

Interpretation:

- the `FlatWork` path now runs end-to-end through owner + lane views + helper interfaces
- it is close enough to the stronger preserved baseline to justify continued migration work
- but it has not yet beaten the preserved mainline on both benchmark shapes

Extended Leopard encode matrix for the `FlatWork`-driven path:

- `32x16_1m`: `20.2759 MB/s`
- `32x16_4m`: `20.8361 MB/s`
- `64x32_64k`: `15.3197 MB/s`
- `64x32_1m`: `16.4482 MB/s`
- `64x32_4m`: `16.2620 MB/s`
- `128x64_1m`: `6.8649 MB/s`

Interpretation:

- the FlatWork path is no longer supported only by a single winning data point
- it now shows a broader pattern of competitive or winning Leopard encode results across adjacent high-fanout shapes
- however, the first `128x64_1m` reading shows that the current implementation does not automatically scale to the
  next fanout tier without further work

Stage-level profile for `128x64_1m`:

- artifact: `target/benchmark-smoke/leopard-encode-profile-128x64_1m.csv`
- `throughput_mb_s`: `7.1985`
- `encode_calls`: `2`
- `encode_chunks`: `64`
- `encode_full_groups`: `0`
- `encode_remainder_groups`: `0`
- `encode_later_group_calls`: `64`
- `fft_stage_calls`: `64`
- `ifft_stage_calls`: `128`

Interpretation:

- the `128x64_1m` scaling cliff is not driven by remainder handling
- it is dominated by the number of later-group encode passes and total `IFFT/FFT` stage invocations at this fanout
- the next optimization round should focus on reducing stage-level repetition rather than continuing with local buffer
  micro-tuning

Follow-up experiment:

- increased the Leopard GF8 chunk size for very high fanout (`>= 192` total shards) from `32 KiB` to `128 KiB`
- resulting `128x64_1m`: `6.9985 MB/s`
- control `64x32_1m`: `15.5582 MB/s`

Interpretation:

- larger chunks alone do not remove the `128x64_1m` scaling cliff
- the cliff is therefore not explained primarily by raw chunk-count overhead

After removing obvious fixed per-call overhead from the `FlatWork` path:

- `64x32_1m`: `15.7147 MB/s`
- `64x32_4m`: `16.2620 MB/s`

Interpretation:

- fixed-overhead cleanup helps the migrated path
- the gain is clearer on the larger-shard case
- `64x32_1m` is still slightly below the preserved `16.1089 MB/s` baseline, so the migrated path remains an
  experiment rather than the new accepted mainline

After tightening the `encode_with_tables(...)` entry path for the `64x32_1m`-focused experiment:

- `64x32_1m`: `16.4482 MB/s`

Interpretation:

- the FlatWork path now clears the preserved `64x32_1m` baseline of `16.1089 MB/s`
- this is the first clear signal that the migrated path can beat the older mainline on the more difficult smaller-shard
  case

## Validation Command

```bash
cargo test benchmark_leopard_setup_32x16_1m_exports_results -- --nocapture
cargo test benchmark_leopard_setup_64x32_1m_exports_results -- --nocapture
cargo test benchmark_leopard_setup_64x32_4m_exports_results -- --nocapture
cargo test benchmark_leopard_encode_64x32_1m_exports_results -- --nocapture
cargo test benchmark_leopard_encode_64x32_4m_exports_results -- --nocapture
```

## Next Steps

1. replace the prototype encode route with a real algorithmic Leopard GF8 implementation
2. add Leopard verify / reconstruct paths
3. only compare Leopard against classic throughput once the algorithm is no longer prototype-grade
