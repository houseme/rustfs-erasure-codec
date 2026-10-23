# GFNI priority: complete affected-catalog review

Review date: 2026-10-23. Frozen sampler `022c0a7476e3d91bee33f86bf4ade55d27122949`,
baseline main `340fc191890c0f6717b6dba26c2afbe68276d64f`, candidate
`449920e428cee723e27f9280f62f917e08f784e0`. The lockfiles are equal. Later
combined main `9f935ee` adds shared ordinary Classic verification and is not
source-identical to this candidate. Measurements keep their original identities.

All planned data were collected before checking statistical failures. Independent
replay matches all 64 worker summaries, 768 observations (192 warmups and 576
measurements), original logs and JSON, fixed counts, rendered samplers, 128 parent
events and all eight complete regression-program outputs. All inputs are unchanged;
the three source overlays, four resource-cleanup checks and original observer
services are restored. Replay executes no codec workload.

All 64 distributions pass (maximum 4.1702%); no sample is short. All eight
regression programs pass their original encode 12% / stream 15% thresholds.
However, three of 32 same-version repeats exceed the unchanged 5% limit.
**The complete affected catalog is not accepted performance evidence.** The
earlier +15.0854% failure remains retained on its original runtime source.

## Every case and pairing

All eleven original small-file encode cases and all five original stream verify
cases ran phase-major A1/B1/B2/A2 with three warmups and nine samples. Counts
come unchanged from the original complete-campaign manifest. Medians use ns/iteration
for encode and ns/block for stream. Each ratio range contains all four candidate /
baseline pairings; these remain unqualified diagnostic ratios.

| Case | Iterations | A1 | B1 | B2 | A2 | Four-pair ratio |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `encode:10:4:16384` | 262,144 | 8,995.41 | 7,089.63 | 7,202.47 | 8,952.15 | 0.788139–0.804552 |
| `encode:10:4:262144` | 16,384 | 182,485.70 | 130,921.74 | 133,274.80 | 183,055.73 | 0.715202–0.730330 |
| `encode:10:4:524288` | 8,192 | 370,356.34 | 266,476.99 | 267,534.77 | 339,308.13 | 0.719515–0.788471 |
| `encode:10:4:65536` | 73,728 | 35,820.94 | 31,080.91 | 31,589.01 | 35,722.18 | 0.867674–0.884297 |
| `encode:4:2:1024` | 14,942,208 | 168.03 | 133.82 | 131.93 | 166.43 | 0.785157–0.804062 |
| `encode:4:2:131072` | 196,608 | 12,999.64 | 11,272.59 | 11,558.58 | 13,243.23 | 0.851196–0.889146 |
| `encode:4:2:16384` | 1,572,864 | 1,740.13 | 1,417.11 | 1,336.77 | 1,788.13 | 0.747580–0.814370 |
| `encode:4:2:262144` | 98,304 | 26,855.95 | 29,414.10 | 29,072.28 | 27,561.24 | 1.054825–1.095254 |
| `encode:4:2:4096` | 5,505,024 | 494.96 | 402.91 | 377.16 | 516.64 | 0.730025–0.814025 |
| `encode:4:2:524288` | 45,056 | 59,314.50 | 59,919.76 | 59,286.77 | 57,997.47 | 0.999532–1.033144 |
| `encode:4:2:65536` | 524,288 | 6,934.32 | 6,035.66 | 6,043.45 | 6,967.16 | 0.866301–0.871527 |
| `verify_stream:10:4:1048576:1048576` | 1,792 | 4,469,036.62 | 4,641,686.16 | 4,453,656.17 | 4,455,217.42 | 0.996558–1.041854 |
| `verify_stream:10:4:16777216:4194304` | 128 | 7,608,978.07 | 7,553,441.51 | 7,590,198.16 | 7,642,236.40 | 0.988381–0.997532 |
| `verify_stream:10:4:65536:65536` | 45,056 | 445,327.97 | 450,554.24 | 440,521.52 | 452,519.81 | 0.973486–1.011736 |
| `verify_stream:4:2:1048576:262144` | 3,712 | 167,551.00 | 176,008.46 | 168,046.15 | 172,102.39 | 0.976431–1.050477 |
| `verify_stream:4:2:65536:65536` | 176,128 | 14,412.90 | 14,648.72 | 14,155.35 | 14,263.58 | 0.982131–1.027002 |

The [complete derived JSON](../benchmarks/release-9.1.0/2026-10-23/gfni-priority-affected-catalog.json)
retains all sixteen cases, every phase aggregate, 32 repeats, 64 comparisons,
eight program outcomes and the complete repeat/service comparison. It includes
the 4+2/256 KiB encode diagnostic ratios above one; no unfavorable pairing is
discarded or substituted with a passing repeat.

## Retained repeat failures

| Case | Pair | Signed latency change | Calling-thread run change |
| --- | --- | ---: | ---: |
| `encode:10:4:524288` | A1/A2 | -8.383334% | -8.378779% |
| `encode:4:2:16384` | B1/B2 | -5.669285% | -5.663793% |
| `encode:4:2:4096` | B1/B2 | -6.391005% | -6.358435% |

The three failing repeats have equal recorded caller fixtures and workload
witnesses, and zero measured process minor/major faults. Calling-thread service
changes track latency changes. All sixteen cases also have exactly equal full
nine-sample minor/major-fault sequences across phases, including the nonzero
stream sequences. Equal counts do not establish equal internal owners, physical
pages, residency or cache state.

A separate complete cross-analysis associates every parent worker window with
the retained 3,514 host snapshots. Across all measured records, maximum calling-
thread runnable wait / wall is 0.038263%; the capture records no outside workload.
The failing pairs have low guest CPU pressure, no memory pressure and no recorded
steal. I/O pressure differs in both directions; these coarse parent-window
observations do not uniquely explain the repeat failures. Guest isolation and
low wait do not prove stable physical-host frequency, cache or memory service.
Host snapshots are not assigned to individual API samples without corresponding
internal absolute timestamps. Calling-thread counters are not exclusive API cycles.

## Implementation and remaining work

Before timing, Linux positively executed the GFNI-priority regression, direct
checked AVX2 byte test and six encoding-dispatch regressions. The formal sampler
was then rebuilt with metrics disabled and the predeclared idle interval completed.
All timing records have automatic override honored and the expected ordinary GFNI
backend label. Source review establishes the eligibility rule; an exported backend
label alone is not an ISA profiler.

The shared cached rule avoids a shape-specific threshold and preserves explicit
override and non-GFNI behavior. No allocator tuning, temporary allocation anchor,
timing-loop change or gate relaxation was introduced. This complete result does
not prove a universal speedup, uniquely explain historical drift, or qualify the
later combined source. Do not repeat this unchanged capture until one passes.

Freeze the final combined source/lock/harness and use a concrete recorded source
or condition change for its complete published-9.0.0 comparison and paired layouts.
Preserve the failed repeats and all earlier captures. Final release still requires
accepted complete evidence and an actual strict positive preflight. No tag or
registry upload follows from this diagnostic.
