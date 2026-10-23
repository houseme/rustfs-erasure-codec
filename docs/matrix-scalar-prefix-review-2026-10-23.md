# Scalar matrix pivot review

This draft evaluates one remaining cost in Classic matrix setup and cache-miss
reconstruction. Its baseline is unreleased main
`ebb60f286b639c266eb3c46ae556cdd7519de188`, not published 9.0.0.
Native ARM64 samples on that exact source still show substantial inversion and
field-multiplication work in cold 32+16 recovery and GF16 32+16 construction.
The ongoing Linux capture retains its separate frozen source and ownership.
No additional workload was started there.

## Runtime boundary

Gaussian elimination has a proven zero prefix before each pivot. Forward
elimination establishes it, row swaps involve only remaining rows, and back
substitution preserves it. Scalar fields now omit that prefix during pivot
normalization and row elimination. The existing disjoint row borrows and shared
field arithmetic remain the only executor; no scratch, unsafe code, shape
threshold, allocator control, backend override or extra policy layer is added.

GF8 batch rows retain their complete previous spans. The first prototype trimmed
all fields: its native GF8 32+16 constructor samples showed scalar-tail top-of-stack
counts increasing from 136 to 298 over the two-second samples. Changing pivot
lengths introduces additional SIMD remainders. These counts and the rejected
comparison support preserving the full byte-field batch path; they do not
uniquely explain every timing fluctuation or establish a version-level slowdown.
The final implementation limits prefix omission to the existing scalar-field
path, including built-in GF16. Pivot order, arithmetic, inverse errors, cache
policy, parity bytes and public reconstruction publication remain unchanged.
VerifyWorkspace and Leopard output ownership are outside this change. Generic
ReconstructShard retains decode-before-publication, late failure and selective
recovery semantics.

## Complete diagnostics and refusals

Each prototype covers the same complete 23-case catalog, in phase-major
A1/B1/B2/A2 order. Both roles calibrate independently toward 200 ms; each case
uses the larger common power-of-two count. Every fresh worker retains three
warmups and nine measurements. Thread CPU time is separate from wall time.
Limits remain 5% repeat, 10% central width, 100 ms minimum sample and 12% local
latency regression; every one of the four cross-version pairings is checked.
Central width is `(sorted[7] - sorted[1]) / sorted[4]` over nine measured values.
No cases, counts or rejected samples were selected after observing results.

| Complete prototype | Observations | Width failures | Repeat failures | Unqualified regression cases | Short samples | Accepted |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| Trim all field rows | 1,104 | 6 | 14 | 2 | 0 | No |
| Trim scalar field rows only | 1,104 | 5 | 4 | 0 | 0 | No |

The first prototype includes an unqualified GF8 32+16 constructor ratio of
1.0973–1.2717 and an encoding-control regression observation. The later GF8
constructor ratio is 0.9976–1.0156, but its complete comparison still fails.
All four later repeat failures are baseline A1/A2: GF8 cold 10+4 / one missing
(5.7082%), GF8 warm 32+16 / one missing (5.4890%), GF16 4+2 construction
(6.2593%) and GF16 10+4 construction (12.5472%). Both roles retain width
failures. This does not qualify any aggregate speedup or uniquely attribute the
residual to environment, frequency, layout or runtime code. There is no third
unchanged capture to search for acceptance.

The [complete numerical artifact](../benchmarks/release-9.1.0/2026-10-23/matrix-scalar-prefix-diagnostics.json)
retains all 2,208 observations, both complete calibration sets, all widths,
repeats, ratios and refusals. Each phase's 12-element wall/CPU arrays retain the
original order, with the first three elements representing warmup. Case IDs are
`field:operation:data:parity:shard_size_elements:missing_data`; GF8 elements are
one byte and GF16 elements are two bytes. Cold cases disable inversion caching;
warm cases retain it. Recovery reuses caller-owned ShardSlot buffers. Fixture
preparation, expected bytes and cross-role witnesses are checked outside timing.
Private binaries, native call stacks, full source plans, original journals and
logs remain retained; the public artifact excludes addresses, host identities,
private paths and executable fingerprints.

Independent rational replay checks all records, phase/case order, common counts,
witness/backend consistency, wall and CPU medians, every width, both repeats and
all four ratios. It matches the original sampler reports without executing the
codec again. The complete later diagnostic table follows; all ratios are
unqualified observations.

