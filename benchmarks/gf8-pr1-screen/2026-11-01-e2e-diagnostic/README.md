# End-to-end baseline scheduling and fault diagnostic

The candidate comparison documented in
[`../2026-11-01-e2e-preflight/README.md`](../2026-11-01-e2e-preflight/README.md)
was rejected after a 5.88% cross-round candidate-level shift. This probe is a
baseline-only diagnostic for potential sources of variation; it does not
compare candidates or rehabilitate that rejected run.

`baseline_sched_probe.rs` retains the ordinary external caller, the three
Option-based reconstruction cases, four Rayon workers, per-phase output
checks, two-second warmup and three 500 ms samples. It snapshots Linux
`/proc/self/task/*/schedstat` and the process minor and major fault counters
before and after each complete phase. The saved per-thread deltas are anonymous
and sorted by thread ID; their positions are not stable identities between
processes. Scheduler columns are execution time in nanoseconds, runnable-wait
time in nanoseconds, and timeslices. Counters include warmup and all timed
iterations; warmup iteration counts and phase wall time are retained so they
can be interpreted in that scope. If the thread set changes or counters move
backwards, the diagnostic stops. Snapshot calls are outside the timed loops,
but still change the executable and can perturb cache or scheduling behavior.

The `/proc/self/stat` parser was unit-tested with parentheses in the process
name and malformed inputs. A separate Apple Silicon smoke variant substituted
synthetic counters, used 5 ms phases and verified output and JSON structure for
three cases. Synthetic policy and sample values confirmed the existing runner
accepts the extra diagnostic fields and still rejects a 6% phase drift. These
are harness checks, not native measurements or performance acceptance.

For a native attempt, link two same-source copies using the existing temporary
benchmark bridge and the manifest from
[`../2026-10-31-freeze-7587132/probe-aa/`](../2026-10-31-freeze-7587132/probe-aa/),
then use the strict end-to-end runner with **no candidate binary**. Refresh both
copies from one immutable current revision, preserve the minimal temporary
package-version difference, and require the existing resource, within-round,
same-source and cross-round gates. A build planned against commit `099758a` was
refused before it started because a competing workload was present.

Page-fault and scheduler accounting cannot reveal CPU frequency, cache state,
brief interference, or prove a cause for a different candidate run. Even a
stable instrumented baseline would be a diagnostic observation, not evidence
that the experimental scheduler is ready for production.

## Native baseline diagnosis

The probe was rebuilt against `d22199f373d9f11ea93fc3dfab6da6f680277913`,
and the parser test passed on native x86 Linux. Both linked codec copies used
the same source apart from the temporary package version. No candidate binary
was supplied. `e2e-sched-d22199f/` retains the results and resource monitor.

The first round completed all three cases. In the second round, the 10+4,
1 MiB, missing-data-0 baseline had +6.87% A1-to-A2 median drift. The runner
rejected the preflight and did not start the last case. Across 337 monitor
observations, CPUs remained 0–15, no named competitor was seen, and the
maximum off-target busy streak was one observation. These checks cannot
exclude brief interference or unobservable host effects.

For the failing case, A1 and A2 had zero recorded process minor or major page
faults; aggregate runnable wait was approximately 70.9 and 71.6 ns per
operation, respectively. Aggregate execution time across the caller and four
workers was approximately 298 and 320 microseconds per operation, including
warmup, timed work and spinning. The intervening B1 phase recorded 255 minor
faults and no major faults; B2 recorded none. These are process-wide counts,
not allocation ownership. The observations do not establish whether the
faults, scheduling, cache state, frequency or another factor caused the drift.
No candidate ran, and the earlier rejected candidate comparison remains
invalid.
