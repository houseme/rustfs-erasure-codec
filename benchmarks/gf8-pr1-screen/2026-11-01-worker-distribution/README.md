# Retained-row worker distribution diagnostic

This baseline-only experiment investigates whether task allocation explains the
previous fixed-worker preflight drift. Its two codec copies use the frozen
`7587132` plus worktree snapshot documented in
[`../2026-10-31-freeze-7587132/README.md`](../2026-10-31-freeze-7587132/README.md).
The build script, parallel scheduler, encoding path, and GF8 x86 dispatch in
that snapshot were checked byte-for-byte against commit `70898c1`. The temporary
bridge, Rayon pool and per-thread affinity are the same diagnostic setup as
[`../2026-11-01-affinity-diagnostic/README.md`](../2026-11-01-affinity-diagnostic/README.md).

`instrumentation.patch` adds cache-line-separated atomic group counters to each
codec copy. `probe.rs` resets and reads them around each sample and asserts the
sum equals the number of operations times the expected group count. These
operations change the measured code and add overhead: the resulting timings are
only diagnostic and cannot validate or compare production performance. The
original streaming runner and its checks are archived in commit `70898c1`.
The runner stopped as soon as the second round exceeded the existing 5%
cross-round gate; no candidate was run. `results/` and `diagnostic.log` retain
the observed samples, resource monitoring, counts, and rejection.

In the first round, both 10+4 cases created four groups per operation, and each
worker processed exactly one quarter of all groups in every measured phase. The
4+2 control created three groups per operation: worker 0 processed one third,
while the other workers shared the remainder unevenly across phases. These are
group counts, not byte counts or a record of per-operation completion order.

In the second round the 10+4 / 1048577 case again distributed groups equally
across the four workers, yet its A-level changed by -5.10% between rounds. The
monitor reported no resource rejection. Thus equal aggregate group counts did
not prevent the observed drift in this instrumented run; they do not establish
its cause or rule out unequal group sizes, cache effects, frequency changes,
or interference below the monitor's detection threshold. The second-round 4+2
case was never started. No inference about the AVX2 candidate follows.
