# GF16 compact recovery plan follow-up

The [previous experiment](gf16-compact-plan-2026-11-01.md) found that keeping
only the used error-locator prefix greatly improved small cache-miss recovery,
but a separate large-shard encoding control regressed. This follow-up traces
the regression and evaluates a combined candidate: compact std recovery plans
and a two-block GFNI butterfly loop. The original full transform remains the
fallback. `no_std` retains the original full array. Public APIs, recovery
publication, shard bytes and plan-cache keys are unchanged.

## Investigation

On the tested x86-64 VM, `perf` software CPU-clock sampling of 10+4, 1 MiB
encoding at 499 Hz showed the inverse GFNI butterfly accounting for
33.80%–36.42% of baseline samples versus 45.55%–45.73% of compact-plan candidate samples.
Hardware counters were unavailable. The two butterfly functions had the same
normalized disassembled instructions and size, but different addresses: the
hot loop began at offsets 32 and 0 respectively within a 64-byte line.
Sampling proportions alone do not measure absolute function time.

The large-only control still reproduced the 1 MiB regression when earlier
shard sizes were removed; the 10+4 user buffers had matching observed offsets
within a 2 MiB range during the first three phases. Separately, forcing all
functions to 64-byte alignment for both experimental binaries reduced the
candidate's 1 MiB differences to +0.41% and −2.74%; this changes the layout of
all functions and is not a production build option. A diagnostic-only inline
assembly experiment moved the hot loop to offset 16 and produced +1.05% and
+0.84% relative to the unmodified baseline. These results support a layout
sensitivity on this compiler and host; they do not isolate a specific hardware
mechanism or predict behavior on other CPUs. Neither alignment experiment ships.

A Rust loop uses the existing single-block strategy for fewer than four vector
blocks and processes two 16-element blocks per iteration for longer inputs,
with a single remaining vector block and the existing scalar tail. The shared
block operation is kept in one closure to avoid duplicating the GFNI computation.
The existing GFNI runtime dispatch remains intact. An earlier unconditional
unroll showed a repeatable +6.00% 16+8, 64 B encoding result in a focused
control. The short-vector branch avoided it in this VM's follow-up: a
40-sample pure-encoding run measured −2.43% and −11.90% for 64 B, and +0.65%
and +2.86% for 1 MiB, compared with the original baseline. The 1 MiB baseline
drift was +1.16% and −1.22%, respectively. These are observations for this
binary and VM, not a guarantee for all link layouts or CPUs.

## Recovery results

The std plan stores only indices below `work_size`, using an inline capacity of
256 values; larger prefixes use heap storage. Small cached locator capacity
therefore falls from 128 KiB to 512 bytes per entry, excluding metadata and FFT
schedules. The 16-entry cache and its key remain unchanged. In the tested x86
release recovery binaries, `decode_plan_for`'s static frame changed from
`0x60000 + 0x368` bytes to `0x40000 + 0x4c8` bytes, excluding saved registers
and called functions. This is a compiler- and target-specific observation, not
a portable stack bound.

In the earlier unconditional-unroll A1/B1/B2/A2 screen, 64 B cache misses
improved 49.30%–65.01%,
and warm-cache cases improved 11.43%–18.79%. For 10+4 with four missing shards,
the cache-miss mean changed from 7.95 µs to 2.78 µs. The 1 MiB 10+4 cold
baseline changed by −44.26% between A1 and A2, so that case is not attributable
from the screen. A separate 40-sample follow-up for 10+4 and 16+8 with four
missing 1 MiB shards produced −0.69% to +3.08% across cold and warm cases;
the +3.08% case had +4.31% baseline drift. Other 1 MiB changes in the initial
screen ranged from −2.03% to +1.58%.

