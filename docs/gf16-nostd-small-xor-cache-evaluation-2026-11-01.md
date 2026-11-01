# GF16 no_std small XOR and plan-cache evaluation

Six experimental approaches were investigated against commit `75e0548`; none
was merged. They targeted x86-64 `no_std` encoding with compile-time AVX2 and
64-byte shards. The shared XOR specialization improved encoding but produced a
repeatable recovery regression. Per-codec plan caching improved repeated
encoding but worsened the first encoding and added per-instance retained state.

## XOR work

The existing general XOR fallback is already auto-vectorized in the tested
AVX2 release build. Its 64-byte path nevertheless passes through generic length
handling. Two prototypes special-cased that length: [inline at the caller](../benchmarks/gf16-nostd-xor64-cache/2026-11-01/xor-inline.patch)
and [inside the existing fallback](../benchmarks/gf16-nostd-xor64-cache/2026-11-01/xor-outlined.patch).
Both check the lengths and use two unaligned AVX2 loads/XORs/stores. The inline
variant increased the encoder radix-4 function from 2,955 to 4,274 bytes in
the measured binary; the outlined version retained that function's size and
increased the XOR function from 517 to 569 bytes.

A test-only [coefficient probe](../benchmarks/gf16-nostd-xor64-cache/2026-11-01/encode-coefficient-probe.rs)
showed that the encoding FFT plan for 10+4 and 16+8 includes a radix-4 block
with sentinel `m01` and `m02`. The counts in its [output](../benchmarks/gf16-nostd-xor64-cache/2026-11-01/encode-coefficient-observations.txt)
are plan blocks, not timed calls. Two encoding-only alternatives were also
screened: [specialize the group-accumulation XOR](../benchmarks/gf16-nostd-xor64-cache/2026-11-01/encode-accumulate.patch)
and [fuse three XORs in the matching forward butterfly](../benchmarks/gf16-nostd-xor64-cache/2026-11-01/encode-sentinel.patch).

| Candidate | 64 B encode 10+4 | 64 B encode 16+8 | Observed tradeoff |
| --- | ---: | ---: | --- |
| Inline XOR | −4.32% | −3.56% | Encoder radix-4 code grew substantially |
| Outlined XOR | −3.68% | −5.09% | Four-missing recovery regressed in a longer follow-up |
| Encode accumulation | −2.64% | −0.22% | Small or inconsistent benefit |
| Encode sentinel | −1.54% | −0.27% | 16+8, 1 MiB encoding +2.17% in the screen |

For outlined XOR, a longer recovery follow-up with four missing shards measured
+2.80% and +2.69% at 64 B, and +2.54% and +2.14% at 4 KiB, for 10+4 and
16+8 respectively. Corresponding A2/A1 baseline drift was −0.15%, −1.01%,
−0.55%, and +0.04%. This is evidence for the tested host and workloads, not a
universal hardware result. The source did not establish why a shared XOR
change affects recovery by those amounts.

## Per-codec plan caching

An [eager prototype](../benchmarks/gf16-nostd-xor64-cache/2026-11-01/cache-eager.patch)
kept a bounded immutable encoding plan in each `no_std` codec. On the measured
64-byte workload, repeated encoding improved by 9.98% for 10+4 and 4.84% for
16+8, but the first encoding of a freshly cloned codec slowed by 7.04% and
4.49%. The codec structure increased by 16 bytes on the tested AArch64 target,
and retained plan memory would remain until that codec was dropped. Initial
recovery screening also contained regressions up to +3.48%.

A [delayed prototype](../benchmarks/gf16-nostd-xor64-cache/2026-11-01/cache-delayed.patch)
kept the first plan temporary and only attempted caching after reuse. Its
64-byte first-encode screen was +4.48% and +2.67%; a longer follow-up was
+4.57% and +3.26%, with baseline drift −0.41% and +0.48%. It also required
additional per-codec state and a bounded-memory policy. The measured cold
regression and added state do not justify adopting it. The delayed prototype's
first-use, concurrent initialization, clone and oversized-plan tests passed
on AArch64. Neither caching prototype received full cross-platform release
validation.

The [cold harness](../benchmarks/gf16-nostd-xor64-cache/2026-11-01/cold-probe.rs)
creates the clone and shard fixture outside the measured closure, then times the
first encode on that clone, including destruction of the moved values. It
checks parity before timing. The warm harness reuses one codec. These are
specific workload definitions; different codec lifetimes may yield different
tradeoffs.

## Method and limits

All x86 comparisons used the same source path, Rust stable, locked dependencies,
`--no-default-features`, `-C target-feature=+avx2`, and an exclusive guest CPU
partition on a Linux VM. A1/B1/B2/A2 phases used 20 samples, 0.5-second warmup,
and a requested two-second measurement per case; the two focused follow-ups
used 40 samples, one-second warmup and five seconds requested measurement. The
reservation was restored; no competing intensive process was observed and
physical-host isolation was not verified. Drift was recorded but did not gate
runs. Changes compare the average of candidate means against the average of
baseline means, and negative means lower latency.

The focused XOR recovery controller accidentally retained the original
20-sample metadata in its context file, although its actual arguments requested
40 samples. The [Criterion stderr excerpt](../benchmarks/gf16-nostd-xor64-cache/2026-11-01/xor-recovery-focused-run.txt)
records the actual warmup and sample count for all phases. The [stage estimates](../benchmarks/gf16-nostd-xor64-cache/2026-11-01/estimates.json)
include all comparisons and allow baseline drift to be recalculated.

The XOR prototypes passed 53 GF16-filtered Linux x86 tests; the encode-only and
cache prototypes passed their 52 GF16-filtered tests. The outlined XOR candidate
also passed the all-features workspace suite excluding fuzz and 287 no_std
library tests on AArch64, as well as 228 AArch64 no_std old/new boundary checks.
These checks do not replace the omitted full compatibility tests for rejected
prototypes. The coefficient probe can be appended to the baseline's
`src/core/leopard_gf16/encode.rs` and run as its exact test with
`--no-default-features`. Experiment patches have zero context and can be
checked against `75e0548` using `git apply --unidiff-zero --check`.
