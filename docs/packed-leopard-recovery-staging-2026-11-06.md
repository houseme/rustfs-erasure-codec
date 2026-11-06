# Packed staging for small Leopard reconstruction

Generic Leopard reconstruction decodes into private staging before publishing
bytes to `ReconstructShard` destinations. This is necessary because a later
destination can reject initialization and because decoder errors and unwinds
must not alter a missing destination. The existing inline path reserves one
slot per shard and therefore falls back to a heap allocation per requested
missing shard when `total_shards * shard_len > 2048`.

The candidate retains the existing inline fast path and adds a separate packed
path on standard-library x86-64 when there are at most 32 shards and
`parity_shards * shard_len <= 2048`. Only requested missing shards occupy the
private 2 KiB stack buffer. Publication continues in shard order from the
original staging storage even when the decoder reborrows output references. A
user-defined shard may report inconsistent state across validation and binding;
an overflow collection preserves the former behavior in that case. No public
API or on-disk representation changes. Other architectures and `no_std` retain
their existing implementation. The new runtime code adds no `unsafe`.

The runtime and diagnostic guard tests cover error and unwind behavior,
reborrowed outputs, partial publication after destination rejection, state
changes in custom shards, the allocation boundary and the existing dynamic
observer's heap-staging catalog. The allocation test counts the calling thread
after warming the codec; it is not a process-wide peak-memory measurement. An
[earlier native prototype allocation probe](../benchmarks/packed-recovery-staging/2026-11-06/prototype-allocations.json)
measured four requests totaling 512 B becoming zero for 16+8/128 B with four
missing shards, on GF8 and GF16. The final candidate's allocation test verifies
zero requests on the covered configurations; the prototype counts are not a
measurement of a different final binary.

## Performance

Measurements compare the same locked source and benchmark setup with the
baseline and candidate. Each stage runs a warmed preallocated fixture, verifies
its result, and records Criterion estimates and raw samples. Percentages are
`(B1 + B2) / (A1 + A2) - 1` using mean times; negative means lower latency.
The same-binary control uses the baseline in all four stages. The monitored
runs pinned a guest CPU and stopped their own benchmark process if another
process exceeded the configured CPU threshold. This does not establish physical
host isolation. No result was excluded for drift.

The native Linux build used rustc 1.99.1 and locked dependencies. In its
[40-sample recovery screen](../benchmarks/packed-recovery-staging/2026-11-06/native-screen.json),
16+8/128 B with four missing preallocated slots changed as follows:

| Backend | Field | Candidate | Same-binary control |
|---|---|---:|---:|
| GFNI/AVX2 | GF8 | -11.25% | -2.63% |
| GFNI/AVX2 | GF16 | -10.27% | -2.84% |
| AVX2 | GF8 | -13.00% | +0.09% |
| AVX2 | GF16 | -6.58% | -1.73% |

A [20-sample cross-compiled boundary screen](../benchmarks/packed-recovery-staging/2026-11-06/boundary-screen.json)
covered one and four missing shards, the exact 2 KiB bound, sizes above the
bound and 33 total shards. At 10+4/512 B with four missing slots, GF8 and GF16
changed -14.08% and -12.71% on GFNI/AVX2 and -14.22% and -9.32% on AVX2;
the corresponding same-binary controls were +0.16%, +3.97%, +0.02% and
-0.40%. The measurements are limited to the tested fixtures.

The [30-sample GFNI/AVX-512 screen](../benchmarks/packed-recovery-staging/2026-11-06/avx512-screen.json)
measured -11.01% for GF8 and -10.46% for GF16 on the same 16+8/128 B
fixture, versus same-binary changes of -1.01% and +0.16%, respectively. In
that screen, the 1 MiB GF16 slot and Option candidate changes were +4.40% and
+4.78%, versus -4.60% and -2.83% for their same-binary controls.
Four [alternating-order cross-compiled repeats](../benchmarks/packed-recovery-staging/2026-11-06/avx512-balanced.json)
measured a mean block change of +3.29% for slots versus -0.74% same-binary,
and -0.81% for Option versus +0.84% same-binary. Four
[alternating-order native repeats](../benchmarks/packed-recovery-staging/2026-11-06/native-avx512-balanced.json)
measured +0.13% for slots versus -1.28% same-binary, and -1.27% for Option
versus -0.28% same-binary. The cross-compiled slot signal remains a possible
regression for that build; the native run did not reproduce its magnitude.

