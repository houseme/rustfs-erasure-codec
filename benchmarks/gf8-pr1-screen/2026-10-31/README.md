# GF8 PR 1: 6+2 candidate screen

The candidate in `6x2-candidate.patch` adds the 6+2 generated shape and extends
both architecture-specific scalar comparison tests. It is retained as an
experiment, not applied to the production dispatcher. The integration regression
and paired probe in the repository include 6+2 even when it takes the fallback.
The candidate can be applied to baseline `bf04fc49d65dd890b3d3b05968e7779f350de2b0`.

`arm64-unisolated.jsonl` is an exploratory A1/B1/B2/A2 run of serial encoding
on an Apple Silicon macOS machine, using `scripts/paired_encode_probe.rs` with
only 6+2 and 10+4 selected, `std` + `simd-accel`, and stable rustc 1.99.1.
The two snapshots had identical source except for the candidate and a temporary
package version change required to link both snapshots into one process. The
probe compares encoded bytes and verifies parity before and after timing. The
10+4 cases are controls; 6+2 is the target.

This run is **not performance acceptance evidence**. CPU resources were not
reserved, a separate Rust compiler was active, and the 10+4 control A2 median
changed by +14.01%, +9.18%, and -28.88% versus A1 at 1 KiB, 32 KiB, and
1 MiB respectively. This fails the 5% baseline drift gate. Stop attribution;
do not infer a speedup or regression from any case in this run.

For acceptance, repeat against the current main revision on isolated physical
hardware, record provenance, run A1/B1/B2/A2 and reject a run if either baseline
or candidate phase drifts beyond the agreed gate. Measure 6+2 and unchanged
controls across relevant sizes, serial encode and verification, plus actual
production concurrency. Repeat on native AVX2 before enabling x86. Check parity
against the generic path, custom matrices, and recovery interoperability.

The separate table-row and AVX-512 codegen proposals from
https://github.com/boylee1111/rustfs-erasure-codec/pull/1 remain unadopted:
they require native x86 functional and isolated performance evidence, including
the reported 10+4 recovery regression and comparison against the default GFNI
backend. SDE instruction counts alone are not a performance measurement.

Validation on this host: the candidate's NEON generated-kernel test passed,
including unaligned slices, edge coefficients, and short tails; the candidate
and unchanged production tree each passed all six
`parallel_codegen_regressions` tests. The candidate also passed a Linux x86-64
`cargo check` with `std simd-avx2`. The complete original PR applied cleanly to
this baseline and passed a Linux x86-64 `cargo check` with only
`std simd-avx512`; the unmodified baseline passed the same check, so the PR's
feature-gating changes are needed only together with its AVX-512 generator.
These cross-checks do not execute x86 instructions.
The original PR also cross-checked with `simd-avx512` in `no_std` and with
`std simd-avx2 simd-avx512`. Its `std simd-avx2`-only check emitted an unused
`__m128i` import warning in `table_rows.rs`; it must be gated to AVX-512 before
this feature combination can pass a warnings-as-errors build.
`avx2-only-import-fix.patch` is a minimal fix against the full PR, verified by
rerunning that exact cross-check with `RUSTFLAGS=-D warnings`; it is not needed
on the unmodified main branch.

## Native x86 functional follow-up

On an Azure x86-64 virtual machine exposing AVX2, AVX-512F/BW and GFNI, using
stable rustc 1.99.1, the 6+2 candidate's direct AVX2 scalar-comparison test and
parallel integration suite passed with `std simd-avx2`. The full PR plus the
AVX2-only import fix passed the direct AVX-512 generated-code and AVX2/AVX-512
row scalar comparisons with `simd-accel`, and the parallel regression suite.
With `std simd-avx512` alone, the direct AVX-512 codegen, row and existing
AVX-512 scalar tests also passed after fixing two pre-existing baseline test
compilation issues. The baseline's empty aligned-shard and AVX-512 scalar
and AVX2 cross-comparison tests passed with their respective feature sets.
The VM's physical host isolation has not been established; these are functional
results, not native performance acceptance evidence.

The full PR plus the local test and import fixes passed the all-features library
suite (443 passed, five ignored) and all eight encoding-dispatch regressions on
this VM. A baseline-versus-baseline serial AVX2 preflight pinned to one guest
vCPU is recorded in `x86-vm-baseline-aa.jsonl`: all nine cases had absolute
A1/A2 and B1/B2 median drift under 1%. Both snapshots had the same sources
apart from the package version needed to link them, and both included the test
compilation fix. The guest remained active with other users and the hypervisor
host's reservation status is unknown; low observed guest drift does not establish
physical isolation. The baseline-versus-candidate program was built but not run
for acceptance.

