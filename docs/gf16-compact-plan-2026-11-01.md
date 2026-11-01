# Compact GF16 recovery plans: rejected experiment

A GF16 recovery plan uses locator indices below `work_size`, but stores all
65,536 `u16` values. An experimental std plan retained exactly the used prefix.
Up to 256 values were stored inline; larger prefixes used heap storage. The
16-entry cache would retain 512 bytes of inline locator capacity per small plan
instead of a 128 KiB locator array, excluding metadata,
keys and FFT schedules. The original full transform still computed large or
dense patterns before its unused tail was discarded. The candidate retained the
full array and existing algorithm for `no_std`. The plan cache key and public
API were unchanged.

**Decision: rejected.** A repeatable large-shard encoding regression outweighs
the small-shard recovery gain. The candidate is preserved as a
[patch](../benchmarks/gf16-compact-plan/2026-11-01/candidate-inline256.patch);
production code remains unchanged at this experiment. The later
[follow-up](gf16-compact-plan-followup-2026-11-02.md) investigates the encoding
regression and evaluates a revised candidate.

The candidate threshold tests compared every used value against the full-transform
oracle, checked the exact representation in the full-transform fallback, and asserted both
the retained length and inline/heap boundary. Existing tests also exercise
concurrent cache use, parallel chunks and recovery publication behavior.

## Measurements

On the Linux x86-64 VM used for the previous sparse-locator evaluation, the
same [recovery probe](../benchmarks/gf16-std-sparse-locator/2026-11-01/std-fwht-probe.rs)
compared baseline `7345307` to the compact plan with the GFNI GF8 backend
selected, 32 rotating
patterns for cache misses and one repeated pattern for cache hits. The codec,
tables and fixtures were already initialized. Four A1/B1/B2/A2 phases used 20
Criterion samples, a 0.5 s warmup and 2 s requested measurement. One worker ran
on a guest CPU partition under a shared lease. Physical-host isolation was not
verified; drift was recorded but did not gate the experiment. Values below
compare the mean of candidate phases to the mean of baseline phases.

| Case | 64 B | 1 MiB |
| --- | ---: | ---: |
| 10+4, four missing, cache miss | −63.67% | −4.65% |
| 16+8, one missing, cache miss | −48.79% | −0.43% |
| 16+8, four missing, cache miss | −49.06% | −1.07% |
| 10+4, four missing, cache hit | −8.66% | −2.22% |
| 16+8, one missing, cache hit | −4.23% | +0.39% |
| 16+8, four missing, cache hit | −4.78% | +0.12% |

The 10+4, four-missing 64 B cache miss changed from 7.90 µs to 2.87 µs;
its baseline drift was −3.35%. The 10+4, 1 MiB cache-miss baseline drift
was +4.62%. Encoding in the mixed screen varied from −4.20% to −1.95% at
64 B, but baseline drift was +7.02% to +8.31%; those results do not establish
an encoding improvement. A separate 40-sample pure-encoding control found
−2.09% and −2.00% at 64 B but +22.39% and +23.98% at 1 MiB, with baseline
drift between −1.37% and +0.99%. A second run gave +24.28% and +20.62% at
1 MiB. A separate candidate with a 64-value inline capacity still gave +22.00%
and +19.72% at 1 MiB. That variant changes the patch's inline capacity and
the corresponding test boundary from 256 to 64. These regressions conflict with the mixed screen and are
the reason for rejecting the change.

The compared binaries placed encoding hot functions at different addresses;
the observed symbols had the same sizes. A code-layout effect is a hypothesis,
not a demonstrated root cause. The cause remains open.

The [threshold probe](../benchmarks/gf16-std-sparse-locator/2026-11-01/threshold-probe.rs)
showed −6.66% and −3.19% for 64 B and 4 KiB at work size 256 with 16 terms;
the 48-work-size/16-term cases changed by −28.63% and −17.01%. The full
transform fallback cases at work size 257 or 17 terms ranged from −2.12% to
−0.48%. This is a small sample of possible configurations, not a bound on
performance for all large codes.

On the tested x86 AVX2 build, the candidate `no_std` recovery benchmark's `.text`
section was byte-identical before and after the change. The later std candidate
benchmark `.text`
matched the measured prototype after the `no_std` type preservation change.
The [raw stage estimates](../benchmarks/gf16-compact-plan/2026-11-01/x86-estimates.json)
retain per-stage measurements and confidence intervals.

## Validation

For the candidate, all-feature workspace tests excluding the fuzz package and
all-target, all-feature Clippy passed on AArch64. AArch64 NEON std and AArch64 and WASM
`no_std` passed the 1,708-case cross-version erasure corpus. A bounded std
AddressSanitizer cross-version fuzz run completed 7,610 iterations without a
crash. This is not exhaustive proof. The experimental diagnostic source
bindings were updated and their focused tests passed; they were restored with the code.
