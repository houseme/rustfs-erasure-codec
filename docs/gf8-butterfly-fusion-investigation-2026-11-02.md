# GF8 butterfly fusion investigation

Leopard GF8 radix-2 butterfly stages perform a lookup-XOR pass and an ordinary
XOR pass over the same lanes. The prototypes fuse both operations into one
AVX2 pass using the existing nibble tables. Forward and inverse order remains
distinct; the sentinel coefficient and unsupported CPUs use the original path.
The candidate paths require `std`, x86-64, AVX2, equal slice lengths, and at
least 64 bytes. No public API or field representation changes are involved.

A software CPU-clock profile of warmed GF8 10+4/64-byte recovery with four
missing shards attributed 20.42%–21.43% of samples to `slice_xor_avx2`,
14.90%–16.32% to `lut_xor_avx2_prebuilt`, and 6.97%–7.41% to the XOR wrapper
across two stages. Other operations also call XOR; these proportions are not
an estimate of what the butterfly fusion can remove. No samples were lost.
The [sampling summary](../benchmarks/gf8-butterfly-fusion/2026-11-02/sampling-summary.json)
records the observed shares and GF16 function sizes.

## First prototype and compiler interaction

The [initial prototype](../benchmarks/gf8-butterfly-fusion/2026-11-02/initial.patch)
reduced GF8 recovery but increased GF16 10+4/64-byte recovery by 13.79% in a
20-sample GFNI screen and 15.62% in an independent 40-sample screen. The latter
baseline drift was −0.72%. A GF16 CPU-clock profile showed the combined
forward/inverse GFNI butterfly self-sample share increasing from approximately
13%–14% to 22%–23%. The baseline compiler inlined the GF16 matrix cache lookup
into the GFNI butterfly; in the initial prototype that lookup became a call.
This is consistent with a code-generation effect, although sampling alone does
not prove that the extra call caused the entire latency difference.

A [forced-inline revision](../benchmarks/gf8-butterfly-fusion/2026-11-02/forced-inline.patch)
removed the large GFNI GF16 recovery regression in subsequent tests, but GF16
4+2/128-byte AVX2 encoding changed +6.40% in a 40-sample repeat, with −1.56%
baseline drift. A GF16 encoding profile did not identify a single function
responsible for that change. A [regular-inline revision](../benchmarks/gf8-butterfly-fusion/2026-11-02/inline.patch)
measured GF16 AVX2 10+4/64-byte recovery with four missing shards at +2.73%,
+3.41% and +4.93% in three independent comparisons; baseline drift was −1.28%,
+3.42% and +2.62%. The visible GF8 gains therefore do not by themselves justify
accepting those variants as general improvements.

## Measurement

Native comparisons used locked dependencies, Rust stable 1.99.1, one worker,
and A1/B1/B2/A2 execution on an x86-64 Linux VM. A shared lease and exclusive
guest CPU partition were held for each run and restored afterwards; physical
host isolation was not verified. Initial screens used 20 Criterion samples,
0.5 s warmup, and a requested 2 s measurement per case. Focused comparisons
used 40 samples, 1 s warmup, and a requested 3 s measurement. Percentages compare
the mean candidate and baseline latencies; negative is faster. Baseline drift
is A2/A1−1 and did not stop runs at the user's request.

The retained [recovery](../benchmarks/gf8-decode-assign/2026-11-02/recovery-probe.rs)
and [GF8 encoding](../benchmarks/gf8-decode-assign/2026-11-02/encode-probe.rs)
probes validate outputs before timing. A [GF16 encoding probe](../benchmarks/gf8-butterfly-fusion/2026-11-02/encode-gf16-probe.rs)
uses the same structure. Full stage estimates are retained in
[measurements](../benchmarks/gf8-butterfly-fusion/2026-11-02/measurements.json).

## Outlined dispatch experiment and decision

A fourth [outlined-dispatch prototype](../benchmarks/gf8-butterfly-fusion/2026-11-02/outlined.patch)
centralized feature detection and the AVX2 call to reduce repeated inline code.
Its initial 20-sample GF8 10+4/64-byte recovery changed −4.76% with GFNI and
−7.24% with AVX2, with respective baseline drifts −1.40% and −2.41%. The
corresponding 16+8/64-byte cases changed −8.59% and −6.98%. Initial GF8
4+2 encoding changed +4.32% at 64 B and +4.28% at 128 B, but an independent
40-sample GFNI comparison measured +0.70% and −1.68%, respectively.

The outlined prototype's GF16 AVX2 10+4/128-byte recovery with four missing
shards changed +4.31% and +3.83% in independent 40-sample comparisons; their
baseline drifts were −4.06% and −3.98%. The 10+4/64-byte one-missing case
changed +5.79% and +3.25%, with drifts −5.38% and −4.21%. A separate 16+8/64 B
four-missing observation changed +5.25%, but its repeat was +0.26%. These are
VM observations rather than proof of the precise cause of each change.

None of the prototypes is included in the production codec. They improved the
measured GF8 paths but did not demonstrate a consistently acceptable result
across the GF16 small-block controls. The outlined prototype adds an extra
dispatch call on x86 even when AVX2 is unavailable; correctness was checked in
non-AVX2 emulation, but native older-CPU performance was not measured.

The direct butterfly reference test passed for all 256 coefficients, both
transform directions, boundary sizes, unaligned views and guard bytes on native
x86-64. On ARM64, the outlined candidate passed workspace all-features tests,
`no_std` library tests, all-target all-feature Clippy and a 31-second run of the
existing contract fuzz target (716,324 executions); these ARM64 tests do not
execute the x86 kernel. An emulated x86 CPU without AVX2 passed 48 GF8 recovery
cases. The ARM64 `std` and `no_std` recovery probe code sections were identical
to baseline. The outlined prototype was not submitted to the published-version
x86 cross-decoding probe, since the performance controls did not justify
retaining it.
