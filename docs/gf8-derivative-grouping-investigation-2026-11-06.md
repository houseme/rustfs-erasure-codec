# Leopard GF8 derivative grouping investigation

The Leopard GF8 formal derivative currently XORs each pair of work lanes
separately. The candidate borrows two disjoint groups of complete, contiguous
lanes and XORs each group in one call. Partial lanes retain the original loop.
The final prototype limits the grouped path to `std` on x86-64; it does not
change a public API or the coded byte representation.

The candidate is **not included in the production codec**. It improves some
measured small-shard recoveries, but the `simd-accel` GF16 encoding controls
showed changes in both directions across repetitions. Two final-candidate AVX2
runs showed a 4.50% and 6.67% latency increase for 16+8/128-byte encoding;
their same-binary controls changed −2.02% and +1.35%, respectively. Two earlier
measurements of the inline prototype changed −0.71% and −2.54% in the same
case. Those are conflicting observations, not proof that the candidate always
causes a regression. They do not establish an acceptable cross-operation result
for this compiler and machine, so adding an unrelated GF16 hint merely to ship
this GF8 optimization is not justified.

## Source-level investigation

A software CPU-clock profile from the preceding investigation attributed about
20%–21% of GF8 recovery samples to `slice_xor_avx2`. Other callers also use
that function, so the profile does not assign that entire share to the formal
derivative. Grouping can reduce calls where a complete work lane is contiguous;
the test compares the full work buffer against an ordered per-lane oracle for
full and partial lanes, boundary lengths, and multiple lane and active counts.

The [initial prototype](../benchmarks/gf8-derivative-grouping/2026-11-06/initial.patch)
measured GF16 10+4/64-byte four-missing recovery at +16.92% with the GFNI
backend, versus +0.36% in its same-binary control. Outlining the derivative in
the [second prototype](../benchmarks/gf8-derivative-grouping/2026-11-06/outlined.patch)
still measured +14.68%, versus −2.36% in its control. The GF16 GFNI butterfly
functions shrank from 827/839 bytes to 635/647 bytes, consistent with an
inlining change. Sampling and function size do not prove how much of the
latency difference was caused by that change.

The [inline prototype](../benchmarks/gf8-derivative-grouping/2026-11-06/inline-prototype.patch)
adds a regular inline hint to the GF16 matrix-cache lookup. Its initial GFNI
recovery screen changed −4.42% for that same GF16 case, with a +2.91%
same-binary control. The [final prototype](../benchmarks/gf8-derivative-grouping/2026-11-06/final-prototype.patch)
also restricts the GF8 grouped path by architecture and updates diagnostic
source fingerprints in the isolated test copy. For the GF16 encoding probes,
the inline and final prototype binaries had identical `.text` and `.rodata`
sections, while their full files and `.data.rel.ro` differed. This supports
comparing their results, but does not establish that every execution input or
layout effect was identical.

## Measurement method

The baseline is commit `ba3893b`. Baseline and candidate used the same source
path, locked dependencies, Rust stable 1.99.1, release build settings and
probe sources. The retained [recovery probe](../benchmarks/gf8-decode-assign/2026-11-02/recovery-probe.rs),
[GF8 encoding probe](../benchmarks/gf8-decode-assign/2026-11-02/encode-probe.rs)
and [GF16 encoding probe](../benchmarks/gf8-butterfly-fusion/2026-11-02/encode-gf16-probe.rs)
check their fixtures before timing.

The native tests ran with one worker on one pinned guest CPU of a Linux
AMD EPYC 9V45 virtual machine. A one-second guest-process monitor found no
competing process using over half a CPU during the retained runs; it cannot
exclude shorter bursts, other guest-level effects, or host interference. No
physical host isolation was verified. Baseline drift was recorded and did not
stop measurements at the user's request.

Each comparison runs A1/B1/B2/A2. Candidate change is
`(B1 + B2) / (A1 + A2) - 1`, using each stage's Criterion mean latency.
The same-binary control runs the baseline in all four stages. Negative values
are faster. Final recovery uses 40 samples, a one-second warmup and requested
three-second measurement; GF8 encoding screens use 20 samples, 0.5 seconds and
two seconds. GF16 encoding repeats use 60 samples, one second and five seconds.

The probe initializes and asserts the named public GF8 backend. Leopard GF8 XOR
selects AVX2 independently based on CPU support. Leopard GF16's GFNI butterfly
requires both CPU support and a previously selected GFNI backend; with
`rust-avx2`, the probe exercises its AVX2 fallback. The default-feature build
omits `simd-gfni` entirely and was checked with the `scalar-rust` public GF8
backend; its GF16 path can still use its independently detected AVX2 kernel.
The backend labels do not imply that every Leopard kernel changes with the
public backend. Native performance on machines without AVX2 was not measured.

## Final candidate recovery results

