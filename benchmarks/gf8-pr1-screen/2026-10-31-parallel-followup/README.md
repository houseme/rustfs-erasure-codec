# GF8 follow-up: measurement diagnostics and parallel row candidate

Historical Python experiment scripts cited below are archived in commit
`70898c1` and are not part of the current source tree. The recorded data and
Rust probes remain available here.

The source baseline is the `fc6795c` worktree snapshot identified by
[`../2026-10-31-followup/baseline-worktree.patch`](../2026-10-31-followup/baseline-worktree.patch).
The previously rejected 6+2 A2-drift result remains invalid; this follow-up
does not use it for candidate performance attribution.

## Baseline measurement diagnostics

In the previous same-process A/A executable, the two same-source
`encode_classic_matrix` copies each contained 623 instructions. Removing
address-dependent operands left identical instruction sequences. Their entry
addresses had 16-byte and 0-byte offsets within a 64-byte boundary. This is
consistent with code layout being one possible contributor to the observed
same-source A/B bias; it does not prove causation, and relocation targets and
data placement can still differ.

An A/A-only diagnostic rebuilt the same sources with
`RUSTFLAGS="-C llvm-args=-align-all-functions=6"`. The two function entries
then aligned to 64 bytes. In `shape-aa-aligned.jsonl` the 6+2/4096 B
same-source A/B difference was +0.40%, versus +7.84% in the previous default
build; all phase drift and same-source bias checks passed the 5% screen. This
is a modified compiler configuration, not a production performance result.

For the independently observed 6+2/8192 B baseline A2 drift failure, 12
baseline-only processes with normal ASLR and with per-process ASLR disabled
had max/min median spreads of 3.68% and 6.43%, respectively. The raw results
are `shape-8192-aslr-{normal,fixed}.jsonl`. Disabling ASLR did not establish a
fix or identify the source of the failed A2 drift.

`shape-8192-scheduler.jsonl` and `diagnose-scheduler.py` record 16 baseline-only
runs with `perf` software counters. Context switches were 13–22 per process,
page faults 485–493, and one reported migration includes the `taskset` launch;
these process-wide counters do not identify a cause of timing variation. The
VM did not expose the requested hardware cycle/instruction counters or a guest
CPU-frequency interface. Do not infer a performance conclusion from the
previously rejected candidate comparison.

## Parallel row experiment

`parallel-row-candidate.patch` is an unapplied, instrumented experiment against
the worktree snapshot above. It extends the previously bounded AVX2 row
candidate to the existing Rayon output-chunk scheduler for 10+4, one output,
and shard sizes at least 1 MiB. Byte views and row dimensions are validated
before scheduling; each chunk validates its input range and writes only its
exclusive output slice. Other shapes, GFNI, and AVX-512 continue through their
existing dispatch. The `#[cfg(test)]` counters prove which path ran and do not
add release-build counting.

On native AVX2 with four available guest vCPUs:

- Instrumented `reconstruct_data_opt` recovered the expected bytes and called
  the candidate row with both one job (one invocation) and four jobs (12 chunk
  invocations). These counts demonstrate dispatch, not a speedup.
- The three-matrix, threshold-boundary, and missing-shard reconstruction
  integration test passed. The generic dispatch boundary test also passed.
- Invalid row length, short inputs, and overflowing ranges were rejected before
  writing; separate custom-Field tests verified that failed row, input and
  output byte-view preparation falls back before writes and produces the
  generic result.
- The all-features library suite passed 446 tests, with five ignored. AVX2-only
  all-target Clippy with warnings denied, AVX-512-only `cargo check`, and
  AVX2-only no_std `cargo check` passed. These checks do not cover every CPU,
  custom Field implementation or concurrency schedule.

The four-vCPU performance probe used `std simd-avx2`, four Rayon workers and
the repository's unmodified recovery benchmark. The first baseline-only round
for 10+4/1 MiB/one missing data shard passed. In its second baseline-only round,
B1/B2 drift was -5.01%, outside the 5% gate, and the preflight stopped before
any candidate timing. Raw stages and `parallel-preflight.py` are saved as
`parallel-aa-*`. A subsequent inspection showed the guest cpuset had narrowed
from 0-15 to 0-7 and another high-CPU process was running; its exact start time
relative to the failed stage is unknown. No competing process was changed.

