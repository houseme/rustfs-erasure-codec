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

## Bounded follow-through

A separately frozen sixteen-worker plan targets the two slow paths and a stable
encode control, with complete A1/B1/B2/A2 repeats. Original variants preserve the
captured loop counts, timing kernels and three warmups/nine samples. A separate
reconstruction variant adds one explicit result-consumption barrier to test an
optimizer/unused-result hypothesis; it has a diagnostic workload marker refused
by formal validation. That hypothesis is not a confirmed defect. Optional
software CPU-clock profiles and a zero-codec synchronous-counter capability check
will be retained with their own limitations, without normalizing API timings.

All variants compile and restore their owned source overlays. The first resource
attempt was refused before any codec measurement because another controlled
measurement owns the requested CPU partition. Its failed preflight and cleanup
are retained; no foreign process or partition is changed. A new resource attempt
is prepared with the same immutable driver/count plan and starts only after the
partition is free. Completed results from the related active control are reviewed
before launching any redundant measurement. Statistical failures will not truncate
remaining controls.

The frozen capture does not match every later tool/dependency byte. A final
candidate still needs its own source-bound accepted evidence and strict positive
release preflight. Keep 9.1.0 unreleased, without a tag or registry upload. See the
[publication blockers](release-9.1.0-blockers.md) and
[release checklist](release-9.1.0-checklist.md).
