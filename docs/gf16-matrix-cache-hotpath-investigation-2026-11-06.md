# GF16 matrix-cache hot-path investigation

The GF16 GFNI butterfly obtains four affine matrices from a per-thread,
64-slot cache. The [cold-path prototype](../benchmarks/gf16-cache-hot-path/2026-11-06/split-nohint.patch)
keeps canonical-table validation, cache indexing and value-before-key
publication unchanged, but moves a cache miss into a cold, non-inlined helper.
The hit path remains in the original lookup function. A separate
[forced-inline version](../benchmarks/gf16-cache-hot-path/2026-11-06/split-forced-inline.patch)
also forces the lookup to inline. Neither changes public APIs, field arithmetic
or encoded bytes.

**Neither prototype is included in production.** The forced-inline version
improved some GF16 screens but showed a repeated GF8 encoding slowdown. The
no-hint version generated different code and avoided that slowdown in an earlier
source copy, but a rebuilt current-source GFNI comparison showed a repeatable
slowdown in a GF16 encoding case. These measurements do not establish a
universal regression across machines or compilers; they are insufficient to
accept the change as an overall performance improvement on the tested setup.

## Why this was investigated

The [GF8 derivative follow-up](gf8-derivative-grouping-investigation-2026-11-06.md)
showed that a GF8-only candidate caused the GF16 GFNI butterfly to call its
matrix-cache lookup rather than inline it. In that screen GF16 10+4/64-byte
recovery changed +17.78%, versus −0.55% in the same-binary control. The
cold-path prototype was tested independently rather than adding a compensating
hint to the GF8 change. In the x86 release recovery binary its forward/inverse
GFNI butterfly functions were 797/809 bytes, versus 827/839 bytes at baseline.
Function size alone is not a latency result.

The forced-inline prototype's initial GFNI encoding screen changed 16+8/128 B
by −8.59% versus +1.64% in its same-binary control. A 60-sample repeat changed
−7.87% versus +3.28%. However, GF8 10+4/64 B encoding changed +3.81% versus
−0.40% in the first screen, and +5.88% versus +1.44% in the independent repeat.
The [interleaved](../benchmarks/gf16-cache-hot-path/2026-11-06/split-interleaved.json),
[screen](../benchmarks/gf16-cache-hot-path/2026-11-06/split-screen.json) and
[repeat](../benchmarks/gf16-cache-hot-path/2026-11-06/split-repeat.json) results
retain all tested cases and controls. Ordinary and forced inline hints produced
identical checked code, read-only data and relocation-read-only data sections
for the GF8 encoding binary. In the GF16 encoding and recovery binaries, all
checked allocated sections matched except the build-id note. The complete files
were not identical.

## Current-source comparison

The initial screens used the same source path and manifest for A and B, but the
isolated server copy contained a pre-existing unrelated `gfni_tail_probe` bench
declaration. A fresh copy of commit `45410e4` was therefore created and 80
tracked runtime and build files were checked against the local source. The
benchmark builder added only the recovery and encoding probe declarations for
the duration of each build and restored the source afterwards. The clean copy
includes the separately committed GF8 safety-comment fix. The later high-fanout
test commit changes test and documentation files, not codec runtime code.

Each comparison ran A1/B1/B2/A2, with same-binary and candidate stages
interleaved. Candidate change is `(B1 + B2) / (A1 + A2) - 1` using Criterion
mean latency; the same-binary control uses the baseline for all four stages.
Negative percentages are faster. The screen used 30 samples, 0.5 seconds of
warmup and a requested 1.5 seconds of measurement per case. The independent
repeat used 60 samples, one second and five seconds respectively.
The probes initialize and assert the named public GF8 backend. GF16 selects its
GFNI butterfly only when both CPU support and a GFNI backend are present; the
`rust-avx2` configuration exercises the GF16 AVX2 fallback.

