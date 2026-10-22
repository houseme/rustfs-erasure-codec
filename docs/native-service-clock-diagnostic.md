# Native service-clock diagnosis

The optional `--native-service-clock` mode of `scripts/diagnose_release_vm.py`
reads guest-CPU TSC/APERF/MPERF counters synchronously in the diagnostic worker,
before and after the original wall window. It avoids assigning counters by the
arrival time of a log message: that message may arrive after the next operation
has started. The ordinary sampler, timer boundaries, original operation loops,
warmups, counts and release acceptance remain unchanged.

This is a separately generated process observer using the existing Rust overlay
renderer. The VM controller retains process/resource orchestration and window
interpretation within its current migration boundary. Its reads, process inspection,
formatting and allocation state can perturb subsequent work. Its observations
are not a substitute for the uninstrumented release campaign.

## Capability probe

Use a clean, frozen controller and a separate clean checkout of published 9.0.0,
inside an already owned isolation reservation. Do not run this probe or any
build concurrently with an existing controlled capture. No command loads kernel
modules, changes MSR permissions, changes frequency or writes MSRs.

```sh
python3 scripts/diagnose_release_vm.py \
  --baseline /path/to/clean-9.0.0 \
  --isolation-cgroup /sys/fs/cgroup/owned-test \
  --output /path/to/new-private-probe \
  --native-service-clock --probe-only
```

The probe pins only its own thread to the declared measuring CPU and performs a
short arithmetic workload with **zero codec calls**. The controller requires
exactly one complete probe record, including its terminating newline. Missing,
duplicate, interrupted or unqualified records stop before any codec calibration.
`--probe-only` never starts the profile sampler. Its completion is not performance
acceptance or proof of physical-host exclusivity.

Linux x86_64 and read access to the existing measuring CPU's MSR device are
required for this optional observation. Other platforms are unsupported; unavailable device reads are retained as
unavailable data. Local container tests and compilation do not prove
that this VM's virtualized counters are faithful or that a native positive probe
has passed. Positive VM validation is pending while the existing complete
layout campaign owns the test resources.

## Counter interpretation

Each point retains the actual CPU, raw counters, TSC read-skew endpoints and a
worker-local monotonic envelope covering process inspection and counter reads.
The before read completes synchronously before the wall timer; the after read
starts after the wall timer. The enclosing interval must contain the original
wall duration. The conservative excess interval divided by the original wall
window must be at most 5% to publish diagnostic counter ratios. This observation
bound is separate from the original 5% same-version drift gate and does not add
a release condition.

The analyzer rejects wrong-CPU points, backwards bounds, counter resets, invalid
values and inactive counter deltas. An unavailable read never becomes a zero
service measurement. An over-wide interval keeps the original sample while
withholding ratios. Even qualified ratios describe the guest measuring CPU,
including observer, kernel and other activity; they are not exclusive API cycles
or GHz, and do not normalize or replace API timings. Virtualized counter fidelity
and physical cache/memory interference remain unproven.

A read failure after a completed operation is serialized as unavailable data;
it cannot panic away that completed observation. Raw observations and diagnostic
reports remain private. Their format cannot enter formal release verification.

## Source ownership

Both ordinary and counter-enabled VM diagnostics use the existing reversible
source-overlay owner and per-source build directory. Build failures, sampling
failures, interrupts and normal completion all restore owned generated inputs.
Unexpected external edits are preserved and cause failure. Final completion is
written only after cleanup and source/resource checks succeed. Failed and partial
raw observations are retained, and only the owned partial checkpoint is updated
as later profiles complete.

The report records the controller source, exact generated diagnostic-overlay
digest and counter-template digest. Formal sampler templates are not edited.
The historical [20.6528% refusal and subsequent profile analysis](a1-service-drift-review-2026-10-22.md)
remain valid; this implementation is not evidence that their mechanism is fixed.

## Probe framing regression

The first Linux probe read available before/after counters, but serial libtest
prefixed the record with `test native_service_clock_probe ... `. The strict
parser correctly refused this non-framed line. The reporter now begins a new
line before its marker. A hardware-free Rust fixture runs through the same
reporter and an actual serial libtest subprocess; it reproduced the rejection
before this fix and passes afterwards. Duplicate, missing and truncated records
remain errors. The original failed attempt is retained and is not promoted into
a successful capability check. A fresh source-bound Linux probe is required.
