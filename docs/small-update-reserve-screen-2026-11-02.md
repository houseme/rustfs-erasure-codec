# Small incremental-update capacity check investigation

The default-off `experimental-small-update` feature uses a separate stack-backed
`SmallVec` for Classic updates of at most 256 elements when the element size is
at most three bytes. In that path `code_chunk_len` cannot exceed the original
shard length, so every chunk is at most 256 elements. `SmallVec::extend` already
reserves for its input iterator, while the inline vector already has capacity for
256 elements. The [candidate](../benchmarks/small-update-reserve/2026-11-02/candidate.patch)
removes the preceding `reserve_exact(old.len())` from that feature-gated helper.
The default, fast-one-parity and larger-shard source paths are unchanged.

This bound also applies when a safe custom `AsRef` later returns a longer view:
chunks are still limited by the previously calculated chunk length. Existing
update validation and the number and order of input and output view accesses
remain unchanged. On local AArch64 with no default features and the experimental
feature enabled, the baseline and candidate produced identical results, byte
arrays and access counts for 18 changing-view cases. They also produced identical
allocation request counts and byte totals in 840 warmed windows across four field
representations and the existing small-update test catalog. The
[view observations](../benchmarks/small-update-reserve/2026-11-02/view-observations.json)
and [allocation observations](../benchmarks/small-update-reserve/2026-11-02/allocation-observations.json)
are retained. Matching observed allocations is not a timing result or proof for
all custom fields.

## Measurement

The [Criterion probe](../benchmarks/small-update-reserve/2026-11-02/criterion-probe.rs)
checks forward and reverse updates against independent full encodes before
timing paired updates. It covers GF8 and tower GF16, 4+2 and 10+4, 64/256/257
and large controls; no-change, identical-input and 4+1 fast-parity controls are
also present. Fixture creation is outside the timed operations. In the candidate
comparison, both binaries enable `experimental-small-update`; the feature
comparison uses an unchanged default-feature baseline and the unchanged feature
implementation.

The x86-64 Linux VM runs used rustc 1.99.1, locked dependencies, one worker and
a guest CPU partition under a shared lease, with the selected GF8 backend
explicitly checked. Every completed comparison used A1/B1/B2/A2 ordering; the
partition was restored after each run. Physical-host isolation was not verified.
Initial screens used 20 Criterion samples, 0.5 s warmup and a requested 2 s
measurement per case. The two-backend focused repeat used 40 samples, 1 s
warmup and a requested 3 s measurement; the independent GFNI repeat requested
5 s per case. All percentages compare mean candidate time with
mean baseline time; negative is faster. Baseline drift is A2 / A1 − 1 and is
reported rather than used to exclude runs. Full
[stage estimates](../benchmarks/small-update-reserve/2026-11-02/estimates.json)
are retained.

With the existing experimental feature enabled versus disabled on the GFNI
backend, 64-element GF8 updates changed −30.43% for 4+2 and −20.60% for 10+4,
while the corresponding 256-element cases changed +6.79% and +7.54%. These
comparisons use separate binaries and do not isolate a single causal mechanism;
they are not a claim that enabling the feature is a universal improvement.
The feature remains default-off.

With the feature enabled in both binaries, the 40-sample GFNI repeat measured
256-element GF8 improvements of 7.03% for 4+2 and 6.10% for 10+4. The
corresponding AVX2 improvements were 9.94% and 6.61%. An additional independent
GFNI run measured an 8.79% improvement for 4+2, with −1.15% baseline drift.
These are two-update round trips, and the improvement applies to the tested
single-changed-shard workload rather than every update.

At 257 elements the inline helper is bypassed. In the first 40-sample GFNI
repeat, 4+2 GF8 changed +3.98% and 10+4 changed −2.00%; baseline drift was
+0.10% and −2.43%. An independent 4+2 GFNI repeat measured +0.47%, with
−0.02% drift. The AVX2 repeat measured −3.53% and −0.53%, with −4.57% and
+0.02% drift. Tower GF16 4+2 at 257 elements changed +3.18% in the first GFNI
repeat and −4.70% in the independent repeat; the latter had +11.29% drift.
These observations do not establish a consistent control-path change.
The GF8 4+2, 1 MiB controls changed −0.20% and −1.62% in the first GFNI and
AVX2 focused runs.

A separate, non-isolated AArch64 local screen used 20 samples, 0.5 s warmup and
2 s requested measurement. GF8 256-element changes were −0.56% and −1.05% for
4+2 and 10+4; 257-element changes were +0.48% and +2.74%. Baseline drift ranged
from +0.44% to +5.03%. This is a local diagnostic, not proof of a gain or
regression across AArch64 machines.

On the measured x86 benchmark, the GF8 helper's compiled size fell by 112 bytes
and the tower GF16 helper's by 133 bytes. With `experimental-small-update`
disabled, the baseline and candidate benchmark `.text` sections were identical.
Function sizes and code layout are compiler- and target-specific.

## Compatibility and validation

On AArch64, the all-feature workspace suite excluding the fuzz package,
all-target/all-feature Clippy, and the no-default-features library suite passed.
Release-mode small-update and incremental-update regression tests passed both
with default features and with no default features, with the experimental
feature enabled. Windows and Linux x86-64 all-feature checks and a WASM no-std
check compiled; Windows and WASM were not executed natively.

On native x86, the all-feature library suite passed 463 tests with five ignored;
the no-default-features library suite passed 290. Small-update and incremental
update regression tests and 18 changing-view comparisons passed with both
`rust-gfni-avx2` and `rust-avx2` selected. The published 9.0.0 compatibility
probe passed its 200 configurations, the additional 36 large GF8, 12 6+2 and
32 small verification cases, and the update resource report was checked on
AArch64 with the candidate feature enabled. These tests do not establish
performance or correctness for every field implementation or architecture.

An unpublished crate archive built from commit `e59febc` contained an
`src/core/encode.rs` byte-identical to that commit. The extracted crate passed
locked offline library checks on AArch64 with the experimental feature, both
with and without default features. Packaging used `--no-verify`; no crate was
published.