| Operation and layout | Backend | Screen candidate | Screen same | Repeat candidate | Repeat same |
| --- | --- | ---: | ---: | ---: | ---: |
| gf16_encode 10x4_64 | rust-gfni-avx2 | +3.64% | -1.22% | +1.25% | +4.11% |
| gf16_encode 16x8_128 | rust-gfni-avx2 | +5.12% | -1.13% | +6.20% | -1.22% |
| gf16_encode 10x4_64 | rust-avx2 | -2.78% | +2.05% | -3.18% | -0.30% |
| gf16_encode 16x8_128 | rust-avx2 | +5.56% | -0.24% | -1.08% | -0.79% |
| gf8_encode 10x4_64 | rust-gfni-avx2 | +1.56% | +1.06% | +4.44% | +0.84% |
| gf8_encode 16x8_128 | rust-gfni-avx2 | +3.63% | -0.14% | +2.48% | +3.77% |
| gf8_encode 10x4_64 | rust-avx2 | -0.74% | -0.90% | +2.62% | +2.73% |
| gf8_encode 16x8_128 | rust-avx2 | +3.47% | -0.66% | +2.40% | +3.50% |

The GFNI 16+8/128 B GF16 result changed from +5.12% in the screen to +6.20%
in the independent repeat; its same-binary controls were −1.13% and −1.22%.
The AVX2 result for that case changed from +5.56% to −1.08%, illustrating why
one observation must not be generalized to every backend. The GF8 GFNI
10+4/64 B repeat changed +4.44%, versus +0.84% in its same-binary control.
The [screen samples](../benchmarks/gf16-cache-hot-path/2026-11-06/clean-screen.json)
and [repeat samples](../benchmarks/gf16-cache-hot-path/2026-11-06/clean-repeat.json)
include all stage estimates, iteration/time samples and monitor summaries. The
earlier [no-hint screen](../benchmarks/gf16-cache-hot-path/2026-11-06/nohint-screen.json)
is retained for comparison; it was built from the server copy with the extra
bench declaration described above.
The [repeat runner](https://github.com/houseme/rustfs-erasure-codec/blob/dc4c62e9f37a5bf848d58ab689237a95a3abbb27/benchmarks/gf16-cache-hot-path/2026-11-06/clean-repeat-runner.py)
is the executed controller with the machine-specific root and CPU replaced by
`RSE_BENCH_ROOT` and `RSE_BENCH_CPU`. It expects the two baseline/candidate
probe pairs as `baseline` and `candidate` under
`clean-45410e4-20261106/binaries-gf16/encode` and
`clean-45410e4-20261106/binaries-gf16-gf8/encode`, and refuses to overwrite an
existing result directory. The prototype patches are zero-context and can be
checked with `git apply --check --unidiff-zero`.

The VM exposed AMD EPYC 9V45 guest CPUs. Runs used one worker pinned to one
guest CPU; physical-host isolation was not verified. The one-second monitor
reported no competing process over half a CPU in completed phases, but cannot
exclude shorter bursts or host interference. Baseline drift was retained and
did not stop the measurements. Software CPU-clock profiling in the related
GF8 investigation did not isolate one function as the source of the observed
cross-path slowdown; hardware cycle and cache-miss events were unavailable.
These limitations and the conflicting results prevent a stronger causal claim.

## Correctness checks and limits

The no-hint prototype passed three native x86 GFNI butterfly tests, including
all multipliers against basis vectors, representative multipliers against every
field value, and unaligned tails. Its native all-features library suite passed
468 tests with five ignored, including cache-collision, alternate-table and
concurrent thread-local tests. A separate high-fanout cross-version test passed
on both baseline and prototype under scalar, AVX2, GFNI/AVX2 and GFNI/AVX-512
overrides. It compares complete parity and mixed data/parity recovery with the
published 9.0.0 codec for 64+16 and 128+32 layouts at 64 and 128 bytes.
The test is now retained as
[`leopard_high_fanout.rs`](../scripts/release-compatibility/tests/leopard_high_fanout.rs).

On ARM64, the isolated prototype passed workspace all-features tests, 295
`no_std` library tests and strict workspace Clippy. The GFNI-only cache code is
not executed on ARM64. The current-source x86-64 cross-target strict Clippy
check also passed after the independent GF8 safety-comment fix. High-fanout
performance and a coverage-guided x86 fuzz run were not performed for the
rejected prototype; they would be required before considering a future variant.

The historical [screen controller](https://github.com/houseme/rustfs-erasure-codec/blob/dc4c62e9f37a5bf848d58ab689237a95a3abbb27/benchmarks/gf16-cache-hot-path/2026-11-06/screen-runner.py)
and repeat controller are preserved at that immutable Git revision; they are
not dependencies of the current source tree.
