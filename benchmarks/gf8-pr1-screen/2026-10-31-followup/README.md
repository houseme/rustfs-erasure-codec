# GF8 PR 1 follow-up: 6+2 and bounded reconstruction

The baseline is commit `fc6795c02f014ed380273c002f46d582a0e9b8d3` plus
`baseline-worktree.patch` (SHA-256
`2f6b7c98ba4247be60e13b396157226c4ba80bf9dc894280451d0ebcc6e93e18`).
The untracked baseline test is saved as `baseline-avx512-interop.rs`. This
snapshot includes the previously adopted AVX-512-only generated encoder; it is
not the historical `bf04fc4` baseline of the previous screens.

`6x2-avx2-candidate.patch` (SHA-256
`9135681a04e41a2f6cd0e80a3e616047c29b22eab4b36f33a9cd4204545abde1`)
is an unapplied experiment. It generates 6+2 for AVX2 only, retains fallback
below 4096 B, and leaves NEON and AVX-512 6+2 dispatch unchanged. On native
AVX2, the direct generated-kernel/scalar test and all five parallel-codegen
integration tests passed; AVX2-only no_std `cargo check` passed. These checks
do not establish a performance benefit or cover every custom matrix.

All screens used the user-confirmed reserved Azure host, AMD EPYC 9V45, stable
rustc 1.99.1, and guest vCPU 8. The paired serial probe used `std simd-avx2`,
three 150 ms samples per phase, byte comparison and verification, and same
process dependency aliases. The second identical copy uses a temporary Cargo
version change to link both copies.

- `shape-aa.jsonl` is a baseline-versus-baseline paired preflight. All A1/A2
  and B1/B2 phase drifts were below 5%, but same-source A/B medians differed by
  +7.84% at 6+2/4096 B. The preflight rejected the setup under the separate
  5% same-source layout-bias gate. This could be a code-layout effect; the
  measurements alone do not establish its cause. No candidate paired screen
  was run.
- An independent process-per-stage probe (`shape-single-sweep.py`) passed at
  6+2/256 B, but its 6+2/1024 B baseline B1/B2 drift was +9.09%; it stopped
  before running that candidate case. The failed preflight is retained in
  `shape-single-*`.
- Twelve baseline-only processes with normal and per-process disabled address
  randomization are in `shape-aslr-normal.jsonl` and `shape-aslr-fixed.jsonl`.
  Their max/min median spreads were 4.06% and 3.88%; this does not identify
  address randomization as the cause and does not validate the failed screen.
- A separate 4 KiB-and-above single-case screen passed the 4096 B preflight and
  comparison. The next case, 8192 B, had A1/A2 drift +6.12%; the sweep stopped
  immediately. The failed candidate comparison and remaining cases must not be
  used for performance attribution. Results and script are `shape-large-*`.

Because the A2 drift gate failed, no performance conclusion or production
activation follows from this follow-up. Do not repeat the candidate screen to
shop for a pass: diagnose and control the variation, then conduct a new
baseline preflight and A1/B1/B2/A2 comparison on the exact current source.

For the separate bounded AVX2 row candidate, the prior
`avx2-row-narrow-candidate.patch` was applied to the current baseline only in a
temporary copy. The threshold and three-matrix reconstruction tests, direct
AVX2 scalar row test, and `std simd-avx2` all-target Clippy with warnings denied
passed. `narrow-row-dispatch.rs` checked a custom GF8 field below and above
1 MiB; it verifies actual row selection for the generic reconstruction API.

`row-execution-instrumentation.patch` is test-only instrumentation, not a
production change. On four available vCPUs, native AVX2, 10+4, one missing data
shard and 1 MiB per shard, `reconstruct_data_opt` invoked the candidate row once
with `max_parallel_jobs(1)` and zero times with `max_parallel_jobs(4)`; both
reconstructed the correct bytes. The parallel policy takes a separate coding
path. The earlier 1-vCPU microbenchmark gain therefore does not demonstrate a
gain for this four-job path, and does not justify the additional maintenance
or production activation on its own. No table-row production code was added.
