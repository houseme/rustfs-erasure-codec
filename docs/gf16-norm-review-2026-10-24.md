# GF16 norm inversion review

Executed candidate: `14b1b9849e12242380e21b5cd5aa5a52845436c1`.
Comparator: unreleased main `a773e742e344a26413b8446480044daa68abd0bd`.

## Arithmetic and affected calls

For the existing tower polynomial `X^2 + 2X + 128`, conjugation replaces `X`
with `X + 2`. An element `[a, b]` therefore has conjugate `[a, b + 2a]` and
GF8 norm `b*(b + 2a) + 128*a^2`. Its inverse is the conjugate multiplied by
the inverse of that nonzero norm. All products and sums here use the existing
GF8 field. This replaces recursive polynomial Euclid in production. Constant
elements `[0, b]` need only one GF8 inverse; zero still inverts to zero, preserving
the historical division-by-zero result.

The change benefits calls that calculate GF16 matrix inverses: codec construction
and cold recovery. Cached recovery and steady-state encoding use the same
existing payload kernels. It adds no allocation, unsafe code, cache policy,
backend override, shape threshold or public API. Field polynomial, element order,
byte representation, encoded parity, error handling and recovery publication are
unchanged. Generic fallible reconstruction, selective recovery and Leopard paths
retain their previous implementations.

## Compatibility executed on the candidate

The old Euclidean implementation is retained only under `cfg(test)` as a separate
algorithm. All 65,536 inverses match it exactly; multiplication verifies the
identity for every nonzero element. Five numerators over every denominator check
327,680 quotients, including zero denominators. Debug and release runs pass.

The complete all-feature codec run passes 538 tests (372 library and 166
integration; 43 existing ignored). Codec-only no_std passes 246 tests. Strict
all-target/all-feature codec Clippy, formatting and diff checks pass. The actual
published 9.0.0 probe passes all 200 exact-parity and bidirectional recovery
configurations, including the historical tower arithmetic vectors.

A separate executable linked to the actual published 9.0.0 crate also compares
all 65,536 inverse results and 327,680 quotients exactly. It does not rely on
the retained local Euclidean oracle.

## Complete local component diagnostic

This is a separate native ARM64 comparison with a frozen diagnostic harness.
All 27 existing constructor, cold/warm recovery, encoding and general-tower
control case labels remain, with two added constant/general denominator controls.
The fixture, including destination reset and reconstruction allocation/drop,
stays identical between roles within this comparison. It does not assert address
or physical-memory equivalence to any earlier campaign.

Each role independently doubles pilot counts until a 200 ms operation batch;
the larger count is shared. Every case runs in phase-major A1/B1/B2/A2 order,
with three warmups and nine measurements in a fresh worker. Wall time and calling
thread CPU time remain separate. Unchanged limits are 10% central width, 5%
same-version repeat, 100 ms minimum measurement and 12% regression across all
four cross-version pairings. Statistical refusals do not stop later workers.

All 116 workers and 1,392 observations completed without execution errors:
348 warmups and 1,044 measurements. Independent replay reproduces all 116
distributions, 58 repeats and 116 cross-version comparisons. There are **30
width failures, 25 repeat failures, five regression cases and zero short samples**.
The complete local comparison is rejected. The five regression cases are GF16
construction 10+4, warm recovery 10+4 / one missing, cold recovery 32+16 / one
missing, encoding 32+16, and the unchanged GF8 constructor 128+64. Both roles
and unchanged controls retain substantial refusals; no unique environment,
frequency, layout or runtime-code cause is established. Isolated arithmetic
ratios are unqualified too; they cannot establish end-to-end encoding/decoding
throughput or supersede the failed whole comparison. No unchanged rerun is used
to select a passing result.

The [complete derived artifact](../benchmarks/release-9.1.0/2026-10-24/gf16-norm-component-review.json)
retains every case, phase median, distribution check, repeat and comparison,
including all refusals. An independent rational replay checks the original logs,
calibration counts, parent events, both source archives and every report field;
it executes no codec workload and leaves original inputs unchanged. Raw samples,
commands, logs and source copies remain privately preserved.

All medians below are ns/operation. These are unqualified observations.

