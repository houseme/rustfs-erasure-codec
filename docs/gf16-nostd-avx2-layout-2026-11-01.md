# GF16 no_std AVX2 layout conversion

On little-endian x86-64 `no_std` builds whose compilation target enables AVX2,
GF16 encoding and recovery now reuse the existing SSSE3 layout-conversion
kernels. AVX2 compilation enables the required SSSE3 instructions. Before the
kernel call, equal buffer lengths and 64-byte block alignment are checked;
other inputs retain the scalar fallback. No public API or shard layout changes.
The existing std runtime dispatch, other architectures and no_std builds
without AVX2 are unchanged.

## Finding the bottleneck

A [test-only probe](../benchmarks/gf16-nostd-ssse3-layout/2026-11-01/small-cost-probe.rs)
measured plan creation, work allocation, the encoder IFFT, FFT, and output
conversion separately in an optimized `no_std +avx2` build. For example, across
four repeated runs, 10+4 with 64-byte shards measured approximately 315–326 ns
for IFFT, 76–78 ns for plan creation, and 48 ns for output conversion, versus
484–491 ns for a complete encode call. With 4 KiB shards, output conversion
alone took approximately 2.1 µs. These isolated loops have different cache and
input evolution from a complete encode, so their timings must not be added or
read as an exact profile of the full call. [All measurements](../benchmarks/gf16-nostd-ssse3-layout/2026-11-01/small-cost-results.json)
and the probe are retained for inspection.

The probe can be reproduced in a disposable checkout of the baseline by
appending the probe file to `src/core/leopard_gf16/encode.rs`, then running the
ignored `small_cost_probe::report_small_encode_costs` test in release mode with
`--no-default-features` and `RUSTFLAGS="-C target-feature=+avx2"`. It uses a
standard-library test harness; the codec is built without its std feature.

## End-to-end results

Baseline and candidate were built at the same path using Rust stable, locked
dependencies, `--no-default-features` and `-C target-feature=+avx2` on an x86-64
Linux VM. The [encoding](../benchmarks/gf16-nostd-avx2-butterfly/2026-11-01/nostd-encode-probe.rs)
and [recovery](../benchmarks/gf16-nostd-avx2/2026-11-01/nostd-fwht-probe.rs)
harnesses validate output bytes. Each case used an A1/B1/B2/A2 sequence with
20 samples, 0.5 seconds warmup, and a requested two-second measurement per case.
Guest CPUs were isolated under a shared reservation and the reservation was
restored. Physical-host isolation was not verified. Drift was recorded but did
not gate the runs. Changes compare the average of the two candidate means with
the average of the two baseline means; negative means lower latency.

| Operation | Shard size | Configurations | Latency change range |
| --- | ---: | --- | ---: |
| Encode | 64 B | 10+4, 16+8 | −14.40% to −14.09% |
| Encode | 1 MiB | 10+4, 16+8 | −61.31% to −55.45% |
| Recovery | 64 B | 10+4, 16+8; one and four missing | −1.11% to −0.53% |
| Recovery | 4 KiB | same | −1.54% to −0.43% |
| Recovery | 1 MiB | same | −6.99% to −2.66% |

The [Criterion stage estimates](../benchmarks/gf16-nostd-ssse3-layout/2026-11-01/estimates.json)
include all baseline and candidate measurements. These results are specific to
the tested host and workload. The final AVX2-only dispatch was compared to the
broader prototype: the encoding and recovery benchmark binaries' `.text`
sections were byte-identical for both baseline and candidate, so the recorded
AVX2 measurements apply to the final machine code.

A broader prototype also enabled this dispatch on `no_std +ssse3` without AVX.
It improved 64-byte encoding by approximately 14%–15%, but 1 MiB recovery
regressed by 3.32%–10.57% across four cases (baseline drift −2.04% to +0.98%).
That broader dispatch was rejected. With the final AVX2-only dispatch, the
baseline and candidate benchmark `.text` sections were byte-identical for both
encoding and recovery under `+ssse3,-avx,-avx2`.

## Compatibility and verification

Native Linux x86 `no_std +avx2` passed 289 debug library tests and 296 release
library tests with `simd-accel`. Native `no_std +ssse3,-avx,-avx2` passed 285
library tests in both debug and release; default x86 no_std also passed 285.
The boundary probe passed 228 published-9.0 versus candidate-9.1 parity and
bidirectional recovery checks under each of `+avx2` and `+ssse3,-avx,-avx2`.
The same probe passed on AArch64. The conversion test exercises lengths 0, 64, 128, 192, 512 and 4096 bytes,
different buffer offsets, and guard bytes against the scalar implementation. Clippy passed for both Linux no_std target settings.

On AArch64 the all-features workspace test suite excluding the fuzz package,
no_std codec tests, formatting and all-target all-feature Clippy passed. The
standard release-compatibility probe passed 200 configurations in both
directions, 200 historical non-fast cases, 36 large GF8 cases and 12 6+2 cases.
A 21-second AArch64 standard-library `fuzz_encode_verify` smoke run completed
83,287 iterations without a crash; that fuzz target does not exercise the new
x86 no_std dispatch.
