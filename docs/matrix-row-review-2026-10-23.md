# Classic matrix row review

The candidate changes Classic matrix setup and cache-miss recovery. Products use
contiguous row accumulation; elimination borrows disjoint source/destination rows
without copying a pivot. GF8-capable fields use the existing batch operator;
other fields retain scalar row arithmetic. Matrix storage, inversion-cache policy,
encoding format and public recovery publication/failure semantics are unchanged.
No shard-shape, allocator, frequency or scheduling threshold is introduced.

Native ARM64 samples on baseline main `549b0ac5fc8bb16723d5dab20c934aea5024f5c0`
identified inversion and multiplication as about 93% of the recorded constructor
stack-top samples for 32+16. Cold 32+16 recovery also showed substantial inversion
cost. This motivated a runtime prototype without changing the ongoing Linux run.

## Complete local diagnostics remain rejected

Both cohorts cover all 23 declared GF8/GF16 constructor, cold recovery, warm
recovery and encoding cases in A1/B1/B2/A2 order. Each fresh worker has three
warmups and nine observations; both roles calibrate independently toward 200 ms,
then use the same maximum fixed count for that case. Timing disables metrics.
Thread CPU time is recorded separately from wall time. Central width is
`(sorted[7] - sorted[1]) / sorted[4]` over the nine measured observations.
Limits remain 10% central width, 5% same-version repeat and 12% local regression;
all four candidate/baseline pairings are retained. Samples must last at least
100 ms. No unchanged campaign was rerun to seek a passing sample.

The first prototype batched every field. Its 1,104 observations have one repeat
failure (GF16 10+4 construction: 5.5817%) and two regression observations (GF16
4+2 and 10+4 construction: maximum ratios 2.1839 and 2.6422). Native samples show
that the GF16 block operator introduces plane conversion and four 1024-byte stack
planes into short matrix rows; memset becomes the largest stack-top cost.
The final prototype confines this batch path to the existing GF8 byte-field
capability and keeps other fields on contiguous scalar rows, with no size cutoff.

The second cohort contains another 1,104 observations. It has **five width
failures, fourteen same-version repeat failures and three regression
observations**, with no short samples. Both baseline and candidate repeats fail;
unchanged warm recovery and encoding controls also vary. CPU and wall records
retain the service variation; they do not uniquely identify its hardware or
layout cause. The first prototype's large GF16 regression observations disappear,
but the complete second comparison is still rejected. No speedup, regression
attribution, Linux/x86 result or published-9.0.0 throughput claim is accepted.

The [complete numerical record](../benchmarks/release-9.1.0/2026-10-23/matrix-row-component-diagnostics.json)
contains both cohorts, every warmup/measured wall and CPU observation, counts,
backend labels, all case summaries and refusal lists. Original binaries, source
patches, logs and native samples are retained privately; process addresses and
host-specific records are excluded from the public numerical record. The second
launch initially refused a copied binary without execute permission before any
calibration or observation; that failure was retained and its copy mode repaired.
Independent replay checks all 2,208 observations and reproduces both refusals.

The table below is the complete second-cohort diagnostic, in ns/operation. Its
ratios remain unqualified because the whole comparison fails the declared gates.

