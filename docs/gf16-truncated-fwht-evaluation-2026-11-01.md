# GF16 truncated FWHT SIMD evaluation

The GF16 error-locator plan uses `fwht16_mtrunc`, whose radix-4 blocks still use
scalar operations. Four independent experiments reused the existing SIMD
radix-4 block, with no public API or format changes. They are based on `6855f44`
and are **not production code**:

* [Direct](../benchmarks/gf16-truncated-fwht/2026-11-01/direct.patch): use the
  existing SIMD block at every stage.
* [Outlined](../benchmarks/gf16-truncated-fwht/2026-11-01/outlined.patch): retain
  the original small-stride loop and call the SIMD block out of line for
  strides of at least 16.
* [Standard SIMD only](../benchmarks/gf16-truncated-fwht/2026-11-01/std-only.patch):
  retain the scalar no_std and unsupported-target paths, and apply the outlined
  block to little-endian x86-64 and AArch64 std builds.
* [x86 cold path](../benchmarks/gf16-truncated-fwht/2026-11-01/cold.patch):
  restrict the std candidate to x86-64 and mark its outlined block cold.

Each patch also contains an independent scalar oracle test, comparing the
entire 65,536-element buffer at eight transform sizes and several truncation
boundaries, including arbitrary nonzero values beyond the truncated prefix.
The direct and outlined x86 no_std candidates each passed 47 GF16-focused
library tests. The std-only candidate passed the AArch64 all-features codec
suite, 282 no_std library tests, the 126-check WASM old/new probe and the
standard release compatibility probe (200 bidirectional configurations, 200
historical non-fast cases, 36 large GF8 and 12 6+2 cases). The x86 std-only and
cold candidates passed the scalar-oracle test. These are functional checks,
not a complete cross-platform or performance qualification for each variant.

## Performance results

Tests used same-path baseline/candidate release binaries, a locked dependency
graph and Rust stable on an x86-64 Linux VM. Guest CPUs were partitioned under a
shared lock; monitoring saw no competing heavy process and resources were
restored. Physical-host isolation was not verified. The first screens used ten
samples, 0.2 s warmup and 0.6 s requested measurement per case; the longer
follow-ups used 20 samples, 0.5 s warmup and 2 s requested measurement.
A1/B1/B2/A2 compares the average Criterion mean of the two candidate stages
with the average of the two baseline stages. Negative percentages mean lower
latency. Drift was recorded but did not gate the experiment.

The [no_std probe](../benchmarks/gf16-nostd-avx2/2026-11-01/nostd-fwht-probe.rs)
uses a std benchmark harness with a no_std codec, 10+4 and 16+8 configurations,
retained `ShardSlot` buffers and one or four missing shards. It checks all
recovered bytes before and after timing. The candidate and baseline use the
ordinary x86 target without explicitly enabling AVX2.

| Candidate and run | Shard size | Median latency change | Range across four cases |
| --- | ---: | ---: | ---: |
| [Direct initial](../benchmarks/gf16-truncated-fwht/2026-11-01/results.json) | 64 B | −6.90% | −9.25% to −5.40% |
| Direct initial | 4 KiB | −3.48% | −6.97% to −1.43% |
| [Direct follow-up](../benchmarks/gf16-truncated-fwht/2026-11-01/results-focused.json) | 1 MiB | +8.46% | +3.45% to +9.72% |
| [Outlined initial](../benchmarks/gf16-truncated-fwht/2026-11-01/results-outlined.json) | 4 KiB | −5.18% | −5.42% to −3.69% |
| [Outlined follow-up](../benchmarks/gf16-truncated-fwht/2026-11-01/results-outlined-focused.json) | 1 MiB | +1.39% | −0.24% to +2.73% |

The [std probe](../benchmarks/gf16-truncated-fwht/2026-11-01/std-fwht-probe.rs)
preselects GFNI-AVX2, uses a single job and checks the recovered bytes and
encoded parity. Its cold cases cycle through 24 or 32 distinct valid erasure
patterns, exceeding the 16-entry plan cache; warm cases reuse one pattern.
The std and no_std benchmarks are separate binaries, so their results are not
compared to one another.

| Candidate and run | Observed change |
| --- | --- |
| [Outlined initial](../benchmarks/gf16-truncated-fwht/2026-11-01/results-std.json) | 64 B cold recovery median −7.84%; 64 KiB cold recovery median −4.26% across three cases per size |
| [Outlined encode follow-up](../benchmarks/gf16-truncated-fwht/2026-11-01/results-std-focused.json) | 16+8 encode +2.98% at 64 KiB and +3.58% at 1 MiB |
| [Std-only initial](../benchmarks/gf16-truncated-fwht/2026-11-01/results-stdonly.json) | 64 B cold recovery median −8.30% and 64 KiB median −4.11% across three cases per size |
| [Std-only encode follow-up](../benchmarks/gf16-truncated-fwht/2026-11-01/results-stdonly-focused.json) | 16+8 encode +1.81% at 64 KiB and +2.16% at 1 MiB |
| [x86 cold path initial](../benchmarks/gf16-truncated-fwht/2026-11-01/results-cold.json) | 16+8 encode +1.62% at 64 KiB and +2.48% at 1 MiB; 10+4 warm 1 MiB recovery +2.86% |

One initial std 1 MiB cold case had A1-to-A2 drift of +67.86%; its positive
candidate change is not treated as an established regression. A local AArch64
screen was not isolated and did not show a consistent cold-recovery gain; it
is preliminary evidence only. The stored JSON retains stage samples, estimates
and baseline changes for independent review.

No experiment is promoted: the direct and outlined variants slowed some
no_std large-shard recovery cases, while all three std-screened variants showed
unfavorable 16+8 encode results in at least one screen. No cause was isolated;
the tests do not establish whether these effects arise from code layout,
compiler decisions, scheduling or another factor. A future candidate needs
fresh recovery, encode, target-matrix and compatibility validation.
