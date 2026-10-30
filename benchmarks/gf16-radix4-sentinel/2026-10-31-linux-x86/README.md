# GF16 radix-4 sentinel fusion screen

This is an isolated experiment; the proposed specialization is **not included
in the production codec**. It targets the radix-4 shape where `log_m01` and
`log_m02` are the modulus sentinel and `log_m23` is an ordinary multiplier.
For that shape, the [prototype](prototype.patch) combines three XOR traversals
of four lanes before or after the existing fused radix-2 butterfly. Other
multiplier shapes, short lanes, non-AVX2 hosts and `no_std` retain the original
operations. No public API is changed.

The test-only [work-count probe](instrumentation.patch) records sentinel masks
for actual radix-4 helper calls during recovery. It uses atomic counters across
Rayon workers and verifies the recovered bytes; the counters are reset after
encoding. The [observations](observations.txt) cover 10+4 and 16+8, 64 KiB and
1 MiB shards, and one or four missing data shards. For one 64 KiB chunk,
10+4 recorded the target mask in 5 of 8 forward and 5 of 8 inverse radix-4
calls. For 16+8 it occurred in 10 of 14 forward and 5 of 14 inverse calls.
These are instrumented work counts, not latency measurements.

On native x86-64 the prototype's AVX2 test compared the specialized forward
and inverse operations against the pairwise reference across boundary lengths
and representative multipliers. It executed on an AVX2-capable host. The GF16
library tests, target-container regression tests and all 96 benchmark
functional cases passed. AArch64 GF16 tests and an x86 `no_std` check passed
separately.

## Performance screen

The native Linux x86-64 screen used an AMD EPYC 9V45 virtual machine. A was
the production computational source at `a2d6bba`; B was A plus the prototype.
Both builds used identical locks, benchmark harness, Rust stable 1.99.1,
`simd-accel` and four Rayon workers. Four phases A1/B1/B2/A2 each completed
96 cases, with ten samples per case, 0.2 seconds warmup and 0.6 seconds
requested measurement time. The
[raw estimates and samples](measurements.json) are retained. An exclusive
guest CPU partition covered CPUs 8–15; periodic checks found no competing heavy
process, and the shared lock and partition were released. Physical-host
isolation was not verified. Drift did not stop the run at the user's request.

For each case, average the two Criterion mean estimates on each side, then
compute `(prototype / baseline - 1) * 100`. Each row gives the median and
range of eight cases: 10+4 and 16+8, with one through four missing shards.
Negative values mean lower latency.

| GF16 recovery entrypoint | Shard size | Median | Range |
| --- | ---: | ---: | ---: |
| Retained slots | 64 KiB | −2.89% | −6.44% to +4.46% |
| Retained slots | 1 MiB | −0.93% | −5.48% to +0.55% |
| Generic Option | 64 KiB | −2.52% | −7.26% to +3.00% |
| Generic Option | 1 MiB | −0.15% | −7.21% to +3.97% |
| Optimized Option | 64 KiB | −3.64% | −8.77% to −1.03% |
| Optimized Option | 1 MiB | −0.77% | −6.51% to +1.12% |

The 64 KiB screen suggests some opportunity, while 1 MiB medians are below 1%
and some cases increase. This is a short virtual-machine screen, not a proof of
no benefit for every workload. Given the mixed results and the extra specialized
kernel, keep the existing implementation until a targeted experiment shows a
larger, reproducible gain across relevant sizes without unacceptable regressions.

To reproduce the test-only source overlays, extract `a2d6bba` into a clean
directory and apply either `instrumentation.patch` or `prototype.patch` with
`git apply`. The former runs the exact test
`core::leopard_gf16::ops::coefficient_work_probe::observe_butterfly_multiplier_categories`
with `RAYON_NUM_THREADS=4`; the latter runs
`core::leopard_gf16::ops::radix4_sentinel_experiment_tests::sentinel_radix4_matches_pairwise_reference`
on an AVX2 host. Confirm that one test ran in each invocation.
