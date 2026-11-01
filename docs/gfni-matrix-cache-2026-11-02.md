# Bounded GFNI matrix cache

GF16 GFNI multiplication builds four 8×8 matrices from each logarithmic
coefficient. Repeated small encodes and recoveries reuse many coefficients, but
previously rebuilt these matrices for every butterfly and fused conversion.
On the measured 16+8, 64 B workload, the matrix-building function accounted
for 55.66%–57.29% of candidate CPU-clock samples. Sampling proportions are
not an absolute-time measurement.

The GFNI path now uses 64 direct-mapped thread-local entries. Keys encode the
coefficient plus one, reserving zero for an empty slot. Each slot stores four
matrices; collisions recompute and replace only that slot. `Cell` keeps access
thread-local without locks or dynamic borrowing. Only the immutable canonical
GF16 tables can use the cache; any different table instance computes directly.
The cache contains coefficients and derived field constants, not shard bytes.
It has no per-entry heap allocation. In the tested x86 binary the TLS segment
increased by exactly 2,304 bytes per thread. That fixed cost exists when this
GFNI implementation is compiled in, including threads that never select GFNI.
Other architectures and `no_std` retain their prior paths.

## Performance

All runs compared the current main baseline with the candidate, built with the
same stable compiler, locked dependencies and source path. The Linux x86-64 VM
used one worker and a guest CPU partition under a shared lease, restored after
each run. Physical-host isolation was not verified. A1/B1/B2/A2 Criterion
screens used 20 samples, 0.5 s warmup and 2 s requested measurement; focused
controls used 40 samples, 1 s warmup and 5 s requested measurement. Reported
changes compare the mean of candidate phases with the mean of baseline phases;
negative means faster. Baseline drift is recorded and did not gate these runs.

The [shape probe](../benchmarks/gfni-matrix-cache/2026-11-02/shape-probe.rs)
selects the GFNI GF8 backend before running the GF16 codec. In its four-shape
screen, 64 B encoding improved 28.58%–68.54%, and 4 KiB improved 6.52%–15.95%.
For 1 MiB the results ranged from −3.80% to +0.90%. A longer two-shape control
measured 64 B improvements of 61.42% and 69.19%, with baseline drift of −9.50%
and −1.37%; the corresponding 1 MiB changes were −1.38% and −1.75%.

The [new-thread probe](../benchmarks/gfni-matrix-cache/2026-11-02/cold-thread-probe.rs)
starts 300 fresh threads per shape and phase. It times each thread's first
encode and then 32 same-thread encodes; fixture cloning, thread creation and
result verification are outside the timed regions. The codec and global tables
are initialized before threads are created. Comparing averages of the two
phase medians, first 64 B encodes improved 10.68% for 10+4 and 24.79% for
16+8; warm encodes improved 59.14% and 68.86%. This does not measure process
startup, thread creation cost or the additional TLS allocation cost.

For 10+4 and 16+8 GF16 recovery, the 64 B cache-miss cases improved
35.20%–46.18% and warm cases improved 39.38%–51.32%. An initial 10+4, 1 MiB
warm result was +5.25%, so a separate 40-sample follow-up was run: its four
1 MiB cold/warm cases ranged from −1.66% to −0.88%, with baseline drift from
−0.68% to +4.75%. One short-run baseline drift reached −7.13%. These are
host- and workload-specific observations, not universal performance bounds.

With the `rust-avx2` GF8 backend explicitly selected instead of GFNI, the
64 B recovery screen ranged from −3.55% to −1.63% and its 1 MiB recovery
cases from −1.75% to +1.06%; 1 MiB encoding ranged from −0.72% to +1.19%.
Selecting that backend disables eligibility for the GF16 GFNI butterfly, but
other GF16 operations still dispatch according to CPU capability.

[Stage estimates](../benchmarks/gfni-matrix-cache/2026-11-02/estimates.json),
[new-thread samples](../benchmarks/gfni-matrix-cache/2026-11-02/cold-thread-samples.json)
and [software-sampling summaries](../benchmarks/gfni-matrix-cache/2026-11-02/small-sampling.json)
are retained. The earlier `refcell64` and `entry64` entries are exploratory
variants, not the final key/value layout.

## Correctness and compatibility

The x86 all-feature library suite passed 458 tests with five ignored. GFNI
tests compare both butterfly directions with the scalar oracle for every
multiplier and basis vector and for every field value at selected multipliers.
Guarded tails cover the short and unrolled loop boundaries. Cache tests cover
coefficient collisions, both zero/sentinel keys, a modified noncanonical table
and concurrent thread-local use. The 1,708-pattern old/new recovery probe passed
with
scalar-rust, rust-avx2 and rust-gfni-avx2 selected for GF8. AArch64 std and
no-std, and WASM no-std, passed the same 1,708-pattern probe. The all-feature
workspace suite excluding the fuzz package and all-target/all-feature Clippy
passed on AArch64, along with 289 no-std library tests. The tested std recovery
and encoding `.text` sections matched builds of the final source; the tested
x86 AVX2 no-std `.text` matched the baseline.

An unpublished crate archive built from commit `1e26441` contained 66 Rust
source files matching its clean checkout byte for byte. Its extracted codec
passed the 1,708-pattern std probe on x86 with scalar-rust, rust-avx2 and
rust-gfni-avx2 selected for GF8, on AArch64 with rust-neon, and the no-std probe
on AArch64 and WASM. The extracted x86 crate also compiled with only
`std,simd-gfni` enabled and with all default features disabled.

## Release diagnostic follow-up

The preceding compact-locator change exposed a diagnostic contract that still
required a fixed 65,536-element GF16 array. The observer now describes the
locator argument as a borrowed view, without inventing an owner capacity; the
validator requires the current active prefix and the published baseline's full
array separately. GF8's inline-array contract is unchanged. A full disposable
source-copy audit completed with 516 workers and 6,192 observations across both
revisions, including the two GF16 decoder supplement cases for each revision.
The spelling configuration excludes only the recorded AArch64 disassembly
whose assembly mnemonic was misidentified as a typo. This diagnostic result
is not a release approval or performance measurement.

To rerun either standalone probe, use a disposable checkout, copy the desired
probe into `benches/gfni_matrix_probe.rs`, and add the following temporary
manifest entry:

```toml
[[bench]]
name = "gfni_matrix_probe"
harness = false
required-features = ["std"]
```

Then select the same backend and run:

```sh
RUSTUP_TOOLCHAIN=stable RSE_BACKEND_OVERRIDE=rust-gfni-avx2 \
  cargo bench --locked -p rustfs-erasure-codec --features simd-accel \
  --bench gfni_matrix_probe
```

Run baseline and candidate from the same source path in separate phases under
an appropriate resource reservation. The Criterion shape probe and the
new-thread probe have different output formats; the latter emits CSV rows of
shape, first-call nanoseconds and total nanoseconds for 32 warm calls.