The candidate is not production-ready: its performance remains unmeasured, the
6+2 A2 failure is not explained, and the test machine is no longer an exclusive
window. A fresh, continuously monitored reservation and repeated baseline
preflights are required before any candidate A/B/B/A or adoption decision.

## 6+2 compatibility test prepared

`6x2-compatibility.rs` checks unaligned input and output slices with guard
bytes, incremental parity equivalence, serial and parallel API results,
reconstruction, three built-in matrix modes, a valid custom matrix, and lengths
on both sides of the candidate's 4096-byte boundary. It passed in a local
Apple Silicon temporary checkout with `simd-neon`; that run exercises the
fallback rather than the x86 AVX2 candidate. The existing direct AVX2 generated
kernel test had already passed on the x86 host, but this new full compatibility
test also passed on native x86 AVX2 in an idle guest. A child-process
dispatch assertion confirmed fallback at 4095 B and generated-kernel
selection at 4096 and 4097 B. This is functional validation only; the failed
performance gate and need for a controlled benchmark remain.

## Exclusive-window recheck

After the user confirmed that the competing task had ended, a sustained idle
check preceded a new four-vCPU baseline-only preflight. The first A/B/B/A
baseline round for 10+4/1 MiB/one missing data shard passed; the second had
B1/B2 drift -5.20%, so it stopped before candidate timing. The raw stages,
`parallel-exclusive-preflight.py` and `parallel-exclusive-monitor.jsonl` are
preserved. During the 200 ms monitor samples, the cpuset remained 0-15, no
named competing benchmark was observed, and vCPU 8-11 steal counters did not
increase. Sampling cannot rule out shorter interruptions, frequency changes or
other causes. The failure is not attributable to a demonstrated resource
change, and the prior failed preflight is not rehabilitated.

The 6+2 compatibility artifact was subsequently executed on native AVX2 in an
idle guest and both its built-in/custom matrix test and 4095/4096/4097-byte
dispatch test passed. This remains functional evidence only. No additional
production optimizer was enabled and no candidate performance claim follows
from either failed preflight.

The saved parallel candidate patch was also applied to an independent local
Apple Silicon checkout. Its `simd-neon` reconstruction compatibility test
passed and x86-64 Linux `std simd-avx512` cross-`cargo check` passed; these are
fallback/build checks and do not execute the AVX2 candidate on Apple Silicon.
The same temporary parallel candidate also passed no-default-feature library
cross-checks for `wasm32-unknown-unknown` and `aarch64-unknown-none` with
`simd-neon`; these checks establish compilation only.

## Rayon scheduling diagnosis

A baseline-only thread snapshot showed four runnable Rayon workers plus the
calling thread bound to vCPUs 8–11. The caller also consumed CPU in the
snapshot, so the binding can oversubscribe those four vCPUs. A separate
baseline-only diagnostic reserved vCPUs 8–12, limited the policy and Rayon to
four jobs, and observed workers and caller on separate vCPUs at the sampled
instant. This changes the affinity environment and is not a comparison of
candidate code.

`parallel-scheduler-diagnostic.jsonl` contains eight process-wide `perf stat`
observations per binding. The four-vCPU runs recorded 48–110 migrations and
36,052–39,689 context switches per process; the five-vCPU runs recorded 3–9
migrations and 38,838–40,519 context switches. Counters include the launcher,
setup, warmup and timing, not just the measured operation. The data supports a
scheduling-confound hypothesis but does not establish the cause of drift.

A separate five-vCPU, four-job baseline A/B/B/A preflight is saved as
`parallel-five-*`. Its very first A1/A2 drift was +5.26%, outside the gate;
the preflight stopped before candidate timing. Continuous 200 ms samples saw
cpuset 0-15 and no named competing benchmark, but cannot rule out shorter
interruptions or frequency effects. Adding a spare vCPU therefore did not
establish a controlled performance environment. No speedup or regression
claim follows and no candidate code has been enabled.

Additional native checks on the same temporary parallel candidate passed after
the initial suite: 10+4 recovery with a valid custom matrix, three missing
positions, one/four job budgets and 1 MiB/1 MiB+1 sizes; two missing data
shards did not select the single-row kernel; and a forced GFNI AVX-512 backend
did not select the AVX2 table-row kernel. The updated candidate patch retains
these tests. All-features, all-targets Clippy with warnings denied passed after
the additions. These checks do not establish performance or replace an
independent A/B/B/A acceptance run.