With `rust-avx2` selected for that earlier candidate instead of GFNI, the
64 B cache-miss cases improved
53.93%–67.13%, and warm cases improved 3.12%–9.42%. The initial 16+8,
1 MiB cold case regressed 25.53% with −1.05% baseline drift, while its warm
case changed +1.16%; the 10+4 cold case had −33.99% baseline drift. A separate
40-sample follow-up did not repeat that large difference: the four 1 MiB
cold/warm cases ranged from +0.47% to +2.45%, with baseline drift from +0.74%
to +1.15%. However, baseline absolute time for 16+8 cold also differed by
roughly 23% between the two runs, so this instability remains an important
limitation. In a third, software-sampled run the same case changed +2.20%;
its baseline and candidate shared the same dominant AVX2 butterfly and XOR
hotspots. Sampling overhead means this is supporting evidence, not a substitute
for an uninstrumented control.

For the final short-vector branch, the GFNI recovery screen measured 64 B
cache-miss improvements of 52.71%–68.55% and warm improvements of
8.05%–15.14%. The 10+4, four-missing 64 B miss changed from 8.83 µs to
2.78 µs, with −14.65% baseline drift. In the same screen the 16+8, four-missing
1 MiB cold result was +13.99%, with +31.21% baseline drift. A 40-sample
follow-up for 10+4 and 16+8, four missing 1 MiB shards measured +0.01% to
+1.10% across cold and warm cases; baseline drift ranged from −0.83% to +3.23%.

The final `rust-avx2` screen measured 64 B cache-miss improvements of
54.28%–68.19% and warm improvements of 4.78%–8.38%. Its six 1 MiB recovery
cases changed by −1.84% to −0.72%; the largest baseline drift in those cases
was −2.30%. The earlier unexplained cold-path variability remains a limit on
how broadly these results can be generalized.

The [original threshold probe](gf16-compact-plan-2026-11-01.md) exercised the
compact locator around 256 entries and 16 terms. Correctness of the combined
candidate is checked at both boundaries. All percentages are host- and
workload-specific and are not guarantees against smaller regressions elsewhere.

## Validation

The combined candidate passed the AArch64 all-feature workspace suite excluding
the fuzz package, all-target/all-feature Clippy, and 289 no-std library tests.
Two bounded std cross-version fuzz runs completed 10,408 and 9,186 iterations.
On x86, 456 all-feature library tests passed with five ignored. With GFNI and
AVX2 detected, the GFNI
butterfly passed the guard/tail tests (including 47–49 and 63–65 elements),
all field values and every multiplier/basis vector. The std old/new
1,708-pattern recovery probe passed on x86 with scalar-rust, rust-avx2 and
rust-gfni-avx2 selected for the GF8 backend, and on AArch64 with NEON. The GF8
selection affects GFNI eligibility; it does not force every GF16 operation to
use that backend. The final no-std WASM probe passed the same corpus; the
prior compact-plan candidate passed it on native AArch64. The final x86 AVX2
no-std benchmark `.text` matched baseline.

An unpublished crate archive built from commit `a7698a1` contained 66 Rust
source files matching its clean checkout byte for byte. The extracted archive
passed the 1,708-pattern cross-version probe on AArch64 with `rust-neon`, on
AArch64 no-std, and on WASM no-std.

The final GFNI recovery and encoding benchmark `.text` sections matched builds
from the final source byte for byte. All benchmarks used locked stable builds
at the same source path, one worker, A1/B1/B2/A2 ordering and a guest CPU partition under a shared lease. Screens used
20 samples, a 0.5 s warmup and 2 s requested measurement; long controls used
40 samples, a 1 s warmup and 5 s requested measurement. Baseline drift is reported and was not used as a gate; physical-host isolation was not
verified. Raw [stage estimates](../benchmarks/gf16-compact-plan/2026-11-02/estimates.json)
and software-sampling summaries for [GFNI](../benchmarks/gf16-compact-plan/2026-11-02/software-sampling.json)
and [AVX2](../benchmarks/gf16-compact-plan/2026-11-02/software-sampling-avx2.json)
are included. Experiments marked as exploratory are not production code.
