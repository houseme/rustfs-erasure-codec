# Retained-output recovery-row diagnostic

This is a temporary, non-production diagnostic for the previously observed
parallel recovery drift. The baseline is commit
`88e52de48e64d0f3299c4b472fbc69e207e59ab9` plus `baseline-source.patch`
(SHA-256 `4bf646031b7a4c22f943f5dc33861d14e80fc500c5961207d8b6749017c9f9c9`).
It includes the already-adopted AVX-512-only encoder and the cross-version
compatibility probe. No production API or dispatch was changed for this work.

`bridge-setup.patch` adds a temporary `bench-internal-row` feature and exposes
`diagnostic_row.rs` only in a disposable copy. The bridge obtains the actual
single-missing-data decoding matrix row and survivor order from the Classic
reconstruction plan, then invokes the existing parallel coding-row routine on
an already allocated output. Planning and output allocation are outside the
measured operation. It is not an end-to-end reconstruction benchmark.

`diagnostic_bridge_test.rs` checked three built-in matrices, three missing data
positions, 65539 and 1048577-byte shards, and one/four requested jobs. The
retained output matched the original encoded data. The baseline and candidate
bridges passed this test on native AVX2; both temporary snapshots passed the
fallback test on Apple Silicon, and the candidate also cross-compiled for x86-64
Linux. This does not cover every matrix, shard size or CPU.

The tested candidate is the existing instrumented
[`parallel-row-candidate.patch`](../2026-10-31-parallel-followup/parallel-row-candidate.patch)
plus [`row-256-byte-experiment.patch`](../2026-10-31-same-process/row-256-byte-experiment.patch).
A separate temporary hit-counter patch (`hit-counter-instrumentation.patch`)
confirmed on native x86 that the bridge selected the candidate for 10+4/1 MiB
with four jobs and did not select it for the 4+2 control. This test-only counter
was not present in timed binaries or applied to the production worktree.

## Baseline preflights and failed candidate run

The user-confirmed reserved Azure VM exposes AMD EPYC 9V45 AVX2. Tests used
stable rustc 1.99.1, five guest vCPUs (8-12), four Rayon workers and four codec
jobs. `baseline_row_probe.rs` uses one codec implementation;
`paired_row_probe.rs` links two copies, with a temporary version change for the
second copy. Both prepare the real recovery row and survivor order before
measurement, use one preallocated output, warm each phase for one second, and
record three 300 ms samples. Every completed phase checks the full output
against the original shard. Chunk length comes from the existing reconstruction
stage policy; it was 95296 B for 10+4 and 209664 B for 4+2 on this host.

`results/` contains two single-implementation baseline rounds, and
`paired-aa-results/` and `paired-aa-post-build-results/` each contain two
same-source dual-copy rounds. All passed the original within-round phase drift
and same-code bias checks. A later cross-round audit found that
`paired-aa-results/` shifted -7.09% for A and -8.89% for B between rounds for
the 10+4 tail case; its original pass is therefore superseded and it is
rejected. The other two sets passed this added check. None proves that the
original end-to-end drift was caused solely by output allocation.

`paired-candidate-results/` contains the separate candidate run. Round 1 passed
phase drift checks, but round 2's 10+4/1048577 B/missing-data-3 case had
A1/A2 drift +5.85%. The script stopped and did not print any candidate deltas.
The candidate comparison is rejected in its entirety; do not calculate or
claim a speedup/regression from these files. Removing repeated output-buffer
allocation did not eliminate every source of baseline variation in this run.

Each result directory includes a 200 ms guest resource monitor. Samples showed
cpuset 0-15, no named competing benchmark, and no steal-counter increase on
vCPUs 8-12. These observations cannot rule out shorter interference or
unobservable host frequency/cache effects. `run_baseline.py` and
`run_candidate.py` retain the preflight and stop rules. No previously failed A2
run is rehabilitated by the new diagnostic preflights.

## Allocation-request diagnostic

