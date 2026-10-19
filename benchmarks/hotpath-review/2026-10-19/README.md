# Targeted placement control and current verification profiles

Tool/source: `a62c6c36ae15a05fb73b5bf6893261978b6ce325`, protocol 13. The
unchanged-version control uses published 9.0.0 (`9891dbe4...`). This directory is a
**diagnostic snapshot**, not release evidence, an accepted full campaign or a
9.0.0/9.1.0 speed comparison. `manifest.json` binds all numerical JSON members.
Production codec algorithms were unchanged by this review.

The native check executed both actual codec checkouts with their own lockfiles:
15 active sampler tests and 23 changed workers each. All workers reported the full
eight-CPU constructor/pool mask, constructor budget eight, four Rayon workers and
one fixed measuring CPU, with actual-affinity checks. The independent Linux heap
probe passed 13 tests, including heap-free affinity observation. Linux generated-
sampler/probe Clippy with warnings denied passed. `validation.json` records both
successful reservation cleanups. One earlier Linux build stopped on a CPU parse
type annotation, before any performance samples; it was corrected in the recorded
tool source, with that failed attempt retained privately.

## Predeclared single-case repeatability control

Case `encode:10:4:524288` ran in two back-to-back fresh workers, **512 iterations**
per observation, three fixed warmups plus nine samples. No adaptive count selection,
case dropping or rerun-to-pass occurred. Source, seed, caller geometry, placement
and codec/pool preparation matched. The generated test comes from the recorded
sampler, with the unchanged baseline codec and original operation loop. The worker
is launched directly from the sole executable returned by Cargo's JSON artifact
record, not by selecting a recent file name. Environment is the recorded source's
`SAMPLING_ENV`, profile `small_file`, mode `sample`, batch zero, OS page size, and
the minimum reserved CPU selected after codec/pool preparation.

| Repeat | Median per encode | Central spread | Short samples |
| --- | ---: | ---: | ---: |
| 1 | 351.7735 us | 1.0338% | 0 |
| 2 | 349.7186 us | 0.8549% | 0 |

The absolute repeat drift is **0.58415%**. Raw `repeat-*.samples.json` retain every
warmup and sample. Recompute medians from the nine `timed_ns / iterations` values;
spread uses second-highest minus second-lowest divided by that median. These
results qualify this one diagnostic window only. The earlier fixed-buffer control
spanned intervening CPU epochs and another time window; this is not a randomized
before/after test proving that placement/pool preparation alone repaired its drift.
It does not complete the 126-case baseline prerequisite or backlog #2897.

## Qualitative current-codec verification profiles

The existing formal Vec caller cases `verify:4:2:16384` and
`verify_with_buffer:4:2:16384` were sampled separately on the same recorded codec
source. Both used 250,000 iterations per observation, the same fixed schedule and
placement. `perf record` used `cpu-clock:u`, 499 Hz and 8,192-byte DWARF stacks.
Each profiled numerical journal is named `*.profiled-samples.json` to distinguish
it from uninstrumented release samples. Its timings must not be used for release
or cross-API throughput conclusions. Capture includes fixed warmups, worker
preparation and deferred export as well as the long operation loops.

There are **2,738 / 2,507 samples**, with zero lost/lost-sample/throttle/unthrottle
records. `hotspots.json` retains every aggregated leaf symbol, event counts and
unresolved fractions. All function counts sum back to each event total. Unresolved
address leaves are grouped, not guessed or silently removed. Raw perf data and
machine-specific paths remain private.

Ordinary verify has about **14.02%** of leaf samples in `SmallVec<[u8;4096]>::resize`;
that resolved initialization entry is absent in caller-buffer verification. The two
GFNI multiply entries total about 58.55% / 78.22% of samples. Unresolved leaves are
21.51% / 17.71%; resolved allocation symbols are not an upper bound on allocator
cost. These are sample shares, not a measured 14% speedup or current GET CPU shares.
The profile uses the existing Vec-of-Vec cases; the inspected consumer passes a
reference Vec and has additional work, engine selection and different lifecycle.

## Allocation screening and next decision

`resource-counts.json` comes from the existing recovery-path probe's six new
verification scenarios. It uses the reference-slice adapter seen in the consumer,
20 warmups and one counted synchronous API call per case. Caller reference and
scratch construction are outside counting. Ordinary verification requests 32 KiB
at 4+2/16 KiB and 128 KiB at 10+4/32 KiB; caller buffer and VerifyWorkspace reuse
request zero. Short unreserved microtimings are intentionally omitted.

This supports investigating reader-owned verification scratch where the selected
consumer engine actually calls ordinary verify. It does not establish request
frequency, end-to-end benefit, cold setup or concurrent retained-memory cost. The
inspected consumer defaults to a different engine. No runtime consumer integration
or Leopard API change is represented here. See the accompanying hot-path review
for generic recovery publication/error constraints and the pending full gates.
