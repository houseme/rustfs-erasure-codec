# Bounded staging for small Leopard recovery

Generic Leopard recovery stages decoded bytes before publishing them to
`ReconstructShard` destinations. This preserves existing behavior when a
later destination rejects initialization and keeps missing-slot backing bytes
unchanged if decoding returns an error or unwinds. A direct write into those
slots would violate that behavior.

The final candidate keeps staged publication but uses a private, zero-initialized
2 KiB stack buffer for standard-library x86-64 calls when there are at most 32
total shards and `total_shards * shard_len <= 2048`. The retained
[zero-context patch](../benchmarks/small-recovery-staging/2026-11-02/x86-final.patch)
can be checked against baseline `3a415cd` with `git apply --unidiff-zero --check`.
Each shard has a disjoint slice;
only required missing shards receive outputs. The decoder finishes before any
destination is initialized, and publication retains the existing order and
partial-publication behavior. Larger calls keep the heap staging path. The
`no_std` and non-x86-64 builds do not use the stack buffer.

The candidate also retains the original output selection if a decoder changes
its output slice references: publication reads the private staging storage and
recorded request mask, rather than trusting the returned references. Fault
injection checks decoder errors, unwinds, output reborrowing and a later
initialization rejection. Boundary checks cover 2 KiB, the first larger size,
32 versus 33 shards and selective recovery. A cross-version test compares full
parity and generic `ShardSlot` recovery against the published 9.0.0 codec for
GF8 and GF16 across those boundaries.

A [calling-thread allocation probe](../benchmarks/small-recovery-staging/2026-11-02/allocation-probe.rs)
measured warmed native x86 runs with GFNI and AVX2 selected. For both Leopard
families, 10+4 at 64 B with four missing outputs changed from four requests
totaling 256 B to zero requests; 16+8 at 128 B remained four requests totaling
512 B because it is outside the stack bound. These are request counts and
requested bytes, not peak live memory or latency. [All observations](../benchmarks/small-recovery-staging/2026-11-02/native-allocations.json)
include one- and four-missing cases and the 33-shard control. Earlier
[AArch64 counts](../benchmarks/small-recovery-staging/2026-11-02/allocations.json)
were taken with an all-`std` prototype and do not describe the final x86-only
activation.

## Performance

The [screen](../benchmarks/small-recovery-staging/2026-11-02/screen-probe.rs)
and [focused probe](../benchmarks/small-recovery-staging/2026-11-02/focused-probe.rs)
use preallocated `ShardSlot` buffers, mark missing slots on each iteration and
check recovered bytes before measurement. The focused probe additionally
checks intact inputs and the separate concrete `Option<Vec<u8>>` API. The
[no-std probe](../benchmarks/small-recovery-staging/2026-11-02/nostd-probe.rs)
links the codec without default features; the benchmark executable itself uses
`std` and Criterion.

On the Linux x86-64 VM, tests used rustc 1.99.1, locked dependencies, one worker,
a guest CPU partition and a shared lease. Each A1/B1/B2/A2 run restored the
partition; physical-host isolation was not verified. Initial screens used 20
samples, a 0.5 s warmup and a requested 2 s measurement per case. Focused runs
used 40 samples, a 1 s warmup and a requested 3 s measurement; the independent
large-control repeat requested 5 s. Percentages compare mean candidate time
with mean baseline time, so negative means faster. Baseline drift is A2 / A1 − 1
and is reported without excluding runs. The full
[stage estimates](../benchmarks/small-recovery-staging/2026-11-02/estimates.json)
are retained.

An exploratory non-isolated AArch64 comparison replaced the fixed `[0u8; 2048]`
with `SmallVec<[u8; 2048]>::new()` followed by `resize(active_bytes, 0)`.
It changed the 4+2/64 B GF8 recovery case by +6.34%, with +1.41% drift, so
that variable-clear variant was not retained.

