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
