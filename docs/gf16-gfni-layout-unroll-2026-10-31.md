# GF16 GFNI layout loop unrolling

The GFNI decode kernels introduced by [layout fusion](gf16-gfni-layout-fusion-2026-10-31.md)
convert and multiply 64-byte blocks. This change processes two such blocks per
loop iteration, then handles a possible final 64-byte block. The existing 4 KiB
GFNI cutoff, backend selection, field arithmetic, byte layout, public API and
fallbacks are unchanged. The per-block helpers are inlined by the measured
x86 build; generated code and performance remain compiler-dependent.

Native x86 GFNI tests compare both conversion directions against scalar
arithmetic for every non-sentinel multiplier, all field values for representative
multipliers, unaligned buffers, guard values, and odd or even whole-block
lengths. Full `all-features`, GF16 `no_std`, all-target Clippy, and separate
`std,simd-avx2` and `std,simd-ssse3` feature builds passed. The feature builds
ran on an AVX2-capable host, not a physical SSSE3-only CPU. Direct tests ran
with `RustGfniAvx2` and `RustGfniAvx512` selected; AArch64 GF16 tests passed on
its unchanged path.

With `RustGfniAvx2` selected, the release compatibility tool passed 200
published/new parity and bidirectional cross-decoding configurations and 200
published-to-candidate non-fast cases. A separate temporary run used 4 KiB
and 8 KiB Leopard shards, exercising the unrolled kernels and passing the same
200 cross-version configurations. Warm single-worker allocation probes with
four missing shards reported identical counts and requested bytes for baseline
and candidate, for both optimized Option and generic `ShardSlot` recovery,
with GF8 selected and unselected. The generic slots retain four shard-sized
staging allocations to preserve failure and unwind semantics.

## Performance screens

The x86 screens compared baseline `88e52de` with the candidate on an AMD EPYC
9V45 Linux VM, using Rust stable 1.99.1, identical dependency locks and four
Rayon workers. Baseline and candidate were compiled from the same source and
Cargo target directories, saved separately and run with the same working
directory. A shared lock and guest-exclusive CPUs 8–15 were held. Monitoring
reported no competing heavy process during the retained screens; the lock and
partition were released after each run. Physical-host isolation was not
established.

A1/B1/B2/A2 phases used ten samples, 0.2 seconds warmup and 0.6 seconds
requested measurement per case. Per case, the two Criterion means on each side
are averaged before calculating percentage change. Each row reports the median
and range over four cases: 10+4 and 16+8, each with one or four missing shards.
Negative values mean lower latency.

| GFNI recovery entrypoint | Shard size | Median | Range |
| --- | ---: | ---: | ---: |
| Retained slots | 4 KiB | −13.22% | −13.48% to −11.04% |
| Optimized Option | 4 KiB | −13.20% | −14.90% to −12.66% |
| Retained slots | 8 KiB | −15.18% | −16.12% to −13.36% |
| Optimized Option | 8 KiB | −15.26% | −15.44% to −14.28% |
| Retained slots | 64 KiB | −16.04% | −16.52% to −13.85% |
| Optimized Option | 64 KiB | −16.24% | −16.56% to −13.23% |
| Retained slots | 1 MiB | −11.35% | −14.63% to −5.99% |
| Optimized Option | 1 MiB | −3.25% | −5.47% to −0.45% |

The [GFNI screen](../benchmarks/gf16-gfni-layout-unroll/2026-10-31-linux-x86/gfni.json)
and [AVX2](../benchmarks/gf16-gfni-layout-unroll/2026-10-31-linux-x86/avx2-control.json)
and [GF8-unselected](../benchmarks/gf16-gfni-layout-unroll/2026-10-31-linux-x86/unselected-control.json)
controls retain all samples. A [longer GFNI follow-up](../benchmarks/gf16-gfni-layout-unroll/2026-10-31-linux-x86/gfni-focused.json)
used 20 samples, 0.5 seconds warmup and two seconds requested measurement. Its
64 KiB medians were −17.52% for retained slots and −17.25% for optimized
Option; its 1 MiB medians were −11.39% and −2.65% respectively. One 1 MiB
optimized Option case in that follow-up was +1.22%.

The [encode benchmark source](../benchmarks/gf16-nibble-basis/2026-10-31/encode_bench.rs)
was identical on both sides. Initial [GFNI](../benchmarks/gf16-gfni-layout-unroll/2026-10-31-linux-x86/encode-gfni.json)
and [AVX2](../benchmarks/gf16-gfni-layout-unroll/2026-10-31-linux-x86/encode-avx2.json)
encode controls showed 1 MiB medians of −1.59% and −1.34% respectively across
two geometries. At the user's request, baseline drift did not stop these runs.
These VM measurements are not a guarantee for other CPUs or workloads.
