# GF16 no_std AVX2 FWHT evaluation

The GF16 error locator uses a full-width Walsh–Hadamard transform when building
an erasure plan. On x86-64, the current no_std path uses SSE2 even when AVX2
is enabled for the entire compilation target. This experiment dispatches to
the existing AVX2 radix-2 and radix-4 kernels only when both `no_std` and the
compile-time `target_feature = "avx2"` are set. The baseline SSE2 and std
runtime dispatch are otherwise unchanged. The [experimental patch](../benchmarks/gf16-nostd-avx2/2026-11-01/avx2-experiment.patch)
is against `d584af6`; it is not production code.

The candidate passed all 46 GF16-targeted library tests on native x86-64 with
AVX2 explicitly enabled, including the full-length SIMD-versus-scalar FWHT
test. Both binaries passed the reconstruction benchmark's byte-for-byte
preflight and postflight checks. The no_std WASM compatibility probe passed
126 old/new checks, but does not exercise x86 AVX2. These checks do not replace
an x86 old/new format-compatibility run or broader feature-matrix validation.

## Controlled comparison

Both versions were built from the same source and target paths with Rust stable,
the same lockfile, `--no-default-features` and `RUSTFLAGS="-C target-feature=+avx2"`.
The feature tree was checked to confirm that the codec's `std` feature was not
enabled. The [benchmark](../benchmarks/gf16-nostd-avx2/2026-11-01/nostd-fwht-probe.rs)
is a std Criterion harness using a no_std codec; it constructs 10+4 and 16+8
GF16 shards, marks one or four retained slots missing, then reconstructs them.
Unlike std, no_std builds the error-locator plan for each call.

The Linux x86-64 VM held an exclusive guest CPU partition and a shared test
lock; monitoring saw no competing heavy process and the reservation was
released. Physical-host isolation was not verified. A1/B1/B2/A2 compares the
average of the two Criterion mean estimates on each side; negative percentages
mean lower latency. Baseline drift did not gate the experiment.

| Run | Shard size | Median latency change | Range across four cases |
| --- | ---: | ---: | ---: |
| [Initial](../benchmarks/gf16-nostd-avx2/2026-11-01/screen.json) | 64 B | −8.42% | −9.16% to −8.37% |
| Initial | 4 KiB | −5.04% | −7.68% to −4.30% |
| Initial | 64 KiB | +5.17% | −0.09% to +10.22% |
| Initial | 1 MiB | +6.44% | +2.35% to +9.37% |
| [Longer follow-up](../benchmarks/gf16-nostd-avx2/2026-11-01/focused.json) | 64 KiB | +4.07% | −1.67% to +7.04% |
| Longer follow-up | 1 MiB | +6.74% | +1.02% to +13.76% |

The initial screen used ten samples, 0.2 s warmup and 0.6 s requested
measurement per case; the follow-up used 20 samples, 0.5 s warmup and 2 s
requested measurement. The stored JSON includes each stage's samples, estimates
and A1-to-A2 baseline change. This experiment establishes a workload-specific
tradeoff, not its cause: compiler and binary-layout effects were not isolated.

The candidate was rejected because of the repeatable large-shard latency
increases. A size-dependent dispatch or alternate kernel would require its own
independent correctness and performance evaluation, including unaffected std,
non-AVX2 and encoding workloads; the results here do not establish that such a
change would solve the regression.
