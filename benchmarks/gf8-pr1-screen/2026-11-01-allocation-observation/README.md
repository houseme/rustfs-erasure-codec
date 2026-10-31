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
phase. It counts consecutive recovered-buffer address equality, capacity
ranges and process-wide minor/major faults. It emits counts only, never actual
addresses, and records no timing. The first phase has one fewer address
comparison because there is no previous buffer; later phases include the
transition from the preceding phase.

Identical virtual addresses do not prove physical-page reuse, and the fault
counter includes all process threads and the observer's own reads. This probe
does not identify which allocation faulted, measure allocator latency or
explain a previous candidate run. Its fixed iteration count also differs from
the earlier timed phases, so its values cannot be used to accept or compare
performance candidates.

The fault parser and address-transition counter passed local unit tests. A
separate Apple Silicon smoke variant used 16 iterations and synthetic zero
fault counters; all three recovery cases and twelve JSON phase records passed
structural checks. This is functional harness validation only. The native
observation has not run: the test machine was executing another workload and
its effective CPU set was restricted. Use fresh same-source snapshots, the
existing temporary bridge and manifest, and a reserved quiet test window for a
native baseline-only run. Never treat the output as candidate acceptance.
