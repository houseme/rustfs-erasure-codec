# Guarded GFNI tail baseline check

The outlined single-byte tail candidate remains an isolated experiment. This
follow-up used baseline commit `35d17b0` and the previously retained
[outlined patch](../benchmarks/gfni-tail-followup/2026-11-04/outlined.patch).
Both Rust benchmark executables were cross-compiled for x86 Linux from the same
source and target directories using stable Rust and locked dependencies. A
[diagnostic benchmark extension](../benchmarks/gfni-tail-followup/2026-11-04/diagnostic-corrupt-bench.patch)
added timed damaged-parity cases to the small-Vec catalog. The permanent
benchmark has a separate opt-in for those cases; its code differs from the
one-off diagnostic extension, so their timings must not be treated as identical.
The permanent benchmark was separately cross-compiled and smoke-tested on native
x86 with GFNI/AVX-512, AVX2 and scalar backends. Each ran 24 cases, including
eight damaged-parity cases, with 1 ms samples; these runs establish only that
the benchmark and its result checks execute on those backends.

On native x86 the candidate's direct single-byte test passed all 256
coefficients and 256 input values with guards. Baseline and candidate each
passed all 24 GFNI/AVX-512 functional smoke cases, including eight timed
corrupt-parity cases. This confirms only the tested behavior, not speed.

The guarded run required two same-source A1/B1/B2/A2 baseline rounds for each
case before candidate timing. Each phase used three 500 ms samples on one
worker, with 5% phase drift, same-source bias and cross-round drift limits.
For the first control, Classic 4+2 at 64 bytes, each round passed its internal
phase and same-source checks. The B phase mean nevertheless fell from 52.61 ns
to 48.86 ns between rounds, or -7.13%, exceeding the cross-round limit. The
run stopped before any candidate timing or later cases. Guest resource
monitoring observed no competing workload, steal, throttle or sustained
off-target load during the recorded windows; the cause of the drift is
unknown, and physical host isolation was not independently established.

The [eight phase records](../benchmarks/gfni-tail-followup/2026-11-04/outlined-baseline-incomplete.json),
[two gate summaries](../benchmarks/gfni-tail-followup/2026-11-04/outlined-baseline-gates.json),
[resource observations](../benchmarks/gfni-tail-followup/2026-11-04/outlined-baseline-monitor.json)
and [native unit test output](../benchmarks/gfni-tail-followup/2026-11-04/outlined-native-test.txt)
are retained. The experiment cannot attribute any candidate speedup or
regression; the outlined tail remains unintegrated. See the independent
[earlier investigation](small-verify-gfni-tail-followup-2026-11-04.md) for
its different measurement protocol and compatibility boundaries.