The control runs also show measurement variability; one existing GF8 10+4/64 B
slot control changed +0.43% while its same-binary control changed -5.83%, and
the concrete Option control changed +2.35% versus -5.14%. Four independent [alternating-order native repeats](../benchmarks/packed-recovery-staging/2026-11-06/native-balanced-controls.json)
of these GFNI controls measured mean block changes of -0.05% for slots versus
-0.68% same-binary, and -1.98% for Option versus +0.26% same-binary. These
are averages of four block ratios, not pooled confidence intervals.

A separate [cross-compiled container screen](../benchmarks/packed-recovery-staging/2026-11-06/container-screen.json)
covered generic `(T, bool)` and `Option` destinations with 20 samples per
stage. On GFNI/AVX2, the 16+8/128 B tuple fixture changed -12.25% for GF8 and
-14.29% for GF16; the generic Option fixture changed -8.88% and -9.40%,
respectively. On AVX2 those changes were -7.29%, -4.71%, -5.49%, and -2.82%.
The measurements include allocation behavior belonging to those public APIs.

The separate [GF8/GF16 encoding controls](../benchmarks/packed-recovery-staging/2026-11-06/encode-screen.json)
showed variable timings, including +8.76% candidate and +3.40% same-binary
for GF16 10+4/64 B on GFNI/AVX2. The corresponding baseline and candidate
executables were byte-identical for each encoding probe; those timing changes
therefore cannot be caused by a change in those executable bytes. This does
not establish performance of other compiler or build configurations.

The [cross-compiled recovery repeat](../benchmarks/packed-recovery-staging/2026-11-06/cross-repeat.json)
contains a GF16 1 MiB candidate stage with a +18.65% aggregate change versus
-2.52% same-binary control, and an AVX2 Option stage with +16.32% versus
-0.06%. Each was driven by one slow stage; [independent repeats](../benchmarks/packed-recovery-staging/2026-11-06/cross-balanced.json)
did not reproduce those magnitudes. These observations remain in the record and do not
prove the absence of smaller regressions. Additional [cross-host screen](../benchmarks/packed-recovery-staging/2026-11-06/cross-screen.json)
and [default/no-std feature screen](../benchmarks/packed-recovery-staging/2026-11-06/feature-screen.json)
retain their own raw samples and controls. Cross-compiled binaries were not
assumed to have identical code generation to the native binaries.

## Compatibility

The all-feature workspace suite, Clippy, no-default-feature library tests,
Windows x86-64 cross-check, WASM no-std cross-check and generated x86 dynamic
observer compile check passed. On Linux x86-64 the all-feature library suite
passed 470 tests with five ignored. Scalar, AVX2, GFNI/AVX2 and GFNI/AVX-512
passed the new allocation, destination, and error-path tests. Published 9.0.0
versus 9.1.0 checks covered 200 configurations per backend plus existing
small and high-fanout cross-version cases; deterministic recovery contract
checks covered 20,000 cases per backend. The [summary](../benchmarks/packed-recovery-staging/2026-11-06/compatibility-summary.json)
records those runs. A 31-second seeded fuzz smoke test completed 655,018
executions on an ARM diagnostic build with the same algorithm enabled; it was
not an x86 coverage-guided fuzz run.

The separate-process [stack probe](../benchmarks/packed-recovery-staging/2026-11-06/stack-results.json)
compared baseline and candidate for 16+8/128 B on both GFNI/AVX2 and AVX2:
GF8 passed with 32 and 64 KiB stacks, GF16 with 512 KiB and 1 MiB stacks. This
is evidence for those fixtures and stack sizes only. Windows and WASM were not
run natively, and performance on untested processors is not established.

For reproduction, register [recovery-probe.rs](../benchmarks/packed-recovery-staging/2026-11-06/recovery-probe.rs)
as a temporary Criterion bench named `packed_recovery` in otherwise identical
baseline and candidate checkouts. Build each with the locked dependencies and
`simd-accel`, place the resulting executables as `baseline` and `candidate`
under `binaries-v5-native/packed_recovery`, and set `RSE_BENCH_ROOT` to the
directory containing `binaries-v5-native` before running [native-screen.py](../benchmarks/packed-recovery-staging/2026-11-06/native-screen.py)
[native-balanced-controls.py](../benchmarks/packed-recovery-staging/2026-11-06/native-balanced-controls.py),
or [native-avx512-balanced.py](../benchmarks/packed-recovery-staging/2026-11-06/native-avx512-balanced.py).
The probes assert the requested backend. The archived
[container probe](../benchmarks/packed-recovery-staging/2026-11-06/container-probe.rs)
can likewise be registered as a temporary Criterion bench. The
[contract probe](../benchmarks/packed-recovery-staging/2026-11-06/contract-probe.rs)
is intended to be copied into the checkout's `tests` directory, where its
relative path to the existing fuzz contract module resolves.