The first inline prototype made 10+4/64 B GF16 intact validation about 35%
slower on both x86 backends. Separating post-validation staging into a
non-inlined `std` helper removed that large regression in the tested binary:
the focused GFNI and AVX2 intact changes were +1.03% and +1.20%, respectively.
On the same focused runs, 10+4/64 B with four missing shards improved 18.54%
(GF8) and 8.99% (GF16) on GFNI, and 18.34% and 6.84% on AVX2. At 128 B the
corresponding improvements were 16.97% and 10.02% on GFNI, and 15.91% and
6.84% on AVX2. The separate Option control at 64 B ranged from −3.16% to
+2.23% across the two fields and backends.

For 10+4/1 MiB GF16, which uses heap staging, the first GFNI focused run changed
+4.31% with +0.97% drift; an independent repeat changed −2.04% with −1.06%
drift. Its Option control changed +3.04% and −1.83%, respectively. These
results do not establish a consistent large-control change.

The `no_std` x86 binary changed code layout despite retaining heap staging.
The initial GF16 run had baseline drift up to +21.92%. A 40-sample focused
repeat measured 64/128 B and 1 MiB GF16 recovery changes from −1.18% to
−0.11%, and intact validation +0.05%; drift ranged from −2.79% to +2.71%.
A local, non-isolated AArch64 no-std screen had much larger variation and is
retained as diagnostic evidence only.

The earlier all-`std` prototype was also screened on non-isolated AArch64. Its
intact GF16 control changed +18.30% (about 1.3 ns), and a separate 40-sample
repeat changed +9.54% (about 0.7 ns), with −2.13% drift. It was not retained
for that architecture. The final x86-only candidate uses heap staging on
AArch64; a local 40-sample control measured intact GF16 +1.42% with +1.89%
drift and intact GF8 −0.14%. These non-isolated observations do not establish
performance on other machines.

The final x86-only candidate's standard-library and `no_std` x86 benchmark
`.text` sections matched the corresponding measured split-prototype binaries
byte for byte. This binds the x86 timings to the final runtime code; it does not
prove identical generated code on other targets. Existing diagnostics still
observe the heap-staging helper for their current Leopard catalog, and a test
prevents silently adding an inline-staging case without extending that observer.
The generated dynamic observer source also passed an AArch64 compile check; the
full dynamic audit was not run.

## Compatibility and stack checks

The final candidate passed the all-feature workspace suite excluding the fuzz
package, Clippy, and no-default-features library and boundary tests on AArch64.
Native x86 GFNI and AVX2 runs passed the boundary and destination-rejection
suites, the all-feature library suite and old/new Leopard cross-recovery test;
Windows x86-64 and WASM were cross-checked but not run natively. An all-`std`
prototype with the same x86 staging algorithm completed 343,370 seeded
`fuzz_codec_contracts` executions in 16 seconds without a reported failure.
The [summary](../benchmarks/small-recovery-staging/2026-11-02/fuzz-summary.json)
is retained. A later [Rust seed generator](../benchmarks/small-recovery-staging/2026-11-02/fuzz-seeds.rs)
reconstructs the same 72-input catalog; it was not the generator used in that
historical run. This is a bounded smoke run, not exhaustive fuzzing of the final build.

The [stack probe](../benchmarks/small-recovery-staging/2026-11-02/stack-probe.rs)
uses separate subprocesses to compare new threads. On native x86 with both
GFNI and AVX2 selected, baseline and candidate passed the tested 32 KiB GF8 and
512 KiB GF16 stacks. Both versions overflowed the tested 128 KiB and 256 KiB
GF16 stacks. On AArch64, both passed 32 KiB GF8 and 256 KiB GF16, and both
failed 128 KiB GF16. These are specific thread sizes and fixtures, not a bound
on every call stack; the baseline GF16 overflow is pre-existing in this probe.
The [native observations](../benchmarks/small-recovery-staging/2026-11-02/native-stack-results.json)
and [AArch64 observations](../benchmarks/small-recovery-staging/2026-11-02/aarch-stack-results.json)
are retained.

An unpublished archive built from commit `af81469` contained a
`src/core/reconstruct.rs` byte-identical to the committed source. The extracted
crate passed locked offline AArch64 library checks with all features and with
no default features. Packaging used `--no-verify`; no crate was published.
