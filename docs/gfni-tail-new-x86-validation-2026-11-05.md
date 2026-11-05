# GFNI tail validation on a new x86-64 host

This follow-up tested the retained [outlined single-byte tail prototype](../benchmarks/gfni-tail-followup/2026-11-04/outlined.patch)
against the current production runtime. The runtime source matches commit
`b61c1f1` for the codec files under test. The baseline and candidate source
archives were cross-compiled on ARM64 with Rust stable 1.99.1, locked
dependencies, `simd-accel`, the same target directory and an x86-64 Linux GNU
2.31 target. The candidate differs by the outlined tail patch. The
[probe source](../benchmarks/gfni-tail-followup/2026-11-04/tail-probe.rs) is
unchanged from the prior investigation.

The Linux VM exposed 32 guest CPUs, 62 GiB memory and GFNI, AVX2 and AVX-512
features. Baseline and candidate binaries were run on guest CPU 8 with one
worker. Existing services remained active. No guest CPU partition was created
and physical-host exclusivity was not established. A process monitor sampled
once per second and stopped the workload if another process exceeded half a
CPU during a sample; it reported no such competing process in completed stages.
This does not rule out interrupts, smaller competing tasks, host activity or
other noise.

## Functional checks

For each of `rust-gfni-avx2`, `rust-avx2` and `rust-gfni-avx512`, both baseline
and candidate passed all 108 probe smoke cases. Candidate tests passed the
single-byte tail reference test, GFNI row tests over coefficients, offsets and
tails, invalid-view rejection before writes, concrete workspace compatibility,
Leopard destination checks and GF8 decode reuse tests. The native row test
reported that both GFNI vector widths executed. For each selected backend, the
published 9.0.0 versus candidate 9.1.0 probe passed 200 configurations of exact
parity and cross-decoding in both directions, plus the probe's larger and
small-block controls. [Validation summary](../benchmarks/gfni-tail-followup/2026-11-05/validation.json)
records the case counts and monitoring observations. This was not a full native
workspace test or a fuzz campaign.

A follow-up ran the complete native library unit-test binary on eight guest
CPUs with one test thread and four Rayon workers. With `simd-accel`, each of
GFNI/AVX2, AVX2 and GFNI/AVX-512 passed 464 tests with five ignored. With all
features, each passed 469 tests with five ignored. An earlier single-CPU run
passed 462 and failed two tests that assert a parallel policy; both passed
with the eight-CPU constraint. The [unit-test summary](../benchmarks/gfni-tail-followup/2026-11-05/unit-suite.json)
and [all-feature summary](../benchmarks/gfni-tail-followup/2026-11-05/unit-allfeatures-summary.json)
record these results. Fourteen selected all-feature integration test binaries
covering encoding views and dispatch, AVX-512 interoperation, changed views,
error handling, compatibility and Leopard recovery also passed 61 tests on
each backend. The [integration summary](../benchmarks/gfni-tail-followup/2026-11-05/integration-allfeatures-summary.json)
records each binary. This was not a complete native workspace test or a native
fuzz campaign.

## Performance observations

The initial screen compared A1/B1/B2/A2 with 20 Criterion samples, 0.5 s warmup
and a requested 2 s measurement for each of eight cases on GFNI/AVX2 and AVX2.
An independent GFNI/AVX2 follow-up ran the same cases with 40 samples, 1 s
warmup and a requested 3 s measurement. It first ran an A/B/B/A comparison
using the same baseline executable in all four phases, then compared baseline
and candidate. Negative changes mean lower mean latency; baseline drift is
A2/A1−1. No drift gate stopped the runs.

| GFNI/AVX2 case | Same-binary comparison | Candidate comparison | Candidate baseline drift |
| --- | ---: | ---: | ---: |
| 4+2, 257 B, valid verification | −0.53% | +2.97% | −0.26% |
| 10+4, 257 B, valid verification | +2.55% | +3.86% | −3.18% |
| 10+4, 257 B, damaged verification | −1.43% | +2.72% | −2.10% |
| 4+2, 64 B, valid verification | −0.21% | −0.34% | −2.67% |

The earlier GFNI/AVX2 screen measured +3.67%, +0.82% and +1.47% for the first
three cases respectively. AVX2 and other controls are in the [screen](../benchmarks/gfni-tail-followup/2026-11-05/screen.json)
and [follow-up](../benchmarks/gfni-tail-followup/2026-11-05/repeat.json) stage
estimates. Some controls that cannot call the changed GFNI tail also moved.
Together with the same-binary variation and unverified host isolation, these
results do not establish a reliable performance gain. The prototype remains
unintegrated; these observations apply only to this VM and workload.

## GFNI/AVX-512 follow-up

The first GFNI/AVX-512 performance attempt stopped during the same-binary A1
phase when another CPU-intensive process appeared; it produced no candidate
measurement. After that work exited, a fresh same-binary and candidate run
completed with the same 40-sample settings and guest CPU binding. The process
monitor reported no competing process over its threshold in the completed
stages, without establishing physical-host isolation.

| GFNI/AVX-512 case | Same-binary comparison | Candidate comparison | Candidate baseline drift |
| --- | ---: | ---: | ---: |
| 4+2, 257 B, valid verification | −2.39% | −4.48% | −0.28% |
| 10+4, 257 B, valid verification | −1.64% | −4.88% | +5.19% |
| 10+4, 257 B, damaged verification | −3.39% | −2.88% | +1.71% |
| 4+2, 257 B, encoding | +5.46% | −2.75% | +2.40% |

The same-binary encoding comparison itself changed +5.46%, and other controls
showed variation. These observations do not establish the candidate's causal
performance effect. All stage estimates are retained in the [AVX-512 results](../benchmarks/gfni-tail-followup/2026-11-05/repeat512.json).
