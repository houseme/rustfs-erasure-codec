# GF16 GFNI layout and multiplication fusion

The GF16 decoder converts each 64-byte user block between split byte planes and
interleaved `u16` work lanes while multiplying by a fixed field element. On
little-endian standard-library x86-64, this change adds AVX2/GFNI kernels that
use four 8-by-8 affine matrices for the multiplication and perform the layout
conversion in the same pass. The matrices use the existing GF16 tables and
matrix construction; no field polynomial, encoded bytes or public API changes.

The new kernels require AVX2 and GFNI, an already selected GF8 backend of
`RustGfniAvx2` or `RustGfniAvx512`, and at least 4 KiB in the current block
range. Smaller ranges and all other backends retain their existing paths. The
shared backend check only observes the existing GF8 selection: it does not
initialize that backend or repeatedly read its environment override. A GF16-only
process that has not selected GF8 therefore retains its previous fallback.

## Correctness and compatibility

Native x86 GFNI tests compare both conversion directions with scalar arithmetic
for every non-sentinel multiplier, and compare all 65,536 field values for
representative multipliers. They also exercise unaligned slices, guard values,
whole-block boundaries and the 4 KiB dispatch boundary. A separate scalar
GFNI-affine model checks every non-sentinel multiplier and all 16 basis
vectors. The existing identity shortcut remains responsible for the sentinel.

The final source passed native x86 full `all-features` tests, GF16 `no_std`
tests, all-target Clippy, and `std,simd-avx2` and `std,simd-ssse3` feature
builds. The latter ran on an AVX2-capable host, not on a physical SSSE3-only
CPU. The direct GFNI tests ran with both `RustGfniAvx2` and `RustGfniAvx512`
selected. AArch64 GF16 tests passed on its unchanged path.

With `RustGfniAvx2` selected, the release compatibility tool passed 200
published/new parity and bidirectional cross-decoding configurations and 200
published-to-candidate non-fast cases. A separate temporary run replaced the
Leopard test lengths with 4 KiB and 8 KiB, exercising the new kernels and
passing the same 200 cross-version configurations. A warm single-worker probe
found identical allocation counts for baseline and candidate in both optimized
Option and generic `ShardSlot` recovery, with GF8 selected and unselected. Four
missing `ShardSlot` destinations still require four shard-sized staging
allocations to preserve generic failure and unwind semantics.

## Performance screens

The x86 screens compared baseline `268e092` with the candidate on an AMD EPYC
9V45 Linux VM, using Rust stable 1.99.1, identical dependency locks and four
Rayon workers. Baseline and candidate were compiled sequentially from the same
source and Cargo target directories; their binaries were saved separately and
run with the same working directory. A shared lock and guest-exclusive CPUs
8–15 were held; monitoring reported no competing heavy process during the
retained screens. The lock and partition were released after each run.
Physical-host isolation was not established.

A1/B1/B2/A2 phases used ten samples, 0.2 seconds warmup and 0.6 seconds
requested measurement per case. Per case, the two Criterion mean estimates on
each side are averaged before calculating the percentage change. Each row
below is the median and range of four cases: 10+4 and 16+8, each with one or
four missing shards. Negative values mean lower latency.

| GFNI recovery entrypoint | Shard size | Median | Range |
| --- | ---: | ---: | ---: |
| Retained slots | 4 KiB | −4.41% | −4.96% to −3.43% |
| Optimized Option | 4 KiB | −4.20% | −5.44% to −3.50% |
| Retained slots | 8 KiB | −5.86% | −6.66% to −4.87% |
| Optimized Option | 8 KiB | −5.90% | −6.18% to −5.44% |
| Retained slots | 64 KiB | −4.44% | −5.18% to −4.02% |
| Optimized Option | 64 KiB | −4.24% | −6.68% to −4.07% |
| Retained slots | 1 MiB | −2.07% | −5.46% to −0.21% |
| Optimized Option | 1 MiB | −0.68% | −3.05% to −0.01% |

The [final GFNI samples](../benchmarks/gf16-gfni-layout/2026-10-31-linux-x86/final-gfni.json)
and [AVX2](../benchmarks/gf16-gfni-layout/2026-10-31-linux-x86/final-avx2-control.json)
and [GF8-unselected](../benchmarks/gf16-gfni-layout/2026-10-31-linux-x86/final-unselected-control.json)
controls retain every sample and estimate. The initial AVX2 control showed a
+1.44% median for 64 KiB optimized Option recovery. A
[longer AVX2 follow-up](../benchmarks/gf16-gfni-layout/2026-10-31-linux-x86/final-avx2-focused.json)
using 20 samples, 0.5 seconds warmup and two seconds requested measurement
showed +0.03% for that entrypoint and −0.17% for retained slots. A comparable
[GFNI 1 MiB follow-up](../benchmarks/gf16-gfni-layout/2026-10-31-linux-x86/final-gfni-focused.json)
showed −1.11% and −2.20% respectively.

The earlier [threshold screen](../benchmarks/gf16-gfni-layout/2026-10-31-linux-x86/prototype-boundary.json)
covered 2 KiB, 4032 B, 4 KiB, 4160 B, 8 KiB, 16 KiB and 64 KiB with the
pre-refactor prototype. At 4 KiB its two recovery medians were −5.59% and
−4.68%; the final-source screen above separately rechecks that threshold.

The [encode benchmark source](../benchmarks/gf16-nibble-basis/2026-10-31/encode_bench.rs)
was identical on both sides. An initial [GFNI encode screen](../benchmarks/gf16-gfni-layout/2026-10-31-linux-x86/final-encode-gfni.json)
showed a +2.89% median at 1 MiB across two geometries. The
[longer GFNI encode follow-up](../benchmarks/gf16-gfni-layout/2026-10-31-linux-x86/final-encode-gfni-focused.json)
showed −0.03% at 1 MiB and −0.70% at 64 KiB; individual 1 MiB changes ranged
from −0.82% to +0.76%. The [AVX2 encode control](../benchmarks/gf16-gfni-layout/2026-10-31-linux-x86/final-encode-avx2.json)
is also retained. At the user's request, baseline drift did not stop these
runs. These VM screens are not a guarantee for other CPUs or workloads.