`allocation_probe.rs` counted process-wide allocation/reallocation requests
only during 100 warmed retained-row calls, not elapsed time or live/RSS memory.
Each of the three cases recorded one request for 1520 B; the retained output
pointer remained unchanged and its bytes matched the original. The subsequent
`allocation_detail_probe.rs` recorded one such request at iteration 42, 48 and
54 respectively (`allocation-detail.jsonl`). Counts include all threads in the
process; the requesting component and causal relationship to timing variation
were not established. Do not describe this probe as allocation-free.

## Streaming gates and off-target CPU monitoring

The original `run_baseline.py` and `run_candidate.py` collected an entire
probe process before evaluating each case. Thus a later case could already
have started by the time an earlier case's failed A2 was detected. They and
their original data remain unchanged for provenance.

The new `baseline_row_probe_stream.rs`, `paired_row_probe_stream.rs`, and
`run_streaming.py` use an explicit per-case acknowledgement: the probe flushes
one complete case and waits; the parent checks the case before allowing the
next one. The runner performs two baseline rounds before optionally starting
two candidate rounds. It rejects nonfinite or nonpositive samples, invalid
iteration counts, unexpected or duplicate cases, mismatched job/CPU policy,
phase drift over 5%, baseline same-source bias over 5%, and A- or B-level
cross-round drift over 5%. It prints candidate latency deltas only after every
requested case and both candidate rounds pass.
On a failed gate or observed resource change it terminates only its own probe.
`test_streaming_gate.py` exercises the gate, invalid data, child termination,
per-case acknowledgements and resource-change stopping behavior.

An initial version of the streaming runner reported `streaming-aa-results/`
as passing the numerical baseline gates, and a synthetic 6% failure test
(`fake-gate-check/`) confirmed that the next case did not start. Post-hoc
analysis of the saved 200 ms full-CPU counters found sustained off-target CPU
activity near the end: vCPU 2 reached 100% for 16 consecutive intervals,
followed by high activity on vCPUs 5-7. The source of that activity was not
captured; the original monitor only recognized named competitors. Therefore
`streaming-aa-posthoc-review.json` supersedes the original pass and rejects
that baseline as insufficiently isolated. No candidate was run from it.

The current streaming runner checks all 16 guest vCPU counters continuously.
It excludes monitor vCPU 0 and test vCPUs 8-12; sustained aggregate busy time
of at least 5% across the remaining ten vCPUs for three consecutive samples
causes a fail-closed stop. It also checks the cpuset, named competitors and
`rustc`/`cargo` processes, target steal counters and cgroup throttling, and
requires three initially quiet intervals before starting. The threshold is a
prespecified diagnostic guard,
not proof of physical isolation; intervals shorter than 200 ms, CPU 0 load,
shared cache/memory effects and unobservable frequency changes remain limits.
The new off-target rule has local behavioral tests but has not yet been used
for a candidate performance run.
A replay test over the retained `streaming-aa-results/monitor.jsonl` confirms
that the new off-target rule detects that recorded activity at monitor sample
197, approximately 41.42 seconds after monitoring began. This is a replay of the
old data, not a new performance result. The streaming gate's local suite now
contains 18 checks, including rejection of malformed samples, immediate
per-case stopping, child-only termination, resource changes, cross-round drift
and recorded-data replays.

`audit_cross_round.py` applies the later cross-round rule to saved baseline
rounds without altering their original summaries. The results in
`cross-round-audit.json` reject `paired-aa-results/` for -7.09% A and -8.89% B
drift, and reject the separately frozen `7587132` scheduling diagnostic for
+9.05% A and +9.32% B drift. The original per-round pass messages are therefore
superseded for these two sets. No failed candidate run is rehabilitated.

The strict streaming runner was then validated on the native VM. A synthetic
probe deliberately reported 6% A2 drift; the runner rejected it before
acknowledging the first case, and the synthetic next-case marker was absent
(`fake-gate-check-v2/`). This is a gate-behavior test, not a performance run.

The first strict baseline-only round (`streaming-v2-aa-results/`) completed the
10+4/1048576 B case, then rejected 10+4/1048577 B because A1/A2 drift was
-5.50% (B1/B2 -1.37%). Only two cases were recorded; the subsequent 4+2
control and all candidate runs were not started. During 76 monitor samples,
cpuset remained 0-15, no named competitor appeared, and the maximum sustained
off-target-load streak was one interval, below the three-interval stop rule.
These observations do not identify the remaining variation or exclude shorter
interference and unobservable physical frequency/cache effects. The failed
preflight is invalid and cannot be used for candidate performance attribution.

