# Complete Linux diagnostic review — 2026-10-22

The explicitly requested complete campaign has finished. Its frozen candidate and
controller are `022c0a7`, against [published 9.0.0](https://github.com/houseme/rustfs-erasure-codec/tree/9891dbe4cfc0b8aaf46b1011bfe9d8de334b51b6). All original
calibrations, A1/B1/B2/A2 workers and paired-layout scans ran before analysis.
An independent invocation of the same frozen verifier reproduces the original
report exactly. This is completed diagnostic collection, not release acceptance.
The [numeric review](../benchmarks/release-9.1.0/2026-10-22/complete-diagnostic-review.json)
contains aggregate coverage and outcomes. Raw observations and host identities
remain private; historical failures and source bindings are retained.

## Coverage and failed checks

Both calibrations contain 966 workers / 11,592 observations. The original four
126-case phases contain 504 workers / 6,048 observations. All paired layouts add
3,072 observations, for 20,712 observations overall. Each layout phase covers
both lengths, every source/scratch offset pair and both scan orders.

| Check | Executed | Failed |
| --- | ---: | ---: |
| Original profile distributions | 504 | 1 |
| Same-version profile repeats | 252 | 9 |
| Profile regression programs | 16 | 16 |
| Layout distributions | 256 | 0 |
| Layout forward/reverse repeats within a phase | 128 | 0 |
| Layout repeats across phases | 128 | 16 |

No original profile sample is below the declared duration floors. All 504 profile
and 256 layout cross-version comparisons remain unqualified. Each of the sixteen
regression programs reports one or more threshold failures; these are completed
statistical checks, not sixteen crashed sampler workers or proof that every case
regressed. The sampler execution journal independently validates all 1,470
calibration/profile invocations and their successful process exits.

The first standalone replay failed because a noninteractive SSH environment could
not locate `rustc`. Its report is retained. Explicitly selecting the captured
compiler through the expected search path completes a new replay without changing
any source, count, raw record or original report. Resource records confirm that
owned partitions, controllers and parent affinities were restored and owned load
holders stopped. Both original observer services were active when checked.

## What the complete data establishes

The offline analysis covers every profile worker and joins its digest-bound
parent start/end interval to 47,653 guest CPU/pressure/memory snapshots. A final
explicit `post_capture` marker has no host snapshot and is retained as a marker;
it is not a missing measurement. The observer interval is approximately one
second. No absolute inner API timestamp was recorded, so the analysis does not
assign an individual host reading to a particular API observation.

Eight failed small-file repeat pairs have equal recorded caller fixture layouts.
Their metric movement follows calling-thread service movement while recorded
runnable waiting is far too small to explain the drift alone. The largest
same-version metric drift is about 103.06%. The failed hotspot repeat has no
caller fixture: its calling-thread counters enclose both operations, while its
candidate metric covers a subwindow. Those scopes cannot be treated as an
additive CPU partition or exclusive candidate cycles. Recorded measured profile
samples have no major faults. None of these facts identifies a unique historical
frequency, cache, allocation-state or invisible-host mechanism. In particular,
the largest unchanged-baseline drift has no measured minor or major faults.

The full data also contains candidate slow paths recurring in all four
cross-phase pairs, including small-file `reconstruct_data:10:4:65536` and stream
`verify_stream:10:4:65536:65536`. Both candidate phases also carry substantial,
repeatable minor-fault activity in those priority windows, absent from the matching
baseline windows. This directs allocation/page-lifetime investigation alongside
kernel/optimizer controls. Process counters do not identify which allocation
faulted, and the fault-free unchanged-baseline example prevents a universal
fault explanation. These are code-investigation priorities. Passing
individual spread/repeat checks does not qualify them under the failed complete
campaign. Retained caller offsets cover neither dynamic output owners nor
physical cache/page geometry. Global guest pressure and calling-thread counters
therefore cannot justify attributing every change to either the codec or the VM.

## Completed fixed-count controls — 2026-10-23

The separate plan completed all sixteen workers and 192 observations: 48 warmups
and 144 measured samples. Independent replay reproduced every twelve-observation
summary exactly, checked every parent start/end journal and original iteration
binding, and verified source restoration. All sixteen software-profiler reports
completed. No sample was short, no distribution was wide, and all eight
same-version repeat checks passed. All 48 result-retention observations remain
rejected by formal workload validation; their marker is unwrapped only in the
private diagnostic replay.

The controller is `75bc034`; the original sampler/candidate remains `022c0a7`
against published 9.0.0. These software-profiled controls do not measure later
integrated main or replace the failed complete release campaign. The
[derived comparison](../benchmarks/release-9.1.0/2026-10-23/output-retention-control.json)
retains all sixteen cross-version comparisons.

| Control, 10+4 / 64 KiB | Candidate/baseline latency ratio across all four pairs |
| --- | ---: |
| Original `reconstruct_data` | 11.7125–11.8830 |
| Same reconstruction plus `black_box(&shards)` | 11.7079–12.1540 |
| Original `verify_stream`, 64 KiB block | 7.6583–7.8247 |
| Original `encode` control | 0.9850–0.9906 |

The result-retention barrier leaves the reconstruction minor-fault contrast
unchanged. All four corresponding original/retained caller fixtures match.
This control does not support unused-output elimination alone as an explanation
of the observed gap. It does not prove identical dynamic owners or physical
memory/cache geometry. Stream verification retains its fault contrast, while
the encode control has no measured minor faults. These complete controls
strengthen allocation/page-lifetime investigation; the earlier fault-free
unchanged-baseline drift remains a counterexample to a universal fault cause.

Whole-process user CPU-clock profiles retain substantial libc execution
and finite-field computation. Unresolved libc addresses, their sampling scope
and exclusion of kernel execution prevent interpreting relative weights as
exclusive API work or converting them into hardware cycles.

Two initial resource attempts were refused before sampling and retain their
original errors and successful cleanup. The third attempt completed with all
source overlays, owned partitions, controllers and parent affinities restored;
both original observer services remained active.

## Counter capability and current-main integration

The optional nested probe was refused before capability measurement: its waiting
parent occupied the same partition, outside the child controller's descendant
set. That invocation error says nothing about hardware availability. Keep the
strict ownership contract and execute the controller in the partition's owning
process. See the [invocation contract](native-service-clock-diagnostic.md#controller-process-ownership).

A separate completed probe on `d7eb79a` already uses that owning-process entry
and the framed reporter now merged into main. Independent replay matches its
single complete raw record, strict counter/enclosure summary, zero codec-call
declaration and restored resources exactly. The enclosure excess is about
0.0784%. This establishes source-bound read/record/envelope capability without
rerunning a probe to select a favorable result. Virtualized counter fidelity,
exclusive API cycles and release qualification remain unproven.

Remote main `db69018` is integrated separately from these historical controls.
Its native encoder prepares and executes borrowed views in one scope, returns
only the result and reads matrix rows directly. Review confirms complete
safe-Field preparation before writes and no generic fallback after an execution
error. The merged implementation passes the encoding/identity/construction/
parallel regression programs, three byte-view storage tests, 128 native-tool
tests, all 273 Python tests, strict codec/tool Clippy, formatting and the
codec-only no_std check. Those checks establish correctness and tool integration;
the original diagnostic ratios do not become measurements of this merged source.

A final candidate still needs source-bound accepted evidence and a strict
positive release preflight. Keep 9.1.0 unreleased, without a tag or registry
upload; preserve every failed campaign and distinguish mechanism, resource and
publication evidence.