## Reserved-host follow-up

After the earlier functional tests, the user confirmed that the Azure physical
host was reserved for this work. The guest exposed 16 vCPUs on AMD EPYC 9V45;
the serial tests were pinned to vCPU 8. Stable rustc 1.99.1 and release builds
were used. The probe sources next to the measurements preserve each tested case
list; `std simd-avx2` was enabled, selecting `rust-avx2` without a backend
override. Both snapshots include the same test-only compile fixes; the
candidate additionally applies `6x2-candidate.patch` and uses package version
9.1.1 solely to link both snapshots in the same process. Each stage reports
three samples, 150 ms each; delta is the ratio of the mean of the two stage
medians. Both baseline and candidate phase drift must stay within 5%.

- `baseline-aa2.jsonl`: second baseline-versus-baseline preflight; all nine
  A1/A2 and B1/B2 case drifts were within 1.2%.
- `shape-abba.jsonl`: all nine cases passed the drift gate. For 6+2, latency
  rose 8.05% at 1 KiB and fell 11.77% at 32 KiB and 10.70% at 1 MiB. Existing
  10+4 and 4+2 shapes were included as controls. These results rule out an
  unconditional adoption for this measured workload; they do not establish
  behavior on other CPU families or in the application.
- `shape-threshold-abba.jsonl`: exploratory 6+2 measurements at 256 B through
  32 KiB; all case drifts passed. `x86-threshold-probe.rs` is the case list.
- `shape-gated-abba.jsonl`: evaluates `6x2-gated-experiment.patch` around a
  proposed 4 KiB threshold. **Rejected in its entirety**: candidate B1/B2
  drift at 8 KiB was -5.09%, outside the gate. No candidate performance
  conclusion or activation decision may be inferred from this run.
- `recovery-aa-{A1,B1,B2,A2}.jsonl`: all twelve AVX2 reconstruction
  baseline-only cases passed the 5% drift gate. The candidate comparison did
  not start: the session's effective cpuset subsequently changed from 0-15 to
  0-7, vCPU 8 became unavailable, and another benchmark process was active.
  No competing process was modified. A fresh resource reservation and baseline
  preflight are required before continuing recovery comparisons.

The experiments use warm serial microbenchmarks; verification, concurrency,
other CPU families, and end-to-end storage effects remain unqualified. No 6+2
or table-row production dispatch was activated on the basis of these screens.

## Recovery after competing workload finished

After the other benchmark exited, the guest's effective cpuset returned to
0-15 and four five-second observations showed no active competing benchmark.
The user had confirmed physical-host reservation. The AVX2 recovery probe was
restarted with the same 12 cases and vCPU 8.

- `recovery-aa2-{A1,B1,B2,A2}.jsonl`: baseline-only preflight **failed** the 5%
  gate because B1/B2 drift was +5.48% for 10+4, 32 KiB, one missing shard,
  retained slots. In B2 the three samples were about 4.91, 5.18, and 6.59 µs;
  the cause was not established.
- `recovery-aa3-*` and `recovery-aa4-*`: two subsequent baseline-only
  diagnostic runs with CPU8 counters, pressure and cpuset sampled at 200 ms
  passed the 5% gate. Their monitors and case timestamps are preserved.
- `recovery-abba2-*`: baseline-versus-full-PR comparison with the same
  monitoring **failed** the gate: candidate B1/B2 drift was +5.36% for that
  same case. Reject the whole comparison; do not infer any candidate speedup
  or regression from its case deltas. The monitor recorded the same 0-15
  cpuset throughout and no CPU8 steal, IRQ or softirq tick increments; this
  observation does not identify or exclude other sources of interference.

The inconsistent 10+4 result remains unexplained. Additional candidate attribution
requires a diagnosed and controlled environment, fresh baseline preflight, and
a new passing A1/B1/B2/A2 comparison. The 200 ms monitoring does not prove
that shorter interference did not occur.

A further baseline-only diagnostic ran the affected 10+4 retained-slots case
in 12 separate processes, with 150 ms samples. The median range was 4879.48 to
4908.94 ns (0.60% max/min spread), recorded in `recovery-repeat-baseline.jsonl`.
This narrows the observation to intermittent or workload-sequence-dependent
variation; it does not establish the cause of either failed run or make the
rejected candidate comparison valid. CPU8 frequency data was not exposed by
the guest's cpufreq sysfs. No new candidate comparison was run.

