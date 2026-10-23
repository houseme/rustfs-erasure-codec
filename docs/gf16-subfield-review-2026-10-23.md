# GF16 subfield slice review

Baseline: unreleased main `ebb60f286b639c266eb3c46ae556cdd7519de188`.
This change targets actual Classic GF16 payload multiplication during encoding
and recovery, separately from matrix planning and PR #73.

Native ARM64 sampling of 10+4 / 1024-element encoding on that exact main source
found substantial byte-plane conversion and initialization. Recorded top-of-stack
counts include 456 for deinterleave and 290 for memset in a two-second sample.
These are path-localization samples, not a latency measurement or a full-version
comparison. Actual encoding-format inspection finds all 8 / 40 / 512 parity
coefficients for 4+2 / 10+4 / 32+16 within the GF8 subfield.

## One algebraic shortcut

For tower elements `[ax, ac]` and a constant multiplier `[0, c]`, the product is
`[gf8_mul(ax, c), gf8_mul(ac, c)]`. Both coordinates can therefore stay in their
existing interleaved byte layout. One existing GF8 multiply or XOR-multiply kernel
handles the complete flattened slice. No plane conversion, per-plane multiply or
plane initialization is needed on this path. Multipliers with a nonzero X
coordinate retain the original tower decomposition. Length validation precedes
the shortcut, including coefficients zero and one.

The implementation adds no unsafe code, scratch, allocation, shape threshold,
backend override or public API. It uses the built-in tower-field representation,
not a generic field-size inference. Existing GF8 dispatch keeps its ISA selection
and fallback. Endianness, field polynomial, stable format identity, matrix
coefficients, encoded bytes and recovery publication remain unchanged. Generic
ReconstructShard retains decode-before-publication, late failure and selective
recovery; VerifyWorkspace and Leopard owned outputs are untouched.

## Executed compatibility checks

The independent element-wise tower oracle covers all 256 constant coefficients,
three input offsets and 15 lengths spanning empty inputs, SIMD tails and 1024
chunk boundaries, for overwrite and accumulation. Output sentinels protect both
sides of the borrowed destination. Existing mixed and pure-X coefficient cases
continue to test the general tower path. Two panic regressions ensure mismatched
lengths remain rejected before the zero-coefficient shortcut.

Final code passes 364 release/all-feature library tests (five existing ignored),
242 codec-only no_std tests, strict all-target/all-feature codec Clippy, formatting
and diff checks. The real published 9.0.0 comparison passes 200 exact-parity
configurations and bidirectional cross-decoding, including historical GF16
arithmetic vectors. Those are functional results, not throughput acceptance.

## Predeclared complete local comparison

All original 23 GF8/GF16 constructor, cold/warm recovery and encoding cases remain.
Four additional general tower coefficient controls cover overwrite/accumulation
at 1024/1025 elements with coefficient `[0x9a, 0x3f]`. The total is 27 cases in
phase-major A1/B1/B2/A2 order. Both roles calibrate independently toward 200 ms;
every case uses their larger common power-of-two count. Fresh workers retain
three warmups and nine measurements, with wall and thread CPU time separate.
Limits remain 5% repeat, 10% central width, 100 ms minimum duration and 12% local
latency regression, checking all four cross-role pairings. No case/count selection
or unchanged rerun is used.

Case IDs remain `field:operation:data:parity:size_in_elements:missing_data`.
GF16 elements occupy two bytes. The general tower controls use zero data/parity
counts as labels because they call the Field slice operations directly; their
multiplier is fixed as above. Accumulation toggles the output with each call and
checks its final value against the independent scalar product outside timing.
Ordinary codec fixtures retain expected-byte and witness checks outside timing.
This native local comparison is against unreleased main, not published 9.0.0,
Linux/x86 or the full release matrix. Original binaries, journals, native samples,
source patches and failures remain privately retained. Public observations exclude
addresses, host identities, private paths and executable fingerprints.

## Complete outcome remains rejected

All 27 cases and 1,296 original observations completed. Independent rational
replay reproduces every wall/CPU median, width, repeat and all four ratio pairs.
There are four width failures, four repeat failures, no short samples and no
regression-threshold case. The full comparison is rejected under the declared
limits; absence of a regression observation does not qualify the campaign.

Width failures are candidate B1 GF16 encode 10+4 (18.2341%), cold 32+16 / one
missing (12.9545%), warm 32+16 / one missing (18.7395%), and baseline A2 GF8 cold
10+4 / one missing (12.7605%). Repeat failures are candidate GF16 construction
10+4 (11.2513%), baseline GF16 construction 32+16 (12.6731%), candidate warm
32+16 / one missing (10.0987%) and candidate encode 32+16 (5.4765%). Both roles
and unchanged constructor/control paths retain failures; no unique attribution
to environment, frequency, allocator or new runtime code is established.

Across the two GF16 encoding and two warm-recovery cases, the unqualified
observed latency reductions span approximately 67%–74%. These are not accepted
speedups or published-9.0.0 deltas. Four general tower controls remain within
1.14% observed regression across the pairings, but the whole comparison still
fails. No unchanged rerun is used to find acceptance.

A post-capture native sample of the frozen candidate's 10+4 encoding path now
has GF8 NEON kernels at the top of the stack. Plane conversion and memset are
absent from the displayed top entries (whose reporting cutoff is five samples).
Together with the algebra and source return before plane construction, this
confirms the intended path change; sample counts are not latency percentages.

