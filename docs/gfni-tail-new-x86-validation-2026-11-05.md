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
records the case counts and monitoring observations. These initial checks did
not include the full native workspace or coverage-guided fuzzing; both were
subsequently exercised as described below.

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
records each binary. A further [35 integration binaries](../benchmarks/gfni-tail-followup/2026-11-05/integration-remaining-summary.json)
ran per backend: 176 tests passed and 38 were ignored; six binaries executed no
tests. Across both groups, each backend ran 49 binaries with 237 tests passed
and 38 ignored. This covers the codec package's built integration tests, not
other workspace packages, doctests or the ignored workloads.

An isolated-container `cargo test --locked --workspace --all-features` run
also passed on native x86-64 with the GFNI/AVX2 backend, including 13 doctests;
ignored tests were not executed. Its [summary](../benchmarks/gfni-tail-followup/2026-11-05/native-workspace-summary.json)
records the resource limits and source scope. The run used the candidate codec
and the documented temporary contract-harness length extension.

A deterministic [Classic GF8 contract probe](../benchmarks/gfni-tail-followup/2026-11-05/native-classic-contract.rs)
passed 20,000 generated inputs on each backend. Its temporary
[length extension](../benchmarks/gfni-tail-followup/2026-11-05/classic-contract-lengths.patch)
adds 256/257/258 and 1023/1024/1025 byte boundaries to the existing contract
harness. The [summary](../benchmarks/gfni-tail-followup/2026-11-05/native-classic-contract-summary.json)
records the exact lengths and counts. This deterministic check is separate from
the native coverage-guided fuzz campaign below.

## Native no_std feature checks

In an isolated native x86-64 container, library tests passed with
`--no-default-features` in three configurations: 294 without extra features,
299 with `simd-gfni`, and 302 with `simd-accel`; none were ignored or failed.
The single-byte tail reference test executed in both SIMD-feature builds.
The no_std runtime backend selects the scalar fallback, so these checks do not
establish runtime GFNI dispatch or GFNI performance. The [summary](../benchmarks/gfni-tail-followup/2026-11-05/nostd-summary.json)
records the commands and scope.

A separate [native cross-version probe](../benchmarks/gfni-tail-followup/2026-11-05/native-nostd-cross.rs)
linked published 9.0.0 and the candidate 9.1.0 with default features disabled
for both. Three builds ran with no extra features, `simd-gfni`, and `simd-accel`;
each passed 84 cases: 48 Classic and 18 each for Leopard GF8 and GF16, across
4+2, 10+4 and 16+8 shapes. The cases cover lengths from one byte to 1 MiB,
including 256/257/258 and 1023/1024/1025 byte Classic tails. Both versions
asserted the `scalar-rust` runtime backend, compared complete encoded shards,
verified each other's parity, recovered a missing data and parity shard in
both directions and rejected parity-tail corruption. Feature-tree checks
confirmed neither codec enabled `std`. The standalone probe binary itself uses
`std` as a test driver; the two codec dependencies do not. The [summary](../benchmarks/gfni-tail-followup/2026-11-05/native-nostd-cross-summary.json),
[manifest template](../benchmarks/gfni-tail-followup/2026-11-05/native-nostd-cross.Cargo.toml.example)
and [lockfile](../benchmarks/gfni-tail-followup/2026-11-05/native-nostd-cross.Cargo.lock)
retain the reproduction inputs. This is functional compatibility evidence, not
a no_std GFNI runtime dispatch or performance result.

## Large cross-version boundaries

A native [cross-version probe](../benchmarks/gfni-tail-followup/2026-11-05/native-large-cross.rs)
compared the published 9.0.0 codec and the candidate on all three x86 backends.
For each backend, 30 cases passed: Classic 4+2, 10+4 and 16+8 at 65,535,
65,536, 65,537, 1,048,575, 1,048,576 and 1,048,577 bytes; and Leopard GF8
and GF16 at the aligned 65,536 and 1,048,576 byte sizes for the same shapes.
Each case compared complete encoded shards, verified peer parity in both
versions, recovered a missing data and parity shard in both directions, and
checked detection of corruption at the parity tail. The [summary](../benchmarks/gfni-tail-followup/2026-11-05/large-cross-summary.json)
records case counts by backend. To reproduce it, place the probe in the
`examples` directory of `scripts/release-compatibility` in an isolated
candidate checkout and run it with the desired backend override and matching
`RSE_EXPECT_BACKEND`.

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

## Native-compiled benchmark follow-up

A later follow-up built the baseline and outlined candidate on the x86-64
server itself using stable Rust, locked dependencies and `simd-accel`. Both
were built sequentially from the same isolated source path and target
directory. Cargo reported the same output path for both builds, and the saved
candidate matched the reported artifact. The temporary codec source was
restored after the builds. Under
GFNI/AVX2, AVX2 and GFNI/AVX-512, each binary passed all 108 probe smoke cases.
The [build and smoke summary](../benchmarks/gfni-tail-followup/2026-11-05/native-build-summary.json)
records those outcomes.

A planned same-binary performance control stopped during its first baseline
phase after monitoring detected a competing CPU-intensive process. The test
child was terminated; no candidate performance stage ran. No latency conclusion
is drawn from that incomplete run. The VM continued to run existing services,
and physical-host exclusivity was not established.

## Native coverage-guided fuzz

The existing `fuzz_codec_contracts` target ran in an isolated x86-64 container
with `cargo-fuzz` 0.13.2, Rust nightly 1.101.0, address sanitizer and
`simd-accel`. Nightly was used only for fuzzing. The candidate's temporary
contract harness included the documented extended Classic lengths. Each backend
started with 24 separate seeds covering Classic GF8 with 4 or 8 data shards,
2 or 4 parity shards and the six extended length indexes. Each 80-byte seed
starts with byte `i` set to `i`; offsets 0, 1, 2, 7 and 8 are set to 0, 0, 0,
0 and 1, while offsets 3, 4 and 5 contain data shards minus one, parity shards
minus one, and a length index in 6 through 11. The harness may also exercise
other codec families as inputs mutate.

The initial 45-second budget per backend completed 366,631 GFNI/AVX2, 356,034
AVX2 and 386,150 GFNI/AVX-512 inputs. A separate 20-second budget per backend
completed 159,278, 158,727 and 157,443 inputs respectively. In the second
run, a temporary [test wrapper](../benchmarks/gfni-tail-followup/2026-11-05/fuzz-backend-check.rs)
asserted the selected backend once per process; this eagerly initializes GF8
dispatch and is a distinct test setup. All six runs completed without a
reported sanitizer failure. The temporary wrapper was restored afterwards.
The [fuzz summary](../benchmarks/gfni-tail-followup/2026-11-05/native-fuzz-summary.json)
retains counts and scope; generated corpora remain in the isolated validation
directory. Finite fuzz runs do not establish correctness for all inputs.

The original `fuzz_encode_verify` and `fuzz_encode_reconstruct` targets were
then run with the same nightly, address sanitizer and three backend overrides,
for a 20-second budget each. Each target started with six valid seeds: 4+2 or
8+4 shards at 129, 193 or 225 bytes, a single iteration, and payload byte `i`
equal to `(37 * i + 11) mod 256`. Reconstruction seeds request one missing
shard with unit stride. Input length was capped at 4,096 bytes. All six runs
completed without a reported sanitizer failure, executing 404,401 inputs in
total. These two targets used backend overrides but did not assert the selected
backend inside the target; the earlier checked contract target did. Their
[individual counts](../benchmarks/gfni-tail-followup/2026-11-05/native-extra-fuzz-summary.json)
and the retained isolated corpora document the bounded test.