| Backend | Case | Candidate | Same binary | Candidate baseline drift |
|---|---|---:|---:|---:|
| rust-gfni-avx2 | gf8_10x4_64_missing4 | -3.56% | +4.57% | -2.03% |
| rust-gfni-avx2 | gf8_10x4_65600_missing4 | -3.32% | +2.16% | -4.36% |
| rust-gfni-avx2 | gf8_10x4_1048576_missing4 | +1.13% | +1.42% | +2.52% |
| rust-gfni-avx2 | gf8_16x8_64_missing4 | -1.07% | +7.67% | +0.63% |
| rust-gfni-avx2 | gf8_16x8_65600_missing4 | -0.65% | +0.97% | -0.14% |
| rust-gfni-avx2 | gf8_16x8_1048576_missing4 | +2.59% | -1.63% | -4.43% |
| rust-gfni-avx2 | gf16_10x4_64_missing4 | +2.61% | -0.67% | -1.78% |
| rust-avx2 | gf8_10x4_64_missing4 | -1.23% | +2.02% | +0.57% |
| rust-avx2 | gf8_10x4_65600_missing4 | -0.33% | +1.69% | +1.34% |
| rust-avx2 | gf8_10x4_1048576_missing4 | -1.40% | -2.69% | +2.06% |
| rust-avx2 | gf8_16x8_64_missing4 | -11.00% | +3.93% | +1.34% |
| rust-avx2 | gf8_16x8_65600_missing4 | -2.52% | -1.59% | -5.24% |
| rust-avx2 | gf8_16x8_1048576_missing4 | +1.52% | -1.73% | -1.11% |
| rust-avx2 | gf16_10x4_64_missing4 | +0.83% | -1.19% | +2.35% |
| rust-gfni-avx512 | gf8_10x4_64_missing4 | -2.11% | -3.46% | -8.50% |
| rust-gfni-avx512 | gf8_10x4_65600_missing4 | +1.96% | +1.06% | +0.61% |
| rust-gfni-avx512 | gf8_10x4_1048576_missing4 | +0.51% | +1.61% | -0.37% |
| rust-gfni-avx512 | gf8_16x8_64_missing4 | -5.70% | -4.38% | -6.65% |
| rust-gfni-avx512 | gf8_16x8_65600_missing4 | -1.49% | +6.77% | +0.27% |
| rust-gfni-avx512 | gf8_16x8_1048576_missing4 | -1.86% | -0.86% | +2.12% |
| rust-gfni-avx512 | gf16_10x4_64_missing4 | +1.84% | +3.20% | -2.29% |

For the default-feature build, GF8 16+8/64-byte four-missing recovery changed
−8.07%, versus +1.15% in the same-binary control; the candidate baseline drift
was +0.70%. GF8 10+4/64-byte recovery changed −6.86%, versus −1.18% in its
control, with −5.88% baseline drift. Larger default-feature recovery cases are
included in the raw results.

For GF16 16+8/128-byte encoding with the GFNI backend, the 40-sample final
screen changed +3.48%, versus −4.72% in the same-binary control; the independent
60-sample repeat changed −0.44%, versus −0.15% in its control. Under default
features the 60-sample comparison changed +1.03%, versus −0.46% in its control.
For the two AVX2 increases cited above, candidate baseline drift was +0.16%
and +4.14%, respectively. These measurements are observations on one VM, not
a guarantee about another system or compiler.

Full final-stage estimates, raw Criterion iteration/time samples, sample counts
and monitor summaries are in [recovery](../benchmarks/gf8-derivative-grouping/2026-11-06/final-recovery.json),
[encoding](../benchmarks/gf8-derivative-grouping/2026-11-06/final-encoding.json),
[GF16 repeat](../benchmarks/gf8-derivative-grouping/2026-11-06/final-gf16-repeat.json)
and [default features](../benchmarks/gf8-derivative-grouping/2026-11-06/final-default-features.json).
The same directory retains the earlier prototype patches and stage estimates.
The [recovery runner](../benchmarks/gf8-derivative-grouping/2026-11-06/recovery-runner.py)
is the executed controller with its machine-specific root and CPU replaced by
`RSE_BENCH_ROOT` and `RSE_BENCH_CPU`. It expects the baseline and candidate
recovery binaries under `derivative-binaries-v4/recovery/` in that root and a
fresh output directory. `RSE_BENCH_BACKEND` selects the asserted backend. The
patches and linked probe sources define the candidate and timed workload; apply
the zero-context patches with `git apply --unidiff-zero`, and build both binaries
at the same source path with the same lockfile and options before using the runner.

## Compatibility and validation

The isolated final prototype passed the x86-64 native all-features library
suite: 469 passed, five ignored. Its direct derivative equivalence test also
passed in the optimized x86 builds for `simd-accel` and default features. The
published 9.0.0 compatibility probe passed in both directions for 200
configurations, including exact parity, under each of the `scalar`, `rust-avx2`,
`rust-gfni-avx2` and `rust-gfni-avx512` override settings. The probe also passed
its large-boundary, 6+2 and corruption-detection cases.

On ARM64 the isolated prototype passed workspace all-features tests, strict
all-target all-features Clippy and 296 `no_std` library tests. All three
existing fuzz targets completed 31-second smoke runs: 713,588 contract,
136,119 reconstruct and 62,454 verify inputs. These ARM checks cover fallback
and format behavior; they do not execute the x86 grouped kernel. The `xtask`
diagnostic source fingerprints were refreshed only in the prototype after
reviewing the source transformation; no production source or snapshot was
changed by this investigation.