## 6+2 cross-version compatibility

The release-compatibility tool now also covers 12 Classic GF8 6+2 cases at
4095, 4096 and 4097 bytes across Vandermonde, Cauchy, Jerasure-like and valid
custom matrices. It compares complete parity and verifies/reconstructs one
missing data plus one missing parity shard across published 9.0.0 and worktree
9.1.0 in both directions. The existing 200 cases and 36 large-GF8 cases remain.
Normal `simd-accel` runs passed on Apple Silicon and native x86.

For direct candidate coverage, a disposable x86 snapshot based on `5dae909`
plus `6x2-interop-baseline-source.patch` applied the unapplied
[`6x2-avx2-candidate.patch`](../2026-10-31-followup/6x2-avx2-candidate.patch)
and `6x2-interop-instrumentation.patch`. The latter selects `simd-avx2` for
both versions, asserts `rust-avx2` for both, and counts generated-encoder calls
only in the temporary new-version crate. The counter was reset immediately
before the 6+2 cases. All 12 cases passed with 32 observed candidate-kernel
executions; the 200 existing and 36 large cases also passed. Checksums and
configuration are in `6x2-interop-result.json`. This is functional evidence,
not a performance measurement or an acceptance of the 6+2 candidate.

## Per-stage CPU and runnable-wait diagnostic

`paired_row_sched_probe.rs` is a baseline-only, same-source dual-copy variant of
the retained-row probe. It snapshots Linux per-thread `schedstat` before and
after each entire phase, records the one-second warmup iteration count, timed
iteration counts and phase wall time, and retains the per-case handshake. The
snapshot fails if the thread set changes. Thread snapshots and formatting are
outside the timed iterations but alter the diagnostic binary, so these results
are not interchangeable with earlier failed runs.

`sched-aa-results/` contains two passing baseline-only rounds under the strict
streaming runner. All three cases passed phase drift and same-source-bias
checks; all output bytes were verified, the monitor saw cpuset 0-15, and its
maximum off-target-load streak was zero. No candidate was run. For these
particular stable 10+4 phases, aggregate runnable wait divided by warmup plus
timed operations was approximately 43-155 ns per operation, while timed median
wall latency was roughly 61-67 µs. The CPU and wait totals include the calling
thread and Rayon workers, span warmup as well as sampling, and are scheduler
accounting rather than hardware-cycle measurements. Spinning is counted as CPU
execution, not runnable wait. Therefore these observations do not prove that
scheduling, memory/cache behavior, frequency changes or other effects caused
or did not cause the earlier failed A2.

The strict runner's latest source, monitored snapshots, phase output and
summary are retained with the diagnostic. Its 5% off-target CPU rule and
200 ms sampling limit remain as documented above. This passing instrumented
baseline does not rehabilitate the rejected candidate run or establish a
production performance gain.

During this diagnostic the shared worktree advanced to `7587132`; it includes a
newer GF16 runtime change after `88e52de`. The recorded retained-row binaries
remain bound to the older `88e52de` plus the saved worktree patch. Even though
the target is GF8, a release binary's layout can change with other runtime code,
so these measurements must not be treated as current-head performance evidence.
A future acceptance run must take a new complete source snapshot and rebuild
both baseline and candidate before running fresh preflights.

A native Linux synthetic handshake test subsequently supplied stable phases but
changed every second-round baseline level by +6%. The runner rejected the
first second-round case, did not acknowledge it or start its successor, and did
not start the supplied candidate binary (`synthetic-cross-check/result/`). This
validates gate control flow with synthetic values only; no codec performance
was measured. `synthetic_cross_probe.py` reproduces the synthetic child.

The runner now also prioritizes a resource-monitor rejection over the
secondary `missing case` symptom if a monitored probe is terminated at EOF.
The fixed-worker diagnostic in
[`../2026-11-01-affinity-diagnostic/README.md`](../2026-11-01-affinity-diagnostic/README.md)
exercised this situation; its interrupted comparison predates the fix and
remains rejected. The gate test suite covers this precedence explicitly.