## Single-case AVX2 recovery screens and narrow candidate

The affected retained-slots case was also run in 12 separate candidate-only
processes (`recovery-repeat-candidate.jsonl`); medians ranged 4845.35-4873.58 ns
(0.58% max/min). A new single-case baseline preflight and separate ABBA both
passed, with a -0.54% latency change for that case. This does not rehabilitate
or replace the rejected full-sweep comparison.

For each of five additional cases, `recovery-single-sweep.py` ran a distinct
four-stage baseline preflight followed by a four-stage base-versus-full-PR
comparison, 150 ms per sample, three samples per stage, on guest CPU 8. Both
stage drift gates passed in each. The observed latency changes were +6.50% and
+3.77% for 4+2 retained-slot recovery (16 KiB, one and two missing data),
-9.44% and +1.26% for 10+4 `reconstruct_data_opt` (1 MiB, one and two
missing data), and +0.28% for 32+16 selective parity reconstruction (64 KiB).
These are CPU- and workload-specific observations; they support rejecting the
full PR's broad activation on this machine.

`avx2-row-narrow-candidate.patch` is an **experiment**, not production code.
It applies the PR's table row machinery only to the AVX2 backend, 10+4, one
output and shard sizes at least 1 MiB. `recovery-narrow-sweep.py` reran the
same five single-case preflights and comparisons; all drift gates passed. The
corresponding changes were -4.11%, -4.38%, -9.10%, +0.44%, and -0.35%.
The 4+2 cases cannot select the new kernel but changed by about 4%, so binary
layout and other effects complicate causal attribution. The only measured
large benefit is one particular large-shard degraded-recovery case on one CPU.
The default `simd-accel` backend on this host is GFNI, so the AVX2-only
candidate does not target that default. Production workload frequency, other
shapes, parity rebuild, concurrent load and other CPU families need evidence
before accepting the extra kernel and dispatch maintenance.

The narrow candidate's `narrow-row-interop.rs` passed with `std simd-avx2` and
a forced AVX2 backend, covering three matrix modes, three shard sizes around
1 MiB and three missing data positions. Its all-features library suite passed
441 tests with five ignored. This confirms those tested behaviors, not every
possible reconstruction layout or production workload.

## AVX-512 encode preflight

The full PR and baseline were built separately with `std simd-avx512`, and the
probe verified that both selected `rust-avx512`. The first baseline-only case,
4+2 at 1 KiB, had B1/B2 drift +8.14%; the sweep stopped before running any PR
candidate timing. The four original samples and sweep script are saved as
`avx512-encode-sweep-*`. The reason for the drift is unknown and no AVX-512
performance claim follows. Do not repeat solely to discard this failed run;
diagnose or control the variability before a new preflight.

A 12-process baseline diagnostic of that 4+2, 1 KiB AVX-512-only case showed
92.45-107.87 ns medians (16.68% max/min); its reason remains unknown. A
separate 10+4, 32 KiB baseline diagnostic had 2.87% spread. Raw results are
`avx512-repeat-baseline*.jsonl`.

The separate `avx512-large-sweep` compared baseline and PR with only
`std simd-avx512`, with a fresh per-case baseline preflight and ABBA; all
reported stage drifts passed 5%. The PR latency changed by -38.35% for 4+2 at
32 KiB, -8.37% and -14.78% for 10+4 at 32 KiB and 1 MiB, and -6.70% for
12+4 at 1 MiB. This compares generated AVX-512 against the generic AVX-512-only
baseline, not against the common AVX2-generated or GFNI configuration.

`gfni-vs-avx512-sweep` compared baseline `simd-accel` auto-selected
`rust-gfni-avx512` against the PR with `std simd-avx512` only and auto-selected
`rust-avx512`. The same four cases passed their individual preflights and ABBA
gates; AVX-512-only PR latency relative to GFNI baseline changed by -18.01%,
+17.98%, +37.86%, and +36.13%, respectively. These builds use different
features and code layout, so this is a backend/configuration comparison, not an
isolated estimate of PR impact on an unchanged configuration.

