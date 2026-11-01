# Sparse GF16 locator for std recovery

The bounded sparse locator used by `no_std` also builds std GF16 recovery plans
when the work size is at most 256 and erased plus padded parity positions total
at most 16. Other patterns retain the full transform. The existing 16-entry plan
cache, its key, ownership and publication behavior are unchanged; no public API
or shard format changes.

Both `0` and `65535` represent zero modulo 65,535. For present shards the sparse
plan uses `65535` so input scaling can use direct conversion; for missing shards
it uses `0` so inverse output scaling selects the direct path. Tests compare the
locator with the full-transform oracle modulo 65,535 and cover both zero
representations in end-to-end recovery.

The std plan builder fills its array in place and calls an out-of-line sparse
helper. In the tested Linux x86-64 release binaries, an earlier version that
returned the array added a roughly 128 KiB helper stack frame. The in-place
version removed that frame; its caller's frame was comparable to the original
full-transform builder. These observations are compiler- and target-specific,
not a portable stack size guarantee. The shared helper is also used by `no_std`.

## Performance

The [std benchmark](../benchmarks/gf16-std-sparse-locator/2026-11-01/std-fwht-probe.rs)
uses 32 distinct erasure masks for its cold case, exceeding the 16-entry LRU,
and one repeated mask for its warm case. Cold means a plan-cache miss: codec
construction, global tables and fixtures are already initialized. Recovered bytes
are checked before and after timing. Both sides were compiled at the same path
with Rust stable, locked dependencies, std and `simd-accel`, using one worker on
a Linux x86-64 VM. A1/B1/B2/A2 phases used 20 Criterion samples, 0.5-second
warmup and two seconds requested measurement; focused follow-ups used 40 samples,
one-second warmup and five seconds requested measurement. The guest CPU
partition and shared lease were restored. Physical-host isolation was not
verified. Drift was recorded without gating runs. Changes compare the mean of
candidate phases with the mean of baseline phases; negative means faster.

| Backend and case | Shard size | Measured change |
| --- | ---: | ---: |
| GFNI, plan-cache miss, three cases | 64 B | −96.88% to −95.80% |
| GFNI, plan-cache miss, three cases | 1 MiB | −6.63% to −2.44% |
| GFNI, warm plan, three cases | 64 B | −3.68% to −0.95% |
| GFNI, warm plan, three cases | 1 MiB | +1.03% to +1.77% |
| AVX2 without GFNI, plan-cache miss, three cases | 64 B | −97.08% to −96.27% |
| AVX2 without GFNI, plan-cache miss, three cases | 1 MiB | −13.34% to −4.52% |
| AVX2 without GFNI, warm plan, three cases | 64 B | −5.70% to −1.75% |
| AVX2 without GFNI, warm plan, three cases | 1 MiB | −4.55% to −0.32% |

For example, GFNI 10+4 with four missing 64-byte shards changed from 251.90 µs
to 7.86 µs. A longer four-missing warm follow-up measured −3.22% and −2.99%
at 64 B, and +0.36% and −0.21% at 1 MiB. The initial GFNI 10+4 1 MiB cold
and warm baseline drift was −4.00% and −4.50%, respectively. These observations
are specific to the tested VM and workload. Backend names refer to the selected
GF8 backend; that selection controls eligibility for GF16 GFNI butterflies.
Other GF16 operations still dispatch independently by CPU capability.

The longer [pure encoding control](../benchmarks/gf16-std-sparse-locator/2026-11-01/encode-only-probe.rs)
measured +2.32% and +3.07% at 64 B, and −0.19% and −0.36% at 1 MiB. The mixed
screen measured −0.01% and −1.53% for 64 B encoding; thus the small encoding
change is sensitive to measurement conditions and a regression cannot be ruled
out. An A/A control on the earlier baseline binaries gave +0.42% and +0.13% at
64 B. These are not guarantees for other hosts.

The in-place refactor also changes the shared `no_std` helper. Its x86 AVX2
benchmark `.text` was not byte-identical to the prior version, so a separate
12-case recovery comparison was run with the same 20-sample phase settings.
Changes ranged from −2.67% to +2.81% over 64 B, 4 KiB and 1 MiB. The
+2.81% result was 16+8 with one missing 4 KiB shard and +0.40% baseline drift;
one 64 B baseline drift was −8.75%. These measurements cannot establish that
the refactor has no small regressions.
The measurements are retained as `inplace_nostd_avx2` in the raw data.

The [threshold probe](../benchmarks/gf16-std-sparse-locator/2026-11-01/threshold-probe.rs)
cycles 32 masks. At the 256-work-size/16-term boundary, 64 B and 4 KiB
cache-miss recovery improved 74.45% and 47.59%; the 48-work-size/16-term cases
improved 93.29% and 82.89%. The 17-term and 257-work-size fallback cases
ranged from −1.44% to +0.70%. The final in-place runs are retained as
`inplace_threshold`.

The [x86 stage estimates](../benchmarks/gf16-std-sparse-locator/2026-11-01/x86-estimates.json)
retain the in-place results under `inplace_*` and earlier experiments under their
own names. The `inplace_avx2` and `inplace_threshold` entries contain the final
AVX2-only and threshold runs, respectively. The separate
[AArch64 screen](../benchmarks/gf16-std-sparse-locator/2026-11-01/aarch64-screen.json)
measured the earlier returned-array prototype without isolation and is
exploratory only.

## Correctness and compatibility

Tests cover the full-transform oracle, boundary cases, present/missing zero
representations, concurrent same- and distinct-pattern cache access, and reuse
across parallel decode chunks. The all-feature workspace suite excluding the
fuzz package and all-target, all-feature Clippy passed on AArch64 for the in-place implementation; targeted
diagnostic source-binding tests passed after regenerating their expected hashes.

The [std cross-version probe](../benchmarks/gf16-std-sparse-locator/2026-11-01/)
shares the 1,708-pattern corpus with the `no_std` probe, including selective
recovery and rejected buffer checks. The in-place implementation passed it on
AArch64 with `scalar-rust` and `rust-neon` backends, and in `no_std` on AArch64
and WASM, and on Linux x86 with AVX2 and with SSSE3 but AVX disabled.
The std probe also passed on Linux x86 with `scalar-rust`,
`rust-avx2` and `rust-gfni-avx2` selected for GF8. In particular, selecting
`scalar-rust` does not force all GF16 operations to scalar code. Linux x86 passed
456 all-feature library tests and 413 std-only library tests, with five ignored
in each run.

An unpublished crate archive for commit `395c498` contained 66 Rust source files
matching the checkout byte for byte. The extracted archive passed the same
1,708-pattern probe on AArch64 in std with `rust-neon`, and in `no_std` on
AArch64 and WASM.

The shared [cross-version fuzz target](../benchmarks/gf16-nostd-sparse-locator/2026-11-01/fuzz/)
supports `--features std-simd` in addition to its default `no_std` mode. Bounded
AddressSanitizer runs of the in-place implementation completed 10,901 std and
19,109 `no_std` iterations without a crash. These are bounded tests, not
exhaustive proof.

Run the std interoperability probe with:

```sh
CARGO_TARGET_DIR=target/std-sparse-compat RUSTUP_TOOLCHAIN=stable \
  cargo run --offline --locked --release --features simd-accel \
  --manifest-path benchmarks/gf16-std-sparse-locator/2026-11-01/Cargo.toml
```

The benchmark sources and raw stage estimates are retained alongside the probe.
The fuzz command in the `no_std` evaluation can be reused with
`--features std-simd` before the target name to exercise std instead.
