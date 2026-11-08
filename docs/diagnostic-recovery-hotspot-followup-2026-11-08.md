# Recovery hotspot follow-up

The complete diagnostic of source `ec2d13abaca111f197441faf968bb6ac876107a1`
against published 9.0.0 collected all 126 cases in each phase and all four
paired-layout phases. The independent diagnostic replay matched the capture
report. It did not approve release: two phase distributions and five repeated
case comparisons exceeded their limits, and the layout report recorded one
wide cell, five within-phase repeat failures and 16 cross-phase repeat failures.
The earlier same-version qualification also failed. These observations do
not establish a candidate regression or permit favorable-case selection.

The diagnostic nevertheless identifies a reproducible question to investigate:
the 32+16, 1 MiB, two-missing-data recovery and selective-recovery hotspot rows
showed lower candidate throughput in all four pairings. For the data-only row,
A1/B1 was approximately 0.718 of the baseline throughput; within-version A1/A2
and B1/B2 repeat differences for that particular row were approximately 0.6%
and 0.5%. The complete cohort's failures still make that ratio unqualified.

The hotspot calls the generic `reconstruct_data` or `reconstruct_some` with
`Option<Vec<u8>>` shards. Static inspection found that the old generic recovery
feeds multiple output rows to one chunk-major coding loop, whereas the current
generic path completes destinations one at a time to preserve fallible
`ReconstructShard` publication semantics. The explicit GF8
`reconstruct_data_opt` path instead uses concrete `Option<Vec<u8>>` destinations
and can process the output rows together. A local instrumented behavior probe
with two missing shards confirmed two serial coding calls for the generic path
and one for the explicitly serial optimized path at 10+4/64 KiB and 32+16/1 MiB;
both paths reconstructed the exact same bytes. The [probe](../benchmarks/classic-recovery-analysis/2026-11-08/call-count-probe.rs)
can be copied into `examples/` and run with `--features benchmark-metrics,simd-accel`.
This is a work-structure observation, not proof of the cause or a measurement
of x86 performance.

`recovery_paths` now includes the 10+4 and 32+16, 1 MiB, missing `[0, 2]`
generic `Option<Vec<u8>>` and retained `ShardSlot` cases, plus a serial
`reconstruct_data_opt` control. Existing automatic-policy option cases retain
their previous adjacent-missing fixture. The added serial control forces one
job, so it tests bulk output behavior separately from parallel policy. Each
fixture verifies recovered bytes before and after measurement.

Example, when resources are reserved and the same binary and environment are
used for each control:

```sh
RSE_RECOVERY_CASE_FILTER='32x16/1048576/missing2+0' \
  cargo bench --locked --features simd-accel --bench recovery_paths
```

Default-feature and `simd-accel` smoke executions of the new probe succeeded on
macOS ARM64. Those non-isolated runs are functional checks only; they do not
qualify any performance claim about the Linux x86-64 diagnostic. A Linux
experiment must compare the generic and serial concrete controls with matched
fixtures, backends, build settings and independent baseline/repeat controls;
retain all samples and do not infer a cause from a failed stability gate.

Do not batch arbitrary generic `ReconstructShard` outputs solely to match the
concrete optimization: a destination may reject initialization or unwind, and
previously published slots must contain complete bytes. No runtime change or
release-evidence threshold was made in this follow-up.

## Native x86 screening

After the structural probe, a private Linux x86-64 guest experiment built the
same benchmark from `17c9a88232442342b96d5981d86d5f390e8aca70` with locked
Rust 1.99.1 and `simd-accel`. Runs pinned one guest CPU inside an eight-CPU
exclusive guest partition; the process monitor did not observe a competing
CPU-intensive task, and the partition was restored. Physical-host isolation
was not established. Each stage has three samples; the screens are not the
release campaign and should not be treated as a universal performance result.
The original stage rows and resource-monitor summaries are retained under
[results](../benchmarks/classic-recovery-analysis/2026-11-08/results/).

On the same binary and GFNI/AVX2 backend, the explicit serial Option path
changed elapsed time by -9.89% for 10+4/1 MiB and +64.09% for 32+16/1 MiB,
compared with generic `Option<Vec<u8>>` recovery. Same-path controls changed
+0.09% and +0.10%, respectively. All four stages in each fixture observed two
allocation requests. This comparison uses missing data indices `[0, 2]` and
does not isolate one kernel by itself.

A separate [rejected screening patch](../benchmarks/classic-recovery-analysis/2026-11-08/rejected-broad-cutoff.patch)
made the existing GFNI multi-row selector decline input counts above 16. The
remaining validated coding loop then performed the same computation. For
32+16/1 MiB serial Option recovery, a reverse-order repeat measured -51.61%
GFNI/AVX2 and -32.28% GFNI/AVX-512 against the unmodified selector; the
same-binary controls were -0.00% and +0.04%. Both baseline and candidate
repeat drifts were below 3% in that repeat. This is evidence for the tested
fixture, not for all inputs or processors.

The same broad cutoff was harmful for smaller Classic encoding. At 64 KiB,
32 inputs and four parity outputs on GFNI/AVX2, it measured +24.05% versus a
+1.10% same-binary control; at 1 KiB and 24+4 it measured +49.45% versus
-0.67%. At 65,537 B and 17+2 it measured +14.26% versus +1.52%. All respective
A and B repeat drifts were below 5% in those screening cells. These results
reject the all-size cutoff.

The 256/512 KiB and 1/2/4 MiB screens suggest a crossover, but several
controls or candidate repeats exceeded 5% drift, including individual 128 KiB
and larger GFNI/AVX-512 cases. They are retained as failures, not selected as
positive evidence. No cutoff is accepted from this single guest or from the
three-sample screens. The tested [grid probe](../benchmarks/classic-recovery-analysis/2026-11-08/gfni-grid-probe.rs)
compares output bytes with `encode_single_sep` before and after measurement and
asserts the requested backend. The earlier 1 MiB grid used a fixed 500 ms version;
the archived probe adds a configurable `GRID_MS` and is not claimed to be the
identical earlier source. Small, middle and large grids used 200 ms per sample;
the 128 KiB and 256/512 KiB reverse repeats used 800 ms. No release threshold,
production dispatch, or public API was changed by the rejected screen.