| Case | A1 | B1 | B2 | A2 | Unqualified four-pair ratio |
| --- | ---: | ---: | ---: | ---: | ---: |
| `gf8:construct:4:2:0:0` | 1460.52 | 1455.97 | 1524.83 | 1422.20 | 0.9969–1.0722 |
| `gf8:construct:10:4:0:0` | 3582.18 | 2784.55 | 2835.62 | 3490.96 | 0.7773–0.8123 |
| `gf8:construct:32:16:0:0` | 63288.38 | 15357.74 | 15588.59 | 62345.86 | 0.2427–0.2500 |
| `gf8:cold:10:4:1024:1` | 1057.91 | 993.37 | 970.12 | 1099.66 | 0.8822–0.9390 |
| `gf8:cold:10:4:1024:4` | 2195.11 | 1672.50 | 1730.15 | 2105.69 | 0.7619–0.8217 |
| `gf8:warm:10:4:1024:1` | 272.79 | 248.10 | 250.47 | 245.08 | 0.9095–1.0220 |
| `gf8:encode:10:4:1024:0` | 603.23 | 723.38 | 604.88 | 598.33 | 1.0027–1.2090 |
| `gf8:cold:32:16:4096:1` | 8376.33 | 10200.89 | 7126.10 | 8324.02 | 0.8507–1.2255 |
| `gf8:cold:32:16:4096:16` | 67127.56 | 48883.07 | 45961.77 | 66112.69 | 0.6847–0.7394 |
| `gf8:warm:32:16:4096:1` | 2590.23 | 2967.29 | 2587.70 | 2594.68 | 0.9973–1.1456 |
| `gf8:encode:32:16:4096:0` | 39439.65 | 41482.11 | 39111.47 | 39169.67 | 0.9917–1.0590 |
| `gf16:construct:4:2:0:0` | 1135.84 | 1122.99 | 1095.50 | 1171.62 | 0.9350–0.9887 |
| `gf16:construct:10:4:0:0` | 5161.02 | 4927.31 | 4269.33 | 4880.03 | 0.8272–1.0097 |
| `gf16:construct:32:16:0:0` | 132522.07 | 104448.79 | 103018.11 | 137138.88 | 0.7512–0.7882 |
| `gf16:cold:10:4:1024:1` | 2638.19 | 2515.06 | 2447.06 | 2685.98 | 0.9110–0.9533 |
| `gf16:cold:10:4:1024:4` | 8719.82 | 7964.55 | 7505.18 | 7838.99 | 0.8607–1.0160 |
| `gf16:warm:10:4:1024:1` | 1601.18 | 1567.56 | 1559.65 | 1540.82 | 0.9741–1.0174 |
| `gf16:encode:10:4:1024:0` | 6097.00 | 6057.42 | 5996.88 | 5938.35 | 0.9836–1.0201 |
| `gf16:cold:32:16:4096:1` | 25431.87 | 24121.08 | 23630.14 | 24751.21 | 0.9292–0.9745 |
| `gf16:cold:32:16:4096:16` | 347810.30 | 309765.06 | 303237.14 | 321299.52 | 0.8718–0.9641 |
| `gf16:warm:32:16:4096:1` | 17497.45 | 18439.45 | 17084.12 | 17146.96 | 0.9764–1.0754 |
| `gf16:encode:32:16:4096:0` | 302012.08 | 286138.88 | 272031.94 | 271405.48 | 0.9007–1.0543 |
| `gf8:construct:128:64:0:0` | 3585598.27 | 322461.75 | 301635.13 | 3537075.28 | 0.0841–0.0912 |

## Compatibility and integration boundary

The independent scalar algebra oracle covers GF8/GF16 products, empty and
rectangular dimensions, vector tails, inverse identities, row swaps and errors.
Core release/all-feature tests pass 361 cases (five existing ignored); codec-only
no_std passes 239. The real published 9.0.0 comparison passes exact parity and
bidirectional cross-decoding in 200 configurations, including historical
GF16 tower-field vectors. This is functional evidence.

The diagnostic observer changes are byte-snapshot expectations only: complete
render comparison finds the new matrix source and eight unchanged members; the
observer edits themselves are identical. Native tooling passes 172 unit and 28
CLI tests. Strict codec Clippy, formatting and diff checks pass. PR CI must
independently validate its exact head.

PR #68 was externally merged as `c5d8530`; reviewed main `0a85348` contains the
same matrix source as the final recorded prototype. Integration does not qualify
the rejected performance comparisons. Do not weaken bounds, select favorable
cases, rewrite the original refusals or promote these diagnostics into release
evidence. After the currently owned Linux capture releases resources, a concrete
controlled condition and final-source binding must support any further complete
comparison. The running frozen capture is not changed or restarted.
The existing #2897 completion markers and final-source ABBA/layout, immutable
evidence and actual strict positive release preflight remain unchanged.


## Integrated-source follow-through

Current main `0a85348` independently passes 361 codec library tests plus 166
integration tests (527 total / 43 existing ignored), 239 codec-only no_std,
211 native tools and 246 Python tests. The 200 published-9.0.0 parity and
bidirectional cross-decoding configurations pass on this actual source, as do
strict codec/tool Clippy, Linux tool compilation checks and package preparation.
These new functional checks do not relabel the original prototype validations.

An independent replay consumes all 2,208 numeric observations and checks every
median, CPU median, width, repeat and all four pair ratios. Both original cohorts'
complete failure lists match, all input bytes remain unchanged, and no codec
workload runs during replay. The source hash of the reviewed matrix matches the
public record. The [integrated functional record](../benchmarks/release-9.1.0/2026-10-23/integrated-main-functional-review.json)
keeps the functional and numerical scopes separate; neither grants performance
approval or replaces the published-baseline Linux release requirements.