The [complete numerical artifact](../benchmarks/release-9.1.0/2026-10-23/gf16-subfield-component-diagnostics.json)
retains all 27 cases, both calibrations, all 1,296 warmup/measured wall and CPU
observations, exact common counts, witnesses, widths, repeats and four-pair
ratios. Each phase's arrays retain all 12 values in original order; the first
three are warmups. The complete diagnostic table follows, in ns/operation.
Every ratio is unqualified.

| Case | A1 | B1 | B2 | A2 | Unqualified ratio range |
| --- | ---: | ---: | ---: | ---: | ---: |
| `gf8:construct:4:2:0:0` | 1496.99 | 1465.49 | 1503.30 | 1503.42 | 0.9748–1.0042 |
| `gf8:construct:10:4:0:0` | 2858.99 | 2797.73 | 2879.52 | 2854.44 | 0.9786–1.0088 |
| `gf8:construct:32:16:0:0` | 15369.23 | 15181.77 | 15259.66 | 15391.16 | 0.9864–0.9929 |
| `gf8:cold:10:4:1024:1` | 1002.58 | 943.57 | 964.74 | 1010.45 | 0.9338–0.9623 |
| `gf8:cold:10:4:1024:4` | 1676.98 | 1715.29 | 1640.22 | 1673.20 | 0.9781–1.0252 |
| `gf8:warm:10:4:1024:1` | 244.31 | 244.83 | 244.73 | 248.18 | 0.9861–1.0021 |
| `gf8:encode:10:4:1024:0` | 597.02 | 601.86 | 597.80 | 600.74 | 0.9951–1.0081 |
| `gf8:cold:32:16:4096:1` | 7101.73 | 7072.24 | 7080.59 | 7150.09 | 0.9891–0.9970 |
| `gf8:cold:32:16:4096:16` | 45919.00 | 45724.96 | 45709.43 | 45912.36 | 0.9954–0.9959 |
| `gf8:warm:32:16:4096:1` | 2588.49 | 2585.53 | 2590.43 | 2587.41 | 0.9989–1.0012 |
| `gf8:encode:32:16:4096:0` | 38997.54 | 39058.30 | 39037.06 | 39133.88 | 0.9975–1.0016 |
| `gf16:construct:4:2:0:0` | 1071.18 | 1050.33 | 1070.95 | 1105.70 | 0.9499–0.9998 |
| `gf16:construct:10:4:0:0` | 4526.21 | 4283.53 | 4765.48 | 4504.56 | 0.9464–1.0579 |
| `gf16:construct:32:16:0:0` | 98739.16 | 92985.07 | 89073.57 | 86225.87 | 0.9021–1.0784 |
| `gf16:cold:10:4:1024:1` | 2445.35 | 1370.79 | 1379.03 | 2420.53 | 0.5606–0.5697 |
| `gf16:cold:10:4:1024:4` | 7351.79 | 2949.04 | 2962.14 | 7438.35 | 0.3965–0.4029 |
| `gf16:warm:10:4:1024:1` | 1550.24 | 440.38 | 438.16 | 1541.08 | 0.2826–0.2858 |
| `gf16:encode:10:4:1024:0` | 5978.77 | 1613.58 | 1574.77 | 5952.77 | 0.2634–0.2711 |
| `gf16:cold:32:16:4096:1` | 23160.57 | 11605.09 | 11264.66 | 23131.12 | 0.4864–0.5017 |
| `gf16:cold:32:16:4096:16` | 296453.00 | 104360.70 | 104011.27 | 294144.12 | 0.3509–0.3548 |
| `gf16:warm:32:16:4096:1` | 17191.04 | 5612.43 | 5045.64 | 16956.00 | 0.2935–0.3310 |
| `gf16:encode:32:16:4096:0` | 273241.27 | 84062.74 | 79459.09 | 270201.17 | 0.2908–0.3111 |
| `gf8:construct:128:64:0:0` | 301725.54 | 301800.98 | 300926.51 | 300057.05 | 0.9974–1.0058 |
| `gf16:multiply_tower:0:0:1024:0` | 155.63 | 155.69 | 155.01 | 154.23 | 0.9960–1.0094 |
| `gf16:accumulate_tower:0:0:1024:0` | 189.51 | 189.26 | 188.14 | 187.13 | 0.9928–1.0114 |
| `gf16:multiply_tower:0:0:1025:0` | 175.08 | 175.10 | 174.46 | 173.45 | 0.9964–1.0095 |
| `gf16:accumulate_tower:0:0:1025:0` | 210.34 | 210.88 | 210.01 | 210.24 | 0.9984–1.0031 |

## Integration and release boundary

During this capture, the maintainer rebased and merged PR #73 as
`8d59e774dd4b3f75eebbd48afdcc2e05d6614d07`. Its complete tree equals checked
`bce7023a38f859d859772e7684b3ad0def81acc6`; its ordinary checks completed
31 success / three conditional skips. Its earlier performance refusals remain
refusals. This assistant did not execute that merge.

The local GF16 comparison retains its frozen parent `ebb60f2`: both roles use
the matrix implementation preceding PR #73. The changed GF16 arithmetic source
is bound separately in the numerical artifact. Later integration or CI on newer
main must not relabel these observations as its complete timing proof. PR #74's
independent scalar length-contract repair retains its own source and checks.

Keep this pure performance implementation draft pending controlled qualification.
Ordinary CI and the published-9.0.0 functional comparison do not satisfy final
performance acceptance. The existing Linux full diagnostic keeps its frozen
sources and exclusive ownership; no compile or new workload was added there.
#2897 remains open with only condition 4 checked in its declared normal-format
scope. Final-source complete ABBA/layout evidence, accepted immutable evidence
and actual strict positive release preflight remain necessary before a 9.1.0
tag/publication. No performance threshold or release condition is weakened.