A dual `std simd-avx2 simd-avx512` baseline automatically selected `rust-avx2`,
as recorded in `dual-encode-sweep-0-aa-A1.jsonl`; the comparison script
intentionally stopped because it required `rust-avx512`. Current automatic
priority prefers GFNI first and AVX2 over AVX-512, and explicit backend
selection disables the whole-codeword generator. Consequently, with `std`, the
PR's AVX-512 generated path primarily applies to AVX-512-only builds; it is not
a generic automatic speedup on AVX2+AVX-512 or GFNI deployments. Changing
backend priority would be a separate decision and is not justified by these
mixed results.

## Reduced AVX-512 codegen candidate

`avx512-narrow-candidate.patch` is a separate, unapplied experiment. It adds
generated AVX-512 encode for 4+2, 10+4 and 12+4 only, and in AVX-512-only
builds avoids native generation below 32 KiB, including no_std. It preserves
existing GFNI and AVX2 auto priority. The exact candidate passed the native
AVX-512 scalar comparison, 440 all-features library tests (five ignored), and
an AVX-512-only no_std `cargo check`.

The `avx512-narrow-sweep-*` data contains fresh independent per-case preflight
and ABBA results. The 4+2/32 KiB comparison passed its phase drift checks, but
10+4/32 KiB candidate B1/B2 drift reached +5.04%, so the sweep stopped and
that comparison is invalid. No production activation decision follows from
this incomplete screen; diagnosis and a new passing experiment are needed.

## Adopted AVX-512-only encode path

The user confirmed the reserved host and a later idle window. The previous
baseline `bf04fc4` and current `fc6795c` differ only in GF16 experiment
artifacts, not runtime source. The final implementation uses three generated
AVX-512 shapes (4+2, 10+4 and 12+4), a 32 KiB minimum shard size for
AVX-512-only builds, and an out-of-line native preparation path for that
configuration. Existing AVX2/GFNI priority is unchanged. Small sizes and other
shapes retain the generic path. Original PR table-row kernels and 6+2 dispatch
are not included.

The original `recovery_paths` benchmark called `rayon::current_num_threads()`
during setup. With the process pinned to a single vCPU, inspection showed a
second thread on that vCPU. A serial-only diagnostic removed the pool query
(`serial-recovery-probe.patch`); inspection then showed one thread, but the
12-process 1 KiB baseline still varied by 19.21%. Thus pool presence was not
established as the cause of the original drift. The standalone same-process
paired probes avoid initializing that pool for serial work. Baseline and
candidate are compiled as two dependency aliases; the second uses a temporary
package-version change to allow linking. The paired probe checks byte equality
and verification before and after timing, and reports three 150 ms samples per
phase. All candidate results below passed the 5% A1/A2 and B1/B2 drift gates,
after separate same-process baseline-versus-baseline preflights. The 1-vCPU
measurements used CPU 8; the 4-vCPU parallel measurements used CPUs 8-11 and
four Rayon workers. Resource checks before and after required cpuset 0-15 and no
`leopard_recover` process.

`paired512-small-v3-*` covers 256 B through 32767 B, and
`paired512-large-v3-*` covers 8192 B, 32767 B, 32768 B, 32769 B and 1 MiB.
For eligible sizes, serial latency changed by approximately -37% to -11% for
4+2, -10% to -14% for 10+4, and -6% to -7% for 12+4. Below threshold, the
observed largest positive serial delta was +2.19% for 4+2 at 8192 B; the small
probe's positive deltas were at most +1.19%. These remain single-machine warm
microbenchmarks; differences can include binary layout and do not establish
storage-level gains or behavior on other CPU families.

`paired512-par-*` covers the parallel API at 32 KiB and 1 MiB. At 32 KiB the
observed latency deltas for 4+2, 10+4 and 12+4 were -39.59%, -6.24% and
-4.91%; at 1 MiB they were +0.49%, -1.14% and -0.45%. Both phase drift gates
passed. The existing parallel scheduler chooses the execution route; these
numbers are API-level measurements, not a claim that all jobs ran the new
kernel.

On physical AVX-512-capable x86 instructions in the reserved VM, the final
worktree passed the direct generated-kernel scalar test and two integration
tests, including a child-process auto-dispatch assertion immediately below,
at and above 32 KiB. The tests cover the three shapes, three matrix modes,
serial and parallel entry points, and reconstructing a missing shard. The
all-features library suite passed 442 tests with five ignored; AVX-512-only
Clippy all-targets with warnings denied passed. The `no_std` AVX-512 library
suite also passed 279 tests with compile-time `avx512f,avx512bw` enabled.
These results do not cover all custom matrices or every CPU implementation.
