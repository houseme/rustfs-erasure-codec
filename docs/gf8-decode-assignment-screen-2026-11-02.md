# GF8 decode assignment investigation

Leopard GF8 decoding already writes its multiplication result directly to the
work lane or requested output. The remaining `mulgf8` operation uses a scalar
256-entry lookup for each byte. Its existing tables also contain pre-split low
and high nibble lookups: for a valid table, their XOR is exactly the full lookup.
This investigation tested AVX2 direct assignment using those tables; it is not a
layout-conversion fusion and does not introduce another pass over user data.

A software CPU-clock profile of warmed 10+4, 64-byte GF8 `ShardSlot` recovery
on a Linux x86-64 VM put `reconstruct_with_resources` at 32.78%–36.67% of
samples. Its inlined work includes the scalar assignment and other decode
operations; the profile does not isolate an exact assignment percentage.
The existing XOR wrapper and AVX2 XOR kernel together appeared in approximately
21%–25% of samples. Four [sampling summaries](../benchmarks/gf8-decode-assign/2026-11-02/sampling.json)
are retained; sampling proportions are not a direct latency breakdown.

Two [fixed-64-byte XOR prototypes](../benchmarks/gf8-decode-assign/2026-11-02/short-xor.patch)
were screened first. The direct version changed 16+8/64-byte GF8 encoding by
+12.50%, and the [inline version](../benchmarks/gf8-decode-assign/2026-11-02/short-xor-inline.patch)
by +9.39%; their baseline drifts were −1.48% and +0.31%. Neither was retained.

The [shared assignment/XOR kernel](../benchmarks/gf8-decode-assign/2026-11-02/shared-assign.patch)
reduced 10+4/64-byte GF8 recovery by roughly 22% in its initial two-backend
screen but also showed small-encoding regressions. Processing of an earlier profile report may have overlapped this screen outside
the benchmark's guest CPU partition; the screen is retained but is not the sole
performance evidence. An [independent assignment kernel](../benchmarks/gf8-decode-assign/2026-11-02/isolated-assign.patch)
kept the existing XOR kernel unchanged. An additional
[noinline dispatch variant](../benchmarks/gf8-decode-assign/2026-11-02/isolated-noinline.patch)
isolated the decode-only assignment from its caller. The zero-context patches
apply to baseline `d25e576` with `git apply --unidiff-zero`.

The vector path is restricted to equal-length slices of at least 64 bytes on
`std` x86-64 after runtime AVX2 detection. Unequal slices retain the original
scalar behavior, including the existing debug assertion. Other architectures,
`no_std` and x86 machines without AVX2 retain scalar assignment. The separate
XOR kernel and the encoded format do not change.

## Measurement

The retained [recovery probe](../benchmarks/gf8-decode-assign/2026-11-02/recovery-probe.rs)
and [encoding probe](../benchmarks/gf8-decode-assign/2026-11-02/encode-probe.rs)
check recovered or encoded bytes before timing. Recovery uses retained
`ShardSlot` buffers and includes marking missing slots; Option and GF16 controls
are separate. The 65,600-byte case crosses 32 KiB chunk boundaries with a
64-byte final chunk. All values are per shard.

Native x86-64 Linux VM measurements used rustc 1.99.1, locked dependencies,
one worker, a guest CPU partition under a shared lease and explicit GFNI or
AVX2 GF8 backend selection. The partition was restored after each run; physical
host isolation was not verified. Each completed comparison used A1/B1/B2/A2.
Initial screens used 20 Criterion samples, 0.5 s warmup and a requested 2 s
measurement per case. Focused and independent control runs used 40 samples,
1 s warmup and a requested 3 s measurement. Changes compare mean candidate with
mean baseline latency; negative is faster. Baseline drift is A2 / A1 − 1 and
is reported without excluding runs. Complete [stage estimates](../benchmarks/gf8-decode-assign/2026-11-02/estimates.json)
are retained.

## Candidate screening

The [separate-module candidate](../benchmarks/gf8-decode-assign/2026-11-02/module-assign.patch)
uses the same prebuilt GF8 tables as the existing path. Its target-specific
kernel assigns into the lane in one pass. It does not change the XOR kernel or
public APIs. The 20-sample screens measured these mean latency changes:

| Recovery workload | GFNI backend | AVX2 backend |
| --- | ---: | ---: |
| GF8 10+4, 64 B, four missing | −23.84% | −21.46% |
| GF8 10+4, 128 B, four missing | −37.47% | −36.44% |
| GF8 10+4, 1 MiB, four missing | −45.89% | −44.55% |
| GF8 16+8, 64 B, four missing | −20.31% | −22.00% |
| GF8 16+8, 128 B, four missing | −25.89% | −26.97% |
| GF16 10+4, 64 B, four missing | +4.98% | +1.01% |

The GF16 values are controls, not a claim that the GF8 assignment kernel ran
in GF16 decoding. The GFNI control's baseline drift was −3.42%. In independent 40-sample repetitions, the GF16
control changed +1.24% and −1.62% on GFNI and AVX2 respectively, while GF8
10+4/64 B recovery changed −22.61% and −22.29%. GF8 10+4/65,600 B changed
−59.72% and −59.34%. The corresponding GF8 64 B baseline drifts were +2.56%
and +4.21%; all stages are retained for interpretation.
A focused 40-sample encoding comparison measured 4+2/64 B at +3.22% (GFNI)
and +1.30% (AVX2); 16+8/64 B was −0.62% and −2.55% respectively. A second
GFNI comparison measured 4+2/64 B at −5.12% (baseline drift +0.19%) and
16+8/64 B at −2.14% (drift −0.96%), so the initial 4+2 change did not repeat.
Additional controls are in the stage estimates. The previous shared and
isolated prototypes showed larger encoding regressions and were not adopted.

The default `std` feature set was tested separately with the scalar generic
GF8 backend; Leopard's runtime AVX2 assignment remains independent of that
backend selection. With 20 samples, 10+4/64 B and 128 B recovery changed
−16.98% and −39.45%, with baseline drift −5.79% and +1.92%. The 40-sample
default encoding controls ranged from −0.87% to +1.75%; the largest drift in
those cases was +2.43%. These observations are specific to the measured VM.

A further identity-coefficient branch was tested on top of the noinline
candidate. It did not consistently improve the representative recovery cases,
so it was rejected; its [incremental patch](../benchmarks/gf8-decode-assign/2026-11-02/identity-on-noinline.patch),
[counts](../benchmarks/gf8-decode-assign/2026-11-02/identity-counts.json) and
stage estimates are retained.

## Correctness checks

The candidate's assignment was compared with the original full table over all
256 coefficients and 256 byte values, including lengths around SIMD boundaries,
unaligned offsets and untouched guards. Release-mode tests check the preexisting
shorter-slice behavior. Native x86-64 executes the AVX2 cases; a non-AVX2 x86
emulation passed 48 recovery cases but is not evidence of native old-CPU speed.

On ARM64, the warmed recovery probe's executable code section was identical to
baseline in both `std` and `no_std` builds. The candidate passed the workspace
all-features tests, the `no_std` library tests, all-target all-feature Clippy,
format checking, and the published 9.0.0 versus candidate 9.1.0 compatibility
probe on ARM64, including cross-decoding in both directions. Native x86-64
GFNI and AVX2 runs passed the assignment unit tests, four targeted recovery
integration test suites, the published-version compatibility probe and 20,000
deterministic GF8 contract cases per backend. The contract cases exercise the
existing fuzz harness deterministically; they are not a libFuzzer campaign.
Native non-AVX2 x86 performance remains unmeasured.
