# GF16 GFNI-AVX512 layout evaluation

This is a performance experiment against baseline `5dae909`; neither prototype
is active production code. Both add a GFNI-AVX512 layout-and-multiply kernel
only when the already selected GF8 backend is `RustGfniAvx512` and the required
CPU features are present. GFNI-AVX2 remains the fallback. The prototypes are
independent alternatives, not patches to stack on one another.

The [first prototype](../benchmarks/gf16-gfni512-layout/2026-10-31-linux-x86/initial.patch)
combines pairs of 256-bit planes into 512-bit operands, applies the four GFNI
affine matrices, then splits the results for the existing layout operations.
The [wide prototype](../benchmarks/gf16-gfni512-layout/2026-10-31-linux-x86/wide.patch)
instead uses 512-bit loads, stores and lane permutations directly. Both retain
the existing 256-bit path for a final 64-byte block.

On native x86 with `RustGfniAvx512` explicitly selected, both prototypes
matched the existing 256-bit GFNI kernels for every non-sentinel multiplier.
They also matched scalar arithmetic for all 65,536 field values with six
representative multipliers, and exercised unaligned buffers, guard values,
whole-block boundaries and odd-block tails. Each side passed 32 recovery
preflights under both `RustGfniAvx512` and `RustGfniAvx2`. These checks do not
replace a full release-compatibility or feature-matrix validation.

## Performance screens

The screens compared baseline `5dae909` with each prototype on an AMD EPYC
9V45 Linux VM, using Rust stable 1.99.1 and four Rayon workers. Baseline and
candidate were compiled from the same source and Cargo target directories,
saved separately and run with the same working directory. A shared lock and
guest-exclusive CPUs 8–15 were held; monitoring reported no competing heavy
process during the retained runs, and the partition and lock were released.
Physical-host isolation was not established.

A1/B1/B2/A2 phases used ten samples, 0.2 seconds warmup and 0.6 seconds
requested measurement per case. Per case the two Criterion mean estimates on
each side are averaged before calculating the percentage change. Each row
reports the median and range over four cases: 10+4 and 16+8, each with one or
four missing shards. Negative values mean lower latency.

| Prototype and selected backend | Entrypoint | Shard size | Median | Range |
| --- | --- | ---: | ---: | ---: |
| [Packed 256-bit, GFNI-AVX512](../benchmarks/gf16-gfni512-layout/2026-10-31-linux-x86/initial-gfni512.json) | retained slots | 4 KiB | −1.92% | −2.63% to −1.56% |
| Same | optimized Option | 4 KiB | −1.75% | −2.63% to −1.14% |
| Same | retained slots | 64 KiB | +0.23% | −0.20% to +2.48% |
| Same | optimized Option | 1 MiB | −0.64% | −1.04% to +1.55% |
| [Direct wide, GFNI-AVX512](../benchmarks/gf16-gfni512-layout/2026-10-31-linux-x86/wide-gfni512.json) | retained slots | 8 KiB | −1.04% | −1.52% to −0.34% |
| Same | optimized Option | 8 KiB | −1.07% | −1.49% to −0.83% |
| Same | retained slots | 64 KiB | −0.68% | −3.75% to 0.00% |
| Same | optimized Option | 1 MiB | −1.10% | −1.64% to −0.17% |

The [first prototype's GFNI-AVX2 control](../benchmarks/gf16-gfni512-layout/2026-10-31-linux-x86/initial-gfni256-control.json)
also showed 4 KiB medians of −2.34% and −2.07%, although it does not select
the new 512-bit kernel. The [wide GFNI-AVX2 control](../benchmarks/gf16-gfni512-layout/2026-10-31-linux-x86/wide-gfni256-control.json)
and [first AVX2 control](../benchmarks/gf16-gfni512-layout/2026-10-31-linux-x86/initial-avx2-control.json)
are retained with all estimates and samples. At the user's request, baseline
drift did not stop these runs. These short VM screens do not establish a
reliable 512-bit advantage or guarantee performance on other CPUs.

The prototypes are not promoted: their end-to-end differences are small and
comparable to control-group changes, while maintaining a separate wide kernel
would add code and another CPU-specific path.
