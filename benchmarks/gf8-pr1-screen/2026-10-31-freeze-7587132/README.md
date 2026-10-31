# Frozen-current retained-row baseline recheck

Historical Python experiment scripts cited below are archived in commit
`70898c1` and are not part of the current source tree. The recorded data and
Rust probes remain available here.

This snapshot is commit `7587132dec4f9bf6a8f55e6855144c6a2e7f8480` plus
`worktree.patch` (SHA-256
`3673518d13d7b70d977df1715e85e508144cf496f0e9d36f168b0ec76545aeaa`).
It includes the current worktree AVX-512-only encoder, the compatibility-tool
changes, and the scheduler regression. The benchmark bridge is temporary and
not part of the product API. The `same` and candidate crate copies use a
temporary package-version change for linking; 21 external package
name/version/source/checksum tuples matched between their lockfiles. Probe
manifests and lockfiles, source diffs, bridge and scripts are saved here.

On native x86 AVX2, the temporary 256-byte parallel-row candidate passed the
real-recovery-row bridge, threshold dispatch, custom-matrix recovery, direct
scalar-row comparison, and one/four-job instrumented dispatch tests. These are
functional checks; no candidate timing ran after the baseline failure below.

The strict streaming runner used the user-confirmed reserved Azure VM, stable
rustc 1.99.1, five guest vCPUs, four Rayon workers and four codec jobs. It
requires initially quiet monitoring, checks target steal and cgroup throttling,
and rejects sustained off-target CPU load. Probes acknowledge each case only
after the gate passes; all output bytes are checked after every phase. See
[`../2026-10-31-kernel-isolation/README.md`](../2026-10-31-kernel-isolation/README.md)
for the monitoring and handshake limitations.

`strict-results/` is the initial same-source dual-copy baseline preflight. Its
first 10+4/1 MiB/one-missing case had A1/A2 drift +10.98% (B1/B2 -0.01%), so
the runner stopped without acknowledging the case. Only one case was recorded;
no later baseline case or candidate was started. During 40 monitor samples the
cpuset remained 0-15, no named competing benchmark appeared and the maximum
off-target-load streak was zero. These observations do not identify the cause
or rule out short interference, cache effects or unobservable host frequency.

`sched-diagnostic/` is a *different, baseline-only instrumented executable*
with per-thread `schedstat` snapshots outside the timed loops. Both of its
rounds passed the then-current within-round phase and same-source gates, and the
original `summary.json` is preserved. A later post-hoc cross-round audit found
the 4+2 control's A-phase mean increased from 40375.42 to 44027.43 ns (+9.05%),
and the B-phase mean increased by 9.32%.
Its original pass is therefore superseded and the two-round preflight is
rejected under the new cross-round gate. No candidate was run. The audit is
[`../2026-10-31-kernel-isolation/cross-round-audit.json`](../2026-10-31-kernel-isolation/cross-round-audit.json).

The current streaming runner now rejects shifts over 5% between repeated A
levels and between repeated B levels in both baseline and candidate phases, in
addition to its within-round gates. It stops before acknowledging the next case
when a gate fails. This rule was added after the two failed records above and is
not retroactively represented by their original runner summaries.

Neither the retained-row candidate nor its 256-byte variant is activated in
the product. A new acceptance run requires an exact current source snapshot,
quiet reserved resources, a baseline that passes every within- and cross-round
gate, and separate end-to-end recovery validation. Passing the isolated-row
probe alone would not prove an end-to-end gain.

## Parallel candidate cross-version functional check

A separate temporary AVX2-only snapshot applied the parallel-row and 256-byte
tile candidates, then added `parallel-cross-version-instrumentation.patch`.
This temporary patch selects `simd-avx2` for the published 9.0.0 and worktree
9.1.0 probe dependencies, asserts both active backends are `rust-avx2`, adds a
counter at the new parallel chunk call, and includes
`parallel_cross_probe_fragment.rs`. It is not applied to the product worktree.

With four available guest vCPUs, four Rayon workers and four codec jobs, eight
Classic GF8 cases passed exact parity comparison and bidirectional cross-version
recovery: 10+4, one missing data shard, shard lengths 1048576/1048577 and
Vandermonde, Cauchy, Jerasure-like and a valid custom matrix. The test observed
192 new parallel-chunk executions after resetting the counter. The existing
200 standard, 36 large-GF8 and 12 six-plus-two compatibility cases also passed.
The recorded configuration and patch checksums are in
`parallel-cross-version-result.json`. This test establishes only the listed
functional compatibility; the counter changes the temporary binary and the run
was not a performance measurement. It does not alter the failed performance
gates or justify enabling the candidate.
