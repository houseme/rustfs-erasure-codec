# End-to-end recovery preflight and path check

The reserved-window retained-row comparison and the end-to-end probe are
documented in [`../2026-11-01-long-preflight/README.md`](../2026-11-01-long-preflight/README.md).
For this attempt, both source copies were refreshed to commit
`94563a5b97822ebfc1406ac50c0a305ccfb79d5a`, with the same temporary bridge and
package-version distinction as the earlier probe. The candidate copy also
applied the byte-balanced patch. The sources were rebuilt with stable Rust on
native x86 Linux, using the AVX2 feature and the default external caller.

The strict runner began the first same-source end-to-end baseline round.
`end-to-end-94563a5/` contains the original result: 38 resource observations,
an empty first-case output, and a resource-monitor rejection. The last
observation reported effective CPUs 0–7 and an active named competitor; earlier
observations reported CPUs 0–15. No complete baseline case was recorded, so
the candidate binary was never started. No latency or correctness conclusion
follows from this interrupted run. Only the runner's own probe was terminated.

The separate `activation-baseline.patch` and `activation-candidate.patch` add a
counter for the shared scheduler branch where outputs are fewer than jobs.
They are applied after the benchmark bridge, and the candidate instrumentation
is applied after the byte-balanced patch. Both patches reproduce the tested
source bytes. `activation_probe.rs` is a functional-only variant
of the end-to-end probe: it omits the AVX2-specific backend requirement, accepts
at least four available CPUs, uses 5 ms phases, resets and checks each copy's
counter for every phase, and checks every shard against the original after
recovery. An Apple Silicon scalar run passed all three cases with four Rayon
workers. Thus both the current scheduler and the experimental scheduler were
reached by the complete `reconstruct_data_opt` path for these cases; output
contents matched. This instrumented smoke test is not a latency comparison and
is not native x86 validation.

Before performance attribution, repeat the complete two-round same-source
preflight and, only if it passes, the candidate rounds on a quiet reserved
machine, with fresh snapshots of the then-current revision. Further GF8 and
GF16, small-size and job-budget coverage remains necessary before a shared
scheduler change can be enabled.

## Refreshed preflight

After the previously observed competing controller ended, another attempt
refreshed and rebuilt both probe binaries from commit
`23dd294141507e6a1b15adb371bc8e74de2a82c7`. The first monitor observation in
`end-to-end-23dd294/` reported CPUs 0–15 but an active named competing process,
so the runner refused to start the baseline. A later observation saw CPUs 0–7
and another baseline process; its identity at the first observation was not
recorded. No codec timing or candidate result was produced. A single earlier
controller ending therefore did not establish a stable exclusive window.

## Rejected end-to-end candidate comparison

A later reserved-window attempt used the current production source at
`5703a27a6599c733e9ef53ebf36874a8a14ff199`. Both same-source baseline rounds
passed the specified gates before the candidate ran. In the second candidate
round, the 10+4 / 1 MiB + 1 / missing-data-3 case had +5.88% B-level
cross-round drift; the gate rejected the complete comparison. The earlier
cases and phases must not be used to claim a candidate gain or regression.

`end-to-end-5703a27/` retains all original baseline, candidate and monitor
records, including the recorded but unacknowledged failing case. The monitor
observed CPUs 0–15, no named competitor and a maximum off-target busy streak
of one across 803 observations. These measurements do not identify the cause
of the drift or rule out short interference, caching, allocation or host
effects. The candidate remains disabled.
