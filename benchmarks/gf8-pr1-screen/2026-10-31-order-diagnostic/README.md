# Baseline order and scheduling diagnostics

Historical Python experiment scripts cited below are archived in commit
`70898c1` and are not part of the current source tree. The recorded data and
Rust probes remain available here.

This is baseline-only follow-up to the rejected A2-drift preflight recorded in
[`../2026-10-31-same-process/README.md`](../2026-10-31-same-process/README.md).
No candidate performance comparison was run in this follow-up. The code
snapshot is the `fc6795c` worktree recorded in
[`../2026-10-31-followup/baseline-worktree.patch`](../2026-10-31-followup/baseline-worktree.patch).
The later `268e092` commit contains only GF16 benchmark/documentation changes.

The user-confirmed reserved AMD EPYC 9V45 Azure VM was used with stable rustc
1.99.1, five guest vCPUs, `std simd-avx2`, four Rayon workers and at most four
codec jobs. Each phase has one second of warmup and three 300 ms samples. The
same-process probe links two baseline copies (one with a temporary package
version for linking), verifies all output shards after every phase, and uses a
5% gate for both A1/A2, B1/B2 and same-source A/B bias. Guest cpuset, named
competing processes, vCPU counters and CPU pressure were sampled every 200 ms;
this does not detect all possible interference or physical frequency changes.

## Case-order diagnostic

`baseline_order_probe.rs` places a 4+2/1 MiB/one-missing baseline control before
and after the two 10+4 cases. In round 1 all gates passed, but the last control
had A1/A2 drift -4.71% and B1/B2 drift -4.75%. In round 2 the first control
had A1/A2 drift -5.08%; the run failed and no performance attribution follows.
The last control in that same round had A1/A2 drift +0.24%. The two rounds do
not establish a consistent first/last effect. `order-aa-{1,2}.jsonl`,
`order-aa-monitor.jsonl`, and `order-preflight.py` preserve the data.

## Thread scheduling diagnostic

`baseline_scheduler_probe.rs` adds Linux `/proc/self/task/*/schedstat` snapshots
outside the timed iterations. It sums CPU execution time, runnable wait time,
and scheduling counts across the process threads; values cover each entire
phase including warmup and measured operations. `order-sched-stderr.log`
records these deltas and the phase wall time. This probe is an instrumented
build; it is not interchangeable with the prior failed run.

Both instrumented baseline rounds passed the 5% gate, although round 2's last
4+2 control had A1/A2 drift -4.98% and its 10+4/two-missing case had -4.50%
same-source bias. The aggregate runnable wait recorded per approximately
1.9-second phase was only a few milliseconds across the threads. This is not
proof that scheduling is irrelevant: the snapshot granularity, code-layout
change, cache effects, allocator behavior, CPU frequency and other factors are
not isolated. The source data is `order-sched-aa-{1,2}.jsonl`,
`order-sched-monitor.jsonl`, and `order-sched-preflight.py`.

All monitor samples in the scheduler run showed guest cpuset 0-15, no named
competing benchmark, and no steal-counter increase on vCPUs 8-12. These are
observations within the monitor's limits, not proof of a stable physical host.
The virtual machine did not expose hardware cycle/instruction counters or a
guest CPU-frequency interface in the previous diagnostics.

## Code-level compatibility work

The main worktree adds `output_chunk_scheduler_covers_each_byte_once` in
`src/core/parallel.rs`. It checks the existing output scheduler across one to
seven outputs, zero and nonzero lengths, multiple chunk sizes, zero through
eight requested jobs, and both scheduling branches. Every byte must be visited
exactly once at its correct row and offset. The focused test passed on Apple
Silicon and native x86; x86 all-features/all-targets Clippy with warnings
denied also passed. This test adds no production scheduling change.

The 256-byte AVX2 candidate remains unapplied. Its earlier failed A2 preflight
is not rehabilitated by the instrumented baseline results; the results here do
not identify a sufficient way to control the original drift or support a
candidate speedup claim.

## Rayon-worker diagnostic

`baseline_rayon_worker_probe.rs` runs the same baseline-only, same-source
comparison inside an explicitly constructed four-thread Rayon pool, rather than
calling from an external thread. This is a different calling context and does
not represent the production caller by itself. The first preflight
(`order-worker-aa-1.jsonl`) failed: the first 4+2 control's same-source A/B
bias was -7.12%, outside the 5% gate. The run stopped and no candidate was
timed. `order-worker-monitor.jsonl` and `order-worker-stderr.log` retain guest
resource and thread-scheduling observations. The monitor saw cpuset 0-15, no
named competitor and no steal-counter increase on vCPUs 8-12, within its
sampling limits. The result does not establish a remedy for the baseline
instability and does not invalidate or replace earlier failed runs.

The worktree also adds `tests/six_plus_two_compatibility.rs`, using the
previously prepared candidate-independent portion of the 6+2 compatibility
test. It compares serial and parallel encoding to incremental parity for three
built-in matrix modes and a custom matrix; checks unaligned input/output guard
bytes and lengths around 4096 B; and reconstructs missing data. It passed on
Apple Silicon with `simd-neon`, and on x86 with `std simd-avx2`; focused x86
Clippy with warnings denied passed. The production code still does not select
an AVX2 6+2 generated kernel.

### Forced-alignment diagnostic

A further baseline-only experiment rebuilt the Rayon-worker diagnostic with
`RUSTFLAGS="-C llvm-args=-align-all-functions=6"`. This changes compiler output
and is not a release-build performance measurement. Round 1 passed all gates.
Round 2 failed at the 10+4/two-missing control: A1/A2 drift was -8.35%.
Its A1 samples were about 138.7 µs, while the A2 samples fell from 132.3 µs to
about 127.0 µs within the phase. The monitor saw cpuset 0-15, no named
competitor and no steal-counter increase on vCPUs 8-12, subject to the same
sampling limits. Thread scheduling data are in `order-worker-aligned-stderr.log`.
This observation shows that the forced-alignment diagnostic did not control all
baseline variation; it does not identify the cause. No candidate was timed.
The raw files and preflight script are `order-worker-aligned-*`.

After adding the scheduler regression, the main worktree's all-features
library suite on Apple Silicon passed 413 tests, with five ignored. The x86
focused scheduler test and all-features/all-targets Clippy were run separately
as noted above.

## Cross-version compatibility at the generated-kernel boundary

The workspace release-compatibility probe now adds 36 Classic GF8 cases:
4+2, 10+4 and 12+4; Vandermonde, Cauchy, Jerasure-like and a valid custom
matrix; and shard lengths 32767, 32768 and 32769. It compares complete 9.0.0
and worktree 9.1.0 parity, then verifies and recovers one missing data plus one
missing parity shard in both directions. The original 200 configurations and
historic arithmetic vectors remain checked.

The normal `simd-accel` probe passed on Apple Silicon and native x86. Separately,
a temporary native x86 snapshot changed only the probe dependencies to
`std simd-avx512`, asserted that both versions selected `rust-avx512`, and added
a counter to the new generated encoder. All 36 large cases and all 200 existing
cases passed, with 96 generated-encoder executions observed. The temporary
`avx512-interop-instrumentation.patch` contains those feature changes,
assertions and counter; it is not applied to the production worktree. The exact
tracked worktree delta and instrumentation checksums are recorded in
`avx512-interop-result.json`. The counter alters the test binary, so this is
functional evidence only, not a performance measurement. It covers the listed
matrices, shapes and lengths rather than all possible formats.