| Case | A1 ns/op | B1 ns/op | B2 ns/op | A2 ns/op | Unqualified ratio range |
| --- | ---: | ---: | ---: | ---: | ---: |
| `gf8:construct:4:2:0:0` | 1544.05 | 1522.91 | 1511.08 | 1545.50 | 0.9777–0.9863 |
| `gf8:construct:10:4:0:0` | 2926.83 | 2970.68 | 2927.41 | 2910.52 | 1.0002–1.0207 |
| `gf8:construct:32:16:0:0` | 15530.43 | 15613.07 | 15493.67 | 15373.98 | 0.9976–1.0156 |
| `gf8:cold:10:4:1024:1` | 999.39 | 1013.77 | 972.23 | 942.34 | 0.9728–1.0758 |
| `gf8:cold:10:4:1024:4` | 1679.18 | 1686.70 | 1701.28 | 1716.82 | 0.9825–1.0132 |
| `gf8:warm:10:4:1024:1` | 247.82 | 246.65 | 246.53 | 245.72 | 0.9948–1.0038 |
| `gf8:encode:10:4:1024:0` | 603.79 | 609.62 | 600.16 | 598.29 | 0.9940–1.0189 |
| `gf8:cold:32:16:4096:1` | 7085.97 | 7149.37 | 7067.97 | 7029.52 | 0.9975–1.0170 |
| `gf8:cold:32:16:4096:16` | 46787.38 | 47144.66 | 45762.76 | 45656.75 | 0.9781–1.0326 |
| `gf8:warm:32:16:4096:1` | 2748.09 | 2679.24 | 2589.01 | 2597.24 | 0.9421–1.0316 |
| `gf8:encode:32:16:4096:0` | 40985.66 | 40260.80 | 39137.47 | 38966.88 | 0.9549–1.0332 |
| `gf16:construct:4:2:0:0` | 1131.86 | 1128.85 | 1083.60 | 1061.01 | 0.9574–1.0639 |
| `gf16:construct:10:4:0:0` | 4876.39 | 4206.81 | 4043.36 | 4264.54 | 0.8292–0.9865 |
| `gf16:construct:32:16:0:0` | 106512.14 | 97116.33 | 95215.11 | 101742.15 | 0.8939–0.9545 |
| `gf16:cold:10:4:1024:1` | 2471.75 | 2427.65 | 2410.93 | 2398.66 | 0.9754–1.0121 |
| `gf16:cold:10:4:1024:4` | 7637.31 | 7549.63 | 7549.78 | 7451.18 | 0.9885–1.0132 |
| `gf16:warm:10:4:1024:1` | 1580.25 | 1570.23 | 1560.10 | 1531.15 | 0.9873–1.0255 |
| `gf16:encode:10:4:1024:0` | 6077.57 | 6048.75 | 6023.62 | 5913.87 | 0.9911–1.0228 |
| `gf16:cold:32:16:4096:1` | 23607.76 | 23102.79 | 23523.57 | 23463.66 | 0.9786–1.0026 |
| `gf16:cold:32:16:4096:16` | 303729.45 | 300170.37 | 297532.80 | 297796.14 | 0.9796–1.0080 |
| `gf16:warm:32:16:4096:1` | 17253.27 | 17166.04 | 17031.47 | 16886.14 | 0.9871–1.0166 |
| `gf16:encode:32:16:4096:0` | 274136.15 | 273870.44 | 272619.30 | 269352.50 | 0.9945–1.0168 |
| `gf8:construct:128:64:0:0` | 302784.63 | 302971.11 | 309934.65 | 298681.80 | 1.0006–1.0377 |

## Executed functional checks

The final source passes 361 release/all-feature library tests (five existing
ignored), 239 codec-only no_std tests, strict all-target/all-feature codec Clippy,
192 native tool unit tests and 29 CLI tests, formatting and diff checks.
The independent scalar algebra tests additionally exercise 156 GF8/GF16
multiple-right-hand-side systems across pivot swaps, normalization and both
elimination passes. Inverse identities and singular/non-square errors remain
covered. The real published 9.0.0 comparison passes 200 exact-parity configurations
and bidirectional cross-decoding, including historical GF16 tower-field vectors.
These are correctness and compatibility results, not throughput acceptance.

A complete generated-source comparison changes only the matrix member among
nine outputs. The inserted observer edits are identical; only after that check
are the two source-inclusive golden digests updated. The sampler execution,
observer kernels and formal release limits remain unchanged.

Keep this implementation draft pending controlled performance qualification.
Ordinary CI is recorded against its actual head separately. Do not merge on
functional checks alone or call the diagnostic ratios accepted improvements.
#2897 remains open with only condition 4 checked within the declared normal
format scope; 1/2/3/5 remain partial and 6 incomplete. Final-source full ABBA and
all layouts, accepted immutable evidence and actual strict positive release
preflight remain required before a 9.1.0 tag or publication.
