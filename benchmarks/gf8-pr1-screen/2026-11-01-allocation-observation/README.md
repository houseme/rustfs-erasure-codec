# Non-timing recovery allocation observation

The baseline diagnosis in
[`../2026-11-01-e2e-diagnostic/README.md`](../2026-11-01-e2e-diagnostic/README.md)
recorded 255 process-wide minor faults in one intermediate phase; the two
adjacent A phases recorded none but still drifted by 6.87%. Those counts do not
identify the allocation responsible or establish causation.

`allocation_reuse_probe.rs` prepares a separate, baseline-only functional
observation. It links two copies of the same codec source and runs the same
three `reconstruct_data_opt` shapes with four Rayon workers. Each of four
phases performs 1,024 reconstructions, explicitly drops the missing Option
buffer before recovery, and checks every shard against the original after the
phase. The temporary baseline instrumentation from the
[activation check](../2026-11-01-e2e-preflight/README.md) must be applied to
both copies; its counters are reset and checked in each phase to confirm that
the fewer-outputs scheduler branch ran. The probe counts consecutive
recovered-buffer address equality, capacity ranges and process-wide minor/major
faults. It emits counts only, never actual
addresses, and records no timing. The first phase has one fewer address
comparison because there is no previous buffer; later phases include the
transition from the preceding phase.

Identical virtual addresses do not prove physical-page reuse, and the fault
counter includes all process threads and the observer's own reads. This probe
does not identify which allocation faulted, measure allocator latency or
explain a previous candidate run. Its fixed iteration count also differs from
the earlier timed phases, so its values cannot be used to accept or compare
performance candidates. The probe flushes each complete case and waits for a
`continue` acknowledgement, allowing a resource-monitoring parent to stop
before the next case if the reserved window changes.

`allocation-monitor.patch` applies to the archived
`2026-11-01-affinity-diagnostic/run_streaming_latest.py` in commit `70898c1`.
It retains the same resource monitor and per-case stopping behavior while
validating non-timing case IDs, capacities, fault counts and address-transition
counts. It refuses candidate binaries and labels a successful run as a
resource/schema check with `performance_gate_passed: false`. It cannot accept a
performance candidate. The generated monitor was reproduced from the patch;
local synthetic tests covered malformed counts, candidate refusal and stopping
before acknowledgement. A functional smoke test also confirmed that the
probe waits between all cases and exits on an invalid acknowledgement.

The fault parser and address-transition counter passed local unit tests. A
separate Apple Silicon smoke variant used 16 iterations and synthetic zero
fault counters; all three recovery cases and twelve JSON phase records passed
structural checks. This is functional harness validation only. The first native
build attempt was deferred while another workload restricted the CPU set.
The temporary branch counters change the executable and add overhead. Never
treat the output as candidate acceptance.

## Native non-timing observation

The probe was built from committed snapshot
`f61bf9612a2f411b56a197ed6789b54423f98080` using two baseline copies with the
temporary bridge, branch counters and package-version distinction. Native x86
Linux passed both parser/transition unit tests. The monitor then completed two
non-timing rounds over all three cases. `allocation-f61bf96/` retains the
original records: resource and schema checks passed, and the summary
explicitly records that no candidate or performance gate was run. Across 12
monitor observations, CPUs remained 0–15, no named competitor was seen and the
off-target busy streak was zero. Shorter interference is not excluded.

In both rounds, every consecutive virtual-address comparison within each case
matched; recovered capacities equaled the requested lengths. The first A1
phase of the 4+2 case recorded 256 process-wide minor faults in each round;
all later phases recorded zero minor faults, and all phases recorded zero major
faults. No addresses were saved. This shows only what the instrumented process
observed in this short run: it does not establish allocator ownership,
physical-page retention or the cause of the earlier performance drift.