| Case | A1 | B1 | B2 | A2 | Unqualified ratio range |
| --- | ---: | ---: | ---: | ---: | ---: |
| `gf8:construct:4:2:0:0` | 1594.52 | 1646.68 | 1577.29 | 1574.65 | 0.9892–1.0457 |
| `gf8:construct:10:4:0:0` | 3068.23 | 3068.96 | 2987.79 | 2951.64 | 0.9738–1.0397 |
| `gf8:construct:32:16:0:0` | 14692.14 | 15074.42 | 14839.38 | 14516.16 | 1.0100–1.0385 |
| `gf8:cold:10:4:1024:1` | 993.71 | 1023.20 | 1042.50 | 985.85 | 1.0297–1.0575 |
| `gf8:cold:10:4:1024:4` | 2009.08 | 1904.13 | 1783.30 | 1731.40 | 0.8876–1.0998 |
| `gf8:warm:10:4:1024:1` | 270.85 | 274.35 | 270.51 | 268.82 | 0.9988–1.0206 |
| `gf8:encode:10:4:1024:0` | 621.74 | 634.68 | 667.66 | 599.40 | 1.0208–1.1139 |
| `gf8:cold:32:16:4096:1` | 7146.56 | 7444.86 | 7298.30 | 7156.99 | 1.0197–1.0417 |
| `gf8:cold:32:16:4096:16` | 48676.05 | 48842.97 | 48013.18 | 46480.83 | 0.9864–1.0508 |
| `gf8:warm:32:16:4096:1` | 2762.06 | 2805.81 | 2848.79 | 2643.84 | 1.0158–1.0775 |
| `gf8:encode:32:16:4096:0` | 41024.41 | 41583.11 | 40137.90 | 47244.32 | 0.8496–1.0136 |
| `gf16:construct:4:2:0:0` | 1169.56 | 1114.80 | 1162.51 | 1138.36 | 0.9532–1.0212 |
| `gf16:construct:10:4:0:0` | 4135.99 | 4802.15 | 4210.17 | 4539.91 | 0.9274–1.1611 |
| `gf16:construct:32:16:0:0` | 94912.25 | 95330.14 | 93326.80 | 91970.72 | 0.9833–1.0365 |
| `gf16:cold:10:4:1024:1` | 1343.65 | 1346.21 | 1380.14 | 1389.70 | 0.9687–1.0272 |
| `gf16:cold:10:4:1024:4` | 3121.29 | 3113.71 | 3118.77 | 3472.91 | 0.8966–0.9992 |
| `gf16:warm:10:4:1024:1` | 469.22 | 630.14 | 471.47 | 466.91 | 1.0048–1.3496 |
| `gf16:encode:10:4:1024:0` | 1601.91 | 1693.29 | 1587.34 | 1747.93 | 0.9081–1.0570 |
| `gf16:cold:32:16:4096:1` | 11595.53 | 13714.38 | 11160.31 | 12269.65 | 0.9096–1.1827 |
| `gf16:cold:32:16:4096:16` | 127444.38 | 110152.40 | 105342.18 | 114296.26 | 0.8266–0.9637 |
| `gf16:warm:32:16:4096:1` | 5675.36 | 5724.18 | 5187.39 | 5476.44 | 0.9140–1.0452 |
| `gf16:encode:32:16:4096:0` | 105472.43 | 92487.12 | 80904.85 | 80197.33 | 0.7671–1.1532 |
| `gf8:construct:128:64:0:0` | 422832.52 | 353919.11 | 291157.51 | 291877.28 | 0.6886–1.2126 |
| `gf16:multiply_tower:0:0:1024:0` | 170.52 | 175.62 | 155.82 | 157.19 | 0.9138–1.1172 |
| `gf16:accumulate_tower:0:0:1024:0` | 239.17 | 195.74 | 190.64 | 190.51 | 0.7971–1.0275 |
| `gf16:multiply_tower:0:0:1025:0` | 181.64 | 176.09 | 174.47 | 174.61 | 0.9606–1.0085 |
| `gf16:accumulate_tower:0:0:1025:0` | 234.66 | 234.51 | 210.41 | 211.71 | 0.8967–1.1077 |
| `gf16:div_const:0:0:0:0` | 8.43 | 1.78 | 1.43 | 8.39 | 0.1696–0.2123 |
| `gf16:div_tower:0:0:0:0` | 14.86 | 2.77 | 2.35 | 15.01 | 0.1567–0.1863 |

This local component diagnostic is not a published-9.0.0 throughput comparison or
final Linux release acceptance. The separate complete Linux capture remains
bound to frozen `9f935ee`; it cannot be relabeled as this candidate. A final
source/lock freeze, complete accepted evidence and strict positive release
preflight remain required. No tag, registry upload or PR merge is performed by
this change.
